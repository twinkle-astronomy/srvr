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
