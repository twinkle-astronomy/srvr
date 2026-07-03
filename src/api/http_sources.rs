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

    #[tokio::test]
    async fn saved_source_appears_in_template_list() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let template = crate::db::create_template(
            &format!("api_http_tpl_{}", std::process::id()),
            "<svg/>",
        )
        .await
        .expect("create template");
        let name = format!("api_http_src_{}", std::process::id());

        let response = router
            .clone()
            .oneshot(
                Request::post("/http-sources")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "id": null,
                            "name": name,
                            "template_id": template.id,
                            "url": "http://example.com/data.json",
                            "created_at": "2026-01-01T00:00:00",
                            "updated_at": "2026-01-01T00:00:00"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success(), "save: {}", response.status());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let saved: crate::models::HttpSource = serde_json::from_slice(&body).unwrap();
        assert!(saved.id.is_some(), "save should assign an id");

        let response = router
            .oneshot(
                Request::get(format!("/templates/{}/http-sources", template.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let sources: Vec<crate::models::HttpSource> = serde_json::from_slice(&body).unwrap();
        assert!(sources.iter().any(|s| s.name == name));
    }
}
