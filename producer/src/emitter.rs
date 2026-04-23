use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use flux_proto::{FluxPacket, MAX_PACKET_BYTES};
use tokio::net::UdpSocket;
use tokio::time::{interval_at, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::config::MetricSpec;
use crate::sim;

pub(crate) async fn run_emitter(
    spec: MetricSpec,
    socket: Arc<UdpSocket>,
    cancel: CancellationToken,
) -> Result<()> {
    let period = period_from_rate(spec.rate_hz).with_context(|| format!("metric {}", spec.name))?;
    let origin = Instant::now();
    let seed = seed_from_name(&spec.name);
    let mut simulator = sim::build(&spec.simulation, seed, origin);
    let mut buf = [0u8; MAX_PACKET_BYTES];
    let mut ticker = interval_at(tokio::time::Instant::from_std(origin + period), period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(()),
            tick = ticker.tick() => {
                let now = tick.into_std();
                let value = simulator.next(now);
                let packet = FluxPacket::new(unix_now_ms(), spec.name.clone(), value);
                let n = match packet.encode(&mut buf) {
                    Ok(n) => n,
                    Err(err) => return Err(anyhow!("encode {}: {err}", spec.name)),
                };
                if let Err(err) = socket.send(&buf[..n]).await {
                    tracing::warn!(metric = %spec.name, %err, "UDP send failed, continuing");
                }
            }
        }
    }
}

fn period_from_rate(rate_hz: f64) -> Result<Duration> {
    if !rate_hz.is_finite() || rate_hz <= 0.0 {
        return Err(anyhow!("rate_hz must be finite and > 0 (got {rate_hz})"));
    }
    let secs = 1.0 / rate_hz;
    Duration::try_from_secs_f64(secs)
        .map_err(|err| anyhow!("rate_hz={rate_hz} produces unrepresentable period: {err}"))
}

fn seed_from_name(name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

fn unix_now_ms() -> u64 {
    static CLOCK_SKEW_WARNED: OnceLock<()> = OnceLock::new();
    static CLOCK_OVERFLOW_WARNED: OnceLock<()> = OnceLock::new();

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => u64::try_from(d.as_millis()).unwrap_or_else(|_| {
            CLOCK_OVERFLOW_WARNED.get_or_init(|| {
                tracing::warn!("system clock exceeds u64 ms; clamping packet timestamp to MAX");
            });
            u64::MAX
        }),
        Err(err) => {
            CLOCK_SKEW_WARNED.get_or_init(|| {
                tracing::warn!(%err, "system clock is before UNIX_EPOCH; using 0 for packet timestamp");
            });
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_from_rate_ok() {
        let p = period_from_rate(2.0).unwrap();
        assert_eq!(p, Duration::from_millis(500));
    }

    #[test]
    fn period_from_rate_rejects_non_positive() {
        assert!(period_from_rate(0.0).is_err());
        assert!(period_from_rate(-1.0).is_err());
        assert!(period_from_rate(f64::NAN).is_err());
        assert!(period_from_rate(f64::INFINITY).is_err());
    }

    #[test]
    fn period_from_rate_rejects_unrepresentable_period() {
        // rate_hz -> 0 makes 1/rate_hz overflow Duration's representable range.
        assert!(period_from_rate(f64::MIN_POSITIVE).is_err());
    }

    #[test]
    fn seed_is_stable_per_name() {
        assert_eq!(seed_from_name("cpu.load"), seed_from_name("cpu.load"));
        assert_ne!(seed_from_name("cpu.load"), seed_from_name("memory.used"));
    }
}
