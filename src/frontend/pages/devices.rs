use dioxus::prelude::*;

use crate::frontend::server_fns::get_devices;

#[component]
pub fn Devices() -> Element {
    let mut refresh_count = use_signal(|| 0u32);

    let devices = use_server_future(move || {
        let _ = refresh_count();
        get_devices()
    })?;

    rsx! {
        div { class: "mb-8 flex items-center justify-between",
            div {
                h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Devices" }
                p { class: "text-gray-500 mt-1", "Registered TRMNL devices" }
            }
            button {
                class: "inline-flex items-center gap-2 px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                onclick: move |_| refresh_count += 1,
                "Refresh"
            }
        }

        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            match devices() {
                Some(Ok(devices)) if devices.is_empty() => rsx! {
                    div { class: "py-16 text-center",
                        p { class: "text-gray-400 text-lg", "No devices registered yet" }
                        p { class: "text-gray-300 text-sm mt-2",
                            "Devices will appear here after calling "
                            code { class: "bg-gray-100 px-1.5 py-0.5 rounded text-xs font-mono", "GET /api/setup" }
                        }
                    }
                },
                Some(Ok(devices)) => rsx! {
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm text-left",
                            thead { class: "text-xs text-gray-400 uppercase tracking-wider bg-gray-50 border-b border-gray-100",
                                tr {
                                    th { class: "px-6 py-3", "Device" }
                                    th { class: "px-6 py-3", "Model" }
                                    th { class: "px-6 py-3", "Size" }
                                    th { class: "px-6 py-3", "Firmware" }
                                    th { class: "px-6 py-3", "Battery" }
                                    th { class: "px-6 py-3", "RSSI" }
                                    th { class: "px-6 py-3", "Last Seen" }
                                }
                            }
                            tbody {
                                for device in devices {
                                    tr { class: "border-b border-gray-50 hover:bg-gray-50/50 transition-colors",
                                        td { class: "px-6 py-4",
                                            div {
                                                span { class: "font-medium text-gray-900", "{device.friendly_id}" }
                                            }
                                            div {
                                                span { class: "text-xs text-gray-400 font-mono", "{device.mac_address}" }
                                            }
                                        }
                                        td { class: "px-6 py-4 text-gray-700", "{device.model}" }
                                        td { class: "px-6 py-4 text-gray-700 font-mono text-xs",
                                            match (device.width, device.height) {
                                                (Some(w), Some(h)) => rsx! { "{w}x{h}" },
                                                _ => rsx! { span { class: "text-gray-300", "—" } },
                                            }
                                        }
                                        td { class: "px-6 py-4 text-gray-700 font-mono text-xs",
                                            match &device.fw_version {
                                                Some(fw) => rsx! { "{fw}" },
                                                None => rsx! { span { class: "text-gray-300", "—" } },
                                            }
                                        }
                                        td { class: "px-6 py-4 text-gray-700 font-mono text-xs",
                                            match &device.battery_voltage {
                                                Some(v) => rsx! { "{v}V" },
                                                None => rsx! { span { class: "text-gray-300", "—" } },
                                            }
                                        }
                                        td { class: "px-6 py-4 text-gray-700 font-mono text-xs",
                                            match &device.rssi {
                                                Some(r) => rsx! { "{r}" },
                                                None => rsx! { span { class: "text-gray-300", "—" } },
                                            }
                                        }
                                        td { class: "px-6 py-4 text-gray-500 text-xs", "{device.last_seen_at}" }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div { class: "py-16 text-center",
                        p { class: "text-red-400 text-sm", "Error: {e}" }
                    }
                },
                None => rsx! {
                    div { class: "flex flex-col items-center justify-center py-12 gap-3",
                        div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                        p { class: "text-sm text-gray-400", "Loading..." }
                    }
                },
            }
        }
    }
}
