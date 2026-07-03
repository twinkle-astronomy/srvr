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
        AuthenticatedUser, Device, DeviceLog, HttpSource, HttpSourceResult, PrometheusQuery,
        PrometheusQueryResult, RangeQuery, RangeQueryResult, RenderContext, Template,
    },
};

// --- Private fetch helpers ---

async fn get<T: for<'de> serde::Deserialize<'de>>(path: &str) -> Result<T, ServerFnError> {
    let resp = gloo_net::http::Request::get(path)
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if !resp.ok() {
        return Err(ServerFnError::new(format!("HTTP {}", resp.status())));
    }
    resp.json().await.map_err(|e| ServerFnError::new(e.to_string()))
}

async fn post<B, T>(path: &str, body: &B) -> Result<T, ServerFnError>
where
    B: serde::Serialize,
    T: for<'de> serde::Deserialize<'de>,
{
    let resp = gloo_net::http::Request::post(path)
        .json(body)
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if !resp.ok() {
        return Err(ServerFnError::new(format!("HTTP {}", resp.status())));
    }
    resp.json().await.map_err(|e| ServerFnError::new(e.to_string()))
}

async fn post_void<B: serde::Serialize>(path: &str, body: &B) -> Result<(), ServerFnError> {
    let resp = gloo_net::http::Request::post(path)
        .json(body)
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if !resp.ok() {
        return Err(ServerFnError::new(format!("HTTP {}", resp.status())));
    }
    Ok(())
}

async fn post_empty<T: for<'de> serde::Deserialize<'de>>(path: &str) -> Result<T, ServerFnError> {
    let resp = gloo_net::http::Request::post(path)
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if !resp.ok() {
        return Err(ServerFnError::new(format!("HTTP {}", resp.status())));
    }
    resp.json().await.map_err(|e| ServerFnError::new(e.to_string()))
}

async fn put_void<B: serde::Serialize>(path: &str, body: &B) -> Result<(), ServerFnError> {
    let resp = gloo_net::http::Request::put(path)
        .json(body)
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if !resp.ok() {
        return Err(ServerFnError::new(format!("HTTP {}", resp.status())));
    }
    Ok(())
}

async fn del(path: &str) -> Result<(), ServerFnError> {
    let resp = gloo_net::http::Request::delete(path)
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if !resp.ok() {
        return Err(ServerFnError::new(format!("HTTP {}", resp.status())));
    }
    Ok(())
}

async fn post_empty_void(path: &str) -> Result<(), ServerFnError> {
    let resp = gloo_net::http::Request::post(path)
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if !resp.ok() {
        return Err(ServerFnError::new(format!("HTTP {}", resp.status())));
    }
    Ok(())
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

pub async fn get_template_context(
    render_context: RenderContext,
) -> Result<Vec<TemplateVar>, ServerFnError> {
    post("/dashboard/context", &render_context).await
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
