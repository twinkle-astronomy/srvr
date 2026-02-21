use tracing::info;


pub async fn get_prometheus() -> Option<f64> {
    let prometheus_url =
    std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://prometheus:9090".to_string());
    let client = prometheus_http_query::Client::try_from(prometheus_url.as_str()).unwrap();
    let query = r#"sht30_reading{location="Front Porch", sensor="temperature"} * 9/5 + 32"#;

    match client.query(query).get().await {
        Ok(response) => response
            .data()
            .as_vector()
            .and_then(|v| v.first().map(|sample| sample.sample().value())),
        Err(e) => {
            info!("Failed to query Prometheus: {}", e);
            None
        }
    }
}