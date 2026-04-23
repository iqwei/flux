use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use flux_proto::{FluxPacket, MetricKind, MetricSummary, ValueKind};

#[derive(Debug, Clone, Copy)]
struct Sample {
    value: f64,
    at: Instant,
}

#[derive(Debug)]
pub struct MetricState {
    kind: MetricKind,
    last: f64,
    sample_count: u64,
    last_update_unix_ms: u64,
    window: VecDeque<Sample>,
}

impl MetricState {
    fn new(kind: MetricKind, value: f64, at: Instant, unix_ms: u64) -> Self {
        let mut window = VecDeque::new();
        window.push_back(Sample { value, at });
        Self {
            kind,
            last: value,
            sample_count: 1,
            last_update_unix_ms: unix_ms,
            window,
        }
    }

    fn observe(&mut self, value: f64, at: Instant, unix_ms: u64) {
        self.last = value;
        self.sample_count = self.sample_count.saturating_add(1);
        self.last_update_unix_ms = unix_ms;
        self.window.push_back(Sample { value, at });
    }

    fn evict_older_than(&mut self, cutoff: Instant) {
        while let Some(front) = self.window.front() {
            if front.at < cutoff {
                self.window.pop_front();
            } else {
                break;
            }
        }
    }

    fn window_stats(&self) -> Option<(f64, f64, f64)> {
        if self.window.is_empty() {
            return None;
        }
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut sum = 0.0_f64;
        for s in &self.window {
            if s.value < min {
                min = s.value;
            }
            if s.value > max {
                max = s.value;
            }
            sum += s.value;
        }
        #[allow(clippy::cast_precision_loss)]
        let avg = sum / self.window.len() as f64;
        Some((min, max, avg))
    }

    fn rate_pps(&self, now: Instant, window: Duration, started_at: Instant) -> f64 {
        let uptime = now.saturating_duration_since(started_at);
        let divisor = uptime.min(window).as_secs_f64();
        if divisor <= 0.0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let samples = self.window.len() as f64;
        samples / divisor
    }

    fn summarize(
        &self,
        name: &str,
        now: Instant,
        window: Duration,
        started_at: Instant,
    ) -> MetricSummary {
        let stats = self.window_stats();
        MetricSummary {
            name: name.to_owned(),
            unit: None,
            kind: self.kind,
            last: Some(self.last),
            min: stats.map(|(mn, _, _)| mn),
            max: stats.map(|(_, mx, _)| mx),
            avg: stats.map(|(_, _, av)| av),
            rate_pps: self.rate_pps(now, window, started_at),
            sample_count: self.sample_count,
            last_update_ms: Some(self.last_update_unix_ms),
        }
    }
}

#[derive(Debug)]
pub struct MetricStore {
    window: Duration,
    started_at: Instant,
    map: HashMap<String, MetricState>,
}

impl MetricStore {
    #[must_use]
    pub fn new(window: Duration, started_at: Instant) -> Self {
        Self {
            window,
            started_at,
            map: HashMap::new(),
        }
    }

    pub fn ingest(&mut self, packet: FluxPacket, at: Instant, unix_ms: u64) {
        let kind = MetricKind::from(&packet.value);
        let value = value_as_f64(&packet.value);
        self.update(packet.name, kind, value, at, unix_ms);
    }

    pub fn update(
        &mut self,
        name: String,
        kind: MetricKind,
        value: f64,
        at: Instant,
        unix_ms: u64,
    ) {
        let cutoff = self.cutoff(at);
        match self.map.get_mut(&name) {
            Some(state) => {
                state.evict_older_than(cutoff);
                state.observe(value, at, unix_ms);
            }
            None => {
                self.map
                    .insert(name, MetricState::new(kind, value, at, unix_ms));
            }
        }
    }

    #[must_use]
    pub fn summaries(&mut self, now: Instant) -> Vec<MetricSummary> {
        let cutoff = self.cutoff(now);
        for state in self.map.values_mut() {
            state.evict_older_than(cutoff);
        }
        let mut out: Vec<MetricSummary> = self
            .map
            .iter()
            .map(|(name, state)| state.summarize(name, now, self.window, self.started_at))
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn cutoff(&self, now: Instant) -> Instant {
        now.checked_sub(self.window).unwrap_or(self.started_at)
    }
}

fn value_as_f64(v: &ValueKind) -> f64 {
    match *v {
        #[allow(clippy::cast_precision_loss)]
        ValueKind::U64(n) => n as f64,
        #[allow(clippy::cast_precision_loss)]
        ValueKind::I64(n) => n as f64,
        ValueKind::F64(n) => n,
        ValueKind::Bool(b) => f64::from(u8::from(b)),
    }
}
