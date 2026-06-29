# Plan: Frontend Test Harness

**Branch:** `frontend-test-harness`

## What & why

The Dioxus dashboard currently has zero automated tests — the only signal that a
frontend change is correct is "it compiled and looked right when someone clicked
through it." This project gives us a way to assert that components actually
*behave* correctly, not just that they render the HTML we expected.

The user's explicit requirement: verify the app **functions**, not merely that a
tree serializes to expected markup.

## Testing strategy (decided)

A layered approach, matching how the app actually runs (client-side WASM SPA;
SSR is not the runtime):

1. **Native VirtualDom tests — targeted units (Phase 1, lands first).**
   Fast, in-crate `cargo test` tests that drive the *real* `dioxus-core`
   reactive runtime (signals, hooks, effects). `dioxus-ssr` is used only as a
   **readout** to serialize the current tree for assertions — it is not the
   runtime under test. Best for: render states (empty / loading / loaded),
   prop-driven conditional UI, and reactivity (mutate injected state → re-render
   → assert the UI changed).

2. **`wasm-bindgen-test` — module/page-level tests (Phase 2).**
   Rust tests compiled to WASM and run in a real **headless browser**, exercising
   the real `dioxus-web` DOM + event layer. Best for: "does this top-level page
   component actually work" — real clicks/inputs on a real DOM. This is where
   genuine interaction testing lives, because dispatching a real DOM event is
   trivial in a browser (`element.click()`), whereas native event dispatch
   requires fiddly `ElementId` targeting.

3. **Playwright — deferred.** Not built in this project. Reserved as a fallback
   only if defects slip through the two tiers above.

### Why this split

Native tests run the same reactive core as production but are blind to the
`dioxus-web` DOM/event binding and cannot call `#[server]` functions (they do
HTTP). The browser tier closes exactly those gaps at the page level. We get fast,
broad, cheap coverage from Phase 1 and faithful end-to-end-ish page coverage from
Phase 2.

## Verified facts (checked against crate sources, not memory)

- `dioxus-ssr` 0.7.x exposes `render(&VirtualDom) -> String`,
  `render_element(Element) -> String`, `pre_render(&VirtualDom)`.
- `dioxus-core` 0.7.9 `VirtualDom`: `new`, `new_with_props`, `with_root_context`,
  `provide_root_context`, `rebuild_in_place`, `render_immediate(&mut impl WriteMutations)`,
  `process_events`, `handle_event(name, event, ElementId, bubbling)`, `runtime`,
  `mark_dirty`. `NoOpMutations` is re-exported for `render_immediate`.
- `dioxus`'s `ssr` module is available whenever the **`server`** feature is on
  (`dioxus/server` → `ssr` → `dep:dioxus-ssr`). So Phase 1 needs **no new runtime
  dep** and fits the existing `cargo test --features server` convention.
- `dioxus-web` can mount into a named root element via `Config::rootname(...)` +
  `launch_virtual_dom` / `launch_cfg` — this is how Phase 2 mounts a component
  into a test `<div>`.
- **Environment gap for Phase 2:** `wasm-bindgen-test` is not currently a
  dependency, and there is **no headless browser or `wasm-bindgen-test-runner`
  installed** in this environment. Phase 2 includes a tooling bootstrap and is
  environment-gated (see Open questions).

## Files

### Phase 1 — native harness

- **`src/frontend/test_harness.rs`** (new, `#[cfg(test)]`): reusable helpers.
  - `render(component)` / `render_with_props(...)` → build VirtualDom,
    `rebuild_in_place`, return HTML string.
  - `render_with_store(store, component)` → injects a populated `AppStore` via
    `with_root_context` for components that call `use_context::<AppStore>()`.
  - `render_in_router(...)` → wraps the component in a `Router` so components that
    use `Link` / `use_navigator` (e.g. `DeviceCard`) don't panic.
  - A small `Harness` handle exposing `html()` and `force_render()` (re-render
    after mutating an *injected* signal, e.g. `store.devices.set(...)`) to support
    reactivity-to-state assertions. **No DOM event dispatch in Phase 1** (per
    decision 3 — genuine click/input lives in Phase 2).
- **`src/frontend/mod.rs`**: add `#[cfg(test)] mod test_harness;`.
- **Inline `#[cfg(test)]` test modules** added to the components/pages covered
  (broad coverage — see below).

### Phase 2 — browser harness

- **Provision a headless browser + runner** (per decision 1): install a headless
  Chrome/Chromium (or Firefox) and `wasm-bindgen-test-runner` so the tier runs
  locally/CI. Document the exact setup steps.
- **`Cargo.toml`**: add `wasm-bindgen-test` as a dev-dependency.
- **`.cargo/config.toml`**: register `wasm-bindgen-test-runner` as the runner for
  `wasm32-unknown-unknown`.
