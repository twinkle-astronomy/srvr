use axum::{Json, extract::Path, http::StatusCode, routing::{delete, get, post}};

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
    models::{RangeQuery, RangeQueryResult},
};

async fn get_queries_for_template(
    auth: AuthSession,
    Path(template_id): Path<i64>,
) -> Result<Json<Vec<RangeQuery>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_range_queries(template_id).await?))
}

async fn save_query(
    auth: AuthSession,
    Json(rq): Json<RangeQuery>,
) -> Result<Json<RangeQuery>, ApiError> {
    require_auth(&auth)?;
    let result = match rq.id {
        Some(id) => {
            crate::db::update_range_query(id, &rq.name, &rq.addr, &rq.query, &rq.duration, &rq.step)
                .await?;
            rq
        }
        None => {
            crate::db::create_range_query(
                rq.template_id,
                &rq.name,
                &rq.addr,
                &rq.query,
                &rq.duration,
                &rq.step,
            )
            .await?
        }
    };
    Ok(Json(result))
}

async fn delete_query(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::delete_range_query(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn execute_query(
    auth: AuthSession,
    Json(query): Json<RangeQuery>,
) -> Result<Json<RangeQueryResult>, ApiError> {
    require_auth(&auth)?;
    match query.fetch_series().await {
        Ok(series) => Ok(Json(RangeQueryResult {
            query_name: query.name,
            series,
            error: None,
        })),
        Err(e) => Ok(Json(RangeQueryResult {
            query_name: query.name,
            series: vec![],
            error: Some(e),
        })),
    }
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route(
            "/templates/{template_id}/range",
            get(get_queries_for_template),
        )
        .route("/range", post(save_query))
        .route("/range/{id}", delete(delete_query))
        .route("/range/execute", post(execute_query))
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
                Request::get("/templates/1/range")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn saved_query_appears_in_template_list() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let template = crate::db::create_template(
            &format!("api_range_tpl_{}", std::process::id()),
            "<svg/>",
        )
        .await
        .expect("create template");
        let name = format!("api_range_q_{}", std::process::id());

        let response = router
            .clone()
            .oneshot(
                Request::post("/range")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "id": null,
                            "name": name,
                            "template_id": template.id,
                            "addr": "http://prometheus:9090",
                            "query": "up",
                            "duration": "1h",
                            "step": "60s",
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
        let saved: crate::models::RangeQuery = serde_json::from_slice(&body).unwrap();
        assert!(saved.id.is_some(), "save should assign an id");

        let response = router
            .oneshot(
                Request::get(format!("/templates/{}/range", template.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let queries: Vec<crate::models::RangeQuery> = serde_json::from_slice(&body).unwrap();
        assert!(queries.iter().any(|q| q.name == name));
    }
}
