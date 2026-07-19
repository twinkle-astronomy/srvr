//! Shared frontend/API types and the client-call surface.
//!
//! - **Web build** (`feature = "web"`): re-exports the real gloo-net fetch
//!   implementations from [`super::api`] (bottom of this file), so callers
//!   import everything from `crate::frontend::server_fns`.
//! - **Server build** (`feature = "server"`, used by the native component-test
//!   tier): the same symbols exist as uniform stubs that always error. The
//!   native tests inject state through the store and never await these
//!   futures; real request handling lives in `src/api/`.

use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use crate::models::{
    AuthenticatedUser, Device, DeviceLog, FirmwareRelease, HttpSource, HttpSourceResult,
    PrometheusQuery, PrometheusQueryResult, RangeQuery, RangeQueryResult, RenderContext,
    Template,
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
// Native test tier stubs
//
// Keep this list in sync with the pub fns in `src/frontend/api.rs` so both
// builds expose the same surface — a fn present in one build but not the
// other compiles in one target and breaks the other.
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
macro_rules! native_test_stubs {
    ($( pub async fn $name:ident($($arg:ty),*) -> $ok:ty; )+) => {
        $(
            #[allow(dead_code)]
            pub async fn $name($(_: $arg),*) -> Result<$ok, ServerFnError> {
                Err(ServerFnError::new(concat!(
                    stringify!($name),
                    " is a native-test-tier stub; the real endpoint lives in src/api"
                )))
            }
        )+
    };
}

#[cfg(feature = "server")]
native_test_stubs! {
    // Auth
    pub async fn login(String, String) -> AuthenticatedUser;
    pub async fn logout() -> ();
    pub async fn setup(String, String) -> AuthenticatedUser;
    pub async fn create_user(String, String) -> ();
    pub async fn change_password(String, String) -> ();
    pub async fn check_auth() -> Option<AuthenticatedUser>;
    pub async fn check_needs_setup() -> bool;
    pub async fn get_server_info() -> ServerInfo;
    // Users
    pub async fn get_all_users() -> Vec<AuthenticatedUser>;
    pub async fn delete_user(i64) -> ();
    // Devices
    pub async fn get_devices() -> Vec<Device>;
    pub async fn get_device_by_id(i64) -> Device;
    pub async fn get_device_logs(i64) -> Vec<DeviceLog>;
    pub async fn delete_device(i64) -> ();
    pub async fn update_device_template(i64, i64) -> ();
    pub async fn update_device_maximum_compatibility(i64, bool) -> ();
    pub async fn update_device_firmware_updates_enabled(i64, bool) -> ();
    pub async fn get_render_context(i64) -> RenderContext;
    pub async fn get_render_context_for_template(i64, i64) -> RenderContext;
    pub async fn get_screen_preview(i64) -> String;
    pub async fn get_screen_preview_for_template(i64, i64) -> String;
    // Templates
    pub async fn get_templates() -> Vec<Template>;
    pub async fn get_default_template() -> Template;
    pub async fn get_template_by_id(i64) -> Template;
    pub async fn create_template(String, String) -> Template;
    pub async fn save_template(i64, String, String) -> ();
    pub async fn delete_template(i64) -> ();
    pub async fn copy_template(i64) -> Template;
    pub async fn get_virtual_render_context(i64) -> RenderContext;
    pub async fn get_template_preview(RenderContext) -> String;
    pub async fn get_template_preview_png(RenderContext) -> String;
    pub async fn get_template_context(RenderContext) -> Vec<TemplateVar>;
    // Prometheus
    pub async fn get_prometheus_queries_for_template(i64) -> Vec<PrometheusQuery>;
    pub async fn save_prometheus_query(PrometheusQuery) -> PrometheusQuery;
    pub async fn delete_prometheus_query(i64) -> ();
    pub async fn execute_prometheus_query(PrometheusQuery) -> PrometheusQueryResult;
    pub async fn execute_prometheus_queries(Vec<PrometheusQuery>) -> Vec<PrometheusQueryResult>;
    // Range queries
    pub async fn get_range_queries_for_template(i64) -> Vec<RangeQuery>;
    pub async fn save_range_query(RangeQuery) -> RangeQuery;
    pub async fn delete_range_query(i64) -> ();
    pub async fn execute_range_query(RangeQuery) -> RangeQueryResult;
    // Firmware
    pub async fn get_firmware_releases() -> Vec<FirmwareRelease>;
    pub async fn upload_firmware_release(String, String, String, Vec<u8>) -> FirmwareRelease;
    pub async fn activate_firmware_release(i64) -> ();
    pub async fn delete_firmware_release(i64) -> ();
    // HTTP sources
    pub async fn save_http_source(HttpSource) -> HttpSource;
    pub async fn delete_http_source(i64) -> ();
    pub async fn execute_http_source(HttpSource) -> HttpSourceResult;
    // Claude AI (template generation)
    pub async fn has_claude_api_key() -> bool;
    pub async fn save_claude_api_key(String) -> ();
    pub async fn delete_claude_api_key() -> ();
    pub async fn execute_ad_hoc_http_fetch(String) -> String;
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
