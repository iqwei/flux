use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SERVER_URL: &str = "ws://127.0.0.1:9001/ws";
pub const DEFAULT_RECONNECT_INITIAL_MS: u64 = 500;
pub const DEFAULT_RECONNECT_MAX_MS: u64 = 10_000;
pub const DEFAULT_RECONNECT_JITTER: f64 = 0.2;
pub const DEFAULT_STALE_AFTER_MS: u64 = 3_000;
pub const DEFAULT_DEAD_AFTER_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DashboardConfig {
    pub server_url: String,
    pub reconnect: ReconnectPolicy,
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ReconnectPolicy {
    pub initial_ms: u64,
    pub max_ms: u64,
    pub jitter: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Thresholds {
    pub stale_after_ms: u64,
    pub dead_after_ms: u64,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER_URL.to_owned(),
            reconnect: ReconnectPolicy::default(),
            thresholds: Thresholds::default(),
        }
    }
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial_ms: DEFAULT_RECONNECT_INITIAL_MS,
            max_ms: DEFAULT_RECONNECT_MAX_MS,
            jitter: DEFAULT_RECONNECT_JITTER,
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            stale_after_ms: DEFAULT_STALE_AFTER_MS,
            dead_after_ms: DEFAULT_DEAD_AFTER_MS,
        }
    }
}

impl DashboardConfig {
    pub fn load(path: Option<&Path>, server_override: Option<&str>) -> Result<Self> {
        let mut cfg = match path {
            Some(p) => Self::from_file(p)?,
            None => Self::default(),
        };
        if let Some(url) = server_override {
            url.clone_into(&mut cfg.server_url);
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading dashboard config {}", path.display()))?;
        Self::from_toml_str(&text)
            .with_context(|| format!("parsing dashboard config {}", path.display()))
    }

    pub fn from_toml_str(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(Into::into)
    }

    fn validate(&self) -> Result<()> {
        if self.server_url.is_empty() {
            return Err(anyhow!("server_url cannot be empty"));
        }
        self.reconnect.validate()?;
        self.thresholds.validate()
    }
}

impl ReconnectPolicy {
    fn validate(&self) -> Result<()> {
        if self.initial_ms == 0 {
            return Err(anyhow!("reconnect.initial_ms must be > 0"));
        }
        if self.max_ms < self.initial_ms {
            return Err(anyhow!("reconnect.max_ms must be >= reconnect.initial_ms"));
        }
        if !self.jitter.is_finite() || !(0.0..=1.0).contains(&self.jitter) {
            return Err(anyhow!(
                "reconnect.jitter must be a finite value in [0.0, 1.0]"
            ));
        }
        Ok(())
    }
}

impl Thresholds {
    fn validate(&self) -> Result<()> {
        if self.stale_after_ms == 0 {
            return Err(anyhow!("thresholds.stale_after_ms must be > 0"));
        }
        if self.dead_after_ms <= self.stale_after_ms {
            return Err(anyhow!(
                "thresholds.dead_after_ms must be > thresholds.stale_after_ms"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        DashboardConfig::default().validate().unwrap();
    }

    #[test]
    fn toml_roundtrips() {
        let cfg = DashboardConfig::default();
        let text = toml::to_string(&cfg).unwrap();
        let parsed = DashboardConfig::from_toml_str(&text).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn toml_rejects_unknown_fields() {
        let text = "server_url = \"ws://x\"\nfoo = 1\n";
        assert!(DashboardConfig::from_toml_str(text).is_err());
    }

    #[test]
    fn server_override_replaces_url() {
        let cfg = DashboardConfig::load(None, Some("ws://example:9999/ws")).unwrap();
        assert_eq!(cfg.server_url, "ws://example:9999/ws");
    }

    #[test]
    fn validate_rejects_empty_url() {
        let mut cfg = DashboardConfig::default();
        cfg.server_url.clear();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_bad_jitter() {
        let mut cfg = DashboardConfig::default();
        cfg.reconnect.jitter = 1.5;
        assert!(cfg.validate().is_err());
        cfg.reconnect.jitter = f64::NAN;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_max_lt_initial() {
        let mut cfg = DashboardConfig::default();
        cfg.reconnect.initial_ms = 2_000;
        cfg.reconnect.max_ms = 1_000;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_dead_le_stale() {
        let mut cfg = DashboardConfig::default();
        cfg.thresholds.stale_after_ms = 5_000;
        cfg.thresholds.dead_after_ms = 5_000;
        assert!(cfg.validate().is_err());
    }
}
