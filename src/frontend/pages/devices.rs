use dioxus::prelude::*;

use crate::frontend::server_fns::{
    delete_device, get_device_by_id, get_device_logs, get_devices, get_screen_preview,
};
use crate::models::{Device, DeviceLog};

#[component]
pub fn Devices() -> Element {
    let mut devices = use_server_future(move || get_devices())?;

    rsx! {
        div { class: "mb-8 flex items-center justify-between",
            div {
                h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Devices" }
                p { class: "text-gray-500 mt-1", "Registered TRMNL devices" }
            }
            button {
                class: "inline-flex items-center gap-2 px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                onclick: move |_| devices.restart(),
                "Refresh"
            }
        }

        match devices() {
            Some(Ok(devices)) if devices.is_empty() => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                    div { class: "py-16 text-center",
                        p { class: "text-gray-400 text-lg", "No devices registered yet" }
                        p { class: "text-gray-300 text-sm mt-2",
                            "Devices will appear here after calling "
                            code { class: "bg-gray-100 px-1.5 py-0.5 rounded text-xs font-mono", "GET /api/setup" }
                        }
                    }
                }
            },
            Some(Ok(devices)) => rsx! {
                div { class: "grid grid-cols-1 gap-6",
                    for device in devices {
                        DeviceCard { key: "{device.id}", device: device }
                    }
                }
            },
            Some(Err(e)) => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                    div { class: "py-16 text-center",
                        p { class: "text-red-400 text-sm", "Error: {e}" }
                    }
                }
            },
            None => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                    div { class: "flex flex-col items-center justify-center py-12 gap-3",
                        div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                        p { class: "text-sm text-gray-400", "Loading..." }
                    }
                }
            },
        }
    }
}

#[component]
fn DeviceCard(device: Device) -> Element {
    rsx! {
        Link {
            to: super::super::Route::DeviceDetail { id: device.id },
            class: "block group",
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden group-hover:shadow-md group-hover:border-gray-200 transition-all",
                div { class: "p-5",
                    div { class: "flex items-start justify-between mb-3",
                        div {
                            h3 { class: "font-medium text-gray-900", "{device.friendly_id}" }
                            p { class: "text-xs text-gray-400 font-mono", "{device.mac_address}" }
                        }
                        span { class: "text-xs text-gray-400 font-mono bg-gray-50 px-2 py-1 rounded",
                            "{device.width}\u{00d7}{device.height}"
                        }
                    }

                    div { class: "grid grid-cols-2 sm:grid-cols-4 gap-x-6 gap-y-2 text-sm",
                        div {
                            span { class: "text-xs text-gray-400", "Model" }
                            p { class: "text-gray-700", "{device.model}" }
                        }
                        div {
                            span { class: "text-xs text-gray-400", "Firmware" }
                            p { class: "text-gray-700 font-mono text-xs",
                                match &device.fw_version {
                                    Some(fw) => rsx! { "{fw}" },
                                    None => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                }
                            }
                        }
                        div {
                            span { class: "text-xs text-gray-400", "Battery" }
                            p { class: "text-gray-700 font-mono text-xs",
                                match (device.percent_charged(), &device.battery_voltage) {
                                    (Some(pct), Some(v)) => rsx! { "{pct:.0}% ({v}V)" },
                                    (None, Some(v)) => rsx! { "{v}V" },
                                    _ => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                }
                            }
                        }
                        div {
                            span { class: "text-xs text-gray-400", "RSSI" }
                            p { class: "text-gray-700 font-mono text-xs",
                                match &device.rssi {
                                    Some(r) => rsx! { "{r}" },
                                    None => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                }
                            }
                        }
                    }

                    p { class: "text-xs text-gray-400 mt-3", "Last seen {device.last_seen_at}" }
                }
            }
        }
    }
}

