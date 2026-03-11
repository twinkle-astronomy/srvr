use dioxus::prelude::*;

use crate::{
    frontend::server_fns::{
        delete_prometheus_query, execute_prometheus_query, save_prometheus_query
    }, models::{PrometheusQuery, Template}
};

#[component]
pub fn PrometheusQueries(
    queries: Store<Vec<PrometheusQuery>>,
    template: ReadSignal<Template>,
) -> Element {
    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            div { class: "p-4 border-b border-gray-100 flex items-center justify-between",
                span { class: "text-sm font-medium text-gray-700",
                    "Prometheus Queries ({queries().len()})"
                }
                div { class: "flex items-center gap-2",

                    button {
                        class: "inline-flex items-center gap-2 px-3 py-1.5 bg-green-700 text-white text-xs font-medium rounded-lg hover:bg-green-600 transition-colors",
                        onclick: move |_| queries.push(PrometheusQuery::new(template().id)),
                        "Add Query"
                    }
                }
            }
            if !queries().is_empty() {
                div { class: "divide-y divide-gray-100",
                    for query in queries.iter() {
                        QueryRow {
                            key: "{query.peek().id.map(|id| id.to_string()).unwrap_or_else(|| query.peek().created_at.and_utc().timestamp_nanos_opt().unwrap_or(0).to_string())}",
                            query: query,
                            on_delete: move |_|{
                                let q = query.peek().clone();
                                queries.write().retain(|x| x.ne(&q));
                                if let Some(id) = q.id {
                                    spawn(async move {delete_prometheus_query(id).await.ok(); });
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
fn QueryRow(query: WriteStore<PrometheusQuery>, on_delete: EventHandler) -> Element {
    let is_new = use_memo(move || query().id.is_none());
    let mut editing = use_signal(|| is_new());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut original = use_signal(|| None::<PrometheusQuery>);

    let result = use_resource(move || {
        let query = query();
        async move { execute_prometheus_query(query).await }
    });

    let can_save = !query().name.trim().is_empty()
        && !query().addr.trim().is_empty()
        && !query().query.trim().is_empty();

    rsx! {
        if editing() {
            div { class: "p-4 bg-gray-50",
                div { class: "flex flex-col gap-3",
                    div { class: "flex gap-3",
                        div { class: "flex-1",
                            label { class: "block text-xs font-medium text-gray-500 mb-1", "Name" }
                            input {
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                  value: "{query().name}",
                                oninput: move |evt| query.write().name = evt.value(),
                            }
                        }
                        div { class: "flex-1",
                            label { class: "block text-xs font-medium text-gray-500 mb-1", "Prometheus Address" }
                            input {
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                value: "{query().addr}",
                                oninput: move |evt| query.write().addr = evt.value(),
                            }
                        }
                    }
                    div {
                        label { class: "block text-xs font-medium text-gray-500 mb-1", "PromQL Query" }
                        input {
                            class: "w-full text-sm font-mono border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                            value: "{query().query}",
                            oninput: move |evt| query.write().query = evt.value(),
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
                                    match save_prometheus_query(query().clone()).await {
                                        Ok(q) => {
                                            *query.write() = q;
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
                                    *query.write() = orig;
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
                        span { class: "text-sm font-medium text-gray-900", "{query().name}" }
                        p { class: "text-xs text-gray-400 mt-0.5",
                            "{query().addr}"
                        }
                    }
                    div { class: "flex items-center gap-2",
                        code { class: "text-xs bg-gray-50 text-gray-600 px-2 py-1 rounded shrink-0",
                            "{query().query}"
                        }
                        button {
                            class: "text-xs text-gray-400 hover:text-gray-600 transition-colors",
                            onclick: move |_| {
                                original.set(Some(query().clone()));
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
                p { class: "text-xs text-red-500 mt-2", "Error: {err}" }
            } else if res.results.is_empty() {
                p { class: "text-xs text-gray-400 mt-2", "No results" }
            } else {
                div { class: "mt-2 overflow-x-auto",
                    table { class: "w-full text-xs",
                        thead {
                            tr { class: "text-left text-gray-500",
                                th { class: "pb-1 pr-4 font-medium", "Labels" }
                                th { class: "pb-1 font-medium text-right", "Value" }
                            }
                        }
                        tbody {
                            for metric in res.results.iter() {
                                tr { class: "border-t border-gray-50",
                                    td { class: "py-1 pr-4 text-gray-600",
                                        {metric.labels.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")}
                                    }
                                    td { class: "py-1 text-right font-mono text-gray-900",
                                        "{metric.value}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
