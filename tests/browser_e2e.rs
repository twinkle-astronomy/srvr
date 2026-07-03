//! Browser end-to-end tests: drive the **real WASM dashboard** in headless
//! Chromium against a **real server** (the `srvr` binary + a throwaway SQLite
//! DB), exercising the same user journeys through the actual UI —
//! logging in, changing a device setting, editing a template, changing a
//! password, and adding a user.
//!
//! These are the payoff of the CSR + JSON-API conversion: with forced hydration
//! gone, the client-rendered app boots in an empty page, so a browser can drive
//! it directly.
//!
//! ## How they run
//!
//! The browser lives in the docker-compose `chrome` service (headless Chromium
//! behind chromedriver); nothing browser-related is installed in the dev image.
//! The `srvr` service sets `WEBDRIVER_URL=http://chrome:4444`, so these run as
//! part of the normal `cargo test --features server` inside the compose
//! environment. The server we spawn binds `0.0.0.0` and advertises this
//! container's network IP so the browser in the other container can reach it.
//!
//! `WEBDRIVER_URL` is **required**: each test fails immediately when it is unset,
//! so a browser tier can't silently stop running. Bring the `chrome` service up
//! before testing (`docker compose up -d chrome`). [`e2e.sh`](../e2e.sh) runs just
//! this crate. The WASM bundle is built on demand (`dx build --platform web`) if
//! missing.

use std::{
    error::Error,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

use fantoccini::{Client, ClientBuilder, Locator};
use serde_json::{Value, json};

type R<T = ()> = Result<T, Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(25);

/// rustls 0.23 requires a process-level `CryptoProvider` to be chosen explicitly
/// when more than one provider is compiled in. Install `ring` once, before any
/// TLS-capable client (reqwest / fantoccini) is constructed.
fn init_crypto() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// A reqwest client with the crypto provider guaranteed installed.
fn http() -> reqwest::Client {
    init_crypto();
    reqwest::Client::new()
}

// ── Shared, lazily-provisioned infrastructure ───────────────────────────────

/// Grab a currently-free TCP port (there's a small TOCTOU window, fine for tests).
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Ensure the WASM bundle exists (build it once if not), return its directory.
fn wasm_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = manifest_dir().join("target/dx/srvr/debug/web/public");
        if !dir.join("index.html").exists() {
            eprintln!("browser_e2e: building WASM bundle (dx build --platform web)…");
            let status = Command::new("dx")
                .args(["build", "--platform", "web"])
                .current_dir(manifest_dir())
                .status()
                .expect("run `dx build` — is the dioxus CLI installed?");
            assert!(status.success(), "dx build failed");
        }
        dir
    })
    .clone()
}

/// The WebDriver endpoint to drive Chromium through, from `WEBDRIVER_URL` — the
/// docker-compose `chrome` service (the `srvr` service sets
/// `WEBDRIVER_URL=http://chrome:4444`). `None` (unset) makes each test fail via
/// `.expect()`: the browser tier is mandatory, not best-effort.
fn webdriver_endpoint() -> Option<String> {
    std::env::var("WEBDRIVER_URL").ok().filter(|u| !u.is_empty())
}

/// This process's primary network IP — the address the browser (running in the
/// `chrome` container) uses to reach the server we spawn. Determined from the OS
/// routing table; no traffic is sent.
fn own_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            Ok(s.local_addr()?.ip().to_string())
        })
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

/// Serialize the browser tests: each drives a full Chromium session, and running
/// five in parallel (cargo's default within a test crate) is needlessly heavy and
/// flaky. Held for the duration of each test.
fn test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// A running `srvr` process with its own throwaway DB. Killed on drop.
struct Server {
    child: Child,
    base: String,
    db_dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.db_dir);
    }
}

