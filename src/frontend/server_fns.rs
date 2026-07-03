use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use crate::models::{
    AuthenticatedUser, Device, DeviceLog, HttpSource, HttpSourceResult, PrometheusQuery,
    PrometheusQueryResult, RangeQuery, RangeQueryResult, RenderContext, Template,
};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    pub time: String,
    pub date: String,
    pub prometheus_url: String,
    pub port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TemplateVar {
    pub path: String,
    pub value: String,
    pub is_error: bool,
}

// ---------------------------------------------------------------------------
// Error type
//
// Replaces dioxus::prelude::ServerFnError (which lived behind dioxus/fullstack).
// The API is kept identical to the subset we use: ::new(msg), Display, Debug.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ServerFnError(String);

impl ServerFnError {
    pub fn new(msg: impl ToString) -> Self {
        Self(msg.to_string())
    }
}

impl std::fmt::Display for ServerFnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ServerFnError {}

// ---------------------------------------------------------------------------
// Server-only utilities
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
pub mod utils {
    use itertools::Itertools;
    use liquid::{
        Object,
        model::{ScalarCow, Value},
    };

    use crate::frontend::server_fns::TemplateVar;

    fn scalar_to_template_var(prefix: &String, value: &ScalarCow<'static>) -> TemplateVar {
        TemplateVar {
            path: prefix.clone(),
            value: value.clone().into_string().into_string(),
            is_error: false,
        }
    }

    fn nil_to_template_var(prefix: &String) -> TemplateVar {
        TemplateVar {
            path: prefix.clone(),
            value: "None".to_string(),
            is_error: false,
        }
    }

    fn value_to_template_var(prefix: &String, vars: &mut Vec<TemplateVar>, value: &Value) {
        match value {
            liquid::model::Value::Scalar(scalar_cow) => {
                vars.push(scalar_to_template_var(prefix, scalar_cow))
            }
            liquid::model::Value::Array(values) => {
                for (i, value) in values.iter().enumerate() {
                    value_to_template_var(&format!("{prefix}[{i}]"), vars, value)
                }
            }
            liquid::model::Value::Object(object) => {
                obj_to_template_var(&format!("{prefix}"), vars, object)
            }
            liquid::model::Value::State(_) => unreachable!(),
            liquid::model::Value::Nil => vars.push(nil_to_template_var(prefix)),
        }
    }
    pub fn obj_to_template_var(prefix: &String, vars: &mut Vec<TemplateVar>, obj: &Object) {
        let mut scalar_vars = vec![];
        let mut object_vars = vec![];

        for (key, value) in obj
            .iter()
            .sorted_by(|(key_l, _), (key_r, _)| key_l.cmp(key_r))
        {
            let prefix = if prefix.len() > 0 {
                format!("{prefix}.{key}")
            } else {
                format!("{key}")
            };
            match value {
                liquid::model::Value::Scalar(scalar_cow) => {
                    scalar_vars.push(scalar_to_template_var(&prefix, scalar_cow))
                }
                liquid::model::Value::Array(values) if key == "points" => {
                    scalar_vars.push(TemplateVar {
                        path: prefix.clone(),
                        value: format!(
                            "{} points — iterate with {{% for p in {prefix} %}}",
                            values.len()
                        ),
                        is_error: false,
                    });
                }
                liquid::model::Value::Array(values) => {
                    for (i, value) in values.iter().enumerate() {
                        value_to_template_var(&format!("{prefix}[{i}]"), &mut scalar_vars, value)
                    }
                }
                liquid::model::Value::Object(object) => {
                    obj_to_template_var(&format!("{prefix}"), &mut object_vars, object)
                }
                liquid::model::Value::State(_) => unreachable!(),
                liquid::model::Value::Nil => vars.push(nil_to_template_var(&prefix)),
            }
        }

        vars.append(&mut scalar_vars);
        vars.append(&mut object_vars);
    }
}

// ---------------------------------------------------------------------------
// Auth helpers (server-only)
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
pub(crate) async fn require_auth() -> Result<AuthenticatedUser, ServerFnError> {
    // The server build of this module exists only for the native component-test
    // tier, which has no session — identity-requiring paths always fail here.
    // Real auth is per-handler in src/api/ (crate::api::require_auth).
    Err(ServerFnError::new("Not authenticated"))
}

