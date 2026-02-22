use dioxus::prelude::*;

use crate::frontend::server_fns::get_server_info;

#[component]
pub fn Dashboard() -> Element {
    let server_info = use_server_future(move || get_server_info())?;

    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Dashboard" }
            p { class: "text-gray-500 mt-1", "TRMNL eink device server overview" }
        }

        div { class: "grid grid-cols-1 md:grid-cols-2 gap-6",
            // Server info card
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6",
                h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4", "Server Info" }
                match server_info() {
                    Some(Ok(info)) => rsx! {
                        div { class: "space-y-3",
                            InfoRow { label: "Status",
                                div { class: "flex items-center gap-2",
                                    span { class: "w-2 h-2 rounded-full bg-emerald-500" }
                                    span { class: "text-sm text-gray-700", "Running" }
                                }
                            }
                            InfoRow { label: "Time",
                                span { class: "text-sm text-gray-700 font-mono", "{info.time}" }
                            }
                            InfoRow { label: "Date",
                                span { class: "text-sm text-gray-700 font-mono", "{info.date}" }
                            }
                            InfoRow { label: "Port",
                                span { class: "text-sm text-gray-700 font-mono", "{info.port}" }
                            }
                            InfoRow { label: "Prometheus",
                                span { class: "text-sm text-gray-700 font-mono text-xs", "{info.prometheus_url}" }
                            }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        p { class: "text-red-400 text-sm", "Error: {e}" }
                    },
                    None => rsx! { LoadingSpinner {} },
                }
            }
        }
    }
}

#[component]
fn InfoRow(label: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "flex items-center justify-between",
            span { class: "text-sm font-medium text-gray-500", "{label}" }
            {children}
        }
    }
}

#[component]
fn LoadingSpinner() -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center py-12 gap-3",
            div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
            p { class: "text-sm text-gray-400", "Loading..." }
        }
    }
}
