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
