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
        services::{ServeDir, ServeFile},
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

    let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
        .with_secure(tls_enabled);
    let auth_backend = crate::auth::Backend;
    let auth_layer =
        axum_login::AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
    let device_api = crate::device::api::router(tls_enabled);
    let auth_api = crate::auth::router();

    // Serve the WASM bundle and static assets.
    // Priority: DIOXUS_ASSET_DIR env var → dx debug build → dx release build → dist/
    let asset_dir = std::env::var("DIOXUS_ASSET_DIR").unwrap_or_else(|_| {
        let candidates = [
            "target/dx/srvr/debug/web/public",
            "target/dx/srvr/release/web/public",
            "dist",
        ];
        candidates
            .iter()
            .find(|p| std::path::Path::new(p).join("index.html").exists())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "dist".to_string())
    });
    let spa = ServeDir::new(&asset_dir)
        .fallback(ServeFile::new(format!("{asset_dir}/index.html")));

    axum::Router::new()
        .route(
            "/metrics",
            get(move || async move { metric_handle.render() }),
        )
        .merge(device_api)
        .merge(auth_api)
        .merge(crate::api::router())
        .fallback_service(spa)
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
