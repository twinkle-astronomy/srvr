#[cfg(feature = "server")]
mod api;
#[cfg(feature = "server")]
mod auth;
#[cfg(feature = "server")]
mod db;
#[cfg(feature = "server")]
pub(crate) mod device;
mod frontend;
#[cfg(feature = "server")]
pub mod hmac;
mod models;
#[cfg(feature = "server")]
pub mod time;
#[cfg(feature = "server")]
mod tls;

#[cfg(feature = "server")]
async fn build_router(tls_enabled: bool) -> axum::Router {
    use axum::routing::get;
    use axum_prometheus::PrometheusMetricLayer;
    use tower_http::{
        cors::{Any, CorsLayer},
        services::ServeDir,
        trace::TraceLayer,
    };

    // Initialize database and run migrations
    let db = crate::db::init().await;
    sqlx::migrate!()
        .run(db)
        .await
        .expect("Failed to run database migrations");
    tracing::info!("Database initialized and migrations applied");

    // Session store for auth
    let session_store = tower_sessions_sqlx_store::SqliteStore::new(db.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    // HMAC secret for signed image URLs: the IMAGE_SIGNATURE_SECRET env var
    // wins when set; otherwise generate a random per-process secret. Signed
    // URLs are only valid for ~60 seconds, so a restart invalidating in-flight
    // URLs is an acceptable edge case — set the env var for a stable key.
    let signing_secret = match std::env::var("IMAGE_SIGNATURE_SECRET") {
        Ok(s) if !s.is_empty() => {
            tracing::info!("using IMAGE_SIGNATURE_SECRET from the environment");
            s
        }
        _ => {
            tracing::info!(
                "IMAGE_SIGNATURE_SECRET not set — using a random signing secret for this run"
            );
            crate::hmac::random_hex_token()
        }
    };
    crate::hmac::init_signing_secret(signing_secret);

    let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
        .with_secure(tls_enabled);
    let auth_backend = crate::auth::Backend;
    let auth_layer =
        axum_login::AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
    let device_api = crate::device::api::router(tls_enabled);
    let auth_api = crate::auth::router();

    // Serve the WASM bundle and static assets.
    // Priority: DIOXUS_ASSET_DIR env var → dx debug/release build → `dx bundle`
    // output (dist/public — the publish image also sets DIOXUS_ASSET_DIR there).
    let asset_dir = std::env::var("DIOXUS_ASSET_DIR").unwrap_or_else(|_| {
        let candidates = [
            "target/dx/srvr/debug/web/public",
            "target/dx/srvr/release/web/public",
            "dist/public",
            "dist",
        ];
        candidates
            .iter()
            .find(|p| std::path::Path::new(p).join("index.html").exists())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "dist/public".to_string())
    });
    if std::path::Path::new(&asset_dir).join("index.html").exists() {
        tracing::info!("serving dashboard assets from {asset_dir}");
    } else {
        tracing::error!(
            "no WASM bundle at {asset_dir} — the dashboard will 404. Build one \
             (`dx build --platform web` / `dx bundle`) or set DIOXUS_ASSET_DIR"
        );
    }
    // The hashed bundle and assets are served by ServeDir services that 404 on
    // a miss — a stale cached page requesting an old hashed bundle must get a
    // clean 404, not index.html with a 200 (the browser would then choke on
    // text/html where it expected JS/WASM). Everything else falls back to
    // index.html for the client-side router, except file-like paths and
    // unmatched API paths, which also 404.
    let index_html = std::sync::Arc::new(format!("{asset_dir}/index.html"));
    let spa_fallback = move |uri: axum::http::Uri| {
        let index_html = index_html.clone();
        async move {
            use axum::response::IntoResponse;
            let path = uri.path();
            let is_index = path == "/" || path == "/index.html";
            let file_like =
                !is_index && path.rsplit('/').next().is_some_and(|seg| seg.contains('.'));
            if file_like || path.starts_with("/api/") || path.starts_with("/dashboard/") {
                return (axum::http::StatusCode::NOT_FOUND, "Not found").into_response();
            }
            match tokio::fs::read(index_html.as_str()).await {
                Ok(bytes) => (
                    [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    bytes,
                )
                    .into_response(),
                Err(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "index.html not found — build the WASM bundle (dx build --platform web)",
                )
                    .into_response(),
            }
        }
    };

    axum::Router::new()
        .route(
            "/metrics",
            get(move || async move { metric_handle.render() }),
        )
        .merge(device_api)
        .merge(auth_api)
        .merge(crate::api::router())
        .nest_service("/assets", ServeDir::new(format!("{asset_dir}/assets")))
        .nest_service("/wasm", ServeDir::new(format!("{asset_dir}/wasm")))
        .fallback(spa_fallback)
        .layer(auth_layer)
        .layer(TraceLayer::new_for_http())
        .layer(prometheus_layer)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}

fn main() {
    #[cfg(feature = "server")]
    {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_file(true)
                    .with_line_number(true)
                    .with_writer(std::io::stdout),
            )
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info,tower_http=debug".into()),
            )
            .init();

        let tls_mode = crate::tls::TlsMode::from_env();

        let serve_plain = || async {
            let router = build_router(false).await;
            let addr = format!(
                "{}:{}",
                std::env::var("IP").unwrap_or_else(|_| "0.0.0.0".to_string()),
                std::env::var("PORT").unwrap_or_else(|_| "8080".to_string()),
            );
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("Failed to bind");
            tracing::info!("Listening on {addr}");
            axum::serve(listener, router).await.expect("Server error");
        };

        match tls_mode {
            crate::tls::TlsMode::Disabled => {
                tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime")
                    .block_on(serve_plain());
            }
            crate::tls::TlsMode::Manual {
                cert_path,
                key_path,
            } => {
                tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime")
                    .block_on(async {
                        let router = build_router(true).await;
                        crate::tls::serve_manual_tls(router, &cert_path, &key_path).await;
                    });
            }
            crate::tls::TlsMode::Acme {
                domains,
                email,
                cache_dir,
                production,
            } => {
                tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime")
                    .block_on(async {
                        let router = build_router(true).await;
                        crate::tls::serve_acme(router, domains, email, cache_dir, production).await;
                    });
            }
        }
    }
    #[cfg(not(feature = "server"))]
    dioxus::launch(frontend::App);
}
