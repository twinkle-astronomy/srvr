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

## Database tests

`db::get()` reads a process-wide `OnceLock` pool, so DB code can't be tested
against a fresh database per test. Use the shared in-memory harness:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::init_test_db;

    #[tokio::test]
    async fn test_thing_round_trip() {
        init_test_db().await;            // idempotent; first caller migrates an in-memory DB
        let t = create_template("t", "<svg/>").await.expect("create");
        // ... exercise db functions against `t.id`
    }
}
```

`init_test_db` (in `src/db.rs`) initializes the global pool with an in-memory
SQLite database and runs all migrations, once per test binary. Because the DB is
shared across tests in the binary, scope rows you create (e.g. by a uniquely
named parent template) so parallel tests don't collide.

## Frontend component tests (native tier)

Dioxus components are tested natively (no browser) via the harness in
`src/frontend/test_harness.rs`, gated `#[cfg(all(test, feature = "server"))]`.
It drives the *real* dioxus-core reactive runtime and serializes the rendered
tree to HTML with dioxus-ssr — SSR is only a **readout** for assertions, not the
runtime the app uses. Tests live inline next to the component, like all others.

Two helpers:

```rust
use crate::frontend::test_harness::{render_with_props, render_with_store};

// Pure presentational components: pass props directly.
let html = render_with_props(DeviceLogs, DeviceLogsProps {
    entries: vec![], error: None, loading: false,
});
assert!(html.contains("No logs received yet"));

// Store-driven pages: build + populate an AppStore the way NavLayout would.
// `make_store` runs inside the runtime (signals need a scope), so it must be a
// plain fn (no captures) — write one small fn per scenario.
fn store_loaded() -> AppStore {
    let mut s = AppStore::new();
    s.devices_loaded.set(true);
    s
}
let html = render_with_store(store_loaded, Devices);
```

Default to targeted `assert!(html.contains(...))` / absence checks rather than
full-string equality (robust to markup churn).

**Limits of the native tier:**
- **No router.** Components that call `use_navigator()` or render `Link` (e.g.
  `Templates`, loaded device/template card lists, `Nav`) can't render natively —
  `RouterContext` isn't constructible outside dioxus-router. These are covered by
  the browser tier instead.
- **No server functions.** Components that fetch via `use_resource` render their
  loading/`None` branch (the async task is spawned, not awaited). Test their
  loaded states by injecting data through the store, or split a presenter out that
  takes the data as props.
- **No DOM events.** Genuine click/input interaction lives in the browser tier.

## Running tests

```bash
cargo test --features server                 # run the real (server-gated) tests
cargo test --features server -- --nocapture  # show println! output
cargo test --features server test_name       # run a single test by name
```

Most tests live behind the `server` feature; plain `cargo test` compiles but
skips them. See the [Definition of done](development-process.md#definition-of-done)
for the full two-target verification.
