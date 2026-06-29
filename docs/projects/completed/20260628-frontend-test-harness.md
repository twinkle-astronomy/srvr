# Frontend Test Harness (Phase 1: native tier)

Gave the Dioxus dashboard its first automated tests. Previously the only signal
that a frontend change was correct was "it compiled and looked right." There is
now a native (no-browser) harness that renders components through the **real
dioxus-core reactive runtime** and asserts on the output, run as part of the
normal `cargo test --features server`.

## What shipped

- **`src/frontend/test_harness.rs`** (gated `#[cfg(all(test, feature = "server"))]`):
  - `render_with_props(Component, Props)` — render a component to an HTML string.
  - `render_with_store(make_store, Page)` — render a zero-prop, store-driven page
    with a populated `AppStore` injected as context.
  - SSR (`dioxus::ssr::render`, available under the `server` feature) is used only
    as a **readout** to serialize the tree for assertions — not as a runtime.
- **13 characterization tests** pinning current behavior:
  - `DeviceLogs` (loading / error / empty / populated), `DetailRow`
    ([devices.rs](../../../src/frontend/pages/devices.rs)).
  - `Devices` page loading vs empty ([devices.rs](../../../src/frontend/pages/devices.rs)).
  - `Users` loading / loaded list / "You" badge / delete-button logic
    ([users.rs](../../../src/frontend/pages/users.rs)).
  - `Dashboard` loading vs loaded server-info ([dashboard.rs](../../../src/frontend/pages/dashboard.rs)).
- Documented in [testing.md](../../testing.md).

## Key design notes

- **Signals need a scope, not just a runtime.** `AppStore::new()` calls
  `Signal::new`, which panics outside a Dioxus scope. Building the store in
  `VirtualDom::in_runtime` is *not* enough. The harness therefore uses a tiny
  host component (`StoreHost`) that builds + provides the store via
  `use_context_provider` and renders the page as a child — mirroring how
  `NavLayout` does it in the real app.
- **`make_store` is a plain `fn` (no captures)** because it's passed as a prop to
  `StoreHost`; write one small builder fn per scenario. The fn pointers are
  wrapped in newtypes with an `fn_addr_eq` `PartialEq` to avoid the
  `unpredictable_function_pointer_comparisons` lint the `#[component]` macro's
  derived `PartialEq` would otherwise trip.
- **Assertions** favor targeted `contains` / absence checks over full-string
  equality, so they survive markup churn.

## Native-tier limits (by design)

- **No router.** `RouterContext::new` is `pub(crate)` and needs route-mapping
  context from the real `Router`, so it can't be constructed standalone.
  Components calling `use_navigator()` or rendering `Link` (`Templates`, loaded
  card lists, `Nav`) can't render natively.
- **No server functions.** `use_resource`-fetching components render their
  loading/`None` branch; test loaded states by injecting via the store.
- **No DOM events.** No click/input simulation.

## Phase 2 (browser tier): investigated, blocked, deferred

The plan was a `wasm-bindgen-test` tier for "does a top-level page work" in a real
browser. The **toolchain was made to work end-to-end** — the wasm test compiled
and executed in headless Chromium — but the tier is **blocked by hydration** and
was not shipped. The scaffolding is preserved on the `csr-rpc-conversion` branch
(WIP commit), not in `main`.

### The blocker

`dioxus_web::run()` decides `should_hydrate = web_config.hydrate || cfg!(feature = "hydrate")`.
The `hydrate` feature is force-enabled by `dioxus/fullstack` (which we need for
`#[server]`), so the client **always** hydrates: it reads
`window.initial_dioxus_hydration_data` (only produced by the server during SSR)
and calls `rehydrate()`, which walks the DOM expecting server-pre-rendered nodes.
Mounting a component into an empty `<div>` panics (`atob` on undefined) *before
any component code runs*. This is unconditional for any `launch`. Isolated
client-only mounts are therefore impossible in this fullstack/hydrating build
without removing `fullstack` — and `fullstack` is what makes `#[server]` compile.

### Provisioning recipe (reproducible, all proven to work)

If the browser tier is revisited, this is the full setup that got Chromium
running the wasm:
- Install `chromium` + `chromium-driver` (Debian package pulls all the runtime
  libs that a hand-downloaded `chrome-headless-shell` would be missing).
- `cargo binstall wasm-bindgen-cli@<lockfile wasm-bindgen version>` for the
  `wasm-bindgen-test-runner`; register it as the wasm32 `runner` in
  `.cargo/config.toml`.
- **IPv4 fix:** the container has no usable IPv6 loopback but `localhost` resolves
  to `::1` first; ChromeDriver/runner then can't bind. Add
  `precedence ::ffff:0:0/96 100` to `/etc/gai.conf` (Docker doesn't overwrite
  that file, unlike `/etc/hosts`).
- **`CHROMEDRIVER_ARGS=--silent`:** the runner treats *any* driver stderr as
  startup failure (`has_failed()` keys on `any_stderr`), and ChromeDriver logs a
  harmless IPv6 `bind() failed` SEVERE; `--silent` suppresses it.
- **`webdriver.json`** with `--no-sandbox --disable-dev-shm-usage` for Chromium in
  a container.
- Scope host-only dev-deps (tokio/mio) to `cfg(not(target_arch = "wasm32"))` so the
  wasm test build doesn't try to compile them.

### Path forward

A faithful browser test for this app is either **E2E against the running server**
(which provides hydration data) or requires converting the frontend to **CSR +
an RPC/REST API** so the build no longer forces hydration. The latter was
analyzed and captured as a separate idea:
[csr-rpc-conversion](../ideas/csr-rpc-conversion.md). Its takeaway: a single
`macro_rules!`-style wrapper generating both client and server glue makes the
~37 `#[server]` conversions mechanical; the real work/risk is the SSR→CSR serving
switch, not the function count.
