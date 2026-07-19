use dioxus::prelude::*;

use crate::frontend::store::AppStore;
use crate::models::FirmwareRelease;

// ESP-IDF app image layout. Confirmed against Espressif's docs, not memory:
// https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/system/app_image_format.html
// "esp_app_desc_t ... has a fixed offset = sizeof(esp_image_header_t) +
// sizeof(esp_image_segment_header_t)" — 24 + 8 = 32 bytes into the file.
//
// That "fixed offset" holds for a plain single-leading-segment build, but
// not universally: a real ESP32-S3 + Rust/embassy firmware
// (`firmware-0.2.1.bin`) inserts an extra 24-byte segment first, landing
// the descriptor at 64 instead. 32 is only a *floor* — the descriptor can
// never start before it, but where it actually lands after that varies by
// chip/toolchain, so scan a bounded window rather than assume one offset.
const ESP_IMAGE_MAGIC: u8 = 0xE9;
const ESP_APP_DESC_MIN_OFFSET: usize = 24 + 8;
const ESP_APP_DESC_SEARCH_WINDOW: usize = 4096;
const ESP_APP_DESC_MAGIC_WORD: [u8; 4] = 0xABCD5432u32.to_le_bytes();
// Within esp_app_desc_t: magic_word (4B) + secure_version (4B) + reserv1[2] (8B) = 16B, then version[32].
const ESP_APP_DESC_VERSION_REL_OFFSET: usize = 16;
const ESP_APP_DESC_VERSION_LEN: usize = 32;

/// Best-effort extraction of the firmware version ESP-IDF embeds in every
/// app image, by scanning for the descriptor's magic word rather than
/// trusting one fixed offset (see layout note above). Returns `None` for
/// anything that doesn't look like a match (wrong image magic, no
/// descriptor found in the search window, non-UTF8, empty, or non-printable
/// content) — callers must treat this as a pre-fill suggestion, not a
/// validated value, and always leave the field manually editable.
fn parse_esp_app_version(bytes: &[u8]) -> Option<String> {
    if bytes.first() != Some(&ESP_IMAGE_MAGIC) {
        return None;
    }
    let search_end = bytes.len().min(ESP_APP_DESC_MIN_OFFSET + ESP_APP_DESC_SEARCH_WINDOW);
    if search_end < ESP_APP_DESC_MIN_OFFSET + 4 {
        return None;
    }
    let desc_offset = (ESP_APP_DESC_MIN_OFFSET..=search_end - 4)
        .find(|&i| bytes[i..i + 4] == ESP_APP_DESC_MAGIC_WORD)?;

    let version_start = desc_offset + ESP_APP_DESC_VERSION_REL_OFFSET;
    let version_end = version_start + ESP_APP_DESC_VERSION_LEN;
    if version_end > bytes.len() {
        return None;
    }
    let field = &bytes[version_start..version_end];
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let version = std::str::from_utf8(&field[..end]).ok()?.trim();
    let looks_like_a_version = !version.is_empty()
        && version.chars().all(|c| c.is_ascii_graphic() || c == ' ');
    if looks_like_a_version { Some(version.to_string()) } else { None }
}

/// Group releases by model, preserving first-appearance order of models.
/// Must NOT assume same-model rows are consecutive: the server list is
/// ordered `model ASC, created_at DESC`, but the store appends a
/// freshly-uploaded release to the end of the local list, so a model's rows
/// can be split — collapsing them here keeps the render keyed uniquely per
/// model either way.
fn group_by_model(releases: &[FirmwareRelease]) -> Vec<(String, Vec<FirmwareRelease>)> {
    let mut groups: Vec<(String, Vec<FirmwareRelease>)> = Vec::new();
    for release in releases {
        match groups.iter_mut().find(|(model, _)| model == &release.model) {
            Some((_, group)) => group.push(release.clone()),
            None => groups.push((release.model.clone(), vec![release.clone()])),
        }
    }
    groups
}

/// Whether an auto-detected version may replace the Version field's current
/// contents: only when the field is empty or still holds the previous
/// auto-detection — never clobber something the admin typed.
fn should_overwrite_version(current: &str, last_detected: Option<&str>) -> bool {
    current.is_empty() || Some(current) == last_detected
}

