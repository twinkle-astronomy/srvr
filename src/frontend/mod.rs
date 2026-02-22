mod pages;
mod components;
pub mod server_fns;

use dioxus::prelude::*;

use pages::{ApiInfo, Dashboard, Devices, TemplateEditor};

#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
enum Route {
    #[layout(NavLayout)]
        #[route("/")]
        Dashboard {},
        #[route("/api-info")]
        ApiInfo {},
        #[route("/devices")]
        Devices {},
        #[route("/template")]
        TemplateEditor {},
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
