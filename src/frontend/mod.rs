mod components;
mod pages;
pub mod server_fns;
pub mod store;

use dioxus::prelude::*;

use pages::{Dashboard, DeviceDetail, Devices, Login, Setup, TemplateEditor, Templates, Users};
use store::AppStore;

#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
enum Route {
    #[route("/login")]
    Login {},
    #[route("/setup")]
    Setup {},
    #[layout(NavLayout)]
        #[route("/")]
        Dashboard {},
        #[route("/devices")]
        Devices {},
        #[route("/devices/:id")]
        DeviceDetail { id: i64 },
        #[route("/templates")]
        Templates {},
        #[route("/template/:id")]
        TemplateEditor { id: i64 },
        #[route("/users")]
        Users {},
    #[end_layout]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
pub fn App() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
        Router::<Route> {}
    }
}

#[component]
fn NavLayout() -> Element {
    let store = use_context_provider(|| AppStore::new());
    let nav = navigator();

    use_effect(move || {
        spawn(store.fetch_needs_setup());
        spawn(store.fetch_current_user());
        spawn(store.fetch_devices());
        spawn(store.fetch_templates());
        spawn(store.fetch_users());
        spawn(store.fetch_server_info());
    });

    let needs_setup = store.needs_setup;
    let current_user = store.current_user;
    let current_user_loaded = store.current_user_loaded;

    match (needs_setup(), current_user_loaded(), current_user()) {
        // Still fetching auth state
        (None, _, _) | (_, false, _) => {
            return rsx! {
                div { class: "flex flex-col items-center justify-center py-32 gap-3",
                    div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                    p { class: "text-sm text-gray-400", "Loading..." }
                }
            };
        }
        // First-run setup required
        (Some(true), _, _) => {
            nav.push(Route::Setup {});
            return rsx! {
                p { class: "text-gray-400 text-center mt-20", "Redirecting to setup..." }
            };
        }
        // Not authenticated
        (_, true, None) => {
            nav.push(Route::Login {});
            return rsx! {
                p { class: "text-gray-400 text-center mt-20", "Redirecting to login..." }
            };
        }
        // Authenticated — fall through
        _ => {}
    }

    rsx! {
        components::Nav {}
        main { class: "max-w-full mx-auto px-4 sm:px-6 lg:px-8 py-8",
            Outlet::<Route> {}
        }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    let path = segments.join("/");
    rsx! {
        div { class: "flex flex-col items-center justify-center py-32 text-center",
            h1 { class: "text-6xl font-bold text-gray-200 mb-4", "404" }
            p { class: "text-xl text-gray-500 mb-2", "Page not found" }
            p { class: "text-sm text-gray-400", "/{path} does not exist." }
            Link {
                to: Route::Dashboard {},
                class: "mt-8 inline-flex items-center gap-2 px-5 py-2.5 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                "Back to Dashboard"
            }
        }
    }
}
