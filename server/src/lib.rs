mod aggregator;
pub mod config;
mod ingest;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use flux_proto::Snapshot;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{broadcast as bcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub use crate::config::{ConfigOverrides, ServerConfig};

use crate::aggregator::aggregator_task;
use crate::ingest::{ingress_task, Event};
use crate::ws::{ws_task, WsState};

#[derive(Debug)]
pub struct BoundServer {
    pub udp_addr: SocketAddr,
    pub ws_addr: SocketAddr,
    cancel: CancellationToken,
    tasks: JoinSet<anyhow::Result<()>>,
}

impl BoundServer {
    pub async fn wait(mut self) -> anyhow::Result<()> {
        let mut first_err: Option<anyhow::Error> = None;
        while let Some(res) = self.tasks.join_next().await {
            let err = match res {
                Ok(Ok(())) => continue,
                Ok(Err(err)) => err,
                Err(join_err) => anyhow::Error::new(join_err),
            };
            if first_err.is_none() {
                first_err = Some(err);
                self.cancel.cancel();
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

pub async fn bind(cfg: ServerConfig, cancel: CancellationToken) -> anyhow::Result<BoundServer> {
    let started_at = Instant::now();

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

    let ws_state = Arc::new(WsState {
        events: event_tx.clone(),
        snapshots: snap_tx.clone(),
    });

    let mut tasks: JoinSet<anyhow::Result<()>> = JoinSet::new();

    {
        let cancel = cancel.clone();
        let tx = event_tx.clone();
        tasks.spawn(async move { ingress_task(udp, tx, cancel).await });
    }

    {
        let cancel = cancel.clone();
        let agg_cfg = cfg.clone();
        tasks.spawn(async move {
            aggregator_task(event_rx, snap_tx, agg_cfg, started_at, cancel).await
        });
    }

    drop(event_tx);

    {
        let cancel = cancel.clone();
        let state = ws_state;
        tasks.spawn(async move { ws_task(ws_listener, state, cancel).await });
    }

    Ok(BoundServer {
        udp_addr,
        ws_addr,
        cancel,
        tasks,
    })
}

pub async fn run(cfg: ServerConfig, cancel: CancellationToken) -> anyhow::Result<()> {
    let server = bind(cfg, cancel).await?;
    tracing::info!(udp = %server.udp_addr, ws = %server.ws_addr, "flux server listening");
    server.wait().await
}