// ---------------------------------------------------------------------------
// Server-only helper: assemble a RenderContext from device + template IDs
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
async fn assemble_render_context(
    device: Device,
    template: Template,
) -> Result<RenderContext, ServerFnError> {
    let to_err = |e: sqlx::Error| ServerFnError::new(e.to_string());

    let prometheus_queries = crate::db::get_prometheus_queries(template.id)
        .await
        .map_err(to_err)?;
    let range_queries = crate::db::get_range_queries(template.id)
        .await
        .map_err(to_err)?;
    let http_sources = crate::db::get_http_sources(template.id)
        .await
        .map_err(to_err)?;

    Ok(RenderContext {
        device,
        template,
        prometheus_queries,
        range_queries,
        http_sources,
    })
}

// ---------------------------------------------------------------------------
// API functions — server implementations
//
// The web build re-exports equivalents from `super::api` (gloo-net fetch calls).
// ---------------------------------------------------------------------------

// --- Auth ---

#[cfg(feature = "server")]
pub async fn login(
    _username: String,
    _password: String,
) -> Result<AuthenticatedUser, ServerFnError> {
    Err(ServerFnError::new("Login is handled by the /dashboard/auth/login HTTP endpoint"))
}

#[cfg(feature = "server")]
pub async fn logout() -> Result<(), ServerFnError> {
    Err(ServerFnError::new("Logout is handled by the /dashboard/auth/logout HTTP endpoint"))
}

#[cfg(feature = "server")]
pub async fn setup(
    _username: String,
    _password: String,
) -> Result<AuthenticatedUser, ServerFnError> {
    Err(ServerFnError::new("Setup is handled by the /dashboard/auth/setup HTTP endpoint"))
}

#[cfg(feature = "server")]
pub async fn create_user(_username: String, _password: String) -> Result<(), ServerFnError> {
    Err(ServerFnError::new(
        "Create user is handled by the /dashboard/auth/create-user HTTP endpoint",
    ))
}

#[cfg(feature = "server")]
pub async fn change_password(
    _current_password: String,
    _new_password: String,
) -> Result<(), ServerFnError> {
    Err(ServerFnError::new(
        "Change password is handled by the /dashboard/auth/change-password HTTP endpoint",
    ))
}

#[cfg(feature = "server")]
pub async fn check_auth() -> Result<Option<AuthenticatedUser>, ServerFnError> {
    // Native test tier has no session; the real handler is src/api/auth.rs.
    Ok(None)
}

#[cfg(feature = "server")]
pub async fn check_needs_setup() -> Result<bool, ServerFnError> {
    let count = crate::db::user_count()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(count == 0)
}

// --- Users ---

#[cfg(feature = "server")]
pub async fn get_all_users() -> Result<Vec<AuthenticatedUser>, ServerFnError> {
    let users = crate::db::get_users()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(users
        .into_iter()
        .map(|u| AuthenticatedUser {
            id: u.id,
            username: u.username,
        })
        .collect())
}

#[cfg(feature = "server")]
pub async fn delete_user(user_id: i64) -> Result<(), ServerFnError> {
    let current = require_auth().await?;
    if current.id == user_id {
        return Err(ServerFnError::new("Cannot delete yourself"));
    }
    let count = crate::db::user_count()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    if count <= 1 {
        return Err(ServerFnError::new("Cannot delete the last user"));
    }
    crate::db::delete_user(user_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

// --- Screen previews ---

#[cfg(feature = "server")]
pub async fn get_screen_preview_for_template(
    device_id: i64,
    template_id: i64,
) -> Result<String, ServerFnError> {
    use base64::Engine;
    let render_context = get_render_context_for_template(device_id, template_id).await?;
    let bmp_bytes = crate::device::renderer::render_screen(&render_context)
        .await
        .map_err(|e| ServerFnError::new(format!("{:?}", e)))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bmp_bytes))
}

#[cfg(feature = "server")]
pub async fn get_template_preview(render_context: RenderContext) -> Result<String, ServerFnError> {
    use base64::Engine;
    let bmp_bytes = crate::device::renderer::render_screen(&render_context)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to render screen: {}", e)))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bmp_bytes))
}

// --- Templates ---

