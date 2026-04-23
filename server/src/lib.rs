//! Flux server: UDP telemetry ingestion, aggregation, WebSocket broadcast.
//!
//! # Concurrency design
//!
//! ```text
//! ingress_task (UDP recv loop) ─┐
//!                               │ mpsc<Event>  (bounded: ingress_buffer)
//!                               ▼
//!                       aggregator_task
//!                               │ broadcast<Arc<Snapshot>>  (bounded: broadcast_buffer)
//!                               ▼
//!                 ws_task ── N × subscriber_task ── WebSocket
//!                               │
//!                               └── SubscriberJoin / SubscriberLeave
//!                                   back onto the same mpsc
//! ```
//!
//! - The aggregator is the sole owner of [`MetricStore`]. No shared mutable
//!   state, no locks.
//! - Ingest fan-in is an mpsc; broadcast fan-out uses `tokio::sync::broadcast`.
//!   A lagging subscriber is disconnected, never blocks the hub.
//! - `CancellationToken` is the single shutdown primitive: every long-lived
//!   task selects on it and exits cleanly.
//!
//! [`MetricStore`]: crate::aggregator::MetricStore

mod aggregator;
mod broadcast;
pub mod config;
mod ingest;

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

use crate::aggregator::{aggregator_task, AggregatorConfig};
use crate::broadcast::{ws_task, WsState};
use crate::ingest::{ingress_task, Event};

#[derive(Debug)]
pub struct BoundServer {
    pub udp_addr: SocketAddr,
    pub ws_addr: SocketAddr,
    tasks: JoinSet<anyhow::Result<()>>,
}

impl BoundServer {
    pub async fn wait(mut self) -> anyhow::Result<()> {
        let mut first_err: Option<anyhow::Error> = None;
        while let Some(res) = self.tasks.join_next().await {
            match res {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    if first_err.is_none() {
                        first_err = Some(err);
                    }
                }
                Err(join_err) => {
                    if first_err.is_none() {
                        first_err = Some(anyhow::Error::new(join_err));
                    }
                }
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
        tasks.spawn(async move { ingress_task(udp, tx, cancel).await.map_err(Into::into) });
    }

    {
        let cancel = cancel.clone();
        let agg_cfg = AggregatorConfig::from(&cfg);
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
        tasks,
    })
}

pub async fn run(cfg: ServerConfig, cancel: CancellationToken) -> anyhow::Result<()> {
    let server = bind(cfg, cancel).await?;
    tracing::info!(udp = %server.udp_addr, ws = %server.ws_addr, "flux server listening");
    server.wait().await
}