#[component]
pub fn Firmware() -> Element {
    let store = use_context::<AppStore>();
    let releases = store.firmware_releases;
    let releases_loaded = store.firmware_releases_loaded;

    let mut model_input = use_signal(String::new);
    let mut version_input = use_signal(String::new);
    // (filename, bytes) — read once when the file is chosen, both to sniff a
    // version and so upload doesn't need to re-read the file on submit.
    let mut selected_file: Signal<Option<(String, Vec<u8>)>> = use_signal(|| None);
    // What the ESP-IDF version sniff found (or didn't) for the currently
    // selected file — shown next to the Version field so detection isn't a
    // silent no-op the user has to take on faith.
    let mut detection_status = use_signal(|| None::<String>);
    // The last auto-detected version: an auto-fill may only replace an
    // empty field or its own previous value (see should_overwrite_version).
    let mut last_detected = use_signal(|| None::<String>);
    let mut upload_error = use_signal(|| None::<String>);
    let mut uploading = use_signal(|| false);

    let groups = group_by_model(&releases());

    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Firmware" }
            p { class: "text-gray-500 mt-1", "OTA firmware releases, tagged by device model" }
        }

        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6 mb-6",
            h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4", "Upload Release" }
            if let Some(ref msg) = upload_error() {
                div { class: "mb-4 p-3 bg-red-50 border border-red-200 rounded-lg",
                    p { class: "text-sm text-red-600", "{msg}" }
                }
            }
            form {
                class: "flex items-end gap-3 flex-wrap",
                onsubmit: move |event| {
                    event.prevent_default();
                    let model = model_input();
                    let version = version_input();
                    let Some((filename, bytes)) = selected_file() else {
                        upload_error.set(Some("Choose a firmware .bin file".to_string()));
                        return;
                    };
                    upload_error.set(None);
                    uploading.set(true);
                    spawn(async move {
                        match store.upload_firmware_release(model, version, filename, bytes).await {
                            Ok(_) => {
                                model_input.set(String::new());
                                version_input.set(String::new());
                                selected_file.set(None);
                                detection_status.set(None);
                                last_detected.set(None);
                                // File inputs are uncontrolled: clearing
                                // `selected_file` alone leaves the browser
                                // element still displaying the old file (and
                                // the next submit would then confusingly say
                                // "Choose a firmware .bin file"). Clear the
                                // DOM element itself.
                                #[cfg(feature = "web")]
                                {
                                    use wasm_bindgen::JsCast;
                                    if let Some(input) = web_sys::window()
                                        .and_then(|w| w.document())
                                        .and_then(|d| d.get_element_by_id("fw_file"))
                                        .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
                                    {
                                        input.set_value("");
                                    }
                                }
                            }
                            Err(e) => upload_error.set(Some(e.to_string())),
                        }
                        uploading.set(false);
                    });
                },

                div { class: "flex-1 min-w-40",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "fw_model", "Model" }
                    input {
                        r#type: "text",
                        id: "fw_model",
                        required: true,
                        placeholder: "trmnl-og",
                        class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                        value: "{model_input()}",
                        oninput: move |e| model_input.set(e.value()),
                    }
                }

                div { class: "flex-1 min-w-40",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "fw_version", "Version" }
                    input {
                        r#type: "text",
                        id: "fw_version",
                        required: true,
                        placeholder: "1.3.0",
                        class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                        value: "{version_input()}",
                        oninput: move |e| version_input.set(e.value()),
                    }
                }

                div { class: "flex-1 min-w-48",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "fw_file", "Firmware (.bin)" }
                    input {
                        r#type: "file",
                        id: "fw_file",
                        accept: ".bin",
                        class: "w-full text-sm text-gray-700",
                        onchange: move |evt| {
                            upload_error.set(None);
                            detection_status.set(None);
                            let Some(file) = evt.files().into_iter().next() else {
                                selected_file.set(None);
                                return;
                            };
                            spawn(async move {
                                match file.read_bytes().await {
                                    Ok(bytes) => {
                                        let bytes = bytes.to_vec();
                                        // Best-effort pre-fill from the binary's
                                        // embedded ESP-IDF version — the field
                                        // stays a normal editable input either
                                        // way, so a miss (or a wrong guess) is
                                        // just overtyped manually. Always report
                                        // what happened rather than failing
                                        // silently — a miss looks identical to
                                        // "nothing ran" otherwise.
                                        match parse_esp_app_version(&bytes) {
                                            Some(version) => {
                                                let current = version_input.peek().clone();
                                                if should_overwrite_version(&current, last_detected.peek().as_deref()) {
                                                    detection_status.set(Some(format!(
                                                        "Detected version \"{version}\" from the file \u{2014} edit if that's wrong."
                                                    )));
                                                    version_input.set(version.clone());
                                                } else {
                                                    detection_status.set(Some(format!(
                                                        "File reports version \"{version}\" \u{2014} keeping your typed \"{current}\"."
                                                    )));
                                                }
                                                last_detected.set(Some(version));
                                            }
                                            None => {
                                                detection_status.set(Some(
                                                    "Couldn't auto-detect a version from this file \u{2014} enter one manually.".to_string(),
                                                ));
                                            }
                                        }
                                        selected_file.set(Some((file.name(), bytes)));
                                    }
                                    Err(e) => {
                                        upload_error.set(Some(format!("Could not read file: {e:?}")));
                                        selected_file.set(None);
                                    }
                                }
                            });
                        },
                    }
                    if let Some(ref msg) = detection_status() {
                        p { class: "text-xs text-gray-400 mt-1", "{msg}" }
                    }
                }

                button {
                    r#type: "submit",
                    disabled: uploading(),
                    class: "px-4 py-1.5 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors disabled:opacity-50",
                    if uploading() { "Uploading..." } else { "Upload" }
                }
            }
        }

        if !releases_loaded() {
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                div { class: "flex flex-col items-center justify-center py-12 gap-3",
                    div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                    p { class: "text-sm text-gray-400", "Loading..." }
                }
            }
        } else if groups.is_empty() {
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                div { class: "py-16 text-center",
                    p { class: "text-gray-400 text-lg", "No firmware releases uploaded yet" }
                }
            }
        } else {
            div { class: "flex flex-col gap-6",
                for (model , group) in groups {
                    FirmwareModelGroup { key: "{model}", model: model.clone(), releases: group }
                }
            }
        }
    }
}

