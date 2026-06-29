use dioxus::prelude::*;

use crate::frontend::store::AppStore;

#[component]
pub fn Dashboard() -> Element {
    let store = use_context::<AppStore>();
    let server_info = store.server_info;

    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Dashboard" }
            p { class: "text-gray-500 mt-1", "TRMNL eink device server overview" }
        }

        div { class: "grid grid-cols-1 md:grid-cols-2 gap-6",
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6",
                h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4", "Server Info" }
                match server_info() {
                    Some(info) => rsx! {
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
                    None => rsx! { LoadingSpinner {} },
                }
            }
        }
    }
}

// Characterization tests for the store-driven Dashboard (no router dependency).
#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::frontend::server_fns::ServerInfo;
    use crate::frontend::store::AppStore;
    use crate::frontend::test_harness::render_with_store;

    fn store_no_info() -> AppStore {
        AppStore::new()
    }

    fn store_with_info() -> AppStore {
        let mut s = AppStore::new();
        s.server_info.set(Some(ServerInfo {
            time: "12:34:56".to_string(),
            date: "2026-06-28".to_string(),
            prometheus_url: "http://prometheus.example".to_string(),
            port: 8080,
        }));
        s
    }

    #[test]
    fn dashboard_shows_spinner_when_server_info_missing() {
        let html = render_with_store(store_no_info, Dashboard);
        assert!(
            html.contains("Loading..."),
            "expected spinner while server info loads, got: {html:?}"
        );
        assert!(
            !html.contains("Running"),
            "must not claim Running before info arrives, got: {html:?}"
        );
    }

    #[test]
    fn dashboard_renders_server_info_when_present() {
        let html = render_with_store(store_with_info, Dashboard);
        assert!(
            html.contains("Running"),
            "expected Running status, got: {html:?}"
        );
        assert!(
            html.contains("12:34:56") && html.contains("8080"),
            "expected time and port rendered, got: {html:?}"
        );
        assert!(
            html.contains("http://prometheus.example"),
            "expected prometheus url rendered, got: {html:?}"
        );
        assert!(
            !html.contains("Loading..."),
            "must not show spinner once info present, got: {html:?}"
        );
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
