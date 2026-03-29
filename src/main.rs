#[cfg(feature = "server")]
mod auth;
#[cfg(feature = "server")]
mod db;
#[cfg(feature = "server")]
mod device;
mod frontend;
mod models;

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

        dioxus::serve(|| async move {
            use axum::routing::get;
            use axum_prometheus::PrometheusMetricLayer;
            use tower_http::{
                cors::{Any, CorsLayer},
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
                .with_secure(false);
            let auth_backend = crate::auth::Backend;
            let auth_layer =
                axum_login::AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

            let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
            let device_api = crate::device::api::router();
            let auth_api = crate::auth::router();

            let router = dioxus::server::router(frontend::App)
                .route(
                    "/metrics",
                    get(move || async move { metric_handle.render() }),
                )
                .route_layer(axum::middleware::from_fn(
                    crate::auth::server_fn_auth_middleware,
                ))
                .merge(device_api)
                .merge(auth_api)
                .layer(auth_layer)
                .layer(TraceLayer::new_for_http())
                .layer(prometheus_layer)
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                );

            Ok(router)
        });
    }
    #[cfg(not(feature = "server"))]
    dioxus::launch(frontend::App);
}