impl Server {
    async fn start() -> Server {
        let wasm = wasm_dir();
        let port = free_port();
        let db_dir = std::env::temp_dir().join(format!("trmnl_e2e_{}_{}", std::process::id(), port));
        std::fs::create_dir_all(&db_dir).unwrap();

        let child = Command::new(env!("CARGO_BIN_EXE_srvr"))
            // Bind all interfaces so a browser in another container can reach us.
            .env("IP", "0.0.0.0")
            .env("PORT", port.to_string())
            .env("DATABASE_URL", format!("sqlite:{}/e2e.db", db_dir.display()))
            .env("IMAGE_SIGNATURE_SECRET", "e2e-test")
            .env("DIOXUS_ASSET_DIR", wasm.display().to_string())
            .env("RUST_LOG", std::env::var("E2E_SERVER_LOG").as_deref().unwrap_or("warn"))
            // Quiet by default; set E2E_SERVER_LOG=info to see server logs inline.
            .stdout(Stdio::null())
            .stderr(if std::env::var("E2E_SERVER_LOG").is_ok() {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .spawn()
            .expect("spawn srvr binary");

        let base = format!("http://{}:{}", own_ip(), port);
        wait_http_ok(&format!("{base}/dashboard/needs-setup"), Duration::from_secs(30)).await;
        Server { child, base, db_dir }
    }
}

async fn wait_http_ok(url: &str, timeout: Duration) {
    let client = http();
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    panic!("server did not become healthy at {url} within {timeout:?}");
}

/// Open a fresh headless-Chromium session against the given WebDriver endpoint.
async fn browser(endpoint: &str) -> Client {
    init_crypto();
    let mut caps = serde_json::Map::new();
    caps.insert(
        "goog:chromeOptions".to_string(),
        json!({
            "args": [
                "--headless=new",
                "--no-sandbox",
                "--disable-dev-shm-usage",
                "--disable-gpu",
                "--window-size=1280,1024",
            ]
        }),
    );
    ClientBuilder::rustls()
        .expect("rustls")
        .capabilities(caps)
        .connect(endpoint)
        .await
        .expect("connect to WebDriver")
}

// ── HTTP fixtures (seed state the way the app's own endpoints do) ────────────

/// Create the first admin via the setup endpoint; return the session cookie.
async fn seed_admin(base: &str, user: &str, pass: &str) -> String {
    let resp = http()
        .post(format!("{base}/dashboard/auth/setup"))
        .json(&json!({ "username": user, "password": pass }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "setup failed: {}", resp.status());
    resp.headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_string())
        .expect("setup should set a session cookie")
}

async fn seed_template(base: &str, cookie: &str, name: &str, content: &str) -> i64 {
    let resp = http()
        .post(format!("{base}/dashboard/templates"))
        .header("Cookie", cookie)
        .json(&json!({ "name": name, "content": content }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "create template: {}", resp.status());
    resp.json::<Value>().await.unwrap()["id"].as_i64().unwrap()
}

/// Register a device the way a real device does (unauthenticated `/api/setup`).
async fn seed_device(base: &str, mac: &str) {
    let resp = http()
        .get(format!("{base}/api/setup"))
        .header("Access-Token", "e2e-token")
        .header("ID", mac)
        .header("model", "og_plus")
        .header("Width", "800")
        .header("Height", "480")
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "device setup: {}", resp.status());
}

async fn get_json(base: &str, cookie: &str, path: &str) -> Value {
    http()
        .get(format!("{base}{path}"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()
}

// ── UI helpers ──────────────────────────────────────────────────────────────

/// Drive the login form and wait until the authenticated shell (Nav) appears.
async fn login(c: &Client, base: &str, user: &str, pass: &str) -> R {
    c.goto(&format!("{base}/login")).await?;
    c.wait()
        .at_most(WAIT)
        .for_element(Locator::Css("#username"))
        .await?
        .send_keys(user)
        .await?;
    c.find(Locator::Css("#password")).await?.send_keys(pass).await?;
    c.find(Locator::Css("button[type='submit']")).await?.click().await?;
    // The Nav (with a Logout button) renders only once authenticated.
    c.wait()
        .at_most(WAIT)
        .for_element(Locator::XPath("//button[contains(., 'Logout')]"))
        .await
        .map_err(|e| format!("login as {user:?}: authenticated shell (Logout button) never appeared: {e}"))?;
    Ok(())
}

/// Wait until an element whose visible text contains `needle` exists.
async fn wait_for_text(c: &Client, needle: &str) -> R {
    let xpath = format!("//*[contains(normalize-space(.), '{needle}')]");
    c.wait()
        .at_most(WAIT)
        .for_element(Locator::XPath(&xpath))
        .await
        .map_err(|e| format!("timed out waiting for text {needle:?} to appear: {e}"))?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn logging_in_lands_on_the_dashboard() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    seed_admin(&server.base, "admin", "hunter2").await;

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        // We navigated off /login to the dashboard shell.
        assert!(c.current_url().await?.path() == "/");
        wait_for_text(&c, "Devices").await?; // Nav link present
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn adding_a_user_shows_it_in_the_list() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    seed_admin(&server.base, "admin", "hunter2").await;

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/users", server.base)).await?;

        let new_user = "teammate_e2e";
        c.wait()
            .at_most(WAIT)
            .for_element(Locator::Css("#new_username"))
            .await?
            .send_keys(new_user)
            .await?;
        c.find(Locator::Css("#new_user_password")).await?.send_keys("welcome1").await?;
        c.find(Locator::XPath("//button[contains(., 'Create')]")).await?.click().await?;

        // The new user appears in the list once the store re-fetches.
        wait_for_text(&c, new_user).await?;

        // Submitting the same username again surfaces the server's error
        // message in the banner (not a bare "HTTP 409").
        c.find(Locator::Css("#new_username")).await?.send_keys(new_user).await?;
        c.find(Locator::Css("#new_user_password")).await?.send_keys("welcome1").await?;
        c.find(Locator::XPath("//button[contains(., 'Create')]")).await?.click().await?;
        wait_for_text(&c, "Already exists").await?;
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn changing_your_password_takes_effect() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    seed_admin(&server.base, "admin", "old-pw").await;

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "old-pw").await?;
        c.goto(&format!("{}/users", server.base)).await?;

        c.wait()
            .at_most(WAIT)
            .for_element(Locator::Css("#current_password"))
            .await?
            .send_keys("old-pw")
            .await?;
        c.find(Locator::Css("#new_password")).await?.send_keys("brand-new-pw").await?;
        c.find(Locator::XPath("//button[contains(., 'Change')]")).await?.click().await?;
        wait_for_text(&c, "Password changed successfully").await?;

        // Log out and back in with the new password to prove it took effect.
        c.find(Locator::XPath("//button[contains(., 'Logout')]")).await?.click().await?;
        c.wait().at_most(WAIT).for_element(Locator::Css("#username")).await?;
        login(&c, &server.base, "admin", "brand-new-pw").await?;
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn changing_a_device_template_persists() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;
    seed_device(&server.base, "AA:BB:CC:DD:EE:01").await;
    let alt_template = seed_template(
        &server.base,
        &cookie,
        "Alternate",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"800\" height=\"480\"/>",
    )
    .await;

    // Look up the seeded device's id via the same API the UI uses.
    let devices = get_json(&server.base, &cookie, "/dashboard/devices").await;
    let device_id = devices[0]["id"].as_i64().unwrap();

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/devices/{}", server.base, device_id)).await?;

        // Wait for the template <select> to populate, then pick the alternate.
        c.wait()
            .at_most(WAIT)
            .for_element(Locator::XPath(&format!(
                "//select//option[@value='{alt_template}']"
            )))
            .await?;
        c.find(Locator::Css("select")).await?.select_by_value(&alt_template.to_string()).await?;
        c.find(Locator::XPath("//button[contains(., 'Save')]")).await?.click().await?;
        wait_for_text(&c, "Saved!").await?;

        // The change is persisted server-side.
        let device = get_json(&server.base, &cookie, &format!("/dashboard/devices/{device_id}")).await;
        assert_eq!(device["template_id"].as_i64(), Some(alt_template));
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn editing_a_template_persists() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;
    let template_id = seed_template(
        &server.base,
        &cookie,
        "Editable",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"800\" height=\"480\"></svg>",
    )
    .await;

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/template/{}", server.base, template_id)).await?;

        let editor = c
            .wait()
            .at_most(WAIT)
            .for_element(Locator::Css("textarea"))
            .await?;
        editor.clear().await?;
        editor
            .send_keys("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"800\" height=\"480\"><rect width=\"100%\" height=\"100%\" fill=\"black\"/></svg>")
            .await?;
        // The green button is the template Save (distinct from sub-section saves).
        c.find(Locator::Css("button.bg-green-700")).await?.click().await?;
        wait_for_text(&c, "Saved!").await?;

        // The edit is persisted server-side.
        let t = get_json(&server.base, &cookie, &format!("/dashboard/templates/{template_id}")).await;
        assert!(
            t["content"].as_str().unwrap().contains("fill=\"black\""),
            "edited content should be saved, got: {}",
            t["content"]
        );
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}