#[component]
fn FirmwareModelGroup(model: String, releases: Vec<FirmwareRelease>) -> Element {
    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            div { class: "px-6 py-4 border-b border-gray-100",
                h2 { class: "text-sm font-semibold text-gray-900", "{model}" }
            }
            div { class: "divide-y divide-gray-100",
                for release in releases {
                    FirmwareReleaseRow { key: "{release.id}", release: release.clone() }
                }
            }
        }
    }
}

#[component]
fn FirmwareReleaseRow(release: FirmwareRelease) -> Element {
    let store = use_context::<AppStore>();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let id = release.id;
    let active = release.active;

    rsx! {
        div { class: "px-6 py-4",
            div { class: "flex items-center justify-between gap-3",
                div {
                    p { class: "text-sm font-medium text-gray-900 font-mono", "{release.version}" }
                    p { class: "text-xs text-gray-400 mt-0.5",
                        "{release.filename} \u{00b7} {release.size_bytes} bytes \u{00b7} {release.created_at}"
                    }
                }
                div { class: "flex items-center gap-2 shrink-0",
                    if active {
                        span { class: "text-xs font-medium text-green-700 bg-green-50 px-2 py-1 rounded",
                            "Active"
                        }
                    } else {
                        button {
                            r#type: "button",
                            disabled: busy(),
                            class: "px-3 py-1.5 text-xs font-medium text-gray-700 border border-gray-200 rounded-lg hover:bg-gray-50 transition-colors disabled:opacity-50",
                            onclick: move |_| {
                                error.set(None);
                                busy.set(true);
                                spawn(async move {
                                    if let Err(e) = store.activate_firmware_release(id).await {
                                        error.set(Some(e.to_string()));
                                    }
                                    busy.set(false);
                                });
                            },
                            "Activate"
                        }
                    }
                    button {
                        r#type: "button",
                        disabled: busy() || active,
                        class: "px-3 py-1.5 text-xs font-medium text-red-600 border border-red-200 rounded-lg hover:bg-red-50 transition-colors disabled:opacity-50",
                        title: if active { "Activate a different release before deleting this one" } else { "" },
                        onclick: move |_| {
                            error.set(None);
                            busy.set(true);
                            spawn(async move {
                                if let Err(e) = store.delete_firmware_release(id).await {
                                    error.set(Some(e.to_string()));
                                }
                                busy.set(false);
                            });
                        },
                        "Delete"
                    }
                }
            }
            if let Some(ref msg) = error() {
                p { class: "text-xs text-red-500 mt-2", "Error: {msg}" }
            }
        }
    }
}