- **`docs/testing.md`**: document both tiers and the browser prerequisites.
- **`src/frontend/test_harness_web.rs`** (new, `#[cfg(all(test, target_arch = "wasm32"))]`):
  mount-into-`<div>` helper using `Config::rootname`, plus DOM query/assert
  helpers via `web-sys`.
- **Inline `#[wasm_bindgen_test]` tests** for one or two top-level pages.

## Making components testable without server functions (decision 2)

Two complementary techniques so the lack of `#[server]` fns in native tests has
minimal impact:

1. **Store injection (no refactor).** `Devices`, `Templates`, `Users`,
   `Dashboard` already read their data from `AppStore` (populated by `NavLayout`,
   not by the page). Injecting a populated store via `with_root_context` covers
   their full empty/loading/loaded behaviour with no server calls.
2. **Container/presenter split (incremental refactor).** Components that fetch
   directly via `use_resource` — `DeviceDetail` (`get_device_logs`,
   `get_screen_preview_for_template`) and the `template_editor/*` pieces
   (`get_template_context`, `get_render_context_for_template`, query/source
   execution) — get a thin **container** (does the `use_resource`) wrapping a pure
   **presenter** that takes the loaded data as props. The presenter is fully
   native-testable; the thin container is left to Phase 2. This is a real
   code-quality improvement (separating data-loading from rendering), applied
   **only where it unlocks meaningful coverage**, not as a big-bang rewrite of all
   of `template_editor`.

## Coverage targets (Phase 1, broad)

Prioritised by testability (pure → store-driven → router-dependent):

- **`DeviceLogs`** (props-driven): loading spinner, error banner, empty state,
  populated table. Ideal first target — pure, no context/router.
- **`DetailRow`**: renders label/value.
- **`DeviceDetail` presenter** (after the container/presenter split): device-info
  rows, screen-preview states, "device not found" branch — fed by props instead of
  `use_resource`.
- **`Devices`** (store-driven): loading vs empty vs loaded branches via injected
  `AppStore` (loaded branch needs the router wrapper for `DeviceCard`'s `Link`).
- **`Templates`**, **`Users`** pages: analogous empty/loaded/loading branches;
  `UserRow` "current user" badge logic.
- **`Dashboard`**: `InfoRow` / `LoadingSpinner` and server-info rendering.
- **`Nav` / `NavLink`**: active-link styling (router wrapper).

Each follows TDD per `development-process.md`: failing test first (failing
because the *behavior* is wrong/missing, shown to user), then implement/adjust,
then refactor. Since most target components already exist, several tests will be
**characterization tests** that pin current behavior — flagged as such.

## How a native test reads (illustrative)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::test_harness::*;

    #[test]
    fn empty_log_list_shows_empty_state() {
        let html = render_with_props(DeviceLogs, DeviceLogsProps {
            entries: vec![], error: None, loading: false,
        });
        assert!(html.contains("No logs received yet"));
        assert!(!html.contains("<table"));
    }
}
```

## Decisions (resolved in review)

1. **Phase 2 environment.** Provision a headless browser + `wasm-bindgen-test-runner`
   as part of this project so the tier actually runs — but **land Phase 1 fully
   first** (it runs today with no new tooling).
2. **Server functions.** Minimise impact via the two techniques above (store
   injection + incremental container/presenter split). No global "injectable
   fetcher" abstraction — keep refactors local and beneficial.
3. **Native event simulation.** Out of scope for Phase 1. Genuine click/input
   interaction is tested in the Phase 2 browser tier. Phase 1 covers render states
   and reactivity-to-injected-state only.
4. **Assertions.** Targeted `assert!(html.contains(...))` / absence checks as the
   default; `pretty_assertions` full-string equality only where exactness matters.

## Sequencing

1. Phase 1 harness (`test_harness.rs`) + first pure target (`DeviceLogs`) to prove
   the TDD loop end to end.
2. Broaden Phase 1 coverage across the store-driven pages and the `DeviceDetail`
   presenter (with its container/presenter split).
3. Phase 2: provision browser/runner, add the WASM harness, write page-level
   `#[wasm_bindgen_test]` tests with real interactions.

## Definition of done

- `cargo test --features server` passes (includes new native tests).
- `cargo check --no-default-features --features web --target wasm32-unknown-unknown`
  still compiles (test harness gated so it never leaks into either build target).
- **Phase 2 headless browser tests actually run and pass** — the browser +
  `wasm-bindgen-test-runner` are provisioned and the `#[wasm_bindgen_test]`
  page-level tests execute green (not merely written/compiled).
- Browser-tier prerequisites and the run command are documented in
  `docs/testing.md`.
