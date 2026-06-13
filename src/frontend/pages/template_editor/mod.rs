use dioxus::prelude::*;

use crate::frontend::server_fns::{get_render_context_for_template, get_virtual_render_context};
use crate::frontend::store::AppStore;
use crate::models::{Device, RenderContext, RenderContextStoreExt};

pub mod template_preview;
use template_preview::TemplatePreview;

pub mod prometheus_queries;
use prometheus_queries::PrometheusQueries;

pub mod http_sources;
use http_sources::HttpSources;

pub mod template_variables;
use template_variables::TemplateVariables;

pub mod template_form;
use template_form::TemplateForm;

#[component]
pub fn TemplateEditor(id: i64) -> Element {
    let store = use_context::<AppStore>();
    let devices = store.devices;
    let mut render_context = use_store(|| None::<RenderContext>);
    let mut selected_device = use_store(|| None::<Device>);
    let render_error = use_store(|| None::<String>);
    let mut template_id = use_signal(|| id);
    if template_id() != id {
        template_id.set(id);
    }

    use_effect(move || match selected_device() {
        None if devices().len() > 0 => selected_device.set(devices().first().cloned()),
        None => selected_device.set(Some(Device::virtual_device())),
        _ => {}
    });

    use_resource(move || async move {
        let id = template_id();
        if let Some(selected_device) = selected_device() {
            let result = if selected_device.id == 0 {
                get_virtual_render_context(id).await
            } else {
                get_render_context_for_template(selected_device.id, id).await
            };
            if let Ok(v) = result {
                render_context.set(Some(v));
            }
        }
    });

    rsx! {
        div { class: "mb-8",
            div { class: "mb-2",
                Link {
                    to: super::super::Route::Templates {},
                    class: "inline-flex items-center gap-1.5 text-sm text-gray-500 hover:text-gray-900 transition-colors",
                    "\u{2190} Back to Templates"
                }
            }
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Template Editor" }
            p { class: "text-gray-500 mt-1", "Edit the SVG Liquid template used for device screens" }
        }
        if let Some(render_context) = render_context.transpose() {
            div { class: "flex flex-wrap items-start gap-6",
                TemplateForm {
                    render_context,
                    selected_device,
                    preview_error: render_error,
                }
                TemplatePreview {
                    render_context,
                    render_error
                }
            }

            PrometheusQueries { queries: render_context.prometheus_queries(), template: render_context.template() }
            HttpSources { sources: render_context.http_sources(), template: render_context.template() }

        }

    }
}
