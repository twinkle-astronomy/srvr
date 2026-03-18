use dioxus::prelude::*;

use crate::frontend::server_fns::{create_template, get_templates};

#[component]
pub fn Templates() -> Element {
    let mut templates = use_server_future(move || get_templates())?;
    let nav = use_navigator();

    let handle_new = move |_| {
        spawn(async move {
            match create_template("New Template".to_string(), String::new()).await {
                Ok(t) => {
                    nav.push(super::super::Route::TemplateEditor { id: t.id });
                }
                Err(e) => {
                    tracing::error!("Failed to create template: {e}");
                }
            }
        });
    };

    rsx! {
        div { class: "mb-8 flex items-center justify-between",
            div {
                h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Templates" }
                p { class: "text-gray-500 mt-1", "Manage display templates for your devices" }
            }
            div { class: "flex items-center gap-3",
                button {
                    class: "inline-flex items-center gap-2 px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                    onclick: handle_new,
                    "New Template"
                }
                button {
                    class: "inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-gray-700 border border-gray-200 rounded-lg hover:bg-gray-50 transition-colors",
                    onclick: move |_| templates.restart(),
                    "Refresh"
                }
            }
        }

        match templates() {
            Some(Ok(templates)) if templates.is_empty() => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                    div { class: "py-16 text-center",
                        p { class: "text-gray-400 text-lg", "No templates yet" }
                        p { class: "text-gray-300 text-sm mt-2", "Create a template to get started" }
                    }
                }
            },
            Some(Ok(templates)) => rsx! {
                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6",
                    for template in templates {
                        Link {
                            key: "{template.id}",
                            to: super::super::Route::TemplateEditor { id: template.id },
                            class: "block group",
                            div { class: "bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden group-hover:shadow-md group-hover:border-gray-200 transition-all",
                                div { class: "p-5",
                                    h3 { class: "font-medium text-gray-900 mb-1", "{template.name}" }
                                    p { class: "text-xs text-gray-400",
                                        "Updated {template.updated_at}"
                                    }
                                }
                            }
                        }
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
