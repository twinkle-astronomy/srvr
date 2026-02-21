use dioxus::prelude::*;

#[component]
pub fn ScreenPreview() -> Element {
    let mut refresh_count = use_signal(|| 0u32);
    let mut width = use_signal(|| 800u32);
    let mut height = use_signal(|| 480u32);

    // let screen = use_server_future(move || {
    //     let _ = refresh_count();
    //     let w = width();
    //     let h = height();
    //     get_screen_preview(1)
    // })?;

    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Screen Preview" }
            p { class: "text-gray-500 mt-1", "Full-size preview of the e-ink display output" }
        }

        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6",
            div { class: "flex items-center justify-between mb-4",
                h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider",
                    "Display Output "
                    span { class: "font-mono text-gray-500", "({width}\u{00d7}{height})" }
                }
                div { class: "flex items-center gap-2",
                    button {
                        class: "px-3 py-1.5 text-xs font-medium rounded-md transition-colors bg-gray-100 text-gray-600 hover:bg-gray-200",
                        onclick: move |_| {
                            width.set(800);
                            height.set(480);
                            refresh_count += 1;
                        },
                        "800\u{00d7}480"
                    }
                    button {
                        class: "px-3 py-1.5 text-xs font-medium rounded-md transition-colors bg-gray-100 text-gray-600 hover:bg-gray-200",
                        onclick: move |_| {
                            width.set(480);
                            height.set(800);
                            refresh_count += 1;
                        },
                        "480\u{00d7}800"
                    }
                    button {
                        class: "px-4 py-1.5 text-xs font-medium rounded-md transition-colors bg-gray-900 text-white hover:bg-gray-700",
                        onclick: move |_| refresh_count += 1,
                        "Refresh"
                    }
                }
            }

            // div { class: "bg-gray-50 rounded-lg p-6 flex items-center justify-center min-h-64",
            //     match screen() {
            //         Some(Ok(Some(b64))) => rsx! {
            //             img {
            //                 src: "data:image/bmp;base64,{b64}",
            //                 alt: "E-ink screen preview",
            //                 class: "max-w-full h-auto border border-gray-200 rounded shadow-sm",
            //                 style: "image-rendering: pixelated;",
            //                 width: "{width}",
            //             }
            //         },
            //         Some(Ok(None)) => rsx! {
            //             div { class: "py-20 text-gray-400 text-sm", "Unable to render screen preview" }
            //         },
            //         Some(Err(e)) => rsx! {
            //             div { class: "py-20 text-red-400 text-sm", "Render error: {e}" }
            //         },
            //         None => rsx! {
            //             div { class: "flex flex-col items-center justify-center py-20 gap-3",
            //                 div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
            //                 p { class: "text-sm text-gray-400", "Rendering..." }
            //             }
            //         },
            //     }
            // }
        }
    }
}
