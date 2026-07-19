//! Client fetch helpers for the WASM build (gloo-net → JSON API).
//!
//! All functions here map 1:1 to `/dashboard/*` Axum handlers from `src/api/`.
//! They're exported via `pub use super::api::*` in `server_fns.rs` so callers
//! continue to import from `crate::frontend::server_fns`.
//!
//! Many are not yet called by the current UI components; they exist as the full
//! API surface. The lints below silence false-positive dead-code warnings that
//! arise because Dioxus dispatches component calls through closures/spawn, which
//! the compiler cannot trace statically.
#![allow(dead_code, unused_imports)]

use crate::{
    frontend::server_fns::{ServerFnError, ServerInfo, TemplateVar},
    models::{
        AuthenticatedUser, Device, DeviceLog, FirmwareRelease, HttpSource, HttpSourceResult,
        PrometheusQuery, PrometheusQueryResult, RangeQuery, RangeQueryResult, RenderContext,
        Template,
    },
};

// --- Private fetch helpers ---
//
// Non-2xx responses carry the ApiError shape `{"error": "..."}` (see
// `ApiError::into_response` in src/api/mod.rs). Surface that message so the
// UI shows e.g. "Cannot delete the last user" rather than a bare status code.

#[derive(serde::Deserialize)]
struct ErrorBody {
    error: String,
}

/// Send a built request; on non-2xx, turn the response into a `ServerFnError`
/// carrying the server's error message (falling back to `HTTP <status>`).
async fn send_checked(
    req: gloo_net::http::Request,
) -> Result<gloo_net::http::Response, ServerFnError> {
    let resp = req.send().await.map_err(|e| ServerFnError::new(e.to_string()))?;
    if resp.ok() {
        return Ok(resp);
    }
    let status = resp.status();
    Err(match resp.json::<ErrorBody>().await {
        Ok(body) => ServerFnError::new(body.error),
        Err(_) => ServerFnError::new(format!("HTTP {status}")),
    })
}

fn build(req: gloo_net::http::RequestBuilder) -> Result<gloo_net::http::Request, ServerFnError> {
    req.build().map_err(|e| ServerFnError::new(e.to_string()))
}

fn with_json<B: serde::Serialize>(
    req: gloo_net::http::RequestBuilder,
    body: &B,
) -> Result<gloo_net::http::Request, ServerFnError> {
    req.json(body).map_err(|e| ServerFnError::new(e.to_string()))
}

async fn parse<T: for<'de> serde::Deserialize<'de>>(
    resp: gloo_net::http::Response,
) -> Result<T, ServerFnError> {
    resp.json().await.map_err(|e| ServerFnError::new(e.to_string()))
}

/// Every local `/dashboard/*` call funnels through one of the verb helpers
/// below; logging here (rather than at each of the ~40 call sites) gives full
/// visibility into every outbound fetch with one change per verb. Each helper
/// wraps its body in an inner `async` block so the `?`-early-return path
/// still gets logged instead of skipping straight past it.
fn log_outcome<T>(method: &str, path: &str, result: &Result<T, ServerFnError>) {
    match result {
        Ok(_) => tracing::debug!("{method} {path} -> ok"),
        Err(e) => tracing::warn!("{method} {path} -> error: {e}"),
    }
}

async fn get<T: for<'de> serde::Deserialize<'de>>(path: &str) -> Result<T, ServerFnError> {
    tracing::debug!("{path}: GET sending");
    let result: Result<T, ServerFnError> = async {
        parse(send_checked(build(gloo_net::http::Request::get(path))?).await?).await
    }
    .await;
    log_outcome("GET", path, &result);
    result
}

async fn post<B, T>(path: &str, body: &B) -> Result<T, ServerFnError>
where
    B: serde::Serialize,
    T: for<'de> serde::Deserialize<'de>,
{
    tracing::debug!("{path}: POST sending");
    let result: Result<T, ServerFnError> = async {
        parse(send_checked(with_json(gloo_net::http::Request::post(path), body)?).await?).await
    }
    .await;
    log_outcome("POST", path, &result);
    result
}

