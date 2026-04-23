use std::collections::VecDeque;
use std::time::{Duration, Instant};

use flux_proto::PipelineHealth;

use crate::clock::duration_to_ms;

#[derive(Debug)]
pub struct HealthTracker {
    packets_received: u64,
    packets_parse_err: u64,
    subscriber_count: u32,
    window: Duration,
    started_at: Instant,
    events: VecDeque<Instant>,
}

impl HealthTracker {
    #[must_use]
    pub fn new(window: Duration, started_at: Instant) -> Self {
        Self {
            packets_received: 0,
            packets_parse_err: 0,
            subscriber_count: 0,
            window,
            started_at,
            events: VecDeque::new(),
        }
    }

    pub fn on_packet(&mut self, at: Instant) {
        self.packets_received = self.packets_received.saturating_add(1);
        self.events.push_back(at);
    }

    pub fn on_parse_error(&mut self) {
        self.packets_parse_err = self.packets_parse_err.saturating_add(1);
    }

    pub fn on_subscriber_join(&mut self) {
        self.subscriber_count = self.subscriber_count.saturating_add(1);
    }

    pub fn on_subscriber_leave(&mut self) {
        self.subscriber_count = self.subscriber_count.saturating_sub(1);
    }

    pub fn snapshot(&mut self, now: Instant) -> PipelineHealth {
        self.evict(now);
        let uptime = now.saturating_duration_since(self.started_at);
        let divisor = uptime.min(self.window).as_secs_f64();
        #[allow(clippy::cast_precision_loss)]
        let ingest_rate_pps = if divisor > 0.0 {
            self.events.len() as f64 / divisor
        } else {
            0.0
        };
        PipelineHealth {
            packets_received: self.packets_received,
            packets_parse_err: self.packets_parse_err,
            subscriber_count: self.subscriber_count,
            ingest_rate_pps,
            uptime_ms: duration_to_ms(uptime),
        }
    }

    fn evict(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window).unwrap_or(self.started_at);
        while let Some(front) = self.events.front() {
            if *front < cutoff {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }
}
