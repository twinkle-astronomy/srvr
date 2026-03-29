use dioxus::prelude::*;

use crate::{
    frontend::server_fns::{delete_http_source, execute_http_source, save_http_source},
    models::{HttpSource, Template},
};

#[component]
pub fn HttpSources(sources: Store<Vec<HttpSource>>, template: ReadSignal<Template>) -> Element {
    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            div { class: "p-4 border-b border-gray-100 flex items-center justify-between",
                span { class: "text-sm font-medium text-gray-700",
                    "HTTP Sources ({sources().len()})"
                }
                div { class: "flex items-center gap-2",
                    button {
                        class: "inline-flex items-center gap-2 px-3 py-1.5 bg-green-700 text-white text-xs font-medium rounded-lg hover:bg-green-600 transition-colors",
                        onclick: move |_| sources.push(HttpSource::new(template().id)),
                        "Add Source"
                    }
                }
            }
            if !sources().is_empty() {
                div { class: "divide-y divide-gray-100",
                    for source in sources.iter() {
                        HttpSourceRow {
                            key: "{source.peek().id.map(|id| id.to_string()).unwrap_or_else(|| source.peek().created_at.and_utc().timestamp_nanos_opt().unwrap_or(0).to_string())}",
                            source: source,
                            on_delete: move |_| {
                                let s = source.peek().clone();
                                sources.write().retain(|x| x.ne(&s));
                                if let Some(id) = s.id {
                                    spawn(async move { delete_http_source(id).await.ok(); });
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HttpSourceRow(source: WriteStore<HttpSource>, on_delete: EventHandler) -> Element {
    let is_new = use_memo(move || source().id.is_none());
    let mut editing = use_signal(|| is_new());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut original = use_signal(|| None::<HttpSource>);

    let result = use_resource(move || {
        let source = source();
        async move { execute_http_source(source).await }
    });

    let can_save = !source().name.trim().is_empty() && !source().url.trim().is_empty();

    rsx! {
        if editing() {
            div { class: "p-4 bg-gray-50",
                div { class: "flex flex-col gap-3",
                    div { class: "flex gap-3",
                        div { class: "flex-1",
                            label { class: "block text-xs font-medium text-gray-500 mb-1", "Name" }
                            input {
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                value: "{source().name}",
                                oninput: move |evt| source.write().name = evt.value(),
                            }
                        }
                        div { class: "flex-[2]",
                            label { class: "block text-xs font-medium text-gray-500 mb-1", "URL" }
                            input {
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                value: "{source().url}",
                                oninput: move |evt| source.write().url = evt.value(),
                            }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "inline-flex items-center px-3 py-1.5 bg-green-700 text-white text-xs font-medium rounded-lg hover:bg-green-600 transition-colors disabled:opacity-50",
                            disabled: !can_save || saving(),
                            onclick: move |_| {
                                saving.set(true);
                                error.set(None);
                                spawn(async move {
                                    match save_http_source(source().clone()).await {
                                        Ok(s) => {
                                            *source.write() = s;
                                            editing.set(false);
                                        }
                                        Err(e) => error.set(Some(e.to_string())),
                                    }
                                    saving.set(false);
                                });
                            },
                            if saving() { "Saving..." } else { "Save" }
                        }
                        button {
                            class: "inline-flex items-center px-3 py-1.5 text-gray-600 text-xs font-medium rounded-lg border border-gray-300 hover:bg-gray-100 transition-colors disabled:opacity-50",
                            disabled: saving(),
                            onclick: move |_| {
                                if is_new() {
                                    on_delete(());
                                } else if let Some(orig) = original() {
                                    *source.write() = orig;
                                    editing.set(false);
                                }
                            },
                            "Cancel"
                        }
                        button {
                            class: "inline-flex items-center px-3 py-1.5 bg-red-600 text-white text-xs font-medium rounded-lg hover:bg-red-500 transition-colors disabled:opacity-50",
                            disabled: saving(),
                            onclick: move |_| {
                                on_delete(());
                            },
                            "Delete"
                        }
                        if let Some(ref err) = error() {
                            span { class: "text-xs text-red-500", "{err}" }
                        }
                    }
                }
            }
        } else {
            div { class: "p-4",
                div { class: "flex items-start justify-between gap-4 mb-1",
                    div {
                        span { class: "text-sm font-medium text-gray-900", "{source().name}" }
                        p { class: "text-xs text-gray-400 mt-0.5 truncate max-w-md",
                            "{source().url}"
                        }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "text-xs text-gray-400 hover:text-gray-600 transition-colors",
                            onclick: move |_| {
                                original.set(Some(source().clone()));
                                editing.set(true);
                            },
                            "Edit"
                        }
                    }
                }
            }
        }
        if let Some(Ok(res)) = result() {
            if let Some(ref err) = res.error {
                div { class: "px-4 pb-3",
                    p { class: "text-xs text-red-500", "Error: {err}" }
                }
            } else if let Some(ref data) = res.data {
                div { class: "px-4 pb-3 overflow-x-auto",
                    pre { class: "text-xs text-gray-600 bg-gray-50 rounded p-2 max-h-32 overflow-y-auto",
                        {serde_json::to_string_pretty(data).unwrap_or_default()}
                    }
                }
            }
        }
    }
}
