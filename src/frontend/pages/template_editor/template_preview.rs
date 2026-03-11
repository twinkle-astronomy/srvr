use dioxus::prelude::*;

use crate::{frontend::server_fns::get_template_preview, models::RenderContext};

#[component]
pub fn TemplatePreview(
    render_context: WriteSignal<RenderContext>,
    render_error: WriteStore<Option<String>>,
) -> Element {
    let mut image = use_store(|| None::<String>);
    let mut preview_loading = use_signal(|| false);
    let (tx, mut rx) = tokio::sync::watch::channel(true);

    use_effect(move || {
        render_context();
        tx.send(true).ok();
    });
    spawn(async move {
        loop {
            if let Err(_) = rx.changed().await {
                break;
            }

            preview_loading.set(true);

            match get_template_preview(render_context().clone()).await {
                Ok(i) => {
                    render_error.set(None);

                    image.set(Some(i))
                }
                Err(e) => {
                    render_error.set(Some(format!("{:?}", e)));
                }
            }

            preview_loading.set(false);
        }
    });

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
                    style: "width: {render_context().device.width}px; height: {render_context().device.height}px;",

                    match image() {
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
