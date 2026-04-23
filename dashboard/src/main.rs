use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use flux_dashboard::{ui, ws_client, App, AppEvent, DashboardConfig, Flow};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

const EVENT_CHANNEL_CAPACITY: usize = 512;
const RENDER_INTERVAL_MS: u64 = 33;
const INPUT_POLL_MS: u64 = 100;

type DashTerminal = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Parser)]
#[command(name = "flux-dashboard", version, about = "Flux terminal dashboard")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    #[arg(long, value_name = "URL")]
    server: Option<String>,
    #[arg(long, value_name = "PATH")]
    log_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log_file.as_deref())?;
    let cfg = DashboardConfig::load(cli.config.as_deref(), cli.server.as_deref())?;

    install_panic_hook();
    let mut terminal = setup_terminal().context("setting up terminal")?;
    let app_result = run_app(cfg, &mut terminal).await;
    let restore_result = restore_terminal(&mut terminal);
    match (app_result, restore_result) {
        (Err(e), _) | (Ok(()), Err(e)) => Err(e),
        (Ok(()), Ok(())) => Ok(()),
    }
}

async fn run_app(cfg: DashboardConfig, terminal: &mut DashTerminal) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<AppEvent>(EVENT_CHANNEL_CAPACITY);
    let cancel = CancellationToken::new();

    let ws_handle = spawn_ws_client(cfg.clone(), tx.clone(), cancel.clone());
    let input_handle = spawn_input_reader(tx.clone(), cancel.clone());
    let signal_handle = spawn_signal_watcher(cancel.clone());
    drop(tx);

    let mut app = App::new(cfg.server_url.clone());
    let mut render_tick = interval(Duration::from_millis(RENDER_INTERVAL_MS));
    render_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    render_tick.tick().await;
    terminal.draw(|f| ui::render(f, &app))?;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            _ = render_tick.tick() => {
                terminal.draw(|f| ui::render(f, &app))?;
            }
            maybe = rx.recv() => {
                let Some(event) = maybe else { break; };
                if matches!(app.on(event), Flow::Quit) {
                    cancel.cancel();
                    break;
                }
            }
        }
    }

    cancel.cancel();
    let _ = ws_handle.await;
    let _ = input_handle.await;
    let _ = signal_handle.await;
    Ok(())
}

fn spawn_ws_client(
    cfg: DashboardConfig,
    tx: mpsc::Sender<AppEvent>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        ws_client::run_ws(cfg.server_url, cfg.reconnect, tx, cancel).await;
    })
}

fn spawn_input_reader(tx: mpsc::Sender<AppEvent>, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        let poll = Duration::from_millis(INPUT_POLL_MS);
        while !cancel.is_cancelled() {
            match crossterm::event::poll(poll) {
                Ok(true) => match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
                        if tx.blocking_send(AppEvent::Key(key)).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        tracing::debug!(%err, "crossterm read error");
                        return;
                    }
                },
                Ok(false) => {}
                Err(err) => {
                    tracing::debug!(%err, "crossterm poll error");
                    return;
                }
            }
        }
    })
}

fn spawn_signal_watcher(cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                if let Err(err) = res {
                    tracing::error!(%err, "ctrl-c handler install failed");
                }
                cancel.cancel();
            }
            () = cancel.cancelled() => {}
        }
    })
}

fn setup_terminal() -> Result<DashTerminal> {
    enable_raw_mode().context("enabling raw mode")?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen).context("entering alternate screen")?;
    let backend = CrosstermBackend::new(out);
    Terminal::new(backend).context("creating terminal backend")
}

fn restore_terminal(terminal: &mut DashTerminal) -> Result<()> {
    let raw = disable_raw_mode();
    let alt = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let cursor = terminal.show_cursor();
    raw.context("disabling raw mode")?;
    alt.context("leaving alternate screen")?;
    cursor.context("restoring cursor")?;
    Ok(())
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}

fn init_tracing(path: Option<&Path>) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let file = std::fs::File::create(path)
        .with_context(|| format!("creating log file {}", path.display()))?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .try_init();
    Ok(())
}
