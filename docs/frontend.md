# Frontend (Dioxus)

## Components

```rust
#[component]
fn MyPage() -> Element {
    let store = use_context::<AppStore>();  // global store provided by NavLayout
    let devices = store.devices;           // Signal<Vec<Device>>

    rsx! {
        div { class: "...", "content" }
    }
}
```

## State patterns

| Pattern | When to use |
|---|---|
| `use_context::<AppStore>()` | Access global app state (devices, templates, users, auth) |
| `use_signal(|| value)` | Simple local state |
| `use_store(|| value)` | Local state for types that derive `Store` |
| `use_resource(|| async { ... })` | Async data fetch; re-runs when deps change |
| `spawn(async { ... })` | Fire-and-forget inside `use_effect` |

Do not call server functions directly in component bodies — use `use_resource` or `spawn`.

SSE / `web-sys` code must be gated: `#[cfg(feature = "web")]`.

### Never write a signal a `use_effect` also reads

`use_effect` subscribes to every signal *read* in its body. Writing one of
those signals from the same effect retriggers it, and the effect writes
again — an unbounded loop that pegs the CPU, floods the server with whatever
the effect fetches, and crashes the browser tab.

```rust
// BAD — subscribes to `generation`, then writes it: infinite loop.
use_effect(move || {
    let next = generation() + 1;
    generation.set(next);
});

// GOOD — `peek()` reads without subscribing.
use_effect(move || {
    let next = *generation.peek() + 1;
    generation.set(next);
});
```

Use `peek()` for any value the effect needs but shouldn't re-run on. Two
effects in this codebase (`template_editor/mod.rs`, `devices.rs`) do read and
write the same signal, but guard the write behind an `is_none()` check so the
second pass is a no-op — that convergence is load-bearing, not incidental.
A regression test for this class of bug has to count requests over time
(`switching_preview_device_does_not_loop_requests` in the browser tier uses
`performance.getEntriesByType('resource')`); each individual request looks
correct, so only the unbounded repetition is observable. Two traps when
writing one:

- **Count every request, not the one the feature is "about".** If the looping
  code fires several requests in sequence and bails early on supersede, the
  flood lands on the *first* call in the chain. A counter watching the last
  one reports single digits while the tab is dying.
- **Reproduce the whole page.** Anything behind a gate (here, the
  `has_claude_api_key` check that hides the AI page's chat column and preview
  panel) must be unlocked in the fixture, or the test exercises a page the
  user never sees. Assert the gate is open rather than assuming it.

Validate such a test by reintroducing the bug and watching it fail — and
check *how* it fails. The first version of this test did fail, but by
crashing the browser tab rather than by its own assertion; a less violent
loop would have slipped past it. Failing for the intended reason is the bar,
not failing at all.

Finally, when a frontend fix appears not to work, suspect the bundle before
the code: `dx build --platform web`, restart, then hard-reload the browser.
WASM caches aggressively, and a stale bundle looks exactly like a fix that
didn't take.

## Global store

`AppStore` is provided by `NavLayout` in `src/frontend/mod.rs`. It holds `Signal<Vec<Device>>`, `Signal<Vec<Template>>`, `Signal<Option<AuthenticatedUser>>`, etc.

Access it anywhere inside the `NavLayout` tree with `use_context::<AppStore>()`.

## Adding a new page

1. Create `src/frontend/pages/mypage.rs` with a public component:
   ```rust
   #[component]
   pub fn MyPage() -> Element { ... }
   ```

2. Register in `src/frontend/pages/mod.rs`:
   ```rust
   mod mypage;
   pub use mypage::MyPage;
   ```

3. Register route in `src/frontend/mod.rs` — add to the `use pages::` import and to the `Route` enum:
   ```rust
   #[route("/mypage")]
   MyPage {},
   ```
   If the page needs the nav bar and auth guard, nest it under `#[layout(NavLayout)]`.

## Routing

Routes are defined as a `#[derive(Routable)]` enum in `src/frontend/mod.rs`. Public routes (login, setup) sit outside `NavLayout`. Protected routes are nested under `#[layout(NavLayout)]`, which enforces auth and provides `AppStore`.
