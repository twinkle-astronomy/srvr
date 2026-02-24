use dioxus::prelude::*;

use crate::frontend::server_fns::get_devices;
use crate::models::Device;

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
                        DeviceCard { key: "{device.id}", device: device, refresh_count: refresh_count }
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
fn DeviceCard(device: Device, refresh_count: Signal<u32>) -> Element {
    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            // Device info
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
