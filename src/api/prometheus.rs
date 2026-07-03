use axum::{Json, extract::Path, http::StatusCode, routing::{delete, get, post}};

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
    models::{PrometheusQuery, PrometheusQueryResult},
};

async fn get_queries_for_template(
    auth: AuthSession,
    Path(template_id): Path<i64>,
) -> Result<Json<Vec<PrometheusQuery>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_prometheus_queries(template_id).await?))
}

async fn save_query(
    auth: AuthSession,
    Json(pq): Json<PrometheusQuery>,
) -> Result<Json<PrometheusQuery>, ApiError> {
    require_auth(&auth)?;
    let result = match pq.id {
        Some(id) => {
            crate::db::update_prometheus_query(id, &pq.name, &pq.addr, &pq.query).await?;
            pq
        }
        None => {
            crate::db::create_prometheus_query(pq.template_id, &pq.name, &pq.addr, &pq.query)
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
    crate::db::delete_prometheus_query(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn execute_query(
    auth: AuthSession,
    Json(query): Json<PrometheusQuery>,
) -> Result<Json<PrometheusQueryResult>, ApiError> {
    use crate::models::{PrometheusMetricResult, server::http_client};
    require_auth(&auth)?;
    let client =
        match prometheus_http_query::Client::from(http_client().clone(), query.addr.as_str()) {
            Ok(c) => c,
            Err(e) => {
                return Ok(Json(PrometheusQueryResult {
                    query_name: query.name,
                    results: vec![],
                    error: Some(format!("Invalid prometheus address: {e}")),
                }));
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
            Ok(Json(PrometheusQueryResult {
                query_name: query.name,
                results: metrics,
                error: None,
            }))
        }
        Err(e) => Ok(Json(PrometheusQueryResult {
            query_name: query.name,
            results: vec![],
            error: Some(e.to_string()),
        })),
    }
}

async fn execute_queries(
    auth: AuthSession,
    Json(queries): Json<Vec<PrometheusQuery>>,
) -> Result<Json<Vec<PrometheusQueryResult>>, ApiError> {
    use crate::models::{PrometheusMetricResult, server::http_client};
    require_auth(&auth)?;
    let mut results = Vec::with_capacity(queries.len());
    for query in &queries {
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
    Ok(Json(results))
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route(
            "/templates/{template_id}/prometheus",
            get(get_queries_for_template),
        )
        .route("/prometheus", post(save_query))
        .route("/prometheus/{id}", delete(delete_query))
        .route("/prometheus/execute", post(execute_query))
        .route("/prometheus/execute-all", post(execute_queries))
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
                Request::get("/templates/1/prometheus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
