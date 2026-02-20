use std::sync::Arc;

use tokio::sync::Mutex;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use axum::routing::get;
    use axum_prometheus::PrometheusMetricLayer;
    use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;
    use diesel_migrations::{MigrationHarness, embed_migrations};
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use tower_http::{
        cors::{Any, CorsLayer},
        trace::TraceLayer,
    };
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    use srvr::app::*;
    use srvr::state::AppState;

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

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

    // Initialize database and run migrations
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "data.db".to_string());
    const MIGRATIONS: diesel_migrations::EmbeddedMigrations = embed_migrations!();

    use diesel::Connection;
    let mut db = diesel::SqliteConnection::establish(&database_url).expect("Connecting to db");
    db.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run migrations");

    tracing::info!("database ready ({})", &database_url);

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let db = Arc::new(Mutex::new(SyncConnectionWrapper::new(db)));

    let state = AppState {
        leptos_options: leptos_options.clone(),
        db: db.clone(),
    };

    let device_api = srvr::device::api::router();

    let app = Router::new()
        .merge(device_api)
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let db = db.clone();
                move || leptos::context::provide_context(db.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .layer(TraceLayer::new_for_http())
        .layer(prometheus_layer)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    tracing::info!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
