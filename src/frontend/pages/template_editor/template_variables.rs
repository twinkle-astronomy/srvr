use dioxus::prelude::*;

use crate::{frontend::server_fns::get_template_context, models::RenderContext};

#[component]
pub fn TemplateVariables(render_context: ReadStore<RenderContext>) -> Element {
    let mut vars_loading = use_signal(|| true);
    let mut vars = use_signal(|| vec![]);

    use_resource(move || {
        vars_loading.set(true);
        async move {
            let tv = get_template_context(render_context()).await;
            vars_loading.set(false);
            match tv {
                Ok(v) => {
                    vars.set(v);
                }
                Err(_) => {}
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
