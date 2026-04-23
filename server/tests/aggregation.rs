#![allow(clippy::unwrap_used, clippy::float_cmp)]

use std::time::{Duration, Instant};

use flux_proto::{FluxPacket, MetricKind, PipelineHealth, ValueKind, PROTOCOL_VERSION};
use flux_server::health::HealthTracker;
use flux_server::metric_store::MetricStore;
use flux_server::{Clock, FakeClock};

const EPS: f64 = 1e-9;

fn packet(name: &str, value: ValueKind, ts_ms: u64) -> FluxPacket {
    FluxPacket {
        version: PROTOCOL_VERSION,
        timestamp_ms: ts_ms,
        name: name.to_owned(),
        value,
    }
}

#[test]
fn summarises_last_min_max_avg_from_window() {
    let start = Instant::now();
    let mut store = MetricStore::new(Duration::from_secs(10), start);
    let mut now = start;

    for v in [10.0_f64, 20.0, 5.0, 30.0, 15.0] {
        now += Duration::from_millis(100);
        store.update("temp".into(), MetricKind::F64, v, now, 42);
    }

    let summaries = store.summaries(now);
    let temp = summaries.iter().find(|s| s.name == "temp").unwrap();
    assert_eq!(temp.last, Some(15.0));
    assert_eq!(temp.min, Some(5.0));
    assert_eq!(temp.max, Some(30.0));
    assert!((temp.avg.unwrap() - 16.0).abs() < EPS);
    assert_eq!(temp.sample_count, 5);
    assert_eq!(temp.last_update_ms, Some(42));
}

#[test]
fn evicts_samples_at_window_boundary() {
    let start = Instant::now();
    let window = Duration::from_secs(1);
    let mut store = MetricStore::new(window, start);

    store.update("x".into(), MetricKind::F64, 1.0, start, 0);
    store.update(
        "x".into(),
        MetricKind::F64,
        2.0,
        start + Duration::from_millis(500),
        0,
    );
    store.update(
        "x".into(),
        MetricKind::F64,
        3.0,
        start + Duration::from_millis(900),
        0,
    );

    let now = start + Duration::from_millis(1500);
    let summaries = store.summaries(now);
    let x = summaries.iter().find(|s| s.name == "x").unwrap();

    assert_eq!(x.sample_count, 3);
    assert_eq!(x.last, Some(3.0));
    assert_eq!(x.min, Some(2.0));
    assert_eq!(x.max, Some(3.0));
    assert!((x.avg.unwrap() - 2.5).abs() < EPS);
}

#[test]
fn empty_window_reports_none_for_stats_but_keeps_last() {
    let start = Instant::now();
    let window = Duration::from_millis(100);
    let mut store = MetricStore::new(window, start);

    store.update("y".into(), MetricKind::F64, 7.0, start, 1);

    let now = start + Duration::from_secs(5);
    let summaries = store.summaries(now);
    let y = summaries.iter().find(|s| s.name == "y").unwrap();

    assert_eq!(y.last, Some(7.0));
    assert_eq!(y.sample_count, 1);
    assert_eq!(y.min, None);
    assert_eq!(y.max, None);
    assert_eq!(y.avg, None);
    assert_eq!(y.rate_pps, 0.0);
}

#[test]
fn rate_pps_divides_by_elapsed_uptime_when_shorter_than_window() {
    let start = Instant::now();
    let mut store = MetricStore::new(Duration::from_secs(10), start);

    for i in 0..5_u64 {
        let at = start + Duration::from_millis(100 * i);
        #[allow(clippy::cast_precision_loss)]
        store.update("r".into(), MetricKind::F64, i as f64, at, 0);
    }

    let now = start + Duration::from_millis(500);
    let summaries = store.summaries(now);
    let r = summaries.iter().find(|s| s.name == "r").unwrap();
    assert!((r.rate_pps - 10.0).abs() < EPS);
}

#[test]
fn rate_pps_divides_by_window_when_uptime_exceeds_it() {
    let start = Instant::now();
    let window = Duration::from_secs(1);
    let mut store = MetricStore::new(window, start);

    for (i, ms) in [2100_u64, 2200, 2300].iter().enumerate() {
        let at = start + Duration::from_millis(*ms);
        #[allow(clippy::cast_precision_loss)]
        store.update("r".into(), MetricKind::F64, i as f64, at, 0);
    }

    let now = start + Duration::from_secs(3);
    let summaries = store.summaries(now);
    let r = summaries.iter().find(|s| s.name == "r").unwrap();
    assert!((r.rate_pps - 3.0).abs() < EPS);
}

