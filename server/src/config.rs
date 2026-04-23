use std::env;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_UDP_PORT: u16 = 9000;
pub const DEFAULT_WS_PORT: u16 = 9001;
pub const DEFAULT_BROADCAST_INTERVAL_MS: u64 = 500;
pub const DEFAULT_INGRESS_BUFFER: usize = 4096;
pub const DEFAULT_BROADCAST_BUFFER: usize = 16;
pub const DEFAULT_ROLLING_WINDOW: usize = 64;
pub const DEFAULT_STALE_AFTER_MS: u64 = 3_000;
pub const DEFAULT_DEAD_AFTER_MS: u64 = 10_000;

const ENV_PREFIX: &str = "FLUX_";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    pub udp_bind: SocketAddr,
    pub ws_bind: SocketAddr,
    pub broadcast_interval_ms: u64,
    pub ingress_buffer: usize,
    pub broadcast_buffer: usize,
    pub rolling_window: usize,
    pub stale_after_ms: u64,
    pub dead_after_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            udp_bind: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DEFAULT_UDP_PORT)),
            ws_bind: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DEFAULT_WS_PORT)),
            broadcast_interval_ms: DEFAULT_BROADCAST_INTERVAL_MS,
            ingress_buffer: DEFAULT_INGRESS_BUFFER,
            broadcast_buffer: DEFAULT_BROADCAST_BUFFER,
            rolling_window: DEFAULT_ROLLING_WINDOW,
            stale_after_ms: DEFAULT_STALE_AFTER_MS,
            dead_after_ms: DEFAULT_DEAD_AFTER_MS,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ConfigOverrides {
    pub udp_port: Option<u16>,
    pub ws_port: Option<u16>,
}

impl ServerConfig {
    pub fn load(path: Option<&Path>, overrides: &ConfigOverrides) -> Result<Self> {
        let mut cfg = match path {
            Some(p) => Self::from_file(p)?,
            None => Self::default(),
        };
        cfg.apply_env()?;
        cfg.apply_overrides(overrides);
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        Self::from_toml_str(&text)
            .with_context(|| format!("parsing config file {}", path.display()))
    }

    pub fn from_toml_str(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(Into::into)
    }

    fn apply_env(&mut self) -> Result<()> {
        if let Some(v) = parse_env::<SocketAddr>("UDP_BIND")? {
            self.udp_bind = v;
        }
        if let Some(v) = parse_env::<SocketAddr>("WS_BIND")? {
            self.ws_bind = v;
        }
        if let Some(v) = parse_env::<u64>("BROADCAST_INTERVAL_MS")? {
            self.broadcast_interval_ms = v;
        }
        if let Some(v) = parse_env::<usize>("INGRESS_BUFFER")? {
            self.ingress_buffer = v;
        }
        if let Some(v) = parse_env::<usize>("BROADCAST_BUFFER")? {
            self.broadcast_buffer = v;
        }
        if let Some(v) = parse_env::<usize>("ROLLING_WINDOW")? {
            self.rolling_window = v;
        }
        if let Some(v) = parse_env::<u64>("STALE_AFTER_MS")? {
            self.stale_after_ms = v;
        }
        if let Some(v) = parse_env::<u64>("DEAD_AFTER_MS")? {
            self.dead_after_ms = v;
        }
        Ok(())
    }

    fn apply_overrides(&mut self, overrides: &ConfigOverrides) {
        if let Some(port) = overrides.udp_port {
            self.udp_bind.set_port(port);
        }
        if let Some(port) = overrides.ws_port {
            self.ws_bind.set_port(port);
        }
    }

    fn validate(&self) -> Result<()> {
        if self.broadcast_interval_ms == 0 {
            return Err(anyhow!("broadcast_interval_ms must be > 0"));
        }
        if self.ingress_buffer == 0 {
            return Err(anyhow!("ingress_buffer must be > 0"));
        }
        if self.broadcast_buffer == 0 {
            return Err(anyhow!("broadcast_buffer must be > 0"));
        }
        if self.rolling_window == 0 {
            return Err(anyhow!("rolling_window must be > 0"));
        }
        if self.stale_after_ms >= self.dead_after_ms {
            return Err(anyhow!("stale_after_ms must be < dead_after_ms"));
        }
        Ok(())
    }
}

fn parse_env<T: FromStr>(suffix: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    let key = format!("{ENV_PREFIX}{suffix}");
    match env::var(&key) {
        Ok(raw) => raw
            .parse::<T>()
            .map(Some)
            .map_err(|e| anyhow!("invalid {key}: {e}")),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(anyhow!("{key} is not valid unicode")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_sensible() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.udp_bind.port(), DEFAULT_UDP_PORT);
        assert_eq!(cfg.ws_bind.port(), DEFAULT_WS_PORT);
        cfg.validate().unwrap();
    }

    #[test]
    fn toml_roundtrips() {
        let cfg = ServerConfig::default();
        let text = toml::to_string(&cfg).unwrap();
        let parsed = ServerConfig::from_toml_str(&text).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn toml_rejects_unknown_fields() {
        let text = "udp_bind = \"0.0.0.0:9000\"\nfoo = 1\n";
        assert!(ServerConfig::from_toml_str(text).is_err());
    }

    #[test]
    fn overrides_apply_ports() {
        let mut cfg = ServerConfig::default();
        cfg.apply_overrides(&ConfigOverrides {
            udp_port: Some(1111),
            ws_port: Some(2222),
        });
        assert_eq!(cfg.udp_bind.port(), 1111);
        assert_eq!(cfg.ws_bind.port(), 2222);
    }

    #[test]
    fn validate_rejects_stale_ge_dead() {
        let mut cfg = ServerConfig::default();
        cfg.stale_after_ms = cfg.dead_after_ms;
        assert!(cfg.validate().is_err());
    }
}
