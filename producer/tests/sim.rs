#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use flux_producer::sim::{build, Simulator};
use flux_producer::Simulation;
use flux_proto::ValueKind;

fn unwrap_f64(v: ValueKind) -> f64 {
    match v {
        ValueKind::F64(x) => x,
        other => panic!("expected F64, got {other:?}"),
    }
}

fn collect(sim: &mut dyn Simulator, origin: Instant, samples: usize, dt: Duration) -> Vec<f64> {
    (0..samples)
        .map(|i| {
            let t = origin + dt * u32::try_from(i).unwrap();
            unwrap_f64(sim.next(t))
        })
        .collect()
}

#[test]
fn random_range_is_deterministic_for_same_seed() {
    let origin = Instant::now();
    let spec = Simulation::RandomRange {
        min: -50.0,
        max: 50.0,
    };
    let mut a = build(&spec, 1337, origin);
    let mut b = build(&spec, 1337, origin);
    for _ in 0..512 {
        assert!((unwrap_f64(a.next(origin)) - unwrap_f64(b.next(origin))).abs() < f64::EPSILON);
    }
}

#[test]
fn random_range_differs_for_different_seeds() {
    let origin = Instant::now();
    let spec = Simulation::RandomRange { min: 0.0, max: 1.0 };
    let mut a = build(&spec, 1, origin);
    let mut b = build(&spec, 2, origin);
    let diffs = (0..64)
        .filter(|_| (unwrap_f64(a.next(origin)) - unwrap_f64(b.next(origin))).abs() > f64::EPSILON)
        .count();
    assert!(
        diffs > 0,
        "different seeds should produce different streams"
    );
}

#[test]
fn sine_respects_offset_plus_or_minus_amplitude() {
    let origin = Instant::now();
    let amplitude = 7.5_f64;
    let offset = 20.0_f64;
    let period_ms = 800_u64;
    let mut sim = build(
        &Simulation::Sine {
            amplitude,
            period_ms,
            offset,
        },
        0,
        origin,
    );
    let samples = collect(
        sim.as_mut(),
        origin,
        usize::try_from(period_ms + 1).unwrap(),
        Duration::from_millis(1),
    );
    for v in samples {
        assert!(
            v >= offset - amplitude - 1e-9 && v <= offset + amplitude + 1e-9,
            "sine value {v} outside [{}, {}]",
            offset - amplitude,
            offset + amplitude
        );
    }
}

#[test]
fn sine_value_at_quarter_period_is_offset_plus_amplitude() {
    let origin = Instant::now();
    let amplitude = 3.0_f64;
    let offset = 1.0_f64;
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
    let at = origin + Duration::from_millis(period_ms / 4);
    let v = unwrap_f64(sim.next(at));
    assert!((v - (offset + amplitude)).abs() < 1e-6, "got {v}");
}

#[test]
fn stepped_cycles_in_order() {
    let origin = Instant::now();
    let values = vec![10.0_f64, 20.0, 30.0, 40.0];
    let dwell_ms = 50_u64;
    let mut sim = build(
        &Simulation::Stepped {
            values: values.clone(),
            dwell_ms,
        },
        0,
        origin,
    );
    for i in 0..values.len() * 3 {
        let at = origin + Duration::from_millis((i as u64) * dwell_ms);
        let expected = values[i % values.len()];
        assert!((unwrap_f64(sim.next(at)) - expected).abs() < f64::EPSILON);
    }
}

#[test]
fn constant_always_returns_same_value() {
    let origin = Instant::now();
    let mut sim = build(&Simulation::Constant { value: -42.25 }, 0, origin);
    for i in 0..10 {
        let at = origin + Duration::from_millis(i * 37);
        assert!((unwrap_f64(sim.next(at)) + 42.25).abs() < f64::EPSILON);
    }
}
