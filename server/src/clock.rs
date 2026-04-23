use std::fmt::Debug;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync + Debug {
    fn now(&self) -> Instant;
    fn unix_ms(&self) -> u64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, duration_to_ms)
    }
}

#[derive(Debug)]
pub struct FakeClock {
    base: Instant,
    state: Mutex<FakeState>,
}

#[derive(Debug, Clone, Copy)]
struct FakeState {
    offset: Duration,
    unix_ms: u64,
}

impl FakeClock {
    #[must_use]
    pub fn new(unix_ms: u64) -> Self {
        Self {
            base: Instant::now(),
            state: Mutex::new(FakeState {
                offset: Duration::ZERO,
                unix_ms,
            }),
        }
    }

    pub fn advance(&self, d: Duration) {
        let mut guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        guard.offset = guard.offset.saturating_add(d);
        guard.unix_ms = guard.unix_ms.saturating_add(duration_to_ms(d));
    }

    pub fn set_unix_ms(&self, unix_ms: u64) {
        let mut guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        guard.unix_ms = unix_ms;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        self.base + guard.offset
    }

    fn unix_ms(&self) -> u64 {
        let guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        guard.unix_ms
    }
}

#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn duration_to_ms(d: Duration) -> u64 {
    let ms = d.as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clock_advances_instant_and_unix_ms() {
        let clock = FakeClock::new(1_700_000_000_000);
        let t0 = clock.now();
        let u0 = clock.unix_ms();
        clock.advance(Duration::from_millis(250));
        assert_eq!(clock.unix_ms() - u0, 250);
        assert_eq!(clock.now().duration_since(t0), Duration::from_millis(250));
    }

    #[test]
    fn system_clock_monotonic() {
        let c = SystemClock;
        let a = c.now();
        let b = c.now();
        assert!(b >= a);
        assert!(c.unix_ms() > 0);
    }
}
