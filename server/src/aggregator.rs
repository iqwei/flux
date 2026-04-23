use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flux_proto::{
    FluxPacket, MetricKind, MetricSummary, PipelineHealth, Snapshot, ValueKind,
    SNAPSHOT_SCHEMA_VERSION,
};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::config::ServerConfig;
use crate::ingest::Event;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AggregatorConfig {
    pub(crate) broadcast_interval_ms: u64,
    pub(crate) rolling_window: usize,
}

impl From<&ServerConfig> for AggregatorConfig {
    fn from(cfg: &ServerConfig) -> Self {
        Self {
            broadcast_interval_ms: cfg.broadcast_interval_ms,
            rolling_window: cfg.rolling_window,
        }
    }
}

#[derive(Debug)]
struct MetricState {
    kind: MetricKind,
    last: f64,
    min: f64,
    max: f64,
    window: VecDeque<f64>,
    window_cap: usize,
    sample_count: u64,
    last_update: Instant,
    last_update_ms: u64,
}

impl MetricState {
    fn new(kind: MetricKind, value: f64, now: Instant, now_ms: u64, window_cap: usize) -> Self {
        let mut window = VecDeque::with_capacity(window_cap.max(1));
        window.push_back(value);
        Self {
            kind,
            last: value,
            min: value,
            max: value,
            window,
            window_cap,
            sample_count: 1,
            last_update: now,
            last_update_ms: now_ms,
        }
    }

    fn observe(&mut self, value: f64, now: Instant, now_ms: u64) {
        self.last = value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        if self.window.len() == self.window_cap {
            self.window.pop_front();
        }
        self.window.push_back(value);
        self.sample_count = self.sample_count.saturating_add(1);
        self.last_update = now;
        self.last_update_ms = now_ms;
    }

    fn summarize(&self, name: &str, now: Instant, interval_s: f64) -> MetricSummary {
        let avg = if self.window.is_empty() {
            None
        } else {
            let sum: f64 = self.window.iter().copied().sum();
            #[allow(clippy::cast_precision_loss)]
            let denom = self.window.len() as f64;
            Some(sum / denom)
        };
        let rate_pps = if interval_s > 0.0 && now >= self.last_update {
            let elapsed = now
                .saturating_duration_since(self.last_update)
                .as_secs_f64();
            if elapsed > interval_s * 4.0 {
                0.0
            } else {
                #[allow(clippy::cast_precision_loss)]
                let samples = self.window.len() as f64;
                samples / interval_s.max(f64::EPSILON)
            }
        } else {
            0.0
        };
        MetricSummary {
            name: name.to_owned(),
            unit: None,
            kind: self.kind,
            last: Some(self.last),
            min: Some(self.min),
            max: Some(self.max),
            avg,
            rate_pps,
            sample_count: self.sample_count,
            last_update_ms: Some(self.last_update_ms),
        }
    }
}

#[derive(Debug, Default)]
struct HealthCounters {
    packets_received: u64,
    packets_parse_err: u64,
    subscriber_count: u32,
    last_tick_packets: u64,
}

#[derive(Debug)]
pub(crate) struct MetricStore {
    map: HashMap<String, MetricState>,
    window_cap: usize,
}

impl MetricStore {
    #[must_use]
    pub(crate) fn new(window_cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            window_cap: window_cap.max(1),
        }
    }

    fn ingest(&mut self, packet: FluxPacket, now: Instant) {
        let value = value_as_f64(&packet.value);
        let kind = MetricKind::from(&packet.value);
        let now_ms = packet.timestamp_ms;
        match self.map.get_mut(&packet.name) {
            Some(state) => state.observe(value, now, now_ms),
            None => {
                self.map.insert(
                    packet.name,
                    MetricState::new(kind, value, now, now_ms, self.window_cap),
                );
            }
        }
    }

    fn snapshot(&self, now: Instant, interval_s: f64) -> Vec<MetricSummary> {
        let mut out: Vec<MetricSummary> = self
            .map
            .iter()
            .map(|(name, state)| state.summarize(name, now, interval_s))
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

pub(crate) async fn aggregator_task(
    mut rx: mpsc::Receiver<Event>,
    snap_tx: broadcast::Sender<Arc<Snapshot>>,
    cfg: AggregatorConfig,
    started_at: Instant,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let mut store = MetricStore::new(cfg.rolling_window);
    let mut health = HealthCounters::default();
    let period = Duration::from_millis(cfg.broadcast_interval_ms);
    let interval_s = period.as_secs_f64();
    let mut ticker = interval(period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker.tick().await;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(()),
            _ = ticker.tick() => {
                let snap = build_snapshot(&store, &mut health, started_at, interval_s);
                let _ = snap_tx.send(Arc::new(snap));
            }
            maybe = rx.recv() => match maybe {
                Some(event) => apply_event(event, &mut store, &mut health),
                None => return Ok(()),
            }
        }
    }
}

