# Testing

## Structure

Tests are inline with `#[cfg(test)]` blocks in the same file as the code being tested. No separate integration test harness exists yet.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_thing() {
        assert_eq!(1 + 1, 2);
    }

    #[tokio::test]
    async fn test_async_thing() {
        // ...
    }
}
```

## MockClock

`MockClock` in `src/time.rs` is compiled only under `cfg(test)`. Use it for any code that depends on the current time:

```rust
#[cfg(test)]
mod tests {
    use crate::time::MockClock;

    #[test]
    fn test_hmac_expiry() {
        let clock = MockClock::new(1_000_000);  // fixed Unix timestamp
        // pass clock to functions that accept &dyn Clock
    }
}
```

## Running tests

```bash
cargo test
cargo test -- --nocapture   # show println! output
cargo test test_name        # run a single test by name
```
