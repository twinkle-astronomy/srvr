use axum::{Json, extract::Path, http::StatusCode, routing::{delete, get, post}};

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
    models::{HttpSource, HttpSourceResult},
};

async fn list_sources(
    auth: AuthSession,
    Path(template_id): Path<i64>,
) -> Result<Json<Vec<HttpSource>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_http_sources(template_id).await?))
}

async fn save_source(
    auth: AuthSession,
    Json(source): Json<HttpSource>,
) -> Result<Json<HttpSource>, ApiError> {
    require_auth(&auth)?;
    let result = match source.id {
        Some(id) => {
            crate::db::update_http_source(id, &source.name, &source.url).await?;
            source
        }
        None => {
            crate::db::create_http_source(source.template_id, &source.name, &source.url).await?
        }
    };
    Ok(Json(result))
}

async fn delete_source(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::delete_http_source(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn execute_source(
    auth: AuthSession,
    Json(source): Json<HttpSource>,
) -> Result<Json<HttpSourceResult>, ApiError> {
    use crate::models::server::http_client;
    require_auth(&auth)?;
    let response = http_client()
        .get(&source.url)
        .header("Accept", "application/json")
        .send()
        .await;
    match response {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(data) => Ok(Json(HttpSourceResult {
                source_name: source.name,
                data: Some(data),
                error: None,
            })),
            Err(e) => Ok(Json(HttpSourceResult {
                source_name: source.name,
                data: None,
                error: Some(format!("Failed to parse JSON: {e}")),
            })),
        },
        Err(e) => Ok(Json(HttpSourceResult {
            source_name: source.name,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route(
            "/templates/{template_id}/http-sources",
            get(list_sources),
        )
        .route("/http-sources", post(save_source))
        .route("/http-sources/{id}", delete(delete_source))
        .route("/http-sources/execute", post(execute_source))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn unauthenticated_returns_401() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(
                Request::get("/templates/1/http-sources")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
