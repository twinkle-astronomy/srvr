use axum::{Json, routing::post};
use serde::Deserialize;

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
};

#[derive(Deserialize)]
struct AdHocFetchBody {
    url: String,
}

async fn execute_ad_hoc_fetch(
    auth: AuthSession,
    Json(body): Json<AdHocFetchBody>,
) -> Result<Json<String>, ApiError> {
    use crate::models::server::http_client;
    require_auth(&auth)?;

    let response = http_client()
        .get(&body.url)
        .send()
        .await
        .map_err(|e| ApiError::bad_request(format!("Fetch failed: {e}")))?;

    if !response.status().is_success() {
        return Err(ApiError::bad_request(format!(
            "Fetch failed: HTTP {}",
            response.status()
        )));
    }

    let text = response
        .text()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read response body: {e}")))?;
    Ok(Json(text))
}

pub fn router() -> axum::Router {
    axum::Router::new().route("/ad-hoc-fetch", post(execute_ad_hoc_fetch))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    /// Spins up a throwaway axum server on an ephemeral loopback port that
    /// always responds with `body`, and returns its base URL.
    async fn spawn_mock_server(body: &'static str) -> String {
        let app = Router::new().route("/mock", get(move || async move { body }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn unauthenticated_returns_401() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(
                Request::post("/ad-hoc-fetch")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"url": "http://example.com"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn response_body_is_passed_through() {
        let base_url = spawn_mock_server("hello from mock server").await;
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;

        let response = router
            .oneshot(
                Request::post("/ad-hoc-fetch")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"url": format!("{base_url}/mock")}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text: String = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(text, "hello from mock server");
    }

    #[tokio::test]
    async fn unreachable_host_returns_bad_request() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::post("/ad-hoc-fetch")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"url": "http://127.0.0.1:1/nope"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
