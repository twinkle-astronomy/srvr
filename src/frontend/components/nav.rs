use dioxus::prelude::*;

use crate::frontend::Route;

#[component]
pub fn Nav() -> Element {
    rsx! {
        nav { class: "bg-gray-950 sticky top-0 z-50 border-b border-gray-800",
            div { class: "max-w-full mx-auto px-4 sm:px-6 lg:px-8 flex items-center h-14 gap-8",
                Link {
                    class: "text-white font-bold text-lg tracking-tight hover:text-gray-300 transition-colors",
                    to: Route::Dashboard {},
                    "TRMNL"
                }
                div { class: "flex items-center gap-1",
                    NavLink { to: Route::Dashboard {}, label: "Dashboard" }
                    NavLink { to: Route::ApiInfo {}, label: "API" }
                    NavLink { to: Route::Devices {}, label: "Devices" }
                    NavLink { to: Route::TemplateEditor {}, label: "Template" }
                }
            }
        }
    }
}

#[component]
fn NavLink(to: Route, label: &'static str) -> Element {
    rsx! {
        Link {
            class: "text-gray-400 hover:text-white hover:bg-white/10 px-3 py-1.5 rounded-md text-sm font-medium transition-colors",
            to: to,
            "{label}"
        }
    }
}