async fn post_void<B: serde::Serialize>(path: &str, body: &B) -> Result<(), ServerFnError> {
    tracing::debug!("{path}: POST sending");
    let result: Result<(), ServerFnError> = async {
        send_checked(with_json(gloo_net::http::Request::post(path), body)?)
            .await
            .map(|_| ())
    }
    .await;
    log_outcome("POST", path, &result);
    result
}

async fn post_empty<T: for<'de> serde::Deserialize<'de>>(path: &str) -> Result<T, ServerFnError> {
    tracing::debug!("{path}: POST sending");
    let result: Result<T, ServerFnError> = async {
        parse(send_checked(build(gloo_net::http::Request::post(path))?).await?).await
    }
    .await;
    log_outcome("POST", path, &result);
    result
}

async fn put_void<B: serde::Serialize>(path: &str, body: &B) -> Result<(), ServerFnError> {
    tracing::debug!("{path}: PUT sending");
    let result: Result<(), ServerFnError> = async {
        send_checked(with_json(gloo_net::http::Request::put(path), body)?)
            .await
            .map(|_| ())
    }
    .await;
    log_outcome("PUT", path, &result);
    result
}

async fn del(path: &str) -> Result<(), ServerFnError> {
    tracing::debug!("{path}: DELETE sending");
    let result: Result<(), ServerFnError> = async {
        send_checked(build(gloo_net::http::Request::delete(path))?)
            .await
            .map(|_| ())
    }
    .await;
    log_outcome("DELETE", path, &result);
    result
}

async fn post_empty_void(path: &str) -> Result<(), ServerFnError> {
    tracing::debug!("{path}: POST sending");
    let result: Result<(), ServerFnError> = async {
        send_checked(build(gloo_net::http::Request::post(path))?)
            .await
            .map(|_| ())
    }
    .await;
    log_outcome("POST", path, &result);
    result
}

// --- Auth ---

pub async fn login(username: String, password: String) -> Result<AuthenticatedUser, ServerFnError> {
    post(
        "/dashboard/auth/login",
        &serde_json::json!({"username": username, "password": password}),
    )
    .await
}

pub async fn logout() -> Result<(), ServerFnError> {
    post_empty_void("/dashboard/auth/logout").await
}

pub async fn setup(username: String, password: String) -> Result<AuthenticatedUser, ServerFnError> {
    post(
        "/dashboard/auth/setup",
        &serde_json::json!({"username": username, "password": password}),
    )
    .await
}

pub async fn create_user(username: String, password: String) -> Result<(), ServerFnError> {
    post_void(
        "/dashboard/auth/create-user",
        &serde_json::json!({"username": username, "password": password}),
    )
    .await
}

pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<(), ServerFnError> {
    post_void(
        "/dashboard/auth/change-password",
        &serde_json::json!({"current_password": current_password, "new_password": new_password}),
    )
    .await
}

pub async fn check_auth() -> Result<Option<AuthenticatedUser>, ServerFnError> {
    get("/dashboard/auth").await
}

pub async fn check_needs_setup() -> Result<bool, ServerFnError> {
    get("/dashboard/needs-setup").await
}

pub async fn get_server_info() -> Result<ServerInfo, ServerFnError> {
    get("/dashboard/server-info").await
}

// --- Users ---

pub async fn get_all_users() -> Result<Vec<AuthenticatedUser>, ServerFnError> {
    get("/dashboard/users").await
}

pub async fn delete_user(user_id: i64) -> Result<(), ServerFnError> {
    del(&format!("/dashboard/users/{user_id}")).await
}

// --- Devices ---

pub async fn get_devices() -> Result<Vec<Device>, ServerFnError> {
    get("/dashboard/devices").await
}

pub async fn get_device_by_id(id: i64) -> Result<Device, ServerFnError> {
    get(&format!("/dashboard/devices/{id}")).await
}

pub async fn get_device_logs(id: i64) -> Result<Vec<DeviceLog>, ServerFnError> {
    get(&format!("/dashboard/devices/{id}/logs")).await
}

pub async fn delete_device(id: i64) -> Result<(), ServerFnError> {
    del(&format!("/dashboard/devices/{id}")).await
}

