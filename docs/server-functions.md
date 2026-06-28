# Server Functions

## Location

All `#[server]` functions live in `src/frontend/server_fns.rs`.

## Auth

Auth is enforced by `server_fn_auth_middleware` in `src/auth.rs` for all POST requests. You do **not** need to check auth inside most server functions.

Only call `require_auth()` when you need the **identity** of the caller:

```rust
#[server]
pub async fn get_templates() -> Result<Vec<Template>, ServerFnError> {
    // No auth call needed — middleware enforces login for all POSTs
    crate::db::get_templates().await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn delete_user(target_id: i64) -> Result<(), ServerFnError> {
    let current = require_auth().await?;  // need caller identity
    if current.id == target_id {
        return Err(ServerFnError::new("Cannot delete yourself"));
    }
    crate::db::delete_user(target_id).await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
```

## Making a server function public

The middleware allowlist checks path substrings. Currently allowlisted: `"check_auth"`, `"check_needs_setup"`.

To add a new public endpoint, add a `path.contains("your_fn_name")` check in `server_fn_auth_middleware` in `src/auth.rs`.

## Error handling

Server functions must return `ServerFnError`. Convert any other error type with `.to_string()`:

```rust
some_result.map_err(|e| ServerFnError::new(e.to_string()))
```

Do not use `?` with custom error types inside `#[server]` — they won't auto-convert to `ServerFnError`.
