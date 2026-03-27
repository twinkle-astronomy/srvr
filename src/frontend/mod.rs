mod components;
mod pages;
pub mod server_fns;

use dioxus::prelude::*;

use pages::{Dashboard, DeviceDetail, Devices, Login, Setup, TemplateEditor, Templates, Users};
use server_fns::{check_auth, check_needs_setup};

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
    let auth = use_server_future(move || check_auth())?;
    let needs_setup = use_server_future(move || check_needs_setup())?;
    let nav = navigator();

    match (needs_setup(), auth()) {
        (Some(Ok(true)), _) => {
            nav.push(Route::Setup {});
            return rsx! {
                p { class: "text-gray-400 text-center mt-20", "Redirecting to setup..." }
            };
        }
        (_, Some(Ok(None))) => {
            nav.push(Route::Login {});
            return rsx! {
                p { class: "text-gray-400 text-center mt-20", "Redirecting to login..." }
            };
        }
        (_, Some(Ok(Some(_)))) => {}
        (Some(Err(e)), _) | (_, Some(Err(e))) => {
            return rsx! {
                p { class: "text-red-400 text-center mt-20", "Error: {e}" }
            };
        }
        _ => {
            return rsx! {
                div { class: "flex flex-col items-center justify-center py-32 gap-3",
                    div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                    p { class: "text-sm text-gray-400", "Loading..." }
                }
            };
        }
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
