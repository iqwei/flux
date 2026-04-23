use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use flux_proto::{MetricSummary, PipelineHealth, Snapshot};

#[derive(Debug, Clone)]
pub enum AppEvent {
    Snapshot(Arc<Snapshot>),
    Connected,
    Disconnected,
    Reconnecting { attempt: u32, in_ms: u64 },
    Tick,
    Key(KeyEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting { attempt: u32, in_ms: u64 },
    Disconnected,
}

#[derive(Debug)]
pub struct App {
    pub server_url: String,
    pub metrics: Vec<MetricSummary>,
    pub health: PipelineHealth,
    pub connection: ConnectionState,
    pub last_snapshot_at: Option<Instant>,
    pub scroll: usize,
    pub filter: Option<String>,
    pub help_open: bool,
}

impl App {
    #[must_use]
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            metrics: Vec::new(),
            health: PipelineHealth::default(),
            connection: ConnectionState::Connecting,
            last_snapshot_at: None,
            scroll: 0,
            filter: None,
            help_open: false,
        }
    }

    pub fn on(&mut self, event: AppEvent) -> Flow {
        match event {
            AppEvent::Snapshot(snap) => {
                self.apply_snapshot(&snap);
                Flow::Continue
            }
            AppEvent::Connected => {
                self.connection = ConnectionState::Connected;
                Flow::Continue
            }
            AppEvent::Disconnected => {
                self.connection = ConnectionState::Disconnected;
                Flow::Continue
            }
            AppEvent::Reconnecting { attempt, in_ms } => {
                self.connection = ConnectionState::Reconnecting { attempt, in_ms };
                Flow::Continue
            }
            AppEvent::Tick => Flow::Continue,
            AppEvent::Key(key) => handle_key(key),
        }
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        self.metrics.clone_from(&snap.metrics);
        self.health = snap.health.clone();
        self.connection = ConnectionState::Connected;
        self.last_snapshot_at = Some(Instant::now());
    }
}

fn handle_key(key: KeyEvent) -> Flow {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return Flow::Continue;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Char('q' | 'Q') | KeyCode::Esc, _) => Flow::Quit,
        (KeyCode::Char('c' | 'C'), mods) if mods.contains(KeyModifiers::CONTROL) => Flow::Quit,
        _ => Flow::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_proto::SNAPSHOT_SCHEMA_VERSION;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    fn empty_snapshot() -> Snapshot {
        Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generated_at_ms: 0,
            uptime_ms: 0,
            metrics: Vec::new(),
            health: PipelineHealth::default(),
        }
    }

    #[test]
    fn starts_in_connecting() {
        let app = App::new("ws://x".into());
        assert_eq!(app.connection, ConnectionState::Connecting);
    }

    #[test]
    fn q_quits() {
        let mut app = App::new("ws://x".into());
        let flow = app.on(AppEvent::Key(key(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert_eq!(flow, Flow::Quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new("ws://x".into());
        let flow = app.on(AppEvent::Key(key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert_eq!(flow, Flow::Quit);
    }

    #[test]
    fn arrow_does_not_quit() {
        let mut app = App::new("ws://x".into());
        let flow = app.on(AppEvent::Key(key(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(flow, Flow::Continue);
    }

    #[test]
    fn snapshot_updates_state() {
        let mut app = App::new("ws://x".into());
        let flow = app.on(AppEvent::Snapshot(Arc::new(empty_snapshot())));
        assert_eq!(flow, Flow::Continue);
        assert_eq!(app.connection, ConnectionState::Connected);
        assert!(app.last_snapshot_at.is_some());
    }

    #[test]
    fn connection_transitions() {
        let mut app = App::new("ws://x".into());
        app.on(AppEvent::Disconnected);
        assert_eq!(app.connection, ConnectionState::Disconnected);
        app.on(AppEvent::Reconnecting {
            attempt: 3,
            in_ms: 500,
        });
        assert_eq!(
            app.connection,
            ConnectionState::Reconnecting {
                attempt: 3,
                in_ms: 500
            }
        );
        app.on(AppEvent::Connected);
        assert_eq!(app.connection, ConnectionState::Connected);
    }
}
