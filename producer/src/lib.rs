#![forbid(unsafe_code)]

pub mod config;
mod emitter;
pub mod sim;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

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
    while let Some(res) = tasks.join_next().await {
        let err = match res {
            Ok(Ok(())) => continue,
            Ok(Err(err)) => err,
            Err(join_err) => anyhow::Error::new(join_err),
        };
        tracing::error!(error = ?err, "emitter task failed");
        if first_err.is_none() {
            first_err = Some(err);
            cancel.cancel();
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
