use dioxus::prelude::*;

use crate::frontend::server_fns::{get_screen_preview, get_server_info, get_temperature};

#[component]
pub fn Dashboard() -> Element {
    let mut refresh_count = use_signal(|| 0u32);

    let temperature = use_server_future(move || {
        let _ = refresh_count();
        get_temperature()
    })?;

    let server_info = use_server_future(move || {
        let _ = refresh_count();
        get_server_info()
    })?;

    let screen = use_server_future(move || {
        let _ = refresh_count();
        get_screen_preview(800, 480)
    })?;

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

            // Temperature card
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6 flex flex-col items-center justify-center",
                h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4 self-start", "Front Porch Temperature" }
                match temperature() {
                    Some(Ok(Some(t))) => rsx! {
                        div { class: "flex items-baseline gap-1 py-6",
                            span { class: "text-6xl font-bold text-gray-900 tabular-nums", "{t:.1}" }
                            span { class: "text-2xl font-light text-gray-400", "\u{00b0}F" }
                        }
                    },
                    Some(Ok(None)) => rsx! {
                        div { class: "py-8",
                            span { class: "text-4xl text-gray-300 font-light", "N/A" }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        p { class: "text-red-400 text-sm py-8", "Error: {e}" }
                    },
                    None => rsx! { LoadingSpinner {} },
                }
            }

            // Screen preview card (full width)
            div { class: "md:col-span-2 bg-white rounded-xl shadow-sm border border-gray-100 p-6",
                div { class: "flex items-center justify-between mb-4",
                    h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider", "Screen Preview" }
                    button {
                        class: "inline-flex items-center gap-2 px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                        onclick: move |_| refresh_count += 1,
                        "Refresh"
                    }
                }
                div { class: "bg-gray-50 rounded-lg p-4 flex items-center justify-center",
                    match screen() {
                        Some(Ok(Some(b64))) => rsx! {
                            img {
                                src: "data:image/bmp;base64,{b64}",
                                alt: "Current e-ink screen",
                                class: "max-w-full h-auto border border-gray-200 rounded",
                                style: "image-rendering: pixelated;",
                            }
                        },
                        Some(Ok(None)) => rsx! {
                            div { class: "py-16 text-gray-400 text-sm", "Unable to render screen preview" }
                        },
                        Some(Err(e)) => rsx! {
                            div { class: "py-16 text-red-400 text-sm", "Error: {e}" }
                        },
                        None => rsx! { LoadingSpinner {} },
                    }
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