#[cfg(feature = "server")]
pub async fn get_templates() -> Result<Vec<Template>, ServerFnError> {
    crate::db::get_templates()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[cfg(feature = "server")]
pub async fn create_template(name: String, content: String) -> Result<Template, ServerFnError> {
    crate::db::create_template(&name, &content)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to create template: {:?}", e)))
}

#[cfg(feature = "server")]
pub async fn copy_template(id: i64) -> Result<Template, ServerFnError> {
    crate::db::copy_template(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to copy template: {:?}", e)))
}

#[cfg(feature = "server")]
pub async fn delete_template(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_template(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete template: {:?}", e)))
}

#[cfg(feature = "server")]
pub async fn save_template(id: i64, name: String, content: String) -> Result<(), ServerFnError> {
    crate::db::update_template(id, &name, &content)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to save template: {:?}", e)))
}

// --- Devices ---

#[cfg(feature = "server")]
pub async fn get_devices() -> Result<Vec<Device>, ServerFnError> {
    crate::db::get_devices()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[cfg(feature = "server")]
pub async fn get_device_logs(id: i64) -> Result<Vec<DeviceLog>, ServerFnError> {
    crate::db::get_device_logs(id, 100)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[cfg(feature = "server")]
pub async fn delete_device(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_device(id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[cfg(feature = "server")]
pub async fn update_device_template(device_id: i64, template_id: i64) -> Result<(), ServerFnError> {
    crate::db::update_device_template(device_id, template_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to update device template: {:?}", e)))
}

#[cfg(feature = "server")]
pub async fn update_device_maximum_compatibility(
    device_id: i64,
    maximum_compatibility: bool,
) -> Result<(), ServerFnError> {
    crate::db::update_device_maximum_compatibility(device_id, maximum_compatibility)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to update maximum compatibility: {:?}", e)))
}

// --- Render contexts ---

#[cfg(feature = "server")]
pub async fn get_render_context(id: i64) -> Result<RenderContext, ServerFnError> {
    let device = crate::db::get_device(id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let template = crate::db::get_template_for_device(id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    assemble_render_context(device, template).await
}

#[cfg(feature = "server")]
pub async fn get_render_context_for_template(
    device_id: i64,
    template_id: i64,
) -> Result<RenderContext, ServerFnError> {
    let device = crate::db::get_device(device_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let template = crate::db::get_template_by_id(template_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    assemble_render_context(device, template).await
}

#[cfg(feature = "server")]
pub async fn get_virtual_render_context(template_id: i64) -> Result<RenderContext, ServerFnError> {
    let device = Device::virtual_device();
    let template = crate::db::get_template_by_id(template_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    assemble_render_context(device, template).await
}

// --- Template context / variables ---

#[cfg(feature = "server")]
pub async fn get_template_context(
    render_context: RenderContext,
) -> Result<Vec<TemplateVar>, ServerFnError> {
    use crate::{device::renderer::render_vars, frontend::server_fns::utils::obj_to_template_var};
    let device_obj = render_vars(&render_context)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let mut vars: Vec<TemplateVar> = vec![];
    obj_to_template_var(&"".to_string(), &mut vars, &device_obj);
    Ok(vars)
}

// --- Prometheus ---

#[cfg(feature = "server")]
pub async fn save_prometheus_query(pq: PrometheusQuery) -> Result<PrometheusQuery, ServerFnError> {
    match pq.id {
        Some(id) => {
            crate::db::update_prometheus_query(id, &pq.name, &pq.addr, &pq.query)
                .await
                .map_err(|e| ServerFnError::new(format!("Unable to update query: {:?}", e)))?;
            Ok(pq)
        }
        None => {
            let f =
                crate::db::create_prometheus_query(pq.template_id, &pq.name, &pq.addr, &pq.query)
                    .await
                    .map_err(|e| ServerFnError::new(format!("Unable to create query: {:?}", e)))?;
            Ok(f)
        }
    }
}

#[cfg(feature = "server")]
pub async fn delete_prometheus_query(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_prometheus_query(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete query: {:?}", e)))
}

#[cfg(feature = "server")]
pub async fn execute_prometheus_query(
    query: PrometheusQuery,
) -> Result<PrometheusQueryResult, ServerFnError> {
    use crate::models::{PrometheusMetricResult, server::http_client};
    let client =
        match prometheus_http_query::Client::from(http_client().clone(), query.addr.as_str()) {
            Ok(c) => c,
            Err(e) => {
                return Ok(PrometheusQueryResult {
                    query_name: query.name.clone(),
                    results: vec![],
                    error: Some(format!("Invalid prometheus address: {e}")),
                });
            }
        };
    match client.query(query.query.as_str()).get().await {
        Ok(response) => {
            let metrics = response
                .data()
                .as_vector()
                .map(|v| {
                    v.iter()
                        .map(|x| PrometheusMetricResult {
                            labels: x.metric().clone(),
                            value: x.sample().value(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(PrometheusQueryResult {
                query_name: query.name.clone(),
                results: metrics,
                error: None,
            })
        }
        Err(e) => Ok(PrometheusQueryResult {
            query_name: query.name.clone(),
            results: vec![],
            error: Some(e.to_string()),
        }),
    }
}

// --- Range queries ---

#[cfg(feature = "server")]
pub async fn save_range_query(rq: RangeQuery) -> Result<RangeQuery, ServerFnError> {
    match rq.id {
        Some(id) => {
            crate::db::update_range_query(
                id,
                &rq.name,
                &rq.addr,
                &rq.query,
                &rq.duration,
                &rq.step,
            )
            .await
            .map_err(|e| ServerFnError::new(format!("Unable to update range query: {:?}", e)))?;
            Ok(rq)
        }
        None => {
            let f = crate::db::create_range_query(
                rq.template_id,
                &rq.name,
                &rq.addr,
                &rq.query,
                &rq.duration,
                &rq.step,
            )
            .await
            .map_err(|e| ServerFnError::new(format!("Unable to create range query: {:?}", e)))?;
            Ok(f)
        }
    }
}

#[cfg(feature = "server")]
pub async fn delete_range_query(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_range_query(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete range query: {:?}", e)))
}

#[cfg(feature = "server")]
pub async fn execute_range_query(query: RangeQuery) -> Result<RangeQueryResult, ServerFnError> {
    match query.fetch_series().await {
        Ok(series) => Ok(RangeQueryResult {
            query_name: query.name.clone(),
            series,
            error: None,
        }),
        Err(e) => Ok(RangeQueryResult {
            query_name: query.name.clone(),
            series: vec![],
            error: Some(e),
        }),
    }
}

// --- HTTP sources ---

#[cfg(feature = "server")]
pub async fn save_http_source(source: HttpSource) -> Result<HttpSource, ServerFnError> {
    match source.id {
        Some(id) => {
            crate::db::update_http_source(id, &source.name, &source.url)
                .await
                .map_err(|e| {
                    ServerFnError::new(format!("Unable to update http source: {:?}", e))
                })?;
            Ok(source)
        }
        None => {
            let f = crate::db::create_http_source(source.template_id, &source.name, &source.url)
                .await
                .map_err(|e| {
                    ServerFnError::new(format!("Unable to create http source: {:?}", e))
                })?;
            Ok(f)
        }
    }
}

#[cfg(feature = "server")]
pub async fn delete_http_source(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_http_source(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete http source: {:?}", e)))
}

#[cfg(feature = "server")]
pub async fn execute_http_source(source: HttpSource) -> Result<HttpSourceResult, ServerFnError> {
    use crate::models::server::http_client;
    let response = http_client()
        .get(&source.url)
        .header("Accept", "application/json")
        .send()
        .await;
    match response {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(data) => Ok(HttpSourceResult {
                source_name: source.name,
                data: Some(data),
                error: None,
            }),
            Err(e) => Ok(HttpSourceResult {
                source_name: source.name,
                data: None,
                error: Some(format!("Failed to parse JSON: {e}")),
            }),
        },
        Err(e) => Ok(HttpSourceResult {
            source_name: source.name,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

// --- Server info ---

#[cfg(feature = "server")]
pub async fn get_server_info() -> Result<ServerInfo, ServerFnError> {
    let now = chrono::Utc::now();
    let prometheus_url =
        std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://prometheus:9090".to_string());
    Ok(ServerInfo {
        time: now.format("%H:%M:%S UTC").to_string(),
        date: now.format("%Y-%m-%d").to_string(),
        prometheus_url,
        port: 8080,
    })
}

// ---------------------------------------------------------------------------
// Web build: re-export fetch implementations from api module
// ---------------------------------------------------------------------------

#[cfg(not(feature = "server"))]
pub use super::api::*;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "server"))]
mod template_var_tests {
    use super::utils::obj_to_template_var;

    #[test]
    fn test_points_array_is_summarized_not_exploded() {
        let series = liquid::object!({
            "points": vec![
                liquid::model::Value::Object(liquid::object!({"t": 1.0, "value": 5.0})),
                liquid::model::Value::Object(liquid::object!({"t": 2.0, "value": 6.0})),
                liquid::model::Value::Object(liquid::object!({"t": 3.0, "value": 7.0})),
            ],
            "min": 5.0,
            "max": 7.0,
            "count": 3_i64,
        });

        let mut vars = vec![];
        obj_to_template_var(&String::new(), &mut vars, &series);

        let points_rows: Vec<_> = vars.iter().filter(|v| v.path.contains("points")).collect();
        assert_eq!(
            points_rows.len(),
            1,
            "points must collapse to a single summary row, not one per sample"
        );
        assert!(
            points_rows[0].value.contains('3'),
            "summary row should mention the point count, got: {}",
            points_rows[0].value
        );
        assert!(vars.iter().any(|v| v.path == "min"));
        assert!(vars.iter().any(|v| v.path == "count"));
    }
}
