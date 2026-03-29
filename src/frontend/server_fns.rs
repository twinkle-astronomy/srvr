use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::models::{
    AuthenticatedUser, Device, DeviceLog, HttpSource, HttpSourceResult, PrometheusQuery,
    PrometheusQueryResult, RenderContext, Template,
};

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

#[cfg(feature = "server")]
mod utils {
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

#[cfg(feature = "server")]
pub(crate) async fn require_auth() -> Result<AuthenticatedUser, ServerFnError> {
    let auth: crate::auth::AuthSession = dioxus::fullstack::FullstackContext::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("Auth extraction failed: {e}")))?;
    match auth.user {
        Some(user) => Ok(AuthenticatedUser {
            id: user.id,
            username: user.username,
        }),
        None => Err(ServerFnError::new("Not authenticated")),
    }
}

#[server]
pub async fn check_auth() -> Result<Option<AuthenticatedUser>, ServerFnError> {
    let auth: crate::auth::AuthSession = dioxus::fullstack::FullstackContext::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("Auth extraction failed: {e}")))?;
    Ok(auth.user.map(|u| AuthenticatedUser {
        id: u.id,
        username: u.username,
    }))
}

#[server]
pub async fn check_needs_setup() -> Result<bool, ServerFnError> {
    let count = crate::db::user_count()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(count == 0)
}

#[server]
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

#[server]
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

#[server]
pub async fn get_screen_preview(device_id: i64) -> Result<String, ServerFnError> {
    use base64::Engine;

    let render_context = get_render_context(device_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?;

    match crate::device::renderer::render_screen(&render_context).await {
        Ok(bmp_bytes) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bmp_bytes);
            Ok(encoded)
        }
        Err(e) => {
            tracing::info!("Failed to render screen: {}", e);
            Err(ServerFnError::new(format!("{:?}", e)))
        }
    }
}

#[server]
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

#[server]
pub async fn get_template_preview(render_context: RenderContext) -> Result<String, ServerFnError> {
    use base64::Engine;

    let bmp_bytes = crate::device::renderer::render_screen(&render_context)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to render screen: {}", e)))?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&bmp_bytes);
    Ok(encoded)
}

#[server]
pub async fn get_default_template() -> Result<Template, ServerFnError> {
    let template = crate::db::get_default_template()
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to query db: {:?}", e)))?;

    Ok(template)
}

#[server]
pub async fn get_templates() -> Result<Vec<Template>, ServerFnError> {
    crate::db::get_templates()
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn get_template_by_id(id: i64) -> Result<Template, ServerFnError> {
    crate::db::get_template_by_id(id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn create_template(name: String, content: String) -> Result<Template, ServerFnError> {
    crate::db::create_template(&name, &content)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to create template: {:?}", e)))
}

#[server]
pub async fn copy_template(id: i64) -> Result<Template, ServerFnError> {
    crate::db::copy_template(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to copy template: {:?}", e)))
}

#[server]
pub async fn delete_template(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_template(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete template: {:?}", e)))
}

#[server]
pub async fn save_template(id: i64, name: String, content: String) -> Result<(), ServerFnError> {
    crate::db::update_template(id, &name, &content)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to save template: {:?}", e)))?;

    Ok(())
}

#[server]
pub async fn update_device_template(device_id: i64, template_id: i64) -> Result<(), ServerFnError> {
    crate::db::update_device_template(device_id, template_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to update device template: {:?}", e)))
}

#[server]
pub async fn update_device_maximum_compatibility(
    device_id: i64,
    maximum_compatibility: bool,
) -> Result<(), ServerFnError> {
    crate::db::update_device_maximum_compatibility(device_id, maximum_compatibility)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to update maximum compatibility: {:?}", e)))
}

#[server]
pub async fn get_devices() -> Result<Vec<Device>, ServerFnError> {
    crate::db::get_devices()
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn get_render_context(id: i64) -> Result<RenderContext, ServerFnError> {
    let device = crate::db::get_device(id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    let template = crate::db::get_template_for_device(id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    let prometheus_queries = crate::db::get_prometheus_queries(template.id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    let http_sources = crate::db::get_http_sources(template.id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    Ok(RenderContext {
        device,
        template,
        prometheus_queries,
        http_sources,
    })
}

#[server]
pub async fn get_render_context_for_template(
    device_id: i64,
    template_id: i64,
) -> Result<RenderContext, ServerFnError> {
    let device = crate::db::get_device(device_id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    let template = crate::db::get_template_by_id(template_id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    let prometheus_queries = crate::db::get_prometheus_queries(template.id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    let http_sources = crate::db::get_http_sources(template.id)
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))?;

    Ok(RenderContext {
        device,
        template,
        prometheus_queries,
        http_sources,
    })
}

#[server]
pub async fn get_device_by_id(id: i64) -> Result<Device, ServerFnError> {
    crate::db::get_device(id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn get_device_logs(id: i64) -> Result<Vec<DeviceLog>, ServerFnError> {
    crate::db::get_device_logs(id, 100)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn delete_device(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_device(id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
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

#[server]
pub async fn delete_prometheus_query(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_prometheus_query(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete query: {:?}", e)))
}

#[server]
pub async fn get_prometheus_queries_for_template(
    template_id: i64,
) -> Result<Vec<PrometheusQuery>, ServerFnError> {
    crate::db::get_prometheus_queries(template_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to query db: {:?}", e)))
}

#[server]
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

#[server]
pub async fn execute_prometheus_queries(
    queries: Vec<PrometheusQuery>,
) -> Result<Vec<PrometheusQueryResult>, ServerFnError> {
    use crate::models::PrometheusMetricResult;

    // let queries = crate::db::get_prometheus_queries(template_id)
    //     .await
    //     .map_err(|e| ServerFnError::new(format!("Unable to query db: {:?}", e)))?;

    let mut results = Vec::with_capacity(queries.len());
    for query in &queries {
        use crate::models::server::http_client;

        let client =
            match prometheus_http_query::Client::from(http_client().clone(), query.addr.as_str()) {
                Ok(c) => c,
                Err(e) => {
                    results.push(PrometheusQueryResult {
                        query_name: query.name.clone(),
                        results: vec![],
                        error: Some(format!("Invalid prometheus address: {e}")),
                    });
                    continue;
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
                results.push(PrometheusQueryResult {
                    query_name: query.name.clone(),
                    results: metrics,
                    error: None,
                });
            }
            Err(e) => {
                results.push(PrometheusQueryResult {
                    query_name: query.name.clone(),
                    results: vec![],
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(results)
}

#[server]
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

#[server]
pub async fn delete_http_source(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_http_source(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete http source: {:?}", e)))
}

#[server]
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

#[server]
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

#[server]
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