#[test]
fn ingest_consumes_flux_packet_and_derives_kind() {
    let start = Instant::now();
    let mut store = MetricStore::new(Duration::from_secs(1), start);

    store.ingest(
        packet("b", ValueKind::Bool(true), 100),
        start + Duration::from_millis(50),
        100,
    );
    store.ingest(
        packet("b", ValueKind::Bool(false), 200),
        start + Duration::from_millis(150),
        200,
    );

    let summaries = store.summaries(start + Duration::from_millis(200));
    let b = summaries.iter().find(|s| s.name == "b").unwrap();
    assert_eq!(b.kind, MetricKind::Bool);
    assert_eq!(b.last, Some(0.0));
    assert_eq!(b.sample_count, 2);
    assert_eq!(b.last_update_ms, Some(200));
}

#[test]
fn parse_error_counter_increments_independently() {
    let start = Instant::now();
    let mut h = HealthTracker::new(Duration::from_secs(1), start);
    h.on_parse_error();
    h.on_parse_error();
    h.on_parse_error();
    let p = h.build(start + Duration::from_millis(10));
    assert_eq!(p.packets_parse_err, 3);
    assert_eq!(p.packets_received, 0);
    assert_eq!(p.ingest_rate_pps, 0.0);
}

#[test]
fn subscriber_count_tracks_joins_and_leaves_saturating() {
    let start = Instant::now();
    let mut h = HealthTracker::new(Duration::from_secs(1), start);
    h.on_subscriber_leave();
    h.on_subscriber_join();
    h.on_subscriber_join();
    h.on_subscriber_leave();
    let p = h.build(start + Duration::from_millis(10));
    assert_eq!(p.subscriber_count, 1);
}

#[test]
fn global_ingest_rate_pps_uses_rolling_window() {
    let start = Instant::now();
    let window = Duration::from_secs(1);
    let mut h = HealthTracker::new(window, start);

    for i in 0..10_u64 {
        h.on_packet(start + Duration::from_millis(100 * i));
    }

    let p = h.build(start + Duration::from_secs(1));
    assert!((p.ingest_rate_pps - 10.0).abs() < EPS);
    assert_eq!(p.packets_received, 10);
    assert_eq!(p.uptime_ms, 1000);

    let idle = h.build(start + Duration::from_secs(3));
    assert!(idle.ingest_rate_pps.abs() < EPS);
    assert_eq!(idle.packets_received, 10);
}

#[test]
fn health_uptime_ms_advances_with_now() {
    let start = Instant::now();
    let h = HealthTracker::new(Duration::from_secs(1), start);
    assert_eq!(h.uptime_ms(start), 0);
    assert_eq!(h.uptime_ms(start + Duration::from_millis(750)), 750);
}

#[test]
fn build_produces_filled_pipeline_health() {
    let start = Instant::now();
    let mut h = HealthTracker::new(Duration::from_secs(1), start);
    h.on_packet(start + Duration::from_millis(10));
    h.on_subscriber_join();
    h.on_parse_error();

    let p: PipelineHealth = h.build(start + Duration::from_millis(500));
    assert_eq!(p.packets_received, 1);
    assert_eq!(p.packets_parse_err, 1);
    assert_eq!(p.subscriber_count, 1);
    assert_eq!(p.uptime_ms, 500);
    assert!(p.ingest_rate_pps > 0.0);
}

#[test]
fn fake_clock_advances_instant_and_unix_ms() {
    let clock = FakeClock::new(1_700_000_000_000);
    let t0 = clock.now();
    let u0 = clock.unix_ms();

    clock.advance(Duration::from_millis(250));
    assert_eq!(clock.unix_ms() - u0, 250);
    assert_eq!(clock.now().duration_since(t0), Duration::from_millis(250));

    clock.set_unix_ms(2_000_000_000_000);
    assert_eq!(clock.unix_ms(), 2_000_000_000_000);
}
