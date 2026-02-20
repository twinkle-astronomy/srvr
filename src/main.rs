use axum::{
    Router, routing::get
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use axum_prometheus::PrometheusMetricLayer;

mod renderer;
mod device;


#[tokio::main]
async fn main() {
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
    // Initialize tracing subscriber to output to stdout
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_file(true)
                .with_line_number(true)
                .with_writer(std::io::stdout)
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into())
        )
        .init();

    let device_api = crate::device::api::router();

    let app = Router::new()
        .route("/", get(root))
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .layer(TraceLayer::new_for_http())
        .layer(prometheus_layer)
        .merge(device_api)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "TRMNL eink device server"
}
