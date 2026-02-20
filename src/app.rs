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
fn spawn_local<F: Future>(_: F) {}
#[cfg(feature = "ssr")]
fn console_error(_: &str) {}

#[cfg(feature = "ssr")]
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="stylesheet" id="leptos" href="/pkg/srvr.css"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body class="bg-gray-50 text-gray-900 min-h-screen">
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
        <div class="max-w-4xl mx-auto px-6 py-12">
            <header class="mb-10">
                <h1 class="text-3xl font-bold tracking-tight">"TRMNL Server"</h1>
                <p class="mt-2 text-gray-500">"E-ink display server with web dashboard."</p>
            </header>

            <section>
                <h2 class="text-lg font-semibold mb-4">"Devices"</h2>
                <div class="grid gap-4 sm:grid-cols-2">
                    {move || devices.get().iter().map(|d| {
                        let name = d.friendly_name.clone()
                            .unwrap_or_else(|| "Unnamed device".to_string());
                        let fw = d.firmware_version.clone()
                            .unwrap_or_else(|| "unknown".to_string());
                        view! {
                            <div class="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
                                <div class="flex items-center justify-between mb-3">
                                    <span class="font-medium">{name}</span>
                                    <span class="text-xs text-gray-400 font-mono">{d.mac_address.clone()}</span>
                                </div>
                                <div class="flex items-center gap-3 text-sm text-gray-500">
                                    <span class="inline-flex items-center rounded-full bg-green-50 px-2 py-0.5 text-xs font-medium text-green-700">"Online"</span>
                                    <span>"fw "{fw}</span>
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </section>
        </div>
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