pub async fn update_device_template(device_id: i64, template_id: i64) -> Result<(), ServerFnError> {
    post_void(
        &format!("/dashboard/devices/{device_id}/template"),
        &serde_json::json!({"template_id": template_id}),
    )
    .await
}

pub async fn update_device_maximum_compatibility(
    device_id: i64,
    maximum_compatibility: bool,
) -> Result<(), ServerFnError> {
    post_void(
        &format!("/dashboard/devices/{device_id}/compat"),
        &serde_json::json!({"enabled": maximum_compatibility}),
    )
    .await
}

pub async fn update_device_firmware_updates_enabled(
    device_id: i64,
    enabled: bool,
) -> Result<(), ServerFnError> {
    post_void(
        &format!("/dashboard/devices/{device_id}/firmware-updates"),
        &serde_json::json!({"enabled": enabled}),
    )
    .await
}

pub async fn get_render_context(id: i64) -> Result<RenderContext, ServerFnError> {
    get(&format!("/dashboard/devices/{id}/render-context")).await
}

pub async fn get_render_context_for_template(
    device_id: i64,
    template_id: i64,
) -> Result<RenderContext, ServerFnError> {
    get(&format!("/dashboard/devices/{device_id}/render-context/{template_id}")).await
}

pub async fn get_screen_preview(device_id: i64) -> Result<String, ServerFnError> {
    get(&format!("/dashboard/devices/{device_id}/preview")).await
}

pub async fn get_screen_preview_for_template(
    device_id: i64,
    template_id: i64,
) -> Result<String, ServerFnError> {
    get(&format!("/dashboard/devices/{device_id}/preview/{template_id}")).await
}

// --- Templates ---

pub async fn get_templates() -> Result<Vec<Template>, ServerFnError> {
    get("/dashboard/templates").await
}

pub async fn get_default_template() -> Result<Template, ServerFnError> {
    get("/dashboard/templates/default").await
}

pub async fn get_template_by_id(id: i64) -> Result<Template, ServerFnError> {
    get(&format!("/dashboard/templates/{id}")).await
}

pub async fn create_template(name: String, content: String) -> Result<Template, ServerFnError> {
    post(
        "/dashboard/templates",
        &serde_json::json!({"name": name, "content": content}),
    )
    .await
}

pub async fn save_template(id: i64, name: String, content: String) -> Result<(), ServerFnError> {
    put_void(
        &format!("/dashboard/templates/{id}"),
        &serde_json::json!({"name": name, "content": content}),
    )
    .await
}

pub async fn delete_template(id: i64) -> Result<(), ServerFnError> {
    del(&format!("/dashboard/templates/{id}")).await
}

pub async fn copy_template(id: i64) -> Result<Template, ServerFnError> {
    post_empty(&format!("/dashboard/templates/{id}/copy")).await
}

pub async fn get_virtual_render_context(template_id: i64) -> Result<RenderContext, ServerFnError> {
    get(&format!("/dashboard/templates/{template_id}/virtual-render-context")).await
}

pub async fn get_template_preview(render_context: RenderContext) -> Result<String, ServerFnError> {
    post("/dashboard/preview", &render_context).await
}

pub async fn get_template_preview_png(
    render_context: RenderContext,
) -> Result<String, ServerFnError> {
    post("/dashboard/preview/png", &render_context).await
}

pub async fn get_template_context(
    render_context: RenderContext,
) -> Result<Vec<TemplateVar>, ServerFnError> {
    post("/dashboard/context", &render_context).await
}

// --- Firmware ---

pub async fn get_firmware_releases() -> Result<Vec<FirmwareRelease>, ServerFnError> {
    get("/dashboard/firmware").await
}

