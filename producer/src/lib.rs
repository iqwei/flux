#![forbid(unsafe_code)]

pub mod config;
mod emitter;
pub mod sim;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::net::UdpSocket;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub use crate::config::{MetricSpec, ProducerConfig, Simulation};

const BIND_ANY: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));

pub async fn run(cfg: ProducerConfig, cancel: CancellationToken) -> Result<()> {
    let socket = UdpSocket::bind(BIND_ANY)
        .await
        .context("binding producer UDP socket")?;
    socket
        .connect(cfg.target)
        .await
        .with_context(|| format!("connecting UDP socket to {}", cfg.target))?;
    let socket = Arc::new(socket);

    tracing::info!(target = %cfg.target, metrics = cfg.metrics.len(), "producer starting");

    let mut tasks: JoinSet<Result<()>> = JoinSet::new();
    for spec in cfg.metrics {
        let socket = Arc::clone(&socket);
        let cancel = cancel.clone();
        tasks.spawn(emitter::run_emitter(spec, socket, cancel));
    }

    let mut first_err: Option<anyhow::Error> = None;
    let mut shutdown_at: Option<Instant> = None;
    let cancelled = cancel.cancelled();
    tokio::pin!(cancelled);

    loop {
        tokio::select! {
            biased;
            () = &mut cancelled, if shutdown_at.is_none() => {
                shutdown_at = Some(Instant::now());
            }
            maybe = tasks.join_next() => {
                let Some(res) = maybe else { break };
                match res {
                    Ok(Ok(())) => {}
                    Err(join) if join.is_cancelled() => {}
                    Ok(Err(err)) => {
                        tracing::error!(error = ?err, "emitter task failed");
                        if first_err.is_none() {
                            first_err = Some(err);
                            shutdown_at.get_or_insert_with(Instant::now);
                            cancel.cancel();
                        }
                    }
                    Err(join_err) => {
                        let err = anyhow::Error::new(join_err);
                        tracing::error!(error = ?err, "emitter task aborted");
                        if first_err.is_none() {
                            first_err = Some(err);
                            shutdown_at.get_or_insert_with(Instant::now);
                            cancel.cancel();
                        }
                    }
                }
            }
        }
    }

    if let Some(start) = shutdown_at {
        tracing::info!(
            elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            "producer shutdown complete"
        );
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
