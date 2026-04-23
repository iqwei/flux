mod aggregator;
pub mod clock;
pub mod config;
pub mod health;
mod ingest;
pub mod metric_store;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use flux_proto::Snapshot;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{broadcast as bcast, mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

pub use crate::clock::{Clock, SystemClock};
pub use crate::config::{ConfigOverrides, ServerConfig};

#[doc(hidden)]
pub use crate::clock::FakeClock;

use crate::aggregator::aggregator_task;
use crate::ingest::{ingress_task, Event};
use crate::ws::{ws_task, WsState};

#[derive(Debug)]
pub struct BoundServer {
    pub udp_addr: SocketAddr,
    pub ws_addr: SocketAddr,
    cancel: CancellationToken,
    shutdown_budget: Duration,
    tasks: JoinSet<anyhow::Result<()>>,
}

impl BoundServer {
    pub async fn wait(mut self) -> anyhow::Result<()> {
        let mut first_err: Option<anyhow::Error> = None;
        loop {
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => break,
                res = self.tasks.join_next() => match res {
                    None => return Ok(()),
                    Some(outcome) => {
                        if let Some(err) = task_error(outcome) {
                            first_err.get_or_insert(err);
                            self.cancel.cancel();
                            break;
                        }
                    }
                }
            }
        }

        let started = Instant::now();
        let remaining = self.shutdown_budget;
        if timeout(remaining, drain_tasks(&mut self.tasks, &mut first_err))
            .await
            .is_ok()
        {
            tracing::info!(
                elapsed_ms = ms_from(started.elapsed()),
                "server shutdown complete"
            );
        } else {
            tracing::warn!(
                budget_ms = ms_from(remaining),
                "shutdown budget exceeded, aborting remaining tasks"
            );
            self.tasks.abort_all();
            while self.tasks.join_next().await.is_some() {}
        }

        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

async fn drain_tasks(
    tasks: &mut JoinSet<anyhow::Result<()>>,
    first_err: &mut Option<anyhow::Error>,
) {
    while let Some(outcome) = tasks.join_next().await {
        if let Some(err) = task_error(outcome) {
            first_err.get_or_insert(err);
        }
    }
}

fn task_error(
    outcome: Result<anyhow::Result<()>, tokio::task::JoinError>,
) -> Option<anyhow::Error> {
    match outcome {
        Ok(Ok(())) => None,
        Ok(Err(err)) => Some(err),
        Err(join) if join.is_cancelled() => None,
        Err(join) => Some(anyhow::Error::new(join)),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn ms_from(d: Duration) -> u64 {
    let ms = d.as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

pub async fn bind(cfg: ServerConfig, cancel: CancellationToken) -> anyhow::Result<BoundServer> {
    bind_with_clock(cfg, Arc::new(SystemClock), cancel).await
}

pub async fn bind_with_clock(
    cfg: ServerConfig,
    clock: Arc<dyn Clock>,
    cancel: CancellationToken,
) -> anyhow::Result<BoundServer> {
    let udp = UdpSocket::bind(cfg.udp_bind)
        .await
        .with_context(|| format!("binding UDP socket on {}", cfg.udp_bind))?;
    let udp_addr = udp.local_addr()?;

    let ws_listener = TcpListener::bind(cfg.ws_bind)
        .await
        .with_context(|| format!("binding WebSocket listener on {}", cfg.ws_bind))?;
    let ws_addr = ws_listener.local_addr()?;

    let (event_tx, event_rx) = mpsc::channel::<Event>(cfg.ingress_buffer);
    let (snap_tx, _) = bcast::channel::<Arc<Snapshot>>(cfg.broadcast_buffer);
    let (ingress_done_tx, ingress_done_rx) = oneshot::channel();
    let (final_snapshot_tx, final_snapshot_rx) = watch::channel::<Option<Arc<Snapshot>>>(None);
    let shutdown_budget = Duration::from_millis(cfg.shutdown_budget_ms);

    let ws_state = Arc::new(WsState {
        events: event_tx.clone(),
        snapshots: snap_tx.clone(),
    });

    let mut tasks: JoinSet<anyhow::Result<()>> = JoinSet::new();

    {
        let cancel = cancel.clone();
        let tx = event_tx.clone();
        tasks.spawn(async move {
            let result = ingress_task(udp, tx, shutdown_budget, cancel).await;
            let _ = ingress_done_tx.send(());
            result
        });
    }

    {
        let cancel = cancel.clone();
        let agg_cfg = cfg.clone();
        let agg_clock = Arc::clone(&clock);
        tasks.spawn(async move {
            aggregator_task(
                event_rx,
                snap_tx,
                final_snapshot_tx,
                agg_cfg,
                agg_clock,
                ingress_done_rx,
                cancel,
            )
            .await
        });
    }

    drop(event_tx);

    {
        let cancel = cancel.clone();
        let state = ws_state;
        tasks.spawn(async move {
            ws_task(
                ws_listener,
                state,
                final_snapshot_rx,
                shutdown_budget,
                cancel,
            )
            .await
        });
    }

    Ok(BoundServer {
        udp_addr,
        ws_addr,
        cancel,
        shutdown_budget,
        tasks,
    })
}

pub async fn run(cfg: ServerConfig, cancel: CancellationToken) -> anyhow::Result<()> {
    let server = bind(cfg, cancel).await?;
    tracing::info!(udp = %server.udp_addr, ws = %server.ws_addr, "flux server listening");
    server.wait().await
}
