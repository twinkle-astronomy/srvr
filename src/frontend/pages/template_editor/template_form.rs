use dioxus::prelude::*;

use crate::frontend::pages::template_editor::TemplateVariables;
use crate::frontend::server_fns::{delete_template, save_template};
use crate::models::{Device, RenderContext, RenderContextStoreExt, TemplateStoreExt};

#[component]
pub fn TemplateForm(
    mut render_context: WriteStore<RenderContext>,
    devices: ReadStore<Vec<Device>>,
    mut selected_device: WriteStore<Option<Device>>,
    preview_error: ReadStore<Option<String>>,
) -> Element {
    let mut save_status = use_signal(|| None::<Result<(), String>>);
    let mut delete_confirming = use_signal(|| false);
    let mut delete_error = use_signal(|| None::<String>);
    let nav = use_navigator();

    rsx! {
        div { class: "flex-1 min-w-0 flex flex-col gap-4",
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                div { class: "p-4 border-b border-gray-100 flex items-center justify-between",
                    div { class: "flex items-center gap-3 flex-1 min-w-0",
                        span { class: "text-sm font-medium text-gray-700 shrink-0", "Name" }
                        input {
                            class: "text-sm border border-gray-200 rounded-lg px-2 py-1 text-gray-800 flex-1 min-w-0 focus:outline-none focus:ring-1 focus:ring-gray-300",
                            value: render_context.template().name(),
                            oninput: move |evt| {
                                save_status.set(None);
                                *render_context.template().name().write() = evt.value();
                            }
                        }
                    }
                    div { class: "flex items-center gap-3 ml-4",
                        span { class: "text-sm font-medium text-gray-700 shrink-0", "Preview Device" }
                        if !devices().is_empty() {
                            select {
                                class: "text-sm border border-gray-200 rounded-lg px-2 py-1 text-gray-600",
                                onchange: move |evt| {
                                    if let Ok(idx) = evt.value().parse::<usize>() {
                                        if let Some(d) = devices().get(idx) {
                                            selected_device.set(Some(d.clone()));
                                        }
                                    }
                                },
                                for (i, dev) in devices().iter().enumerate() {
                                    option { value: "{i}", "{dev.friendly_id} ({dev.width}\u{00d7}{dev.height})" }
                                }
                            }
                        } else {
                            span { class: "text-xs text-gray-400", "No devices" }
                        }
                    }
                }

                textarea {
                    class: "w-full h-96 p-4 font-mono text-sm text-gray-800 bg-gray-50 border-0 focus:outline-none focus:ring-0 resize-y",
                    spellcheck: false,
                    value: render_context.template().content(),
                    oninput: move |evt| {
                        save_status.set(None);
                        *render_context.template().content().write() = evt.value();

                    }
                }

            }

            div { class: "flex items-center gap-3",
                button {
                    class: "inline-flex items-center gap-2 px-4 py-2 bg-green-700 text-white text-sm font-medium rounded-lg hover:bg-green-600 transition-colors",
                    onclick: move |_| {
                        save_status.set(None);
                        spawn(async move {
                            match save_template(
                                render_context.template().id().read().clone(),
                                render_context.template().name().read().clone(),
                                render_context.template().content().read().clone(),
                            ).await {
                                Ok(()) => save_status.set(Some(Ok(()))),
                                Err(e) => save_status.set(Some(Err(e.to_string()))),
                            }
                        });
                    },
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
                match preview_error() {
                    Some(e) => rsx! {
                        span { class: "text-sm text-red-600", "Error: {e}" }
                    },
                    None => rsx! {},
                }

                div { class: "ml-auto",
                    if delete_confirming() {
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm text-gray-500", "Delete this template?" }
                            button {
                                class: "px-3 py-1.5 bg-red-600 text-white text-sm font-medium rounded-lg hover:bg-red-700 transition-colors",
                                onclick: move |_| {
                                    delete_error.set(None);
                                    let template_id = render_context.template().id().read().clone();
                                    spawn(async move {
                                        match delete_template(template_id).await {
                                            Ok(()) => {
                                                nav.push(super::super::super::Route::Templates {});
                                            }
                                            Err(e) => {
                                                delete_error.set(Some(e.to_string()));
                                                delete_confirming.set(false);
                                            }
                                        }
                                    });
                                },
                                "Confirm"
                            }
                            button {
                                class: "px-3 py-1.5 text-sm text-gray-500 hover:text-gray-900 transition-colors",
                                onclick: move |_| delete_confirming.set(false),
                                "Cancel"
                            }
                        }
                    } else {
                        button {
                            class: "px-3 py-1.5 text-sm text-red-500 border border-red-200 rounded-lg hover:bg-red-50 transition-colors",
                            onclick: move |_| {
                                delete_error.set(None);
                                delete_confirming.set(true);
                            },
                            "Delete"
                        }
                    }
                    if let Some(ref err) = delete_error() {
                        span { class: "text-sm text-red-500 ml-2", "{err}" }
                    }
                }
            }
            TemplateVariables { render_context }
        }

    }
}
