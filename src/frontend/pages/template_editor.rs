use std::ops::DerefMut;

use dioxus::prelude::*;

use crate::frontend::server_fns::{
    create_prometheus_query, delete_prometheus_query, execute_prometheus_queries, get_devices,
    get_prometheus_queries_for_template, get_template, get_template_preview, save_template,
    update_prometheus_query,
};
use crate::models::{Device, PrometheusQuery, PrometheusQueryResult, Template};
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

        if let Some(ref tmpl) = template() {
            PrometheusQueries { template_id: tmpl.id }
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

#[component]
fn PrometheusQueries(template_id: i64) -> Element {
    let mut queries =
        use_server_future(move || get_prometheus_queries_for_template(template_id))?;
    let mut query_results = use_signal(|| None::<Vec<PrometheusQueryResult>>);
    let mut results_loading = use_signal(|| false);
    let mut show_add_form = use_signal(|| false);

    let query_list = match queries() {
        Some(Ok(ref q)) => q.clone(),
        _ => vec![],
    };

    let fetch_results = move |_| {
        results_loading.set(true);
        spawn(async move {
            match execute_prometheus_queries(template_id).await {
                Ok(results) => query_results.set(Some(results)),
                Err(e) => {
                    tracing::error!("Failed to execute queries: {e}");
                    query_results.set(None);
                }
            }
            results_loading.set(false);
        });
    };

    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            div { class: "p-4 border-b border-gray-100 flex items-center justify-between",
                span { class: "text-sm font-medium text-gray-700",
                    "Prometheus Queries ({query_list.len()})"
                }
                div { class: "flex items-center gap-2",
                    if !query_list.is_empty() {
                        button {
                            class: "inline-flex items-center gap-2 px-3 py-1.5 bg-gray-900 text-white text-xs font-medium rounded-lg hover:bg-gray-700 transition-colors disabled:opacity-50",
                            disabled: results_loading(),
                            onclick: fetch_results,
                            if results_loading() { "Running..." } else { "Run Queries" }
                        }
                    }
                    button {
                        class: "inline-flex items-center gap-2 px-3 py-1.5 bg-green-700 text-white text-xs font-medium rounded-lg hover:bg-green-600 transition-colors",
                        onclick: move |_| show_add_form.set(!show_add_form()),
                        if show_add_form() { "Cancel" } else { "Add Query" }
                    }
                }
            }
            if show_add_form() {
                AddQueryForm {
                    template_id,
                    on_saved: move |_| {
                        show_add_form.set(false);
                        queries.restart();
                    },
                }
            }
            if !query_list.is_empty() {
                div { class: "divide-y divide-gray-100",
                    for query in query_list.iter() {
                        QueryRow {
                            query: query.clone(),
                            result: query_results().and_then(|results| {
                                results.into_iter().find(|r| r.query_name == query.name)
                            }),
                            on_changed: move |_| queries.restart(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AddQueryForm(template_id: i64, on_saved: EventHandler) -> Element {
    let mut name = use_signal(String::new);
    let mut addr = use_signal(|| "http://prometheus:9090".to_string());
    let mut query = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let can_save = !name().trim().is_empty() && !addr().trim().is_empty() && !query().trim().is_empty();

    rsx! {
        div { class: "p-4 border-b border-gray-100 bg-gray-50",
            div { class: "flex flex-col gap-3",
                div { class: "flex gap-3",
                    div { class: "flex-1",
                        label { class: "block text-xs font-medium text-gray-500 mb-1", "Name" }
                        input {
                            class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                            placeholder: "e.g. scrape_duration",
                            value: "{name}",
                            oninput: move |evt| name.set(evt.value()),
                        }
                    }
                    div { class: "flex-1",
                        label { class: "block text-xs font-medium text-gray-500 mb-1", "Prometheus Address" }
                        input {
                            class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                            value: "{addr}",
                            oninput: move |evt| addr.set(evt.value()),
                        }
                    }
                }
                div {
                    label { class: "block text-xs font-medium text-gray-500 mb-1", "PromQL Query" }
                    input {
                        class: "w-full text-sm font-mono border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                        placeholder: "e.g. up",
                        value: "{query}",
                        oninput: move |evt| query.set(evt.value()),
                    }
                }
                div { class: "flex items-center gap-3",
                    button {
                        class: "inline-flex items-center px-3 py-1.5 bg-green-700 text-white text-xs font-medium rounded-lg hover:bg-green-600 transition-colors disabled:opacity-50",
                        disabled: !can_save || saving(),
                        onclick: move |_| {
                            let n = name().trim().to_string();
                            let a = addr().trim().to_string();
                            let q = query().trim().to_string();
                            saving.set(true);
                            error.set(None);
                            spawn(async move {
                                match create_prometheus_query(template_id, n, a, q).await {
                                    Ok(()) => on_saved.call(()),
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                saving.set(false);
                            });
                        },
                        if saving() { "Saving..." } else { "Save Query" }
                    }
                    if let Some(ref err) = error() {
                        span { class: "text-xs text-red-500", "{err}" }
                    }
                }
            }
        }
    }
}

#[component]
fn QueryRow(
    query: PrometheusQuery,
    result: Option<PrometheusQueryResult>,
    on_changed: EventHandler,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut edit_name = use_signal(|| query.name.clone());
    let mut edit_addr = use_signal(|| query.addr.clone());
    let mut edit_query = use_signal(|| query.query.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let query_id = query.id;

    let can_save =
        !edit_name().trim().is_empty() && !edit_addr().trim().is_empty() && !edit_query().trim().is_empty();

    if editing() {
        return rsx! {
            div { class: "p-4 bg-gray-50",
                div { class: "flex flex-col gap-3",
                    div { class: "flex gap-3",
                        div { class: "flex-1",
                            label { class: "block text-xs font-medium text-gray-500 mb-1", "Name" }
                            input {
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                value: "{edit_name}",
                                oninput: move |evt| edit_name.set(evt.value()),
                            }
                        }
                        div { class: "flex-1",
                            label { class: "block text-xs font-medium text-gray-500 mb-1", "Prometheus Address" }
                            input {
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                value: "{edit_addr}",
                                oninput: move |evt| edit_addr.set(evt.value()),
                            }
                        }
                    }
                    div {
                        label { class: "block text-xs font-medium text-gray-500 mb-1", "PromQL Query" }
                        input {
                            class: "w-full text-sm font-mono border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                            value: "{edit_query}",
                            oninput: move |evt| edit_query.set(evt.value()),
                        }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "inline-flex items-center px-3 py-1.5 bg-green-700 text-white text-xs font-medium rounded-lg hover:bg-green-600 transition-colors disabled:opacity-50",
                            disabled: !can_save || saving(),
                            onclick: move |_| {
                                let n = edit_name().trim().to_string();
                                let a = edit_addr().trim().to_string();
                                let q = edit_query().trim().to_string();
                                saving.set(true);
                                error.set(None);
                                spawn(async move {
                                    match update_prometheus_query(query_id, n, a, q).await {
                                        Ok(()) => {
                                            editing.set(false);
                                            on_changed.call(());
                                        }
                                        Err(e) => error.set(Some(e.to_string())),
                                    }
                                    saving.set(false);
                                });
                            },
                            if saving() { "Saving..." } else { "Save" }
                        }
                        button {
                            class: "inline-flex items-center px-3 py-1.5 bg-white text-gray-700 text-xs font-medium rounded-lg border border-gray-200 hover:bg-gray-50 transition-colors",
                            onclick: move |_| {
                                edit_name.set(query.name.clone());
                                edit_addr.set(query.addr.clone());
                                edit_query.set(query.query.clone());
                                editing.set(false);
                            },
                            "Cancel"
                        }
                        button {
                            class: "inline-flex items-center px-3 py-1.5 bg-red-600 text-white text-xs font-medium rounded-lg hover:bg-red-500 transition-colors disabled:opacity-50",
                            disabled: saving(),
                            onclick: move |_| {
                                saving.set(true);
                                spawn(async move {
                                    match delete_prometheus_query(query_id).await {
                                        Ok(()) => on_changed.call(()),
                                        Err(e) => error.set(Some(e.to_string())),
                                    }
                                    saving.set(false);
                                });
                            },
                            "Delete"
                        }
                        if let Some(ref err) = error() {
                            span { class: "text-xs text-red-500", "{err}" }
                        }
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "p-4",
            div { class: "flex items-start justify-between gap-4 mb-1",
                div {
                    span { class: "text-sm font-medium text-gray-900", "{query.name}" }
                    p { class: "text-xs text-gray-400 mt-0.5",
                        "{query.addr}"
                    }
                }
                div { class: "flex items-center gap-2",
                    code { class: "text-xs bg-gray-50 text-gray-600 px-2 py-1 rounded shrink-0",
                        "{query.query}"
                    }
                    button {
                        class: "text-xs text-gray-400 hover:text-gray-600 transition-colors",
                        onclick: move |_| editing.set(true),
                        "Edit"
                    }
                }
            }
            if let Some(ref res) = result {
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
}
