use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use flux_server::{ConfigOverrides, ServerConfig};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Parser)]
#[command(
    name = "flux-server",
    version,
    about = "Flux aggregation + WebSocket broadcast server"
)]
struct Cli {
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    #[arg(long, value_name = "PORT")]
    udp_port: Option<u16>,
    #[arg(long, value_name = "PORT")]
    ws_port: Option<u16>,
    #[arg(long, value_name = "LEVEL")]
    log: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log.as_deref())?;

    let overrides = ConfigOverrides {
        udp_port: cli.udp_port,
        ws_port: cli.ws_port,
    };
    let cfg = ServerConfig::load(cli.config.as_deref(), &overrides)?;
    tracing::debug!(?cfg, "loaded configuration");

    let cancel = CancellationToken::new();
    spawn_shutdown(cancel.clone());

    flux_server::run(cfg, cancel).await
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
