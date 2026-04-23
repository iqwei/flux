use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    U64,
    I64,
    F64,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSummary {
    pub name: String,
    pub unit: Option<String>,
    pub kind: MetricKind,
    pub last: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub avg: Option<f64>,
    pub rate_pps: f64,
    pub sample_count: u64,
    pub last_update_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PipelineHealth {
    pub packets_received: u64,
    pub packets_parse_err: u64,
    pub subscriber_count: u32,
    pub ingest_rate_pps: f64,
    pub uptime_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub uptime_ms: u64,
    pub metrics: Vec<MetricSummary>,
    pub health: PipelineHealth,
}
