#![allow(clippy::unwrap_used)]

use flux_proto::{
    MetricKind, MetricSummary, PipelineHealth, Snapshot, ValueKind, SNAPSHOT_SCHEMA_VERSION,
};

fn sample_snapshot() -> Snapshot {
    Snapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at_ms: 1_700_000_000_000,
        uptime_ms: 12_345,
        metrics: vec![
            MetricSummary {
                name: "cpu.temp".into(),
                unit: Some("celsius".into()),
                kind: MetricKind::F64,
                last: Some(42.5),
                min: Some(30.0),
                max: Some(55.0),
                avg: Some(45.0),
                rate_pps: 10.0,
                sample_count: 256,
                last_update_ms: Some(1_700_000_000_000),
            },
            MetricSummary {
                name: "requests".into(),
                unit: None,
                kind: MetricKind::U64,
                last: Some(12.0),
                min: Some(0.0),
                max: Some(100.0),
                avg: Some(12.0),
                rate_pps: 1.0,
                sample_count: 1,
                last_update_ms: Some(1_700_000_000_500),
            },
        ],
        health: PipelineHealth {
            packets_received: 1_024,
            packets_parse_err: 2,
            subscriber_count: 3,
            ingest_rate_pps: 512.0,
            uptime_ms: 12_345,
        },
    }
}

#[test]
fn snapshot_roundtrips_through_json() {
    let original = sample_snapshot();
    let json = serde_json::to_string(&original).unwrap();
    let decoded: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn snapshot_json_carries_schema_version() {
    let snap = sample_snapshot();
    let value: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
    assert_eq!(
        value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(u64::from(SNAPSHOT_SCHEMA_VERSION)),
    );
}

#[test]
fn metric_kind_serialises_as_snake_case() {
    for (kind, expected) in [
        (MetricKind::U64, "\"u64\""),
        (MetricKind::I64, "\"i64\""),
        (MetricKind::F64, "\"f64\""),
        (MetricKind::Bool, "\"bool\""),
    ] {
        assert_eq!(serde_json::to_string(&kind).unwrap(), expected);
        let decoded: MetricKind = serde_json::from_str(expected).unwrap();
        assert_eq!(decoded, kind);
    }
}

#[test]
fn metric_kind_from_value_kind_matches_tag() {
    let cases = [
        (ValueKind::U64(0), MetricKind::U64),
        (ValueKind::I64(-1), MetricKind::I64),
        (ValueKind::F64(0.0), MetricKind::F64),
        (ValueKind::Bool(false), MetricKind::Bool),
    ];
    for (value, expected) in cases {
        assert_eq!(MetricKind::from(&value), expected);
        assert_eq!(MetricKind::from(value), expected);
    }
}

#[test]
fn pipeline_health_default_is_zeroed() {
    let health = PipelineHealth::default();
    assert_eq!(health.packets_received, 0);
    assert_eq!(health.packets_parse_err, 0);
    assert_eq!(health.subscriber_count, 0);
    assert_eq!(health.ingest_rate_pps.to_bits(), 0.0f64.to_bits());
    assert_eq!(health.uptime_ms, 0);
}
