use dioxus::prelude::*;

use crate::frontend::server_fns::{
    create_prometheus_query, delete_prometheus_query, execute_prometheus_queries, get_devices,
    get_prometheus_queries_for_template, get_template, get_template_context, get_template_preview,
    save_template, update_prometheus_query,
};
use crate::models::{Device, PrometheusQuery, PrometheusQueryResult, Template};

#[component]
pub fn TemplateEditor() -> Element {
    let mut devices = use_store(|| vec![]);
    let mut selected_device = use_store(|| None::<Device>);
    let mut template = use_store(|| None::<Template>);
    let mut image = use_store(|| None::<String>);
    let mut render_error = use_store(|| None::<String>);
    let mut preview_loading = use_signal(|| false);
    let mut preview_pending = use_signal(|| true);
    let mut preview_skipped = use_signal(||false);

    use_effect(move || match selected_device() {
        None if devices().len() > 0 => selected_device.set(devices().first().cloned()),
        _ => {}
    });

    use_resource(move || async move {
        match get_devices().await {
            Ok(v) => {
                devices.set(v);
            }
            Err(_) => {}
        }
    });

    use_resource(move || async move {
        match get_template().await {
            Ok(v) => {
                template.set(Some(v));
            }
            Err(_) => {}
        }
    });

    use_effect(move || {
        if let (Some(device), Some(template)) = (selected_device(), template()) {
            if !(preview_pending() || preview_skipped()) {
                return;
            }

            if *preview_loading.peek() {
                preview_skipped.set(true);
                return;
            }
            if preview_skipped() {
                preview_skipped.set(false);
            }
            spawn(async move {
                preview_loading.set(true);
                match get_template_preview(device.id, template).await {
                    Ok(i) => {
                        render_error.set(None);

                        image.set(Some(i))
                    }
                    Err(e) => {
                        render_error.set(Some(format!("{:?}", e)));
                    }
                }
                preview_pending.set(false);
                preview_loading.set(false);
            });
        }
    });

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
                preview_error: render_error,
                preview_pending
            }
            TemplatePreview {
                selected_device,
                preview_loading,
                preview_b64: image,
            }
        }

        if let Some(template) = template() {
            PrometheusQueries { template_id: template.id }
        }

    }
}

