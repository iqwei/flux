use flux_proto::MetricKind;

pub const MISSING: &str = "—";

#[must_use]
pub fn value(v: Option<f64>, kind: MetricKind) -> String {
    let Some(x) = v else {
        return MISSING.to_owned();
    };
    if !x.is_finite() {
        return MISSING.to_owned();
    }
    match kind {
        MetricKind::U64 | MetricKind::I64 => format!("{x:.0}"),
        MetricKind::F64 => format!("{x:.3}"),
        MetricKind::Bool => bool_value(x).to_owned(),
    }
}

#[must_use]
pub fn rate(pps: f64) -> String {
    if pps.is_finite() {
        format!("{pps:.1}/s")
    } else {
        MISSING.to_owned()
    }
}

#[must_use]
pub fn rate_bare(pps: f64) -> String {
    if pps.is_finite() {
        format!("{pps:.1}")
    } else {
        MISSING.to_owned()
    }
}

#[must_use]
pub fn age(age_ms: Option<u64>) -> String {
    let Some(ms) = age_ms else {
        return MISSING.to_owned();
    };
    if ms < 1_000 {
        return format!("{ms}ms");
    }
    let total_secs = ms / 1_000;
    if total_secs < 60 {
        let tenths = (ms % 1_000) / 100;
        return format!("{total_secs}.{tenths}s");
    }
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins < 60 {
        return format!("{mins}m{secs:02}s");
    }
    let hours = mins / 60;
    let m = mins % 60;
    format!("{hours}h{m:02}m")
}

#[must_use]
pub fn uptime(ms: u64) -> String {
    if ms < 1_000 {
        return format!("{ms}ms");
    }
    let total_secs = ms / 1_000;
    if total_secs < 60 {
        return format!("{total_secs}s");
    }
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins < 60 {
        return format!("{mins}m{secs:02}s");
    }
    let hours = mins / 60;
    let m = mins % 60;
    if hours < 24 {
        return format!("{hours}h{m:02}m");
    }
    let days = hours / 24;
    let h = hours % 24;
    format!("{days}d{h:02}h")
}

#[must_use]
pub fn unit(u: Option<&str>) -> &str {
    u.filter(|s| !s.is_empty()).unwrap_or("")
}

#[must_use]
pub fn countdown_secs(ms: u64) -> String {
    let secs = ms / 1_000;
    let tenths = (ms % 1_000) / 100;
    format!("{secs}.{tenths}s")
}

fn bool_value(x: f64) -> &'static str {
    if x == 0.0 {
        "false"
    } else {
        "true"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_missing_on_none_or_nan() {
        assert_eq!(value(None, MetricKind::F64), MISSING);
        assert_eq!(value(Some(f64::NAN), MetricKind::F64), MISSING);
        assert_eq!(value(Some(f64::INFINITY), MetricKind::F64), MISSING);
    }

    #[test]
    fn value_respects_kind() {
        assert_eq!(value(Some(12.6), MetricKind::U64), "13");
        assert_eq!(value(Some(-3.2), MetricKind::I64), "-3");
        assert_eq!(value(Some(1.234_567), MetricKind::F64), "1.235");
        assert_eq!(value(Some(0.0), MetricKind::Bool), "false");
        assert_eq!(value(Some(1.0), MetricKind::Bool), "true");
    }

    #[test]
    fn rate_formats_decimals() {
        assert_eq!(rate(0.0), "0.0/s");
        assert_eq!(rate(12.345), "12.3/s");
        assert_eq!(rate(f64::NAN), MISSING);
    }

    #[test]
    fn rate_bare_strips_suffix() {
        assert_eq!(rate_bare(0.0), "0.0");
        assert_eq!(rate_bare(12.345), "12.3");
        assert_eq!(rate_bare(f64::NAN), MISSING);
        assert_eq!(rate_bare(f64::INFINITY), MISSING);
    }

    #[test]
    fn countdown_secs_formats_tenths() {
        assert_eq!(countdown_secs(0), "0.0s");
        assert_eq!(countdown_secs(500), "0.5s");
        assert_eq!(countdown_secs(999), "0.9s");
        assert_eq!(countdown_secs(1_000), "1.0s");
        assert_eq!(countdown_secs(12_300), "12.3s");
    }

    #[test]
    fn age_scales() {
        assert_eq!(age(None), MISSING);
        assert_eq!(age(Some(0)), "0ms");
        assert_eq!(age(Some(250)), "250ms");
        assert_eq!(age(Some(1_500)), "1.5s");
        assert_eq!(age(Some(65_000)), "1m05s");
        assert_eq!(age(Some(3_725_000)), "1h02m");
    }

    #[test]
    fn uptime_scales() {
        assert_eq!(uptime(0), "0ms");
        assert_eq!(uptime(5_000), "5s");
        assert_eq!(uptime(65_000), "1m05s");
        assert_eq!(uptime(3_725_000), "1h02m");
        assert_eq!(uptime(90_000_000), "1d01h");
    }

    #[test]
    fn unit_blank_for_none_or_empty() {
        assert_eq!(unit(None), "");
        assert_eq!(unit(Some("")), "");
        assert_eq!(unit(Some("m/s")), "m/s");
    }
}
