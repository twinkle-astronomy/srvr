use std::ops::DerefMut;

use dioxus::prelude::*;

use crate::frontend::server_fns::{get_devices, get_template, get_template_preview, save_template};
use crate::models::{Device, Template};
use dioxus::logger::tracing::info;

#[component]
pub fn TemplateEditor() -> Element {
    let mut template = use_signal(|| None::<Template>);
    let mut preview_b64 = use_signal(|| None::<String>);
    let mut preview_loading = use_signal(|| false);
    let mut save_status = use_signal(|| None::<Result<(), String>>);
    let mut selected_device = use_signal(|| None::<Device>);

    // Uses peek() so it only re-runs when selected_device changes, not on every keystroke
    let mut fetch_preview = move || {
        if let Some(device) = selected_device.peek().clone() {
            if let Some(tmpl) = template() {
                let template = Template {
                    id: tmpl.id,
                    content: tmpl.content.clone(),
                    created_at: tmpl.created_at.clone(),
                    updated_at: tmpl.updated_at.clone(),
                };
                preview_loading.set(true);
                spawn(async move {
                    match get_template_preview(device.id, template).await {
                        Ok(b64) => preview_b64.set(b64),
                        Err(e) => {
                            tracing::error!("Preview error: {e}");
                            preview_b64.set(None);
                        }
                    }
                    preview_loading.set(false);
                });
            }
        }
    };

    // Auto-preview on initial load and when device selection changes.
    // Must be registered before the ? operators so it runs on every render.
    use_effect(move || {
        if selected_device().is_some() && !preview_loading() && preview_b64().is_none() {
            info!("Loading preview");
            fetch_preview();
        }
    });

    let devices = use_server_future(get_devices)?;
    let srv_template = use_server_future(get_template)?;

    if template().is_none() {
        if let Some(Ok(ref tmpl)) = srv_template() {
            template.set(Some(tmpl.clone()));
        }
    }

    if selected_device().is_none() {
        if let Some(Ok(ref devs)) = devices() {
            if let Some(first) = devs.first() {
                selected_device.set(Some(first.clone()));
            }
        }
    }

    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Template Editor" }
            p { class: "text-gray-500 mt-1", "Edit the SVG Liquid template used for device screens" }
        }

        div { class: "flex flex-wrap items-start gap-6",
            TemplateForm {
                template,
                devices,
                selected_device,
                save_status,
                on_preview: move |_| fetch_preview(),
                on_save: move |_| {
                    if let Some(tmpl) = template() {
                        save_status.set(None);
                        fetch_preview();
                        spawn(async move {
                            match save_template(tmpl.id, tmpl.content).await {
                                Ok(()) => save_status.set(Some(Ok(()))),
                                Err(e) => save_status.set(Some(Err(e.to_string()))),
                            }
                        });
                    }
                },
            }
            TemplatePreview {
                selected_device,
                preview_loading,
                preview_b64,
            }
        }
    }
}

#[component]
fn TemplateForm(
    mut template: Signal<Option<Template>>,
    devices: Resource<Result<Vec<Device>, ServerFnError>>,
    mut selected_device: Signal<Option<Device>>,
    save_status: Signal<Option<Result<(), String>>>,
    on_preview: EventHandler,
    on_save: EventHandler,
) -> Element {
    rsx! {
        div { class: "flex-1 min-w-0 flex flex-col gap-4",
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                div { class: "p-4 border-b border-gray-100 flex items-center justify-between",
                    span { class: "text-sm font-medium text-gray-700", "Template" }
                    match devices() {
                        Some(Ok(ref devs)) if !devs.is_empty() => rsx! {
                            select {
                                class: "text-sm border border-gray-200 rounded-lg px-2 py-1 text-gray-600",
                                onchange: move |evt| {
                                    if let Some(Ok(ref devs)) = devices() {
                                        if let Ok(idx) = evt.value().parse::<usize>() {
                                            if let Some(d) = devs.get(idx) {
                                                selected_device.set(Some(d.clone()));
                                            }
                                        }
                                    }
                                },
                                for (i, dev) in devs.iter().enumerate() {
                                    option { value: "{i}", "{dev.friendly_id} ({dev.width}\u{00d7}{dev.height})" }
                                }
                            }
                        },
                        _ => rsx! {
                            span { class: "text-xs text-gray-400", "No devices" }
                        },
                    }
                }
                if let Some(tmpl) = template() {
                textarea {
                    class: "w-full h-96 p-4 font-mono text-sm text-gray-800 bg-gray-50 border-0 focus:outline-none focus:ring-0 resize-y",
                    spellcheck: false,
                    value: "{tmpl.content}",
                    oninput: move |evt| {
                        let mut writable = template.write();
                        if let Some(t) = writable.deref_mut() {
                            t.content = evt.value();
                        }
                    },
                }
                }
            }

            div { class: "flex items-center gap-3",
                button {
                    class: "inline-flex items-center gap-2 px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                    disabled: selected_device().is_none(),
                    onclick: move |_| on_preview.call(()),
                    "Preview"
                }
                button {
                    class: "inline-flex items-center gap-2 px-4 py-2 bg-green-700 text-white text-sm font-medium rounded-lg hover:bg-green-600 transition-colors",
                    onclick: move |_| on_save.call(()),
                    "Save"
                }
                match save_status() {
                    Some(Ok(())) => rsx! {
                        span { class: "text-sm text-green-600", "Saved!" }
                    },
                    Some(Err(e)) => rsx! {
                        span { class: "text-sm text-red-500", "Error: {e}" }
                    },
                    None => rsx! {},
                }
            }
        }
    }
}

#[component]
fn TemplatePreview(
    selected_device: Signal<Option<Device>>,
    preview_loading: Signal<bool>,
    preview_b64: Signal<Option<String>>,
) -> Element {
    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            div { class: "p-4 border-b border-gray-100",
                span { class: "text-sm font-medium text-gray-700", "Preview" }
            }
            div { class: "p-4 bg-gray-50",
                if let Some(ref dev) = selected_device() {
                    div {
                        class: "flex items-center justify-center bg-white border border-gray-200 rounded shadow-sm",
                        style: "width: {dev.width}px; height: {dev.height}px;",
                        if preview_loading() {
                            div { class: "flex flex-col items-center justify-center gap-2",
                                div { class: "w-5 h-5 border-2 border-gray-200 border-t-gray-400 rounded-full animate-spin" }
                                p { class: "text-xs text-gray-300", "Rendering..." }
                            }
                        } else {
                            match preview_b64() {
                                Some(b64) => rsx! {
                                    img {
                                        src: "data:image/bmp;base64,{b64}",
                                        alt: "Template preview",
                                        class: "max-w-none",
                                        style: "image-rendering: pixelated;",
                                    }
                                },
                                None => rsx! {
                                    p { class: "text-gray-300 text-sm", "No preview available" }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