fn apply_event(event: Event, store: &mut MetricStore, health: &mut HealthCounters) {
    match event {
        Event::Packet(packet, at) => {
            health.packets_received = health.packets_received.saturating_add(1);
            store.ingest(packet, at);
        }
        Event::ParseError => {
            health.packets_parse_err = health.packets_parse_err.saturating_add(1);
        }
        Event::SubscriberJoin => {
            health.subscriber_count = health.subscriber_count.saturating_add(1);
        }
        Event::SubscriberLeave => {
            health.subscriber_count = health.subscriber_count.saturating_sub(1);
        }
    }
}

fn build_snapshot(
    store: &MetricStore,
    health: &mut HealthCounters,
    started_at: Instant,
    interval_s: f64,
) -> Snapshot {
    let now = Instant::now();
    let uptime_ms = u64_millis(now.saturating_duration_since(started_at));
    let generated_at_ms = unix_now_ms();
    let delta = health
        .packets_received
        .saturating_sub(health.last_tick_packets);
    health.last_tick_packets = health.packets_received;
    #[allow(clippy::cast_precision_loss)]
    let ingest_rate_pps = if interval_s > 0.0 {
        delta as f64 / interval_s
    } else {
        0.0
    };
    Snapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at_ms,
        uptime_ms,
        metrics: store.snapshot(now, interval_s),
        health: PipelineHealth {
            packets_received: health.packets_received,
            packets_parse_err: health.packets_parse_err,
            subscriber_count: health.subscriber_count,
            ingest_rate_pps,
            uptime_ms,
        },
    }
}

#[allow(clippy::cast_precision_loss)]
fn value_as_f64(v: &ValueKind) -> f64 {
    match *v {
        ValueKind::U64(n) => n as f64,
        ValueKind::I64(n) => n as f64,
        ValueKind::F64(n) => n,
        ValueKind::Bool(b) => {
            if b {
                1.0
            } else {
                0.0
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn u64_millis(d: Duration) -> u64 {
    let ms = d.as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, u64_millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_proto::{FluxPacket, PROTOCOL_VERSION};

    fn packet(name: &str, value: ValueKind, ts: u64) -> FluxPacket {
        FluxPacket {
            version: PROTOCOL_VERSION,
            timestamp_ms: ts,
            name: name.to_owned(),
            value,
        }
    }

    #[test]
    fn ingest_tracks_min_max_last_and_count() {
        let mut store = MetricStore::new(4);
        let t0 = Instant::now();
        store.ingest(packet("m", ValueKind::F64(10.0), 1), t0);
        store.ingest(packet("m", ValueKind::F64(5.0), 2), t0);
        store.ingest(packet("m", ValueKind::F64(20.0), 3), t0);
        let out = store.snapshot(t0, 1.0);
        let summary = out.iter().find(|s| s.name == "m").unwrap();
        assert_eq!(summary.last, Some(20.0));
        assert_eq!(summary.min, Some(5.0));
        assert_eq!(summary.max, Some(20.0));
        assert_eq!(summary.sample_count, 3);
    }

    #[test]
    fn window_capacity_trims_old_samples() {
        let mut store = MetricStore::new(2);
        let t0 = Instant::now();
        for (i, v) in [1.0_f64, 2.0, 3.0].iter().enumerate() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            store.ingest(packet("m", ValueKind::F64(*v), i as u64), t0);
        }
        let state = store.map.get("m").unwrap();
        assert_eq!(state.window.len(), 2);
        assert_eq!(state.window.front().copied(), Some(2.0));
        assert_eq!(state.window.back().copied(), Some(3.0));
    }
}
