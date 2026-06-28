# Models

## Location

Shared data types live in `src/models/mod.rs`.

## Standard derive list

```rust
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MyModel {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}
```

If the model will be used with `use_store()` in Dioxus components, add `Store`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Store)]
pub struct MyModel { ... }
```

## Server-only impls

Server-specific implementations (rendering, HTTP fetching, etc.) belong in `src/models/server.rs`, not `mod.rs`. The `server.rs` module is already cfg-gated via its declaration in `mod.rs`.

```rust
// src/models/server.rs — already under #[cfg(feature = "server")]
impl Template {
    pub async fn render(&self, ctx: &RenderContext) -> Result<String, liquid::Error> { ... }
}
```

Do not add server-only code to `mod.rs` — it must compile for the WASM target too.

## Feature flags

- `#[cfg(feature = "server")]` — gates server-only code (db, axum, TLS, rendering)
- `#[cfg(feature = "web")]` — gates WASM/browser-only code (EventSource, wasm_bindgen)
- `#[cfg_attr(feature = "server", derive(sqlx::FromRow))]` — conditional derive for DB-mapped types

## Error types

Each module defines its own error enum using `thiserror`:

```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    SqlxError(#[from] sqlx::Error),
    #[error("{0}")]
    LiquidError(#[from] liquid::Error),
}
```

Use `From` impls for all upstream crate errors so `?` works cleanly within the module.