#[component]
fn TemplateForm(
    mut template: WriteStore<Option<Template>>,
    devices: ReadStore<Vec<Device>>,
    mut selected_device: WriteStore<Option<Device>>,
    // save_status: Signal<Option<Result<(), String>>>,
    preview_error: ReadStore<Option<String>>,
    preview_pending: Signal<bool>,
    // on_preview: EventHandler,
    // on_save: EventHandler,
) -> Element {
    let mut save_status = use_signal(|| None::<Result<(), String>>);

    let selected_id = use_memo(move || selected_device.read().as_ref().map(|d| d.id));
    let template_id = use_memo(move || template.read().as_ref().map(|d| d.id));

    rsx! {
        div { class: "flex-1 min-w-0 flex flex-col gap-4",
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                div { class: "p-4 border-b border-gray-100 flex items-center justify-between",
                    span { class: "text-sm font-medium text-gray-700", "Template" }
                        if !devices().is_empty() {
                                select {
                                    class: "text-sm border border-gray-200 rounded-lg px-2 py-1 text-gray-600",
                                    onchange: move |evt| {
                                        if let Ok(idx) = evt.value().parse::<usize>() {
                                            if let Some(d) = devices().get(idx) {
                                                selected_device.set(Some(d.clone()));
                                                preview_pending.set(true)
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

                textarea {
                    class: "w-full h-96 p-4 font-mono text-sm text-gray-800 bg-gray-50 border-0 focus:outline-none focus:ring-0 resize-y",
                    spellcheck: false,
                    value: template().map(|x| x.content).unwrap_or_else(|| "".to_string()),
                    oninput: move |evt| {
                        save_status.set(None);

                        if let Some(template) = template.write().as_mut() {
                            info!("Updating template");

                            preview_pending.set(true);
                            template.content = evt.value();
                        }
                    }
                }

            }

            div { class: "flex items-center gap-3",
                button {
                    class: "inline-flex items-center gap-2 px-4 py-2 bg-green-700 text-white text-sm font-medium rounded-lg hover:bg-green-600 transition-colors",
                    onclick: move |_| {
                        save_status.set(None);
                        spawn(async move {
                            if let Some(template) = template() {
                                match save_template(template.id, template.content).await {
                                    Ok(()) => save_status.set(Some(Ok(()))),
                                    Err(e) => save_status.set(Some(Err(e.to_string()))),
                                }
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
            }
            TemplateVariables {device_id: selected_id, template_id }
        }
    }
}

#[component]
fn TemplateVariables(device_id: Memo<Option<i64>>, template_id: Memo<Option<i64>>) -> Element {
    let mut vars_loading = use_signal(|| true);
    let mut vars = use_signal(|| vec![]);

    use_resource(move || {
        vars_loading.set(true);
        async move {
            if let (Some(device_id), Some(template_id)) = (device_id(), template_id()) {
                let tv = get_template_context(device_id, template_id).await;
                vars_loading.set(false);
                match tv {
                    Ok(v) => {
                        vars.set(v);
                    }
                    Err(_) => {}
                }
            }
        }
    });

    let mut vars_open = use_signal(|| false);
    rsx! {
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
            button {
                class: "w-full px-4 py-2.5 flex items-center justify-between text-left hover:bg-gray-50 transition-colors",
                onclick: move |_| vars_open.set(!vars_open()),
                div { class: "flex items-center gap-2",
                    span { class: "text-xs font-medium text-gray-600", "Available Template Variables" }
                    if vars_loading() {
                        div { class: "w-3 h-3 border-2 border-gray-200 border-t-gray-400 rounded-full animate-spin" }
                    }
                }
                span { class: "text-xs text-gray-400", if vars_open() { "▲" } else { "▼" } }

            }
            if vars_open() {
                div { class: "border-t border-gray-100 p-4",

                    table { class: "w-full text-xs",
                        thead {
                            tr { class: "text-left text-gray-400 border-b border-gray-100",
                                th { class: "pb-1.5 font-medium pr-4", "Variable" }
                                th { class: "pb-1.5 font-medium", "Value" }
                            }
                        }
                        tbody { class: "divide-y divide-gray-50",
                            for var in vars.iter() {
                                tr {
                                    td { class: "py-1.5 pr-4",
                                        code { class: "text-blue-700 bg-blue-50 px-1 rounded",
                                            {format!("{{{{ {} }}}}", var.path)}
                                        }
                                    }
                                    td {
                                        class: if var.is_error { "py-1.5 font-mono text-red-500" } else { "py-1.5 font-mono text-gray-700" },
                                        "{var.value}"
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

#[component]
fn TemplatePreview(
    selected_device: WriteStore<Option<Device>>,
    preview_loading: Signal<bool>,
    preview_b64: ReadStore<Option<String>>,
) -> Element {
    match selected_device() {
        Some(device) => {
            rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                    div { class: "p-4 border-b border-gray-100",
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm font-medium text-gray-700", "Preview" }
                            if preview_loading() {
                                div { class: "w-3 h-3 border-2 border-gray-200 border-t-gray-400 rounded-full animate-spin" }
                            }
                        }
                    }
                    div { class: "p-4 bg-gray-50",
                        div {
                            class: "flex items-center justify-center bg-white border border-gray-200 rounded shadow-sm",
                            style: "width: {device.width}px; height: {device.height}px;",

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
        None => {
            rsx! { "Loading" }
        }
    }
}

#[component]
fn PrometheusQueries(template_id: i64) -> Element {
    let mut queries = use_server_future(move || get_prometheus_queries_for_template(template_id))?;
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

    let can_save =
        !name().trim().is_empty() && !addr().trim().is_empty() && !query().trim().is_empty();

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

    let can_save = !edit_name().trim().is_empty()
        && !edit_addr().trim().is_empty()
        && !edit_query().trim().is_empty();

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
