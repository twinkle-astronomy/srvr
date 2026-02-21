
#[cfg(feature = "server")]
mod db;
#[cfg(feature = "server")]
mod device;
mod frontend;

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

            let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
            let device_api = crate::device::api::router();

            let router = dioxus::server::router(frontend::App)
                .route("/metrics", get(move || async move { metric_handle.render() }))
                .merge(device_api)
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
