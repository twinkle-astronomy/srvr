# CSR + Explicit API Conversion

Converted the dashboard from a Dioxus Fullstack app (SSR + WASM hydration,
`#[server]` RPC) to a client-side-rendered WASM app talking to a plain JSON API.
This removed the forced-hydration build that blocked browser testing (see
[20260628-frontend-test-harness](20260628-frontend-test-harness.md)) and made
the client/server boundary an ordinary HTTP API you can `curl`.

## What shipped

- **JSON API at `/dashboard/*`** ([src/api/](../../../src/api/)): seven
  domain-split Axum routers (auth, devices, templates, users, prometheus, range,
  http_sources) replacing 33 `#[server]` functions. `ApiError(StatusCode, String)`
  with `From<sqlx::Error>` (RowNotFound → 404); auth is per-handler via
  `require_auth(&auth)?` instead of path-substring middleware
  (`server_fn_auth_middleware` is gone). Handler tests inline, using
  `api::test_support::auth_router` for 401 coverage. See [api.md](../../api.md).
- **Client fetch layer** ([src/frontend/api.rs](../../../src/frontend/api.rs)):
  gloo-net helpers, one fn per endpoint, re-exported through `server_fns.rs` so
  the store's imports didn't change.
- **CSR serving** in `main.rs`: `ServeDir` (from `DIOXUS_ASSET_DIR` in dev,
  `./dist` in prod) with an `index.html` fallback for the SPA router; merged with
  the device API, form-auth routes, and the new dashboard API.
- **Dev workflow** (`./dev.sh`): `dx serve` (web platform) can't run our custom
  Axum routes, so the script runs the Axum server (hot-reloaded by cargo-watch)
  alongside `dx serve`, with `Dioxus.toml` path-prefix proxies forwarding
  `/dashboard`, `/api`, `/auth`, `/render`, `/metrics` to it.
- **Browser E2E tier** ([tests/browser_e2e.rs](../../../tests/browser_e2e.rs)):
  fantoccini drives the real WASM app in headless Chromium against a spawned
  `srvr` + throwaway SQLite DB — login, add user, change password, change device
  template, edit template. The browser is the docker-compose `chrome` service
  (own Dockerfile stage; nothing browser-related in the dev image);
  `WEBDRIVER_URL` is required — the tests fail without it, so the tier can't
  silently stop running. `./e2e.sh` runs just this tier.

## Deviations from the plan

- **Phase 3**: instead of deleting the server-side function bodies,
  `server_fns.rs` keeps `#[cfg(feature = "server")]` direct-to-db impls so the
  native component-test tier (which compiles the store under the `server`
  feature) still renders pages. The web build re-exports the fetch versions
  (`pub use super::api::*`); a small `ServerFnError` shim replaces the dioxus
  type. Session-requiring paths in that module intentionally fail — real auth
  lives in `src/api/`.
- **Phase 5**: shipped a fantoccini WebDriver E2E tier instead of
  `wasm-bindgen-test` component mounts. Full journeys through the real app
  (router, fetches, DOM events, a real server) cover strictly more than isolated
  mounts, at the cost of needing the compose `chrome` service.
- **Phase 4**: `Dioxus.toml` moved to `default_platform = "web"` (not
  "fullstack" as planned) — with the fullstack feature gone, dx only needs to
  build the WASM side; the server is a plain cargo binary.

## Verification

- `cargo test --features server`: 42 native + 5 browser E2E, all green.
- `cargo check --no-default-features --features web --target wasm32-unknown-unknown`: clean.
- Browser sweep of all 9 routes (dashboard, devices, device detail, templates,
  template editor, users, login, setup, 404): pages load with **no application
  console errors**. The only console noise is dioxus-web's dev-only hot-reload
  client (`ws://…/_dioxus`) failing to find a dx dev server — gated on
  `debug_assertions`, absent from the release bundle the Dockerfile ships.
- `curl /dashboard/devices` with a session cookie returns bare JSON; 401 without.

## Retrospective

**What worked**

- **Spike-first paid off exactly as intended.** Phase 1 validated the riskiest
  assumption (a CSR mount doesn't panic) before any of the 33 endpoints were
  converted, so the bulk conversion proceeded with no surprises.
- **The plan file enabled seamless session recovery.** A session was lost
  mid-project; the phased plan plus working tree was enough to resume at
  Phase 5 without rework.
- **Keeping the store's import surface stable** (`server_fns` re-exporting the
  fetch layer) made the client conversion mechanical — store logic unchanged.

**Friction / surprises**

- `dx serve` has no "forward everything" proxy option, forcing per-prefix
  proxy entries in `Dioxus.toml` and a two-process `dev.sh`.
- dx's default `--debug-symbols` DWARF makes wasm-opt abort; fixed with
  `strip = "debuginfo"` in the release profile (see Cargo.toml comment).
- rustls 0.23 needs an explicit process-level CryptoProvider before any TLS
  client in the E2E harness (see `init_crypto` in browser_e2e.rs).
- Running tower-sessions migrations on the shared sqlx test pool corrupted
  `_sqlx_migrations` for other tests; solved with an in-memory session store in
  `api::test_support`.

**What to change**

- The `#[cfg(feature = "server")]` impls in `server_fns.rs` duplicate logic now
  living in `src/api/` handlers (e.g. delete-user guards). They exist only so
  the native test tier compiles; a future cleanup could shrink them to inert
  stubs and move the remaining real logic (`utils::obj_to_template_var`) out.
