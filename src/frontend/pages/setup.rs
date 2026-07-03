use dioxus::prelude::*;

use crate::frontend::{
    Route,
    server_fns::{check_needs_setup, setup},
};

#[component]
pub fn Setup() -> Element {
    let mut needs_setup = use_signal(|| None::<bool>);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);
    let mut completed = use_signal(|| false);
    let nav = navigator();

    use_effect(move || {
        spawn(async move {
            if let Ok(v) = check_needs_setup().await {
                needs_setup.set(Some(v));
            }
        });
    });

    if let Some(false) = needs_setup() {
        nav.push(Route::Login {});
        return rsx! { p { class: "text-gray-400 text-center mt-20", "Redirecting to login..." } };
    }

    if completed() {
        nav.push(Route::Dashboard {});
        return rsx! { p { class: "text-gray-400 text-center mt-20", "Redirecting..." } };
    }

    rsx! {
        div { class: "min-h-screen flex items-center justify-center bg-gray-50",
            div { class: "w-full max-w-sm",
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-8",
                    div { class: "text-center mb-8",
                        h1 { class: "text-2xl font-bold text-gray-900 tracking-tight", "TRMNL" }
                        p { class: "text-gray-500 mt-1 text-sm", "Create your admin account" }
                    }

                    if let Some(ref msg) = error() {
                        div { class: "mb-4 p-3 bg-red-50 border border-red-200 rounded-lg",
                            p { class: "text-sm text-red-600", "{msg}" }
                        }
                    }

                    form {
                        class: "space-y-4",
                        onsubmit: move |event| {
                            event.prevent_default();
                            let u = username();
                            let p = password();
                            error.set(None);
                            submitting.set(true);
                            spawn(async move {
                                match setup(u, p).await {
                                    Ok(_) => completed.set(true),
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                submitting.set(false);
                            });
                        },

                        div {
                            label {
                                class: "block text-sm font-medium text-gray-700 mb-1",
                                r#for: "username",
                                "Username"
                            }
                            input {
                                r#type: "text",
                                id: "username",
                                name: "username",
                                required: true,
                                autocomplete: "username",
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-2 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                value: "{username()}",
                                oninput: move |e| username.set(e.value()),
                            }
                        }

                        div {
                            label {
                                class: "block text-sm font-medium text-gray-700 mb-1",
                                r#for: "password",
                                "Password"
                            }
                            input {
                                r#type: "password",
                                id: "password",
                                name: "password",
                                required: true,
                                autocomplete: "new-password",
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-2 focus:outline-none focus:ring-1 focus:ring-gray-300",
                                value: "{password()}",
                                oninput: move |e| password.set(e.value()),
                            }
                        }

                        button {
                            r#type: "submit",
                            disabled: submitting(),
                            class: "w-full px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors disabled:opacity-50",
                            if submitting() { "Creating account..." } else { "Create Account" }
                        }
                    }
                }
            }
        }
    }
}
