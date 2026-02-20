use crate::models::Device;
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

#[cfg(not(feature = "ssr"))]
use leptos::task::spawn_local;
#[cfg(not(feature = "ssr"))]
use leptos::{leptos_dom::logging::console_error, prelude::*};

#[cfg(feature = "ssr")]
fn spawn_local<F: Future>(_: F) { }
#[cfg(feature = "ssr")]
fn console_error(_: &str) { }

#[cfg(feature = "ssr")]
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Title text="TRMNL Server"/>
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=path!("") view=HomePage />
                    <Route path=path!("about") view=AboutPage />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let (devices, set_devices) = signal::<Vec<Device>>(vec![]);

    spawn_local(async move {
        console_error("getting devices");

        match get_server_info().await {
            Ok(devices) => {
                set_devices.set(devices);
            }
            Err(e) => {
                console_error(format!("Unable to get devices: {:?}", e).as_str());
            }
        }
    });

    view! {
        <h1>"TRMNL Server"</h1>
        <p>"E-ink display server with web dashboard."</p>
        {move || devices.get().iter().map(|n| { n.mac_address.clone()}).into_iter().collect::<Vec<_>>()}

    }
}

#[component]
fn AboutPage() -> impl IntoView {
    view! {
        <h1>"About"</h1>
        <p>"TRMNL e-ink device server built with Rust, Axum, and Leptos."</p>
    }
}

#[server(prefix = "/leptos-api")]
pub async fn get_server_info() -> Result<Vec<Device>, ServerFnError> {
    use crate::state::AppState;

    let db = leptos::context::use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("Database not available"))?;

    Ok(db
        .get_all_devices()
        .await?
        .into_iter()
        .map(|x| x.into())
        .collect())
}
