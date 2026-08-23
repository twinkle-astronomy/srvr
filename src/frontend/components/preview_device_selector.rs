use dioxus::prelude::*;

use crate::frontend::store::AppStore;
use crate::models::Device;

/// How a device is labelled in the preview dropdown: dimensions plus the
/// render mode, so the list itself explains why picking a different device
/// changes how the preview looks.
pub fn device_option_label(device: &Device) -> String {
    let depth = if device.supports_2bit_grayscale {
        "2-bit"
    } else {
        "1-bit"
    };
    format!(
        "{} ({}\u{00d7}{}, {})",
        device.friendly_id, device.width, device.height, depth
    )
}

/// The device a preview should be rendered for. Shared by the manual
/// template editor and the AI generator so both pages offer the same list and
/// the same labels.
///
/// The virtual device is always offered, not just when no real devices exist
/// — previewing a template at a neutral 800x480 is useful even once you own
/// hardware.
#[component]
pub fn PreviewDeviceSelector(selected_device: WriteStore<Option<Device>>) -> Element {
    let store = use_context::<AppStore>();
    let devices = store.devices;

    // Index 0 is always the virtual device; real devices follow, so the
    // option value is an index into `[virtual] ++ devices`.
    let options = use_memo(move || {
        let mut opts = vec![Device::virtual_device()];
        opts.extend(devices());
        opts
    });

    // Read `selected_device` reactively (not via `peek`) so the rendered
    // `value` follows a selection made anywhere else — the editor seeds it
    // from the device list on first load.
    let selected_index = use_memo(move || match selected_device() {
        Some(d) => options().iter().position(|o| o.id == d.id).unwrap_or(0),
        None => 0,
    });

    rsx! {
        div { class: "flex items-center gap-3",
            span { class: "text-sm font-medium text-gray-700 shrink-0", "Preview Device" }
            select {
                class: "text-sm border border-gray-200 rounded-lg px-2 py-1 text-gray-600",
                value: "{selected_index()}",
                onchange: move |evt| {
                    if let Ok(idx) = evt.value().parse::<usize>() {
                        if let Some(d) = options().get(idx) {
                            selected_device.set(Some(d.clone()));
                        }
                    }
                },
                for (i, dev) in options().iter().enumerate() {
                    option { value: "{i}", "{device_option_label(dev)}" }
                }
            }
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    fn device(friendly_id: &str, grayscale: bool) -> Device {
        Device {
            friendly_id: friendly_id.to_string(),
            width: 800,
            height: 480,
            supports_2bit_grayscale: grayscale,
            ..Device::virtual_device()
        }
    }

    #[test]
    fn label_marks_a_grayscale_device_as_2_bit() {
        assert_eq!(
            device_option_label(&device("kitchen", true)),
            "kitchen (800\u{00d7}480, 2-bit)"
        );
    }

    #[test]
    fn label_marks_a_legacy_device_as_1_bit() {
        assert_eq!(
            device_option_label(&device("hallway", false)),
            "hallway (800\u{00d7}480, 1-bit)"
        );
    }
}
