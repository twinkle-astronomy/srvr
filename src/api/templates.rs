use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    api::{ApiError, assemble_render_context, require_auth},
    auth::AuthSession,
    frontend::server_fns::TemplateVar,
    models::{RenderContext, Template},
};

#[derive(Deserialize)]
struct CreateTemplateBody {
    name: String,
    content: String,
}

#[derive(Deserialize)]
struct UpdateTemplateBody {
    name: String,
    content: String,
}

async fn get_templates(auth: AuthSession) -> Result<Json<Vec<Template>>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_templates().await?))
}

async fn get_default_template(auth: AuthSession) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_default_template().await?))
}

async fn get_template_by_id(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::get_template_by_id(id).await?))
}

async fn create_template(
    auth: AuthSession,
    Json(body): Json<CreateTemplateBody>,
) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(
        crate::db::create_template(&body.name, &body.content).await?,
    ))
}

async fn copy_template(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<Template>, ApiError> {
    require_auth(&auth)?;
    Ok(Json(crate::db::copy_template(id).await?))
}

async fn save_template(
    auth: AuthSession,
    Path(id): Path<i64>,
    Json(body): Json<UpdateTemplateBody>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::update_template(id, &body.name, &body.content).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_template(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    require_auth(&auth)?;
    crate::db::delete_template(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_virtual_render_context(
    auth: AuthSession,
    Path(id): Path<i64>,
) -> Result<Json<RenderContext>, ApiError> {
    require_auth(&auth)?;
    let device = crate::models::Device::virtual_device();
    let template = crate::db::get_template_by_id(id).await?;
    Ok(Json(assemble_render_context(device, template).await?))
}

async fn get_template_preview(
    auth: AuthSession,
    Json(ctx): Json<RenderContext>,
) -> Result<Json<String>, ApiError> {
    use base64::Engine;
    require_auth(&auth)?;
    let bmp = crate::device::renderer::render_screen(&ctx)
        .await
        .map_err(|e| ApiError::internal(format!("{e:?}")))?;
    Ok(Json(
        base64::engine::general_purpose::STANDARD.encode(&bmp),
    ))
}

async fn get_template_context(
    auth: AuthSession,
    Json(ctx): Json<RenderContext>,
) -> Result<Json<Vec<TemplateVar>>, ApiError> {
    use crate::frontend::server_fns::utils::obj_to_template_var;
    require_auth(&auth)?;
    let obj = crate::device::renderer::render_vars(&ctx)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut vars = vec![];
    obj_to_template_var(&String::new(), &mut vars, &obj);
    Ok(Json(vars))
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/templates", get(get_templates).post(create_template))
        .route("/templates/default", get(get_default_template))
        .route(
            "/templates/{id}",
            get(get_template_by_id)
                .put(save_template)
                .delete(delete_template),
        )
        .route("/templates/{id}/copy", post(copy_template))
        .route(
            "/templates/{id}/virtual-render-context",
            get(get_virtual_render_context),
        )
        .route("/preview", post(get_template_preview))
        .route("/context", post(get_template_context))
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
            .oneshot(Request::get("/templates").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_then_fetch_template_round_trips() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let name = format!("api_tpl_roundtrip_{}", std::process::id());
        let response = router
            .clone()
            .oneshot(
                Request::post("/templates")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": name,
                            "content": "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success(), "create: {}", response.status());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: crate::models::Template = serde_json::from_slice(&body).unwrap();
        assert_eq!(created.name, name);

        let response = router
            .oneshot(
                Request::get(format!("/templates/{}", created.id))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let fetched: crate::models::Template = serde_json::from_slice(&body).unwrap();
        assert_eq!(fetched.name, name);
    }

    #[tokio::test]
    async fn unknown_template_returns_404() {
        let (router, cookie) = crate::api::test_support::login_session(super::router()).await;
        let response = router
            .oneshot(
                Request::get("/templates/999999999")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