#[component]
pub fn DeviceDetail(id: i64) -> Element {
    let device = use_server_future(move || get_device_by_id(id))?;
    let screen = use_server_future(move || get_screen_preview(id))?;
    let logs = use_server_future(move || get_device_logs(id))?;

    rsx! {
        div { class: "mb-6",
            Link {
                to: super::super::Route::Devices {},
                class: "inline-flex items-center gap-1.5 text-sm text-gray-500 hover:text-gray-900 transition-colors",
                "← Back to Devices"
            }
        }

        match device() {
            Some(Ok(Some(device))) => rsx! {
                div { class: "mb-8 flex items-start justify-between",
                    div {
                        h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "{device.friendly_id}" }
                        p { class: "text-gray-500 mt-1 font-mono text-sm", "{device.mac_address}" }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "text-sm text-gray-400 font-mono bg-gray-50 px-3 py-1.5 rounded-lg border border-gray-100",
                            "{device.width}\u{00d7}{device.height}"
                        }
                        DeleteButton { device_id: device.id }
                    }
                }

                div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6",
                    div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6",
                        h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4", "Device Info" }
                        div { class: "divide-y divide-gray-50",
                            DetailRow { label: "Model", value: device.model.clone() }
                            DetailRow {
                                label: "Firmware",
                                value: device.fw_version.clone().unwrap_or("\u{2014}".to_string())
                            }
                            DetailRow {
                                label: "Battery",
                                value: match (device.percent_charged(), &device.battery_voltage) {
                                    (Some(pct), Some(v)) => format!("{pct:.0}% ({v}V)"),
                                    (None, Some(v)) => format!("{v}V"),
                                    _ => "\u{2014}".to_string(),
                                }
                            }
                            DetailRow {
                                label: "RSSI",
                                value: device.rssi.clone().unwrap_or("\u{2014}".to_string())
                            }
                            DetailRow { label: "Last Seen", value: device.last_seen_at.clone() }
                            DetailRow { label: "Registered", value: device.created_at.clone() }
                            DetailRow { label: "Access Token", value: device.access_token.clone() }
                        }
                    }

                    div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6",
                        h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4", "Screen Preview" }
                        match screen() {
                            Some(Ok(b64)) => rsx! {
                                img {
                                    class: "w-full rounded border border-gray-100",
                                    src: "data:image/bmp;base64,{b64}",
                                    alt: "Screen preview",
                                }
                            },
                            Some(Err(e)) => rsx! {
                                p { class: "text-sm text-red-400", "Error: {e}" }
                            },
                            None => rsx! {
                                div { class: "flex flex-col items-center justify-center h-32 gap-3",
                                    div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                                    p { class: "text-sm text-gray-400", "Loading preview..." }
                                }
                            },
                        }
                    }
                }
                DeviceLogs { logs: logs() }
            },
            Some(Ok(None)) => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-16 text-center",
                    p { class: "text-gray-400", "Device not found" }
                }
            },
            Some(Err(e)) => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-16 text-center",
                    p { class: "text-red-400 text-sm", "Error: {e}" }
                }
            },
            None => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100",
                    div { class: "flex flex-col items-center justify-center py-12 gap-3",
                        div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                        p { class: "text-sm text-gray-400", "Loading..." }
                    }
                }
            },
        }
    }
}

