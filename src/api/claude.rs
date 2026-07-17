//! Server-side proxy for the Claude Messages API.
//!
//! The browser never sees the user's Anthropic API key: the WASM app POSTs
//! the Messages API request body here, and this handler attaches the calling
//! user's stored key and forwards it upstream. The upstream response (status
//! and body) is passed through verbatim — including Anthropic's own error
//! JSON — so the client can surface API errors exactly as if it had called
//! Anthropic directly. The agentic loop itself stays entirely in WASM; this
//! is key custody plus forwarding, nothing more.

use axum::{Json, response::IntoResponse, response::Response, routing::post};

use crate::{
    api::{ApiError, require_auth},
    auth::AuthSession,
};

/// Claude legitimately takes minutes to generate a long response (the WASM
/// client requests up to 128k output tokens), so this boundary needs a far
/// larger bound than the shared client's 20s default — set per-request, which
/// overrides the client-level timeout. Anthropic caps non-streaming requests
/// at ~10 minutes.
const UPSTREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Upstream base URL, overridable via `ANTHROPIC_BASE_URL` (used by tests to
/// point at a mock; also handy for routing through an API-compatible gateway).
fn anthropic_base_url() -> String {
    std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string())
}

async fn proxy_messages(
    auth: AuthSession,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    let user = require_auth(&auth)?;
    let Some(key) = crate::db::get_claude_api_key(user.id).await? else {
        return Err(ApiError::bad_request(
            "No Claude API key configured — add yours on the Users page",
        ));
    };

    let upstream = crate::models::server::http_client()
        .post(format!("{}/v1/messages", anthropic_base_url()))
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .timeout(UPSTREAM_TIMEOUT)
        .send()
        .await
        .map_err(|e| ApiError::bad_request(format!("Claude API request failed: {e}")))?;

    let status = axum::http::StatusCode::from_u16(upstream.status().as_u16())
        .unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
    let bytes = upstream
        .bytes()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read Claude API response: {e}")))?;

    Ok((
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        bytes,
    )
        .into_response())
}

pub fn router() -> axum::Router {
    axum::Router::new().route("/claude/messages", post(proxy_messages))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, Request, StatusCode},
        response::IntoResponse,
        routing::post,
        Json,
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    /// Mock Anthropic upstream: echoes back the auth headers and body it
    /// received, or returns a canned 429 error when the body asks for one —
    /// so one test can cover both success and error passthrough without two
    /// tests racing on the `ANTHROPIC_BASE_URL` env var.
    async fn spawn_mock_anthropic() -> String {
        async fn mock_messages(headers: HeaderMap, Json(body): Json<Value>) -> axum::response::Response {
            if body.get("make_error").is_some() {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"type": "error", "error": {"type": "rate_limit_error"}})),
                )
                    .into_response();
            }
            Json(json!({
                "received_key": headers.get("x-api-key").and_then(|v| v.to_str().ok()),
                "received_version": headers.get("anthropic-version").and_then(|v| v.to_str().ok()),
                "echo": body,
            }))
            .into_response()
        }
        let app = Router::new().route("/v1/messages", post(mock_messages));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        format!("http://{addr}")
    }

    async fn body_json(response: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).expect("body should be JSON")
    }

    #[tokio::test]
    async fn unauthenticated_returns_401() {
        let router = crate::api::test_support::auth_router(super::router()).await;
        let response = router
            .oneshot(
                Request::post("/claude/messages")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"model": "claude-opus-4-8"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_key_returns_bad_request_without_touching_network() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::post("/claude/messages")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"model": "claude-opus-4-8"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_json(response).await;
        assert!(
            body["error"].as_str().unwrap_or_default().contains("No Claude API key"),
            "error should say the key is missing, got: {body}"
        );
    }

    #[tokio::test]
    async fn forwards_key_and_body_and_passes_upstream_responses_through() {
        let base_url = spawn_mock_anthropic().await;
        // Process-global, but this is the only test that sets or reads it, so
        // parallel test threads can't observe a torn value.
        unsafe { std::env::set_var("ANTHROPIC_BASE_URL", &base_url) };

        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let save = router
            .clone()
            .oneshot(
                Request::put("/claude-api-key")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"key": "sk-ant-test-key"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(save.status().is_success(), "saving the key failed: {}", save.status());

        // Success path: the stored key and version header are attached
        // server-side and the request body arrives upstream unchanged.
        let response = router
            .clone()
            .oneshot(
                Request::post("/claude/messages")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"model": "claude-opus-4-8", "max_tokens": 5}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["received_key"], "sk-ant-test-key");
        assert_eq!(body["received_version"], "2023-06-01");
        assert_eq!(body["echo"]["model"], "claude-opus-4-8");
        assert_eq!(body["echo"]["max_tokens"], 5);

        // Error path: upstream status and error body pass through verbatim,
        // not wrapped in this server's ApiError shape.
        let response = router
            .oneshot(
                Request::post("/claude/messages")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"make_error": true}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = body_json(response).await;
        assert_eq!(body["error"]["type"], "rate_limit_error");
    }
}
