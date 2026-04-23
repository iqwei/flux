use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use flux_producer::ProducerConfig;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Parser)]
#[command(name = "flux-producer", version, about = "Flux UDP telemetry producer")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    #[arg(long, value_name = "ADDR")]
    target: Option<SocketAddr>,
    #[arg(long, value_name = "LEVEL")]
    log: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log.as_deref())?;

    let cfg = ProducerConfig::load(cli.config.as_deref(), cli.target)?;
    tracing::debug!(?cfg, "loaded producer configuration");

    let cancel = CancellationToken::new();
    spawn_shutdown(cancel.clone());

    flux_producer::run(cfg, cancel).await
}

fn init_tracing(level: Option<&str>) -> Result<()> {
    let filter = match level {
        Some(explicit) => EnvFilter::try_new(explicit)
            .with_context(|| format!("invalid --log filter: {explicit}"))?,
        None => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    };
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_level(true)
        .try_init();
    Ok(())
}

fn spawn_shutdown(cancel: CancellationToken) {
    tokio::spawn(async move {
        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                match res {
                    Ok(()) => tracing::info!("ctrl-c received, shutting down"),
                    Err(err) => tracing::error!(%err, "failed to install ctrl-c handler"),
                }
                cancel.cancel();
            }
            () = cancel.cancelled() => {}
        }
    });
}