#[component]
fn DeviceLogs(logs: Option<Result<Vec<DeviceLog>, ServerFnError>>) -> Element {
    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            div { class: "px-6 py-4 border-b border-gray-100",
                h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider", "Logs" }
            }
            match logs {
                Some(Ok(entries)) if entries.is_empty() => rsx! {
                    div { class: "py-12 text-center",
                        p { class: "text-sm text-gray-400", "No logs received yet" }
                    }
                },
                Some(Ok(entries)) => rsx! {
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-xs font-mono",
                            thead {
                                tr { class: "border-b border-gray-100 text-left",
                                    th { class: "px-4 py-2 text-gray-400 font-medium whitespace-nowrap", "Received" }
                                    th { class: "px-4 py-2 text-gray-400 font-medium", "Message" }
                                    th { class: "px-4 py-2 text-gray-400 font-medium whitespace-nowrap", "Source" }
                                    th { class: "px-4 py-2 text-gray-400 font-medium whitespace-nowrap", "Wake" }
                                    th { class: "px-4 py-2 text-gray-400 font-medium whitespace-nowrap", "WiFi" }
                                    th { class: "px-4 py-2 text-gray-400 font-medium whitespace-nowrap", "Battery" }
                                }
                            }
                            tbody {
                                for log in entries {
                                    tr { class: "border-b border-gray-50 hover:bg-gray-50 transition-colors",
                                        td { class: "px-4 py-2 text-gray-400 whitespace-nowrap", "{log.logged_at}" }
                                        td { class: "px-4 py-2 text-gray-700 max-w-xs truncate",
                                            match &log.message {
                                                Some(m) if !m.is_empty() => rsx! { span { title: "{m}", "{m}" } },
                                                _ => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                            }
                                        }
                                        td { class: "px-4 py-2 text-gray-500 whitespace-nowrap",
                                            match (&log.source_path, log.source_line) {
                                                (Some(p), Some(l)) => rsx! { "{p}:{l}" },
                                                (Some(p), None) => rsx! { "{p}" },
                                                _ => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                            }
                                        }
                                        td { class: "px-4 py-2 text-gray-500 whitespace-nowrap",
                                            match &log.wake_reason {
                                                Some(r) => rsx! { "{r}" },
                                                None => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                            }
                                        }
                                        td { class: "px-4 py-2 text-gray-500 whitespace-nowrap",
                                            match (&log.wifi_status, log.wifi_signal) {
                                                (Some(s), Some(sig)) => rsx! { "{s} ({sig}dBm)" },
                                                (Some(s), None) => rsx! { "{s}" },
                                                _ => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                            }
                                        }
                                        td { class: "px-4 py-2 text-gray-500 whitespace-nowrap",
                                            match log.battery_voltage {
                                                Some(v) => rsx! { "{v:.3}V" },
                                                None => rsx! { span { class: "text-gray-300", "\u{2014}" } },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div { class: "py-8 text-center",
                        p { class: "text-sm text-red-400", "Error: {e}" }
                    }
                },
                None => rsx! {
                    div { class: "flex flex-col items-center justify-center py-12 gap-3",
                        div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                        p { class: "text-sm text-gray-400", "Loading logs..." }
                    }
                },
            }
        }
    }
}

#[component]
fn DeleteButton(device_id: i64) -> Element {
    let mut confirming = use_signal(|| false);
    let nav = use_navigator();

    let handle_delete = move |_| async move {
        if let Err(e) = delete_device(device_id).await {
            tracing::error!("Failed to delete device: {e}");
        } else {
            nav.push(super::super::Route::Devices {});
        }
    };

    if confirming() {
        rsx! {
            div { class: "flex items-center gap-2",
                span { class: "text-sm text-gray-500", "Delete this device?" }
                button {
                    class: "px-3 py-1.5 bg-red-600 text-white text-sm font-medium rounded-lg hover:bg-red-700 transition-colors",
                    onclick: move |e| handle_delete(e),
                    "Confirm"
                }
                button {
                    class: "px-3 py-1.5 text-sm text-gray-500 hover:text-gray-900 transition-colors",
                    onclick: move |_| confirming.set(false),
                    "Cancel"
                }
            }
        }
    } else {
        rsx! {
            button {
                class: "px-3 py-1.5 text-sm text-red-500 border border-red-200 rounded-lg hover:bg-red-50 transition-colors",
                onclick: move |_| confirming.set(true),
                "Delete"
            }
        }
    }
}

#[component]
fn DetailRow(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex justify-between items-start py-2.5",
            span { class: "text-xs text-gray-400 shrink-0 mr-4", "{label}" }
            span { class: "text-sm text-gray-700 font-mono text-right break-all", "{value}" }
        }
    }
}
