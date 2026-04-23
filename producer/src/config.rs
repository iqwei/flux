use std::fs;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use flux_proto::MAX_NAME_LEN;
use serde::{Deserialize, Serialize};

pub const DEFAULT_TARGET: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9000));

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ProducerConfig {
    pub target: SocketAddr,
    pub metrics: Vec<MetricSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSpec {
    pub name: String,
    #[serde(default)]
    pub unit: Option<String>,
    pub rate_hz: f64,
    pub simulation: Simulation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum Simulation {
    Constant {
        value: f64,
    },
    RandomRange {
        min: f64,
        max: f64,
    },
    Sine {
        amplitude: f64,
        period_ms: u64,
        offset: f64,
    },
    Stepped {
        values: Vec<f64>,
        dwell_ms: u64,
    },
}

impl Default for ProducerConfig {
    fn default() -> Self {
        Self {
            target: DEFAULT_TARGET,
            metrics: default_metrics(),
        }
    }
}

impl ProducerConfig {
    pub fn load(path: Option<&Path>, target_override: Option<SocketAddr>) -> Result<Self> {
        let mut cfg = match path {
            Some(p) => Self::from_file(p)?,
            None => Self::default(),
        };
        if let Some(target) = target_override {
            cfg.target = target;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading producer config {}", path.display()))?;
        Self::from_toml_str(&text)
            .with_context(|| format!("parsing producer config {}", path.display()))
    }

    pub fn from_toml_str(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(Into::into)
    }

    fn validate(&self) -> Result<()> {
        if self.metrics.is_empty() {
            return Err(anyhow!("producer config must declare at least one metric"));
        }
        let mut seen = std::collections::HashSet::with_capacity(self.metrics.len());
        for metric in &self.metrics {
            metric.validate()?;
            if !seen.insert(metric.name.as_str()) {
                return Err(anyhow!("duplicate metric name: {}", metric.name));
            }
        }
        Ok(())
    }
}

impl MetricSpec {
    fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("metric name cannot be empty"));
        }
        if self.name.len() > MAX_NAME_LEN {
            return Err(anyhow!(
                "metric {} exceeds MAX_NAME_LEN={MAX_NAME_LEN}",
                self.name
            ));
        }
        if !self.rate_hz.is_finite() || self.rate_hz <= 0.0 {
            return Err(anyhow!(
                "metric {} rate_hz must be finite and > 0 (got {})",
                self.name,
                self.rate_hz
            ));
        }
        self.simulation.validate(&self.name)
    }
}

impl Simulation {
    fn validate(&self, metric: &str) -> Result<()> {
        match self {
            Self::Constant { value } => require_finite(*value, metric, "constant.value"),
            Self::RandomRange { min, max } => {
                require_finite(*min, metric, "random_range.min")?;
                require_finite(*max, metric, "random_range.max")?;
                if min > max {
                    return Err(anyhow!("metric {metric} random_range requires min <= max"));
                }
                Ok(())
            }
            Self::Sine {
                amplitude,
                period_ms,
                offset,
            } => {
                require_finite(*amplitude, metric, "sine.amplitude")?;
                require_finite(*offset, metric, "sine.offset")?;
                if *period_ms == 0 {
                    return Err(anyhow!("metric {metric} sine.period_ms must be > 0"));
                }
                Ok(())
            }
            Self::Stepped { values, dwell_ms } => {
                if values.is_empty() {
                    return Err(anyhow!("metric {metric} stepped.values cannot be empty"));
                }
                for (idx, v) in values.iter().enumerate() {
                    require_finite(*v, metric, &format!("stepped.values[{idx}]"))?;
                }
                if *dwell_ms == 0 {
                    return Err(anyhow!("metric {metric} stepped.dwell_ms must be > 0"));
                }
                Ok(())
            }
        }
    }
}

fn require_finite(value: f64, metric: &str, field: &str) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(anyhow!(
            "metric {metric} {field} must be finite (got {value})"
        ))
    }
}

fn default_metrics() -> Vec<MetricSpec> {
    vec![
        MetricSpec {
            name: "cpu.load".to_string(),
            unit: Some("percent".to_string()),
            rate_hz: 5.0,
            simulation: Simulation::Sine {
                amplitude: 25.0,
                period_ms: 4_000,
                offset: 50.0,
            },
        },
        MetricSpec {
            name: "memory.used_mb".to_string(),
            unit: Some("MB".to_string()),
            rate_hz: 2.0,
            simulation: Simulation::RandomRange {
                min: 2_048.0,
                max: 8_192.0,
            },
        },
        MetricSpec {
            name: "sensor.online".to_string(),
            unit: None,
            rate_hz: 1.0,
            simulation: Simulation::Stepped {
                values: vec![1.0, 1.0, 0.0, 1.0],
                dwell_ms: 1_500,
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        let cfg = ProducerConfig::default();
        cfg.validate().unwrap();
        assert_eq!(cfg.target, DEFAULT_TARGET);
        assert_eq!(cfg.metrics.len(), 3);
    }

    #[test]
    fn toml_roundtrips() {
        let cfg = ProducerConfig::default();
        let text = toml::to_string(&cfg).unwrap();
        let parsed = ProducerConfig::from_toml_str(&text).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn toml_rejects_unknown_fields() {
        let text = r#"
target = "127.0.0.1:9000"
mystery = 1

[[metrics]]
name = "m"
rate_hz = 1.0
simulation = { kind = "constant", value = 1.0 }
"#;
        assert!(ProducerConfig::from_toml_str(text).is_err());
    }

    #[test]
    fn duplicate_metric_names_rejected() {
        let cfg = ProducerConfig {
            target: DEFAULT_TARGET,
            metrics: vec![
                MetricSpec {
                    name: "a".into(),
                    unit: None,
                    rate_hz: 1.0,
                    simulation: Simulation::Constant { value: 1.0 },
                },
                MetricSpec {
                    name: "a".into(),
                    unit: None,
                    rate_hz: 1.0,
                    simulation: Simulation::Constant { value: 2.0 },
                },
            ],
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn target_override_applied() {
        let addr: SocketAddr = "10.0.0.5:7000".parse().unwrap();
        let cfg = ProducerConfig::load(None, Some(addr)).unwrap();
        assert_eq!(cfg.target, addr);
    }

    #[test]
    fn rate_hz_must_be_positive_finite() {
        let mut cfg = ProducerConfig::default();
        cfg.metrics[0].rate_hz = 0.0;
        assert!(cfg.validate().is_err());
        cfg.metrics[0].rate_hz = f64::NAN;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn sine_zero_period_rejected() {
        let cfg = ProducerConfig {
            target: DEFAULT_TARGET,
            metrics: vec![MetricSpec {
                name: "x".into(),
                unit: None,
                rate_hz: 1.0,
                simulation: Simulation::Sine {
                    amplitude: 1.0,
                    period_ms: 0,
                    offset: 0.0,
                },
            }],
        };
        assert!(cfg.validate().is_err());
    }
}
