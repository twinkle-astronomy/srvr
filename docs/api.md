# Dashboard JSON API

## Location

The dashboard's data layer is a plain JSON HTTP API under the `/dashboard/`
prefix (chosen to avoid colliding with the device API at `/api/*`). Server-side
handlers live in [src/api/](../src/api/), one file per domain (devices,
templates, users, prometheus, range, http_sources, auth), each exposing a
`router()` merged in [src/api/mod.rs](../src/api/mod.rs).

The client side is [src/frontend/api.rs](../src/frontend/api.rs) (`web` feature):
gloo-net fetch helpers, one function per endpoint. They're re-exported through
`src/frontend/server_fns.rs` (`pub use super::api::*`) so the store keeps
importing from `crate::frontend::server_fns`.

## Handler pattern

Auth is **per-handler**, not middleware: take the `AuthSession` extractor and
call `require_auth(&auth)?` first. Only `check_auth`, `check_needs_setup`, and
the login/setup endpoints skip it.

```rust
async fn get_all_users(auth: AuthSession) -> Result<Json<Vec<AuthenticatedUser>>, ApiError> {
    require_auth(&auth)?;
    let users = crate::db::get_users().await?;
    Ok(Json(users.into_iter().map(...).collect()))
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/users", get(get_all_users))
        .route("/users/{id}", delete(delete_user))
}
```

Call `require_auth(&auth)?` for the 401 gate; its return value is the caller's
identity when you need it (e.g. "cannot delete yourself").

## Error handling

Handlers return `Result<Json<T>, ApiError>` (or `Result<StatusCode, ApiError>`
for no-content responses). `ApiError(StatusCode, String)` renders as
`{"error": "..."}` with that status.

- `sqlx::Error` converts via `?` — `RowNotFound` becomes 404, anything else 500.
- Constructors for common cases: `ApiError::unauthorized()`, `::internal(msg)`,
  `::bad_request(msg)`, `::conflict(msg)`; or construct directly for others
  (e.g. `ApiError(StatusCode::UNPROCESSABLE_ENTITY, msg)`).

Responses are **bare JSON** (`Vec<Device>`, not `{data: [...]}`) — the HTTP
status carries the error signal.

## Adding an endpoint

1. Add the handler + route in the matching `src/api/*.rs` file.
2. Add a fetch helper with the same name in `src/frontend/api.rs`
   (use the private `get`/`post`/`post_void` helpers).
3. If the native component-test tier needs it to exist, add a matching
   `#[cfg(feature = "server")]` fn in `src/frontend/server_fns.rs`
   (the store compiles against that module in both builds).
4. Test in the handler file: at minimum an auth-required check via
   `crate::api::test_support::auth_router`, plus the success path.

```rust
#[tokio::test]
async fn unauthenticated_returns_401() {
    let router = crate::api::test_support::auth_router(super::router()).await;
    let response = router
        .oneshot(Request::get("/users").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

## Screen previews

`GET /dashboard/devices/{id}/preview[/{template_id}]` and
`POST /dashboard/preview` all return a **base64 PNG** as a bare JSON string,
rendered in whichever mode the relevant device is configured for — 2-bit
grayscale when `supports_2bit_grayscale` is set, otherwise an 8-bit PNG of
the 1-bit render. They share `render_preview_png` in
[src/api/mod.rs](../src/api/mod.rs); the branch lives there so clients can
hardcode `data:image/png` rather than re-deriving it.

Previews are always PNG even in 1-bit mode (`render_screen_png` is
pixel-identical to the BMP). Devices themselves still receive a real 1-bit
BMP from `/render/screen.bmp` — only the dashboard moved.

## Binary/file uploads

Most endpoints are JSON in, JSON out. The one exception is firmware
upload (`POST /dashboard/firmware`, [src/api/firmware.rs](../src/api/firmware.rs)):
it takes `axum::extract::Multipart` instead of `Json<T>`, and needs
`DefaultBodyLimit::max(...)` layered onto that specific route — axum caps
request bodies read via `Bytes`-based extractors (which `Multipart` is
one of) at 2MB by default. On the client side,
[src/frontend/api.rs](../src/frontend/api.rs)'s `upload_firmware_release`
builds a `web_sys::FormData` + `Blob` and passes it directly as the
`gloo_net` request body — do **not** set a `Content-Type` header
yourself; the browser derives `multipart/form-data; boundary=...` from
the `FormData` body, and an explicit header would omit the boundary.

## The `ServerFnError` shim

`src/frontend/server_fns.rs` defines a small `ServerFnError(String)` (same API
surface as the old dioxus type: `::new(msg)`, `Display`). All client fetch
helpers and store code use it. Don't use `?` with other error types in frontend
code — convert with `ServerFnError::new(e.to_string())`.
