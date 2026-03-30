use std::net::SocketAddr;

pub enum TlsMode {
    Disabled,
    Manual {
        cert_path: String,
        key_path: String,
    },
    Acme {
        domains: Vec<String>,
        email: Option<String>,
        cache_dir: String,
        production: bool,
    },
}

impl TlsMode {
    pub fn from_env() -> Self {
        let cert_path = std::env::var("TLS_CERT_PATH").ok();
        let key_path = std::env::var("TLS_KEY_PATH").ok();

        if let (Some(cert_path), Some(key_path)) = (cert_path, key_path) {
            return TlsMode::Manual {
                cert_path,
                key_path,
            };
        }

        if let Ok(domain_str) = std::env::var("ACME_DOMAIN") {
            let domains: Vec<String> = domain_str.split(',').map(|s| s.trim().to_string()).collect();
            let email = std::env::var("ACME_EMAIL").ok();
            let cache_dir = std::env::var("ACME_CACHE_DIR")
                .unwrap_or_else(|_| ".data/acme".to_string());
            let production = std::env::var("ACME_STAGING")
                .map(|v| v != "true" && v != "1")
                .unwrap_or(true);
            return TlsMode::Acme {
                domains,
                email,
                cache_dir,
                production,
            };
        }

        TlsMode::Disabled
    }

}

pub fn https_addr() -> SocketAddr {
    let ip = std::env::var("IP").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("HTTPS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(443);
    format!("{ip}:{port}").parse().expect("Invalid HTTPS address")
}

pub fn http_redirect_addr() -> SocketAddr {
    let ip = std::env::var("IP").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    format!("{ip}:{port}").parse().expect("Invalid HTTP redirect address")
}

pub async fn run_http_redirect_server(bind_addr: SocketAddr, https_port: u16) {
    let app = axum::Router::new().fallback(move |headers: axum::http::HeaderMap, uri: axum::http::Uri| async move {
        let host_header = headers
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("localhost");
        let host_str = host_header.split(':').next().unwrap_or(host_header);
        let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
        let redirect_url = if https_port == 443 {
            format!("https://{host_str}{path_and_query}")
        } else {
            format!("https://{host_str}:{https_port}{path_and_query}")
        };
        axum::response::Redirect::permanent(&redirect_url)
    });

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .expect("Failed to bind HTTP redirect server");
    tracing::info!("HTTP redirect server listening on {}", bind_addr);
    axum::serve(listener, app).await.expect("HTTP redirect server failed");
}

pub async fn serve_manual_tls(
    router: axum::Router,
    cert_path: &str,
    key_path: &str,
) {
    let https_addr = https_addr();
    let http_addr = http_redirect_addr();

    let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .expect("Failed to load TLS certificate/key");

    tracing::info!("HTTPS server listening on {}", https_addr);
    tokio::spawn(run_http_redirect_server(http_addr, https_addr.port()));

    axum_server::bind_rustls(https_addr, config)
        .serve(router.into_make_service())
        .await
        .expect("HTTPS server failed");
}

pub async fn serve_acme(
    router: axum::Router,
    domains: Vec<String>,
    email: Option<String>,
    cache_dir: String,
    production: bool,
) {
    use rustls_acme::caches::DirCache;
    use rustls_acme::AcmeConfig;
    use tokio_stream::StreamExt;

    let https_addr = https_addr();
    let http_addr = http_redirect_addr();

    let mut config = AcmeConfig::new(domains)
        .cache(DirCache::new(cache_dir))
        .directory_lets_encrypt(production);

    if let Some(email) = email {
        config = config.contact_push(format!("mailto:{email}"));
    }

    let mut state = config.state();
    let acceptor = state.axum_acceptor(state.default_rustls_config());

    tokio::spawn(async move {
        loop {
            match state.next().await {
                Some(Ok(ok)) => tracing::info!("ACME event: {:?}", ok),
                Some(Err(err)) => tracing::error!("ACME error: {:?}", err),
                None => break,
            }
        }
    });

    tracing::info!(
        "HTTPS server (ACME) listening on {} ({})",
        https_addr,
        if production { "production" } else { "staging" }
    );
    tokio::spawn(run_http_redirect_server(http_addr, https_addr.port()));

    axum_server::bind(https_addr)
        .acceptor(acceptor)
        .serve(router.into_make_service())
        .await
        .expect("HTTPS ACME server failed");
}
