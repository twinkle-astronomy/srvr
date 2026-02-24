use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::models::{Device, PrometheusQuery, PrometheusQueryResult, Template};

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
                        value_to_template_var(&format!("{prefix}[{}]", i + 1), &mut scalar_vars, value)
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

#[server]
pub async fn get_screen_preview(device_id: i64) -> Result<Option<String>, ServerFnError> {
    use crate::db::{get_device, get_template};
    use base64::Engine;

    let device = get_device(device_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?
        .ok_or_else(|| {
            ServerFnError::new(format!("Unable to find device with id: {:?}", device_id))
        })?;

    let template = get_template()
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?;

    match crate::device::renderer::render_screen(&device, &template).await {
        Ok(bmp_bytes) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bmp_bytes);
            Ok(Some(encoded))
        }
        Err(e) => {
            tracing::info!("Failed to render screen: {}", e);
            Ok(None)
        }
    }
}

#[server]
pub async fn get_template_preview(
    device_id: i64,
    template: Template,
) -> Result<Option<String>, ServerFnError> {
    use crate::db::get_device;
    use base64::Engine;

    let device = get_device(device_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unablle to query db: {:?}", e)))?
        .ok_or_else(|| {
            ServerFnError::new(format!("Unable to find device with id: {:?}", device_id))
        })?;

    let bmp_bytes = crate::device::renderer::render_screen(&device, &template)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to render screen: {}", e)))?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&bmp_bytes);
    Ok(Some(encoded))
}

#[server]
pub async fn get_template() -> Result<Template, ServerFnError> {
    let template = crate::db::get_template()
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to query db: {:?}", e)))?;

    Ok(template)
}

#[server]
pub async fn save_template(id: i64, content: String) -> Result<(), ServerFnError> {
    crate::db::update_template(id, &content)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to save template: {:?}", e)))?;

    Ok(())
}

#[server]
pub async fn get_devices() -> Result<Vec<Device>, ServerFnError> {
    crate::db::get_devices()
        .await
        .map_err(|e: sqlx::Error| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn update_prometheus_query(
    id: i64,
    name: String,
    addr: String,
    query: String,
) -> Result<(), ServerFnError> {
    crate::db::update_prometheus_query(id, &name, &addr, &query)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to update query: {:?}", e)))
}

#[server]
pub async fn delete_prometheus_query(id: i64) -> Result<(), ServerFnError> {
    crate::db::delete_prometheus_query(id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to delete query: {:?}", e)))
}

#[server]
pub async fn create_prometheus_query(
    template_id: i64,
    name: String,
    addr: String,
    query: String,
) -> Result<(), ServerFnError> {
    crate::db::create_prometheus_query(template_id, &name, &addr, &query)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to create query: {:?}", e)))
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
pub async fn execute_prometheus_queries(
    template_id: i64,
) -> Result<Vec<PrometheusQueryResult>, ServerFnError> {
    use crate::models::PrometheusMetricResult;

    let queries = crate::db::get_prometheus_queries(template_id)
        .await
        .map_err(|e| ServerFnError::new(format!("Unable to query db: {:?}", e)))?;

    let mut results = Vec::with_capacity(queries.len());
    for query in &queries {
        let client = match prometheus_http_query::Client::try_from(query.addr.as_str()) {
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
pub async fn get_template_context(
    device_id: i64,
    _: i64,
) -> Result<Vec<TemplateVar>, ServerFnError> {
    use crate::{device::renderer::render_vars, frontend::server_fns::utils::obj_to_template_var};

    let device = crate::db::get_device(device_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new(format!("Device {device_id} not found")))?;

    let template = crate::db::get_template()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let device_obj = render_vars(&device, &template)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    // device_obj.
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
