use dioxus::prelude::*;

#[component]
pub fn ApiInfo() -> Element {
    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "API Reference" }
            p { class: "text-gray-500 mt-1", "Device API endpoints for TRMNL e-ink displays" }
        }

        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6",
            h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-6", "Endpoints" }
            div { class: "space-y-4",
                Endpoint {
                    method: "GET",
                    path: "/api/display",
                    description: "Fetch the next screen for the device to display. Returns image URL, refresh rate, and firmware update status.",
                    headers: vec!["Access-Token (required)", "Battery-Voltage", "FW-Version", "RSSI", "Width", "Height"],
                }
                Endpoint {
                    method: "GET",
                    path: "/api/display/current",
                    description: "Fetch the current/latest screen. Returns image URL with rendered timestamp.",
                    headers: vec!["Access-Token (required)", "Width", "Height", "FW-Version"],
                }
                Endpoint {
                    method: "POST",
                    path: "/api/log",
                    description: "Submit device logs. Accepts a JSON body with a logs[] array.",
                    headers: vec!["Access-Token (required)"],
                }
                Endpoint {
                    method: "GET",
                    path: "/api/setup",
                    description: "Device setup and registration. Returns API key and friendly device ID.",
                    headers: vec!["ID (required, MAC address)", "Model (required)", "Width", "Height", "FW-Version"],
                }
                Endpoint {
                    method: "GET",
                    path: "/render/screen.bmp",
                    description: "Render and return the BMP image for display. Returns 1-bit monochrome BMP.",
                    headers: vec![],
                }
                Endpoint {
                    method: "GET",
                    path: "/metrics",
                    description: "Prometheus metrics endpoint for monitoring.",
                    headers: vec![],
                }
            }
        }
    }
}

#[component]
fn Endpoint(
    method: &'static str,
    path: &'static str,
    description: &'static str,
    headers: Vec<&'static str>,
) -> Element {
    let method_classes = match method {
        "POST" => "bg-blue-50 text-blue-700 ring-blue-600/20",
        _ => "bg-emerald-50 text-emerald-700 ring-emerald-600/20",
    };

    rsx! {
        div { class: "border border-gray-100 rounded-lg p-4 hover:border-gray-200 transition-colors",
            div { class: "flex items-center gap-3",
                span { class: "inline-flex items-center px-2 py-0.5 rounded text-xs font-bold font-mono ring-1 ring-inset {method_classes}",
                    "{method}"
                }
                code { class: "text-sm font-semibold text-gray-800", "{path}" }
            }
            p { class: "text-sm text-gray-500 mt-2 leading-relaxed", "{description}" }
            if !headers.is_empty() {
                div { class: "flex flex-wrap gap-1.5 mt-3",
                    for header in headers {
                        span { class: "inline-flex items-center px-2 py-0.5 rounded bg-gray-50 text-gray-600 text-xs font-mono ring-1 ring-inset ring-gray-200",
                            "{header}"
                        }
                    }
                }
            }
        }
    }
}