/// The only multipart upload in the codebase — everything else round-trips
/// JSON. `Content-Type` is intentionally left unset: the browser derives the
/// `multipart/form-data; boundary=...` header itself from the `FormData`
/// body, and setting it manually would omit the boundary and break parsing.
pub async fn upload_firmware_release(
    model: String,
    version: String,
    filename: String,
    bytes: Vec<u8>,
) -> Result<FirmwareRelease, ServerFnError> {
    let form = web_sys::FormData::new().map_err(|e| ServerFnError::new(format!("{e:?}")))?;
    form.append_with_str("model", &model)
        .map_err(|e| ServerFnError::new(format!("{e:?}")))?;
    form.append_with_str("version", &version)
        .map_err(|e| ServerFnError::new(format!("{e:?}")))?;

    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let parts = js_sys::Array::new();
    parts.push(&array);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|e| ServerFnError::new(format!("{e:?}")))?;
    form.append_with_blob_and_filename("file", &blob, &filename)
        .map_err(|e| ServerFnError::new(format!("{e:?}")))?;

    let req = gloo_net::http::Request::post("/dashboard/firmware")
        .body(form)
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let result: Result<FirmwareRelease, ServerFnError> =
        async { parse(send_checked(req).await?).await }.await;
    log_outcome("POST", "/dashboard/firmware", &result);
    result
}

pub async fn activate_firmware_release(id: i64) -> Result<(), ServerFnError> {
    post_empty_void(&format!("/dashboard/firmware/{id}/activate")).await
}

pub async fn delete_firmware_release(id: i64) -> Result<(), ServerFnError> {
    del(&format!("/dashboard/firmware/{id}")).await
}

// --- Prometheus ---

pub async fn get_prometheus_queries_for_template(
    template_id: i64,
) -> Result<Vec<PrometheusQuery>, ServerFnError> {
    get(&format!("/dashboard/templates/{template_id}/prometheus")).await
}

pub async fn save_prometheus_query(pq: PrometheusQuery) -> Result<PrometheusQuery, ServerFnError> {
    post("/dashboard/prometheus", &pq).await
}

pub async fn delete_prometheus_query(id: i64) -> Result<(), ServerFnError> {
    del(&format!("/dashboard/prometheus/{id}")).await
}

pub async fn execute_prometheus_query(
    query: PrometheusQuery,
) -> Result<PrometheusQueryResult, ServerFnError> {
    post("/dashboard/prometheus/execute", &query).await
}

pub async fn execute_prometheus_queries(
    queries: Vec<PrometheusQuery>,
) -> Result<Vec<PrometheusQueryResult>, ServerFnError> {
    post("/dashboard/prometheus/execute-all", &queries).await
}

// --- Range queries ---

pub async fn get_range_queries_for_template(
    template_id: i64,
) -> Result<Vec<RangeQuery>, ServerFnError> {
    get(&format!("/dashboard/templates/{template_id}/range")).await
}

pub async fn save_range_query(rq: RangeQuery) -> Result<RangeQuery, ServerFnError> {
    post("/dashboard/range", &rq).await
}

pub async fn delete_range_query(id: i64) -> Result<(), ServerFnError> {
    del(&format!("/dashboard/range/{id}")).await
}

pub async fn execute_range_query(query: RangeQuery) -> Result<RangeQueryResult, ServerFnError> {
    post("/dashboard/range/execute", &query).await
}

// --- HTTP sources ---

pub async fn save_http_source(source: HttpSource) -> Result<HttpSource, ServerFnError> {
    post("/dashboard/http-sources", &source).await
}

pub async fn delete_http_source(id: i64) -> Result<(), ServerFnError> {
    del(&format!("/dashboard/http-sources/{id}")).await
}

pub async fn execute_http_source(source: HttpSource) -> Result<HttpSourceResult, ServerFnError> {
    post("/dashboard/http-sources/execute", &source).await
}

// --- Claude AI (template generation) ---

/// Whether the current user has a Claude API key configured. The key itself
/// never reaches the browser — Claude calls go through the server-side
/// `/dashboard/claude/messages` proxy, which attaches it.
pub async fn has_claude_api_key() -> Result<bool, ServerFnError> {
    get("/dashboard/claude-api-key").await
}

pub async fn save_claude_api_key(key: String) -> Result<(), ServerFnError> {
    put_void("/dashboard/claude-api-key", &serde_json::json!({"key": key})).await
}

pub async fn delete_claude_api_key() -> Result<(), ServerFnError> {
    del("/dashboard/claude-api-key").await
}

pub async fn execute_ad_hoc_http_fetch(url: String) -> Result<String, ServerFnError> {
    post("/dashboard/ad-hoc-fetch", &serde_json::json!({"url": url})).await
}
