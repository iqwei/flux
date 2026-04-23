use std::f64::consts::TAU;
use std::time::Instant;

use flux_proto::ValueKind;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::config::Simulation;

pub trait Simulator: Send {
    fn next(&mut self, now: Instant) -> ValueKind;
}

#[must_use]
pub fn build(spec: &Simulation, seed: u64, origin: Instant) -> Box<dyn Simulator> {
    match spec {
        Simulation::Constant { value } => Box::new(ConstantSim { value: *value }),
        Simulation::RandomRange { min, max } => Box::new(RandomRangeSim {
            rng: SmallRng::seed_from_u64(seed),
            min: *min,
            max: *max,
        }),
        Simulation::Sine {
            amplitude,
            period_ms,
            offset,
        } => Box::new(SineSim {
            amplitude: *amplitude,
            period_ms: *period_ms,
            offset: *offset,
            origin,
        }),
        Simulation::Stepped { values, dwell_ms } => Box::new(SteppedSim {
            values: values.clone(),
            dwell_ms: *dwell_ms,
            origin,
        }),
    }
}

struct ConstantSim {
    value: f64,
}

impl Simulator for ConstantSim {
    fn next(&mut self, _now: Instant) -> ValueKind {
        ValueKind::F64(self.value)
    }
}

struct RandomRangeSim {
    rng: SmallRng,
    min: f64,
    max: f64,
}

impl Simulator for RandomRangeSim {
    fn next(&mut self, _now: Instant) -> ValueKind {
        let value = if (self.max - self.min).abs() < f64::EPSILON {
            self.min
        } else {
            self.rng.gen_range(self.min..=self.max)
        };
        ValueKind::F64(value)
    }
}

struct SineSim {
    amplitude: f64,
    period_ms: u64,
    offset: f64,
    origin: Instant,
}

impl Simulator for SineSim {
    fn next(&mut self, now: Instant) -> ValueKind {
        let elapsed_ms = now.saturating_duration_since(self.origin).as_secs_f64() * 1_000.0;
        #[allow(clippy::cast_precision_loss)]
        let period = self.period_ms as f64;
        let phase = TAU * (elapsed_ms / period);
        ValueKind::F64(self.offset + self.amplitude * phase.sin())
    }
}

struct SteppedSim {
    values: Vec<f64>,
    dwell_ms: u64,
    origin: Instant,
}

impl Simulator for SteppedSim {
    fn next(&mut self, now: Instant) -> ValueKind {
        let len = self.values.len().max(1);
        let elapsed_ms = now.saturating_duration_since(self.origin).as_millis();
        let dwell = u128::from(self.dwell_ms.max(1));
        let step = elapsed_ms / dwell;
        let modulo = u128::try_from(len).unwrap_or(u128::MAX);
        let idx = usize::try_from(step % modulo).unwrap_or(0);
        ValueKind::F64(self.values.get(idx).copied().unwrap_or(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn f64_of(v: ValueKind) -> f64 {
        match v {
            ValueKind::F64(x) => x,
            other => panic!("expected F64, got {other:?}"),
        }
    }

    #[test]
    fn constant_repeats() {
        let origin = Instant::now();
        let mut sim = build(&Simulation::Constant { value: 3.5 }, 0, origin);
        for _ in 0..5 {
            assert!((f64_of(sim.next(origin)) - 3.5).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn random_range_seeded_is_deterministic() {
        let origin = Instant::now();
        let spec = Simulation::RandomRange {
            min: -10.0,
            max: 10.0,
        };
        let mut a = build(&spec, 42, origin);
        let mut b = build(&spec, 42, origin);
        for _ in 0..128 {
            assert!((f64_of(a.next(origin)) - f64_of(b.next(origin))).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn random_range_stays_within_bounds() {
        let origin = Instant::now();
        let spec = Simulation::RandomRange {
            min: -1.0,
            max: 1.0,
        };
        let mut sim = build(&spec, 7, origin);
        for _ in 0..1024 {
            let v = f64_of(sim.next(origin));
            assert!((-1.0..=1.0).contains(&v), "value {v} out of range");
        }
    }

    #[test]
    fn sine_bounded_by_amplitude_plus_offset() {
        let origin = Instant::now();
        let amplitude = 5.0;
        let offset = 10.0;
        let period_ms = 1_000_u64;
        let mut sim = build(
            &Simulation::Sine {
                amplitude,
                period_ms,
                offset,
            },
            0,
            origin,
        );
        for step_ms in 0..=period_ms {
            let v = f64_of(sim.next(origin + Duration::from_millis(step_ms)));
            assert!(
                v >= offset - amplitude - 1e-9 && v <= offset + amplitude + 1e-9,
                "sine {v} outside bounds at {step_ms}ms"
            );
        }
    }

    #[test]
    fn stepped_cycles_through_values() {
        let origin = Instant::now();
        let values = vec![1.0, 2.0, 3.0];
        let dwell_ms = 100_u64;
        let mut sim = build(
            &Simulation::Stepped {
                values: values.clone(),
                dwell_ms,
            },
            0,
            origin,
        );
        for i in 0..9_u64 {
            let at = origin + Duration::from_millis(i * dwell_ms);
            let idx = usize::try_from(i).unwrap() % values.len();
            let expected = values[idx];
            assert!((f64_of(sim.next(at)) - expected).abs() < f64::EPSILON);
        }
    }
}
