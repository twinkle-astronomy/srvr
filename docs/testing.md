# Testing

## Structure

Tests are inline with `#[cfg(test)]` blocks in the same file as the code being
tested. The one exception is the browser end-to-end tier, which lives in the
separate [tests/browser_e2e.rs](../tests/browser_e2e.rs) integration crate (see
below).

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

`init_test_db` (in `src/db.rs`) initializes the global pool with a **named,
shared-cache** in-memory SQLite database (`file:trmnl_shared_test?mode=memory&cache=shared`)
and runs all migrations, once per test binary. Shared-cache + a leaked keep-alive
connection is what lets many `#[tokio::test]`s — each on its own runtime — see the
same migrated schema; a plain `sqlite::memory:` is private per-connection, so a
second test on a different runtime would reconnect to an empty DB ("no such
table"). Because the DB is shared across the binary, scope rows you create (e.g.
with a process-unique suffix) so parallel tests don't collide, and don't assert on
global counts.

**Watch out for `create_device`'s default template.** `get_default_template()`
returns whichever template row has the lowest `id` in the *entire* shared-cache
DB, lazily inserting one the first time any test calls it — it is not scoped
per test. Every `create_device()` fixture points at that one shared row unless
you reassign it. Asserting on JSON shape is fine either way, but if your test
renders and decodes *real* pixel output (fetching `/render/screen.bmp` or
`/render/screen_2bit.png` and decoding the image), create your own template
and call `update_device_template(device.id, template.id)` before rendering —
otherwise a concurrent test that creates/edits/deletes templates can leave
that shared row in a state your test never expected (e.g. a
`UsvgError(ParsingFailed(NoRootNode))` from empty content), and the failure
will only show up intermittently under full parallel `cargo test` runs, not
when run alone.

## Browser end-to-end tests

[tests/browser_e2e.rs](../tests/browser_e2e.rs) drives the **real WASM dashboard
in headless Chromium** against a **real `srvr` process + throwaway SQLite DB**,
exercising full user journeys through the actual UI: logging in, adding a user,
changing a password, changing a device's template, and editing a template.

The browser runs as the docker-compose **`chrome`** service — headless Chromium
behind chromedriver (Dockerfile `chrome` stage). Nothing browser-related is
installed in the dev image. The `srvr` service sets
`WEBDRIVER_URL=http://chrome:4444`, so these run as part of the normal
`cargo test --features server` inside compose:

```bash
docker compose up -d chrome      # start the browser service
cargo test --features server     # browser E2E run alongside everything else
```

The harness (`fantoccini`) spawns the server via `CARGO_BIN_EXE_srvr` (bound to
`0.0.0.0`, advertising this container's network IP so the browser container can
reach it), builds the WASM bundle if missing, and gives each test its own server
+ fresh DB; `reqwest` seeds fixtures over HTTP the way the app's own endpoints do.
Tests are serialized (one Chromium session at a time).

`WEBDRIVER_URL` is **required**: each browser test fails immediately when it is
unset, so the tier can't silently stop running. `cargo test --features server`
therefore needs the `chrome` service up (it is, inside the compose environment).
[`e2e.sh`](../e2e.sh) runs just this crate; set `E2E_SERVER_LOG=info` to see the
spawned server's logs inline.

These are the payoff of the CSR + JSON-API conversion: with forced hydration
gone, the client-rendered app boots into an empty page, so a browser can drive it
directly (see the retrospective in
[20260628-frontend-test-harness](projects/completed/20260628-frontend-test-harness.md)).

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

**Limits of the native tier** (the [browser E2E tier](#browser-end-to-end-tests)
above covers these gaps):
- **No router.** Components that call `use_navigator()` or render `Link` (e.g.
  `Templates`, loaded device/template card lists, `Nav`) can't render natively —
  `RouterContext` isn't constructible outside dioxus-router.
- **No API fetches.** Components that fetch via `use_resource` render their
  loading/`None` branch (the async task is spawned, not awaited). Test their
  loaded states by injecting data through the store, or split a presenter out that
  takes the data as props.
- **No DOM events.** Genuine click/input interaction isn't covered by this tier.

## Running tests

```bash
cargo test --features server                 # run the real (server-gated) tests
cargo test --features server -- --nocapture  # show println! output
cargo test --features server test_name       # run a single test by name
```

Most tests live behind the `server` feature; plain `cargo test` compiles but
skips them. See the [Definition of done](development-process.md#definition-of-done)
for the full two-target verification.
