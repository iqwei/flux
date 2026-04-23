use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use flux_proto::{MetricSummary, PipelineHealth, Snapshot};

use crate::config::{DashboardConfig, Thresholds};

const PAGE_SCROLL: usize = 10;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Snapshot(Arc<Snapshot>),
    Connected,
    Disconnected,
    Reconnecting { attempt: u32, in_ms: u64 },
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
    Reconnecting {
        attempt: u32,
        in_ms: u64,
        since: Instant,
    },
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Fresh,
    Stale,
    Dead,
}

#[derive(Debug)]
pub struct App {
    pub server_url: String,
    pub thresholds: Thresholds,
    pub metrics: Vec<MetricSummary>,
    pub health: PipelineHealth,
    pub connection: ConnectionState,
    pub last_snapshot_at: Option<Instant>,
    pub snapshot_generated_at_ms: Option<u64>,
    pub scroll: usize,
    pub filter: String,
    pub filter_mode: bool,
    pub help_open: bool,
}

impl App {
    #[must_use]
    pub fn new(cfg: &DashboardConfig) -> Self {
        Self {
            server_url: cfg.server_url.clone(),
            thresholds: cfg.thresholds,
            metrics: Vec::new(),
            health: PipelineHealth::default(),
            connection: ConnectionState::Connecting,
            last_snapshot_at: None,
            snapshot_generated_at_ms: None,
            scroll: 0,
            filter: String::new(),
            filter_mode: false,
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
                self.connection = ConnectionState::Reconnecting {
                    attempt,
                    in_ms,
                    since: Instant::now(),
                };
                Flow::Continue
            }
            AppEvent::Key(key) => self.handle_key(key),
        }
    }

    fn apply_snapshot(&mut self, snap: &Snapshot) {
        self.metrics.clone_from(&snap.metrics);
        self.health = snap.health.clone();
        self.connection = ConnectionState::Connected;
        self.last_snapshot_at = Some(Instant::now());
        self.snapshot_generated_at_ms = Some(snap.generated_at_ms);
    }

    #[must_use]
    pub fn visible_metrics(&self) -> Vec<&MetricSummary> {
        if self.filter.is_empty() {
            return self.metrics.iter().collect();
        }
        let needle = self.filter.to_lowercase();
        self.metrics
            .iter()
            .filter(|m| m.name.to_lowercase().contains(&needle))
            .collect()
    }

    #[must_use]
    pub fn age_ms(&self, m: &MetricSummary) -> Option<u64> {
        let now = self.snapshot_generated_at_ms?;
        let last = m.last_update_ms?;
        Some(now.saturating_sub(last))
    }

    #[must_use]
    pub fn liveness(&self, age_ms: Option<u64>) -> Liveness {
        match age_ms {
            Some(age) if age >= self.thresholds.dead_after_ms => Liveness::Dead,
            Some(age) if age >= self.thresholds.stale_after_ms => Liveness::Stale,
            _ => Liveness::Fresh,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Flow {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return Flow::Continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
        {
            return Flow::Quit;
        }
        if self.help_open {
            if matches!(
                key.code,
                KeyCode::Char('?' | 'q' | 'Q') | KeyCode::Esc | KeyCode::Enter
            ) {
                self.help_open = false;
            }
            return Flow::Continue;
        }
        if self.filter_mode {
            self.handle_filter_key(key);
            return Flow::Continue;
        }
        self.handle_nav_key(key)
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filter_mode = false;
                self.scroll = 0;
            }
            KeyCode::Enter => {
                self.filter_mode = false;
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.scroll = 0;
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.scroll = 0;
            }
            _ => {}
        }
    }

    fn handle_nav_key(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Char('q' | 'Q') => return Flow::Quit,
            KeyCode::Char('?') => self.help_open = true,
            KeyCode::Char('/') => self.filter_mode = true,
            KeyCode::Esc => self.filter.clear(),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(PAGE_SCROLL),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(PAGE_SCROLL),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = usize::MAX,
            _ => {}
        }
        Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_proto::{MetricKind, SNAPSHOT_SCHEMA_VERSION};

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    fn metric(name: &str, last_update_ms: Option<u64>) -> MetricSummary {
        MetricSummary {
            name: name.to_owned(),
            unit: None,
            kind: MetricKind::F64,
            last: Some(0.0),
            min: Some(0.0),
            max: Some(0.0),
            avg: Some(0.0),
            rate_pps: 0.0,
            sample_count: 0,
            last_update_ms,
        }
    }

    fn snapshot(generated_at_ms: u64, metrics: Vec<MetricSummary>) -> Snapshot {
        Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generated_at_ms,
            uptime_ms: 0,
            metrics,
            health: PipelineHealth::default(),
        }
    }

    fn app() -> App {
        App::new(&DashboardConfig::default())
    }

    #[test]
    fn starts_in_connecting() {
        assert_eq!(app().connection, ConnectionState::Connecting);
    }

    #[test]
    fn q_quits() {
        let mut a = app();
        assert_eq!(
            a.on(AppEvent::Key(key(KeyCode::Char('q'), KeyModifiers::NONE))),
            Flow::Quit
        );
    }

    #[test]
    fn ctrl_c_quits_in_any_mode() {
        let mut a = app();
        a.filter_mode = true;
        assert_eq!(
            a.on(AppEvent::Key(key(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            Flow::Quit
        );
    }

    #[test]
    fn q_does_not_quit_in_filter_mode() {
        let mut a = app();
        a.filter_mode = true;
        let flow = a.on(AppEvent::Key(key(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert_eq!(flow, Flow::Continue);
        assert_eq!(a.filter, "q");
    }

    #[test]
    fn slash_enters_filter_mode_and_typing_accumulates() {
        let mut a = app();
        a.on(AppEvent::Key(key(KeyCode::Char('/'), KeyModifiers::NONE)));
        a.on(AppEvent::Key(key(KeyCode::Char('c'), KeyModifiers::NONE)));
        a.on(AppEvent::Key(key(KeyCode::Char('p'), KeyModifiers::NONE)));
        a.on(AppEvent::Key(key(KeyCode::Char('u'), KeyModifiers::NONE)));
        assert_eq!(a.filter, "cpu");
        a.on(AppEvent::Key(key(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(!a.filter_mode);
        assert_eq!(a.filter, "cpu");
    }

    #[test]
    fn esc_clears_filter() {
        let mut a = app();
        a.filter = "xyz".into();
        a.on(AppEvent::Key(key(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(a.filter.is_empty());
    }

    #[test]
    fn help_toggles() {
        let mut a = app();
        a.on(AppEvent::Key(key(KeyCode::Char('?'), KeyModifiers::NONE)));
        assert!(a.help_open);
        a.on(AppEvent::Key(key(KeyCode::Char('?'), KeyModifiers::NONE)));
        assert!(!a.help_open);
    }

    #[test]
    fn arrows_scroll() {
        let mut a = app();
        a.on(AppEvent::Key(key(KeyCode::Down, KeyModifiers::NONE)));
        a.on(AppEvent::Key(key(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(a.scroll, 2);
        a.on(AppEvent::Key(key(KeyCode::Up, KeyModifiers::NONE)));
        assert_eq!(a.scroll, 1);
        a.on(AppEvent::Key(key(KeyCode::PageDown, KeyModifiers::NONE)));
        assert_eq!(a.scroll, 1 + PAGE_SCROLL);
        a.on(AppEvent::Key(key(KeyCode::Home, KeyModifiers::NONE)));
        assert_eq!(a.scroll, 0);
        a.on(AppEvent::Key(key(KeyCode::End, KeyModifiers::NONE)));
        assert_eq!(a.scroll, usize::MAX);
    }

    #[test]
    fn snapshot_records_generated_at() {
        let mut a = app();
        a.on(AppEvent::Snapshot(Arc::new(snapshot(
            10_000,
            vec![metric("m", Some(9_000))],
        ))));
        assert_eq!(a.snapshot_generated_at_ms, Some(10_000));
        assert_eq!(a.age_ms(&a.metrics[0]), Some(1_000));
    }

    #[test]
    fn liveness_buckets() {
        let mut a = app();
        a.thresholds = Thresholds {
            stale_after_ms: 1_000,
            dead_after_ms: 3_000,
        };
        assert_eq!(a.liveness(None), Liveness::Fresh);
        assert_eq!(a.liveness(Some(500)), Liveness::Fresh);
        assert_eq!(a.liveness(Some(1_000)), Liveness::Stale);
        assert_eq!(a.liveness(Some(2_999)), Liveness::Stale);
        assert_eq!(a.liveness(Some(3_000)), Liveness::Dead);
    }

    #[test]
    fn visible_metrics_filters_case_insensitive() {
        let mut a = app();
        a.metrics = vec![
            metric("cpu.temp", Some(0)),
            metric("GPU.temp", Some(0)),
            metric("disk.io", Some(0)),
        ];
        a.filter = "TEMP".into();
        let visible: Vec<_> = a.visible_metrics().iter().map(|m| m.name.clone()).collect();
        assert_eq!(visible, vec!["cpu.temp", "GPU.temp"]);
    }

    #[test]
    fn connection_transitions() {
        let mut a = app();
        a.on(AppEvent::Disconnected);
        assert_eq!(a.connection, ConnectionState::Disconnected);
        a.on(AppEvent::Reconnecting {
            attempt: 3,
            in_ms: 500,
        });
        assert!(matches!(
            a.connection,
            ConnectionState::Reconnecting {
                attempt: 3,
                in_ms: 500,
                ..
            }
        ));
        a.on(AppEvent::Connected);
        assert_eq!(a.connection, ConnectionState::Connected);
    }
}
