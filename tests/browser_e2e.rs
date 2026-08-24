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

/// Upload a firmware release the way the admin UI does (multipart), return its id.
async fn seed_firmware_release(base: &str, cookie: &str, model: &str, version: &str) -> i64 {
    let form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .text("version", version.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"fake-firmware-bytes".to_vec())
                .file_name("fw.bin")
                .mime_str("application/octet-stream")
                .unwrap(),
        );
    let resp = http()
        .post(format!("{base}/dashboard/firmware"))
        .header("Cookie", cookie)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "upload firmware: {}", resp.status());
    resp.json::<Value>().await.unwrap()["id"].as_i64().unwrap()
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

/// Wait until the element matching `css`'s live `value` DOM property equals
/// `expected`. `wait_for_text` doesn't work here: an `<input>`'s current
/// value is a DOM property, not text content, so XPath text matching never
/// sees it.
async fn wait_for_input_value(c: &Client, css: &str, expected: &str) -> R {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(el) = c.find(Locator::Css(css)).await {
            if el.prop("value").await?.as_deref() == Some(expected) {
                return Ok(());
            }
        }
        if Instant::now() > deadline {
            return Err(format!("{css} never reached value {expected:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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

/// Polls until the device page's preview `<img>` has a `src`, then returns it.
/// The preview is fetched after mount, so the element exists before its src does.
async fn wait_for_preview_src(c: &Client) -> Result<String, Box<dyn std::error::Error>> {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        if let Ok(img) = c.find(Locator::Css("img[alt='Screen preview']")).await {
            if let Ok(Some(src)) = img.attr("src").await {
                if !src.is_empty() {
                    return Ok(src);
                }
            }
        }
        if std::time::Instant::now() > deadline {
            return Err("timed out waiting for the screen preview to load".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
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

#[tokio::test]
async fn activating_a_firmware_release_shows_the_active_badge() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;
    let v1 = seed_firmware_release(&server.base, &cookie, "og_plus", "1.0.0").await;
    let _v2 = seed_firmware_release(&server.base, &cookie, "og_plus", "1.1.0").await;

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/firmware", server.base)).await?;

        // Both uploaded releases are listed, grouped under their model.
        wait_for_text(&c, "og_plus").await?;
        wait_for_text(&c, "1.0.0").await?;
        wait_for_text(&c, "1.1.0").await?;
        // Exact-text match: contains() would also hit the "Activate" buttons.
        assert!(
            c.find_all(Locator::XPath("//span[normalize-space(.)='Active']")).await?.is_empty(),
            "neither release should be active before either is activated"
        );

        // Activate the first release from the UI.
        c.find(Locator::XPath(
            "//p[contains(., '1.0.0')]/ancestor::div[contains(@class, 'flex items-center justify-between')][1]//button[contains(., 'Activate')]",
        ))
        .await?
        .click()
        .await?;
        // Wait for the badge span by exact text — wait_for_text("Active")
        // would match the sibling "Activate" button as a substring and
        // return before the activate round-trip finishes.
        c.wait()
            .at_most(WAIT)
            .for_element(Locator::XPath("//span[normalize-space(.)='Active']"))
            .await?;

        // Persisted server-side, and the DB-level "one active per model" constraint holds.
        let active = get_json(&server.base, &cookie, "/dashboard/firmware").await;
        let active_release = active
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"].as_i64() == Some(v1))
            .unwrap();
        assert_eq!(active_release["active"], json!(true));

        // The now-active release's Delete button is disabled client-side
        // (the plan calls for this: "Delete (disabled if active)") — the
        // server's 409-on-delete-active-release is defense in depth behind
        // a control the UI never lets a user actually press.
        let delete_btn = c
            .find(Locator::XPath(
                "//p[contains(., '1.0.0')]/ancestor::div[contains(@class, 'flex items-center justify-between')][1]//button[contains(., 'Delete')]",
            ))
            .await?;
        assert!(
            delete_btn.attr("disabled").await?.is_some(),
            "Delete must be disabled for the active release"
        );
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn enabling_firmware_updates_on_a_device_persists() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;
    seed_device(&server.base, "AA:BB:CC:DD:EE:02").await;

    let devices = get_json(&server.base, &cookie, "/dashboard/devices").await;
    let device_id = devices[0]["id"].as_i64().unwrap();
    assert_eq!(
        devices[0]["firmware_updates_enabled"],
        json!(false),
        "should default off"
    );

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/devices/{}", server.base, device_id)).await?;
        wait_for_text(&c, "Firmware Updates").await?;

        // The checkbox itself is `sr-only` (visually hidden); a real user
        // clicks the wrapping `<label>` (the visible pill), which forwards
        // the click to its associated `<input>` per normal HTML semantics.
        c.find(Locator::XPath(
            "//h2[contains(., 'Firmware Updates')]/following-sibling::div[1]//label",
        ))
        .await?
        .click()
        .await?;
        wait_for_text(&c, "Saved!").await?;

        let device = get_json(&server.base, &cookie, &format!("/dashboard/devices/{device_id}")).await;
        assert_eq!(device["firmware_updates_enabled"], json!(true));
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

/// Counts requests the page has made to `needle`, via the browser's own
/// Resource Timing buffer. Used to assert a bounded number of fetches, which
/// is the only way to catch a reactive-loop regression from the outside: the
/// symptom is unbounded *repetition* of a request that is individually correct.
///
/// Count broadly. An earlier version watched only `/dashboard/preview` and
/// would have undercounted a slower loop: the switch effect fires three
/// sequential fetches and superseded iterations bail out after the first, so
/// the flood lands on the *earliest* call in the chain, not the one the
/// feature is "about". (That version did still fail on the real bug — but
/// via a browser-tab crash, i.e. incidentally, not by its own assertion.)
async fn resource_request_count(c: &Client, needle: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let script = format!(
        "return window.performance.getEntriesByType('resource') \
         .filter(function (e) {{ return e.name.indexOf('{needle}') !== -1; }}).length;"
    );
    let v = c.execute(&script, vec![]).await?;
    Ok(v.as_u64().unwrap_or(0))
}

/// Names the busiest `/dashboard/*` path since the buffer was last cleared —
/// so a failure says *which* request is looping instead of just "too many".
async fn busiest_dashboard_path(c: &Client) -> Result<String, Box<dyn std::error::Error>> {
    let script = "\
        var counts = {}; \
        window.performance.getEntriesByType('resource').forEach(function (e) { \
            var i = e.name.indexOf('/dashboard/'); \
            if (i === -1) return; \
            var p = e.name.slice(i).split('?')[0]; \
            counts[p] = (counts[p] || 0) + 1; \
        }); \
        var out = Object.keys(counts).map(function (k) { return k + '=' + counts[k]; }); \
        out.sort(function (a, b) { \
            return parseInt(b.split('=')[1]) - parseInt(a.split('=')[1]); \
        }); \
        return out.slice(0, 5).join(', ');";
    let v = c.execute(script, vec![]).await?;
    Ok(v.as_str().unwrap_or("").to_string())
}

#[tokio::test]
async fn switching_preview_device_does_not_loop_requests() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;

    // Two real devices, mirroring the reported setup: a 1-bit one and a 2-bit
    // one, so the switch is between two real devices and crosses a mode
    // boundary — not just Virtual -> device.
    seed_device(&server.base, "AA:BB:CC:DD:EE:05").await;
    seed_device(&server.base, "AA:BB:CC:DD:EE:06").await;
    let template_id = seed_template(
        &server.base,
        &cookie,
        "loop-regression",
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480"></svg>"#,
    )
    .await;

    let devices = get_json(&server.base, &cookie, "/dashboard/devices").await;
    assert_eq!(devices.as_array().unwrap().len(), 2, "need two devices");
    let grayscale_id = devices[1]["id"].as_i64().unwrap();
    http()
        .post(format!("{}/dashboard/devices/{grayscale_id}/grayscale", server.base))
        .header("cookie", &cookie)
        .json(&json!({ "enabled": true }))
        .send()
        .await
        .unwrap();

    // Essential to the repro: without a key the page renders *only* the
    // header, hiding the chat column and preview panel behind the `has_key`
    // gate. A real user has a key, so the full page — and every effect and
    // signal read in it — is live. The key is never called here; only its
    // presence is checked, to unlock the rest of the UI.
    let resp = http()
        .put(format!("{}/dashboard/claude-api-key", server.base))
        .header("cookie", &cookie)
        .json(&json!({ "key": "sk-ant-e2e-not-a-real-key" }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "seeding claude key: {}", resp.status());

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/template/{}/generate", server.base, template_id))
            .await?;
        c.wait()
            .at_most(WAIT)
            .for_element(Locator::XPath("//select//option[contains(., '2-bit')]"))
            .await?;
        // Guard the setup itself. Without these the test can silently decay
        // into exercising only the page header — the first version ran with
        // the key gate closed, so it never rendered the chat column or
        // preview panel the loop actually lives alongside.
        let gated = c
            .execute(
                "return document.body.innerText.indexOf('No Claude API key configured') !== -1;",
                vec![],
            )
            .await?;
        assert_eq!(
            gated.as_bool(),
            Some(false),
            "the Claude-key gate must be open, or the chat column and preview panel \
             never render and the loop has nothing to run in"
        );
        let n_options = c
            .execute("return document.querySelectorAll('select option').length;", vec![])
            .await?;
        assert_eq!(
            n_options.as_u64(),
            Some(3),
            "expected Virtual + two seeded devices in the preview selector"
        );

        // Settle the initial page load, then measure only what the switch
        // itself causes.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        c.execute("window.performance.clearResourceTimings();", vec![]).await?;

        // Options are [Virtual, device0 (1-bit), device1 (2-bit)]. Select the
        // 1-bit device first, so the page sits on a real 1-bit device exactly
        // as reported, then switch to the 2-bit one.
        c.find(Locator::Css("select")).await?.select_by_value("1").await?;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        c.execute("window.performance.clearResourceTimings();", vec![]).await?;

        c.find(Locator::Css("select")).await?.select_by_value("2").await?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Count *all* dashboard traffic: a switch costs a render-context, a
        // context, and a preview — call it under a dozen with retries. A loop
        // produces hundreds.
        let fired = resource_request_count(&c, "/dashboard/").await?;
        let breakdown = busiest_dashboard_path(&c).await?;
        assert!(
            fired <= 12,
            "switching preview device should cost a handful of requests, got {fired} in 3s \
             — a reactive loop is re-running the switch effect. Busiest paths: {breakdown}"
        );
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn device_page_preview_renders_in_the_devices_configured_mode() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;
    seed_device(&server.base, "AA:BB:CC:DD:EE:04").await;

    let devices = get_json(&server.base, &cookie, "/dashboard/devices").await;
    let device_id = devices[0]["id"].as_i64().unwrap();

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/devices/{}", server.base, device_id)).await?;
        wait_for_text(&c, "Screen Preview").await?;

        // Previews are always PNG now, in both modes — the client hardcodes
        // the mime, so a server that went back to BMP would render a broken
        // image here rather than failing any unit test.
        let src = wait_for_preview_src(&c).await?;
        assert!(
            src.starts_with("data:image/png;base64,"),
            "preview should be served as a PNG data URL, got: {}",
            &src[..src.len().min(40)]
        );

        // Flip the device to 2-bit and confirm the preview actually changes —
        // the pixels differ because the grayscale path stops thresholding.
        let before = src;
        c.find(Locator::XPath(
            "//h2[contains(., '2-bit Grayscale')]/following-sibling::div[1]//label",
        ))
        .await?
        .click()
        .await?;
        wait_for_text(&c, "Saved!").await?;
        c.refresh().await?;
        wait_for_text(&c, "Screen Preview").await?;

        let after = wait_for_preview_src(&c).await?;
        assert!(
            after.starts_with("data:image/png;base64,"),
            "grayscale preview should still be a PNG data URL"
        );
        assert_ne!(
            before, after,
            "enabling 2-bit grayscale must change what the preview shows"
        );
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn enabling_2bit_grayscale_on_a_device_persists() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;
    seed_device(&server.base, "AA:BB:CC:DD:EE:03").await;

    let devices = get_json(&server.base, &cookie, "/dashboard/devices").await;
    let device_id = devices[0]["id"].as_i64().unwrap();
    assert_eq!(
        devices[0]["supports_2bit_grayscale"],
        json!(false),
        "should default off"
    );

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/devices/{}", server.base, device_id)).await?;
        wait_for_text(&c, "2-bit Grayscale").await?;

        c.find(Locator::XPath(
            "//h2[contains(., '2-bit Grayscale')]/following-sibling::div[1]//label",
        ))
        .await?
        .click()
        .await?;
        wait_for_text(&c, "Saved!").await?;

        let device = get_json(&server.base, &cookie, &format!("/dashboard/devices/{device_id}")).await;
        assert_eq!(device["supports_2bit_grayscale"], json!(true));
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn selecting_a_firmware_binary_prefills_the_embedded_version() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    let cookie = seed_admin(&server.base, "admin", "hunter2").await;

    // A synthetic ESP-IDF app image matching what parse_esp_app_version
    // expects (src/frontend/pages/firmware.rs): esp_image_header_t (24B) +
    // esp_image_segment_header_t (8B) = 32B, then esp_app_desc_t with its
    // real magic word and an embedded version string at struct offset 16.
    let mut image = vec![0u8; 32 + 256];
    image[0] = 0xE9; // ESP_IMAGE_HEADER_MAGIC
    image[32..36].copy_from_slice(&0xABCD5432u32.to_le_bytes()); // ESP_APP_DESC_MAGIC_WORD
    let version = b"9.9.9-e2e";
    image[48..48 + version.len()].copy_from_slice(version);
    let image_bytes: Vec<Value> = image.iter().map(|b| json!(b)).collect();

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/firmware", server.base)).await?;
        c.wait().at_most(WAIT).for_element(Locator::Css("#fw_file")).await?;

        // Headless Chrome has no OS file dialog, and the `chrome` container
        // shares no filesystem with this test process (see
        // docker-compose.yml — only `srvr` mounts `.:/app`), so a real
        // `send_keys(path)` upload isn't reachable from here. Synthesize the
        // File entirely in-browser via DataTransfer instead: this drives the
        // exact same `onchange` -> FormEvent::files() -> read_bytes() path a
        // real file-picker selection would, just without a real OS dialog.
        c.execute(
            r#"
            const bytes = new Uint8Array(arguments[0]);
            const file = new File([bytes], "fw.bin", {type: "application/octet-stream"});
            const dt = new DataTransfer();
            dt.items.add(file);
            const input = document.getElementById("fw_file");
            input.files = dt.files;
            input.dispatchEvent(new Event("change", {bubbles: true}));
            "#,
            vec![Value::Array(image_bytes)],
        )
        .await?;

        // The version field should auto-populate from the embedded esp_app_desc_t.
        wait_for_input_value(&c, "#fw_version", "9.9.9-e2e").await?;

        // It's still a normal editable field (manual fallback) — prove that
        // by overriding it, then submit and confirm the *typed* value wins.
        let version_field = c.find(Locator::Css("#fw_version")).await?;
        version_field.clear().await?;
        version_field.send_keys("9.9.9-manual-override").await?;
        c.find(Locator::Css("#fw_model")).await?.send_keys("e2e-esp-model").await?;
        c.find(Locator::XPath("//button[contains(., 'Upload')]")).await?.click().await?;
        wait_for_text(&c, "e2e-esp-model").await?;

        // A successful upload must fully reset the form — including the
        // uncontrolled file input (remounted via a key bump), which would
        // otherwise keep displaying the old file while selected_file is
        // None, making the next submit fail confusingly.
        wait_for_input_value(&c, "#fw_version", "").await?;
        wait_for_input_value(&c, "#fw_file", "").await?;

        let releases = get_json(&server.base, &cookie, "/dashboard/firmware").await;
        let uploaded = releases
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["model"] == json!("e2e-esp-model"))
            .expect("uploaded release should be listed");
        assert_eq!(
            uploaded["version"],
            json!("9.9.9-manual-override"),
            "manually overriding the pre-filled version must win"
        );
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn selecting_a_multi_segment_firmware_binary_still_detects_the_version() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    seed_admin(&server.base, "admin", "hunter2").await;

    // First 192 bytes of a real device's firmware (`firmware-0.2.1.bin`,
    // ESP32-S3 + Rust/embassy). Regression fixture: this toolchain inserts
    // an extra 24-byte leading segment before esp_app_desc_t, landing the
    // descriptor at file offset 64 rather than the 32 a "fixed offset"
    // assumption predicts — parse_esp_app_version originally missed this
    // and silently left the version field empty. Same bytes as
    // REAL_ESP32S3_EMBASSY_HEADER in src/frontend/pages/firmware.rs.
    #[rustfmt::skip]
    let image: Vec<u8> = vec![
        0xe9, 0x07, 0x02, 0x20, 0x14, 0x88, 0x37, 0x40, 0xee, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00,
        0x00, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xf8, 0xfc, 0xc8, 0x3f, 0x18, 0x00, 0x00, 0x00,
        0x0a, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00,
        0x05, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00, 0x00, 0x40, 0x00, 0x0d, 0x3c, 0xcc, 0x47, 0x04, 0x00,
        0x32, 0x54, 0xcd, 0xab, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x30, 0x2e, 0x32, 0x2e, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x65, 0x73, 0x70, 0x33, 0x32, 0x73, 0x33, 0x2d, 0x65, 0x6d, 0x62, 0x61, 0x73, 0x73, 0x79, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x30, 0x30, 0x3a, 0x30, 0x30, 0x3a, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x4a, 0x61, 0x6e, 0x20, 0x20, 0x31, 0x20, 0x32, 0x30, 0x32, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x76, 0x35, 0x2e, 0x35, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let image_bytes: Vec<Value> = image.iter().map(|b| json!(b)).collect();

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/firmware", server.base)).await?;
        c.wait().at_most(WAIT).for_element(Locator::Css("#fw_file")).await?;

        c.execute(
            r#"
            const bytes = new Uint8Array(arguments[0]);
            const file = new File([bytes], "firmware-0.2.1.bin", {type: "application/octet-stream"});
            const dt = new DataTransfer();
            dt.items.add(file);
            const input = document.getElementById("fw_file");
            input.files = dt.files;
            input.dispatchEvent(new Event("change", {bubbles: true}));
            "#,
            vec![Value::Array(image_bytes)],
        )
        .await?;

        wait_for_input_value(&c, "#fw_version", "0.2.1").await?;
        // Detection is reported explicitly, not a silent side effect.
        wait_for_text(&c, "Detected version").await?;
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}

#[tokio::test]
async fn selecting_a_non_esp_file_reports_detection_failure_instead_of_silence() -> R {
    let endpoint = webdriver_endpoint().expect("getting webdriver endpoint");
    let _guard = test_lock().lock().await;
    let server = Server::start().await;
    seed_admin(&server.base, "admin", "hunter2").await;

    let c = browser(&endpoint).await;
    let result = async {
        login(&c, &server.base, "admin", "hunter2").await?;
        c.goto(&format!("{}/firmware", server.base)).await?;
        c.wait().at_most(WAIT).for_element(Locator::Css("#fw_file")).await?;

        // 256 zero bytes: not an ESP image at all (wrong magic byte).
        c.execute(
            r#"
            const bytes = new Uint8Array(256);
            const file = new File([bytes], "not-firmware.bin", {type: "application/octet-stream"});
            const dt = new DataTransfer();
            dt.items.add(file);
            const input = document.getElementById("fw_file");
            input.files = dt.files;
            input.dispatchEvent(new Event("change", {bubbles: true}));
            "#,
            vec![],
        )
        .await?;

        // Avoid an apostrophe in the needle — wait_for_text builds a naive
        // single-quoted XPath literal that can't contain one.
        wait_for_text(&c, "auto-detect a version").await?;
        // And the version field must still be empty and freely typeable —
        // detection failing must not block the manual fallback.
        let version_field = c.find(Locator::Css("#fw_version")).await?;
        assert_eq!(version_field.prop("value").await?.as_deref(), Some(""));
        version_field.send_keys("7.0.0").await?;
        assert_eq!(version_field.prop("value").await?.as_deref(), Some("7.0.0"));
        Ok(())
    }
    .await;
    let _ = c.close().await;
    result
}
