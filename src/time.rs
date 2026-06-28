use std::time::{SystemTime, UNIX_EPOCH};

/// Abstraction for getting current time to allow deterministic testing.
pub trait Clock {
    fn now_secs(&self) -> i64;
}

/// Blanket implementation for references to types implementing Clock
impl<T: Clock> Clock for &T {
    fn now_secs(&self) -> i64 {
        (*self).now_secs()
    }
}

/// Parse a Prometheus-style duration string (`s`/`m`/`h`/`d` suffix) into seconds.
/// e.g. `"1h"` -> 3600, `"30m"` -> 1800, `"60s"` -> 60, `"2d"` -> 172800.
pub fn parse_duration_secs(s: &str) -> Result<i64, String> {
    let s = s.trim();
    let unit = s
        .chars()
        .last()
        .ok_or_else(|| "empty duration".to_string())?;
    let mult = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86400,
        _ => return Err(format!("unknown duration unit in {s:?}")),
    };
    let num = &s[..s.len() - unit.len_utf8()];
    let value: i64 = num
        .parse()
        .map_err(|_| format!("invalid duration number in {s:?}"))?;
    Ok(value * mult)
}

/// Compute a Prometheus range-query window from a fixed `now` plus duration/step
/// strings. Returns `(start_secs, end_secs, step_secs)` where the window is
/// `now - duration ..= now`. Pure and deterministic so it can be tested with an
/// explicit `now` instead of threading a clock through the async render path.
pub fn range_window(now_secs: i64, duration: &str, step: &str) -> Result<(i64, i64, f64), String> {
    let duration_secs = parse_duration_secs(duration)?;
    let step_secs = parse_duration_secs(step)?;
    Ok((now_secs - duration_secs, now_secs, step_secs as f64))
}

/// Real system clock implementation
#[derive(Clone)]
pub struct RealClock;

impl Clock for RealClock {
    fn now_secs(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before Unix Epoch")
            .as_secs() as i64
    }
}

/// Mock clock strictly for testing
#[cfg(test)]
#[derive(Clone)]
pub struct MockClock {
    pub time: i64,
}

#[cfg(test)]
impl Clock for MockClock {
    fn now_secs(&self) -> i64 {
        self.time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_secs_units() {
        assert_eq!(parse_duration_secs("1h"), Ok(3600));
        assert_eq!(parse_duration_secs("30m"), Ok(1800));
        assert_eq!(parse_duration_secs("60s"), Ok(60));
        assert_eq!(parse_duration_secs("2d"), Ok(172800));
        assert!(
            parse_duration_secs("banana").is_err(),
            "non-numeric junk should error"
        );
        assert!(
            parse_duration_secs("10x").is_err(),
            "unknown unit should error"
        );
        assert!(parse_duration_secs("").is_err(), "empty should error");
    }

    #[test]
    fn test_range_window_computes_start_end_step() {
        let now = 1_700_000_000;
        assert_eq!(range_window(now, "1h", "60s"), Ok((now - 3600, now, 60.0)));
        assert_eq!(range_window(now, "30m", "5m"), Ok((now - 1800, now, 300.0)));
        assert!(
            range_window(now, "banana", "60s").is_err(),
            "bad duration should propagate"
        );
        assert!(
            range_window(now, "1h", "5x").is_err(),
            "bad step should propagate"
        );
    }

    #[test]
    fn test_mock_clock() {
        let clock = MockClock { time: 12345 };
        assert_eq!(clock.now_secs(), 12345);

        let clock2 = MockClock { time: 0 };
        assert_eq!(clock2.now_secs(), 0);
    }

    #[test]
    fn test_real_clock_returns_valid_timestamp() {
        let real_clock = RealClock;
        let now_secs = real_clock.now_secs();

        // Should be a valid Unix timestamp (after mid-2023)
        assert!(now_secs > 1_700_000_000, "Timestamp should be valid");
    }

    #[test]
    fn test_clock_clone() {
        let real = RealClock;
        let _r2 = real.clone();
    }
}