// Characterization tests for the store-driven grouping/list rendering. File
// upload and the router-dependent activate/delete buttons aren't covered
// here (no DOM events / router in the native SSR test tier — see
// docs/testing.md); those paths are exercised by the handler tests in
// src/api/firmware.rs and (if extended) the browser E2E tier.
#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::frontend::test_harness::render_with_store;

    fn release(id: i64, model: &str, version: &str, active: bool) -> FirmwareRelease {
        FirmwareRelease {
            id,
            model: model.to_string(),
            version: version.to_string(),
            filename: format!("{version}.bin"),
            size_bytes: 1024,
            active,
            created_at: "2026-07-18 00:00:00".to_string(),
        }
    }

    /// Builds a minimal but structurally valid ESP-IDF app image: the
    /// 24-byte image header + 8-byte segment header (32 bytes total, only
    /// byte 0 populated — the rest doesn't matter for this parser), then an
    /// `esp_app_desc_t` with a real magic word and the given version string
    /// null-padded into its 32-byte field.
    fn synthetic_esp_image(version: &str) -> Vec<u8> {
        let mut buf = vec![0u8; 32 + 256];
        buf[0] = ESP_IMAGE_MAGIC;
        buf[ESP_APP_DESC_MIN_OFFSET..ESP_APP_DESC_MIN_OFFSET + 4].copy_from_slice(&ESP_APP_DESC_MAGIC_WORD);
        let vbytes = version.as_bytes();
        assert!(vbytes.len() < ESP_APP_DESC_VERSION_LEN, "test version must fit the 32-byte field");
        let version_offset = ESP_APP_DESC_MIN_OFFSET + ESP_APP_DESC_VERSION_REL_OFFSET;
        buf[version_offset..version_offset + vbytes.len()].copy_from_slice(vbytes);
        buf
    }

    /// First 192 bytes of a real device's uploaded firmware
    /// (`firmware-0.2.1.bin`, ESP32-S3 + Rust/embassy build). Unlike a plain
    /// single-segment ESP-IDF/Arduino build, this toolchain inserts an
    /// extra 24-byte leading segment before the descriptor, landing
    /// `esp_app_desc_t` at file offset 64, not the 32 a "fixed offset"
    /// assumption predicts. Captured via `od -A x -t x1z -v`.
    #[rustfmt::skip]
    const REAL_ESP32S3_EMBASSY_HEADER: [u8; 192] = [
        0xe9, 0x07, 0x02, 0x20, 0x14, 0x88, 0x37, 0x40, 0xee, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00,
        0x00, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xf8, 0xfc, 0xc8, 0x3f, 0x18, 0x00, 0x00, 0x00,
        0x0a, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00,
        0x05, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00, 0x00, 0x40, 0x00, 0x0d, 0x3c, 0xcc, 0x47, 0x04, 0x00,
        0x32, 0x54, 0xcd, 0xab, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x30, 0x2e, 0x32, 0x2e, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x65, 0x73, 0x70, 0x33, 0x32, 0x73, 0x33, 0x2d, 0x65, 0x6d, 0x62, 0x61, 0x73, 0x73, 0x79, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x30, 0x30, 0x3a, 0x30, 0x30, 0x3a, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x4a, 0x61, 0x6e, 0x20, 0x20, 0x31, 0x20, 0x32, 0x30, 0x32, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x76, 0x35, 0x2e, 0x35, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn parse_esp_app_version_finds_descriptor_past_an_extra_leading_segment() {
        assert_eq!(
            parse_esp_app_version(&REAL_ESP32S3_EMBASSY_HEADER),
            Some("0.2.1".to_string()),
        );
    }

    #[test]
    fn parse_esp_app_version_extracts_embedded_version() {
        let image = synthetic_esp_image("1.2.3-og");
        assert_eq!(parse_esp_app_version(&image), Some("1.2.3-og".to_string()));
    }

    #[test]
    fn parse_esp_app_version_returns_none_for_non_esp_image() {
        let not_an_image = vec![0u8; 512]; // wrong magic byte (0x00, not 0xE9)
        assert_eq!(parse_esp_app_version(&not_an_image), None);
    }

    #[test]
    fn parse_esp_app_version_returns_none_for_truncated_file() {
        let mut image = synthetic_esp_image("1.2.3");
        image.truncate(40); // shorter than the version field's offset + length
        assert_eq!(parse_esp_app_version(&image), None);
    }

    #[test]
    fn parse_esp_app_version_returns_none_when_app_desc_magic_mismatched() {
        let mut image = synthetic_esp_image("1.2.3");
        image[ESP_APP_DESC_MIN_OFFSET] ^= 0xFF; // corrupt just the app_desc magic word
        assert_eq!(parse_esp_app_version(&image), None);
    }

    #[test]
    fn group_by_model_groups_consecutive_same_model_rows() {
        let releases = vec![
            release(1, "trmnl-og", "1.0.0", false),
            release(2, "trmnl-og", "1.1.0", true),
            release(3, "trmnl-v2", "2.0.0", false),
        ];
        let groups = group_by_model(&releases);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "trmnl-og");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "trmnl-v2");
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn group_by_model_merges_non_consecutive_same_model_rows() {
        // The store appends a fresh upload to the end of the local list, so
        // a model's rows can be split around another model. They must still
        // collapse into one group — two groups sharing a model means two
        // FirmwareModelGroup nodes with the same Dioxus key.
        let releases = vec![
            release(1, "trmnl-og", "1.0.0", false),
            release(3, "trmnl-v2", "2.0.0", false),
            release(2, "trmnl-og", "1.1.0", false),
        ];
        let groups = group_by_model(&releases);
        assert_eq!(
            groups.len(),
            2,
            "same-model rows must form one group even when not consecutive"
        );
        assert_eq!(groups[0].0, "trmnl-og");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "trmnl-v2");
    }

    #[test]
    fn detected_version_overwrites_only_empty_or_previously_detected_fields() {
        // Empty field: take the detection.
        assert!(should_overwrite_version("", None));
        assert!(should_overwrite_version("", Some("0.2.0")));
        // Field still holds the previous detection: a new file's detection
        // may replace it.
        assert!(should_overwrite_version("0.2.0", Some("0.2.0")));
        // Admin typed something else: never clobber it.
        assert!(!should_overwrite_version("2.0.0-hotfix", None));
        assert!(!should_overwrite_version("2.0.0-hotfix", Some("0.2.0")));
    }

    fn store_loading() -> AppStore {
        AppStore::new()
    }

    fn store_empty() -> AppStore {
        let mut s = AppStore::new();
        s.firmware_releases_loaded.set(true);
        s
    }

    fn store_with_releases() -> AppStore {
        let mut s = AppStore::new();
        s.firmware_releases.set(vec![
            release(1, "trmnl-og", "1.0.0", false),
            release(2, "trmnl-og", "1.1.0", true),
        ]);
        s.firmware_releases_loaded.set(true);
        s
    }

    #[test]
    fn firmware_page_shows_spinner_before_load() {
        let html = render_with_store(store_loading, Firmware);
        assert!(
            html.contains("Loading..."),
            "expected loading spinner before firmware releases load, got: {html:?}"
        );
    }

    #[test]
    fn firmware_page_shows_empty_state_when_loaded_and_empty() {
        let html = render_with_store(store_empty, Firmware);
        assert!(
            html.contains("No firmware releases uploaded yet"),
            "expected empty-state copy, got: {html:?}"
        );
    }

    #[test]
    fn firmware_page_lists_releases_grouped_by_model_with_active_badge() {
        let html = render_with_store(store_with_releases, Firmware);
        assert!(html.contains("trmnl-og"), "expected model name, got: {html:?}");
        assert!(html.contains("1.0.0") && html.contains("1.1.0"), "expected both versions, got: {html:?}");
        assert!(html.contains("Active"), "expected the active badge, got: {html:?}");
        assert!(
            html.contains("Activate"),
            "expected an Activate button for the inactive release, got: {html:?}"
        );
    }
}
