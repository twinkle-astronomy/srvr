use dioxus::prelude::*;

use crate::frontend::Route;
use crate::frontend::server_fns::check_needs_setup;

#[component]
pub fn Setup() -> Element {
    let needs_setup = use_server_future(move || check_needs_setup())?;
    let nav = navigator();

    if let Some(Ok(false)) = needs_setup() {
        nav.push(Route::Login {});
        return rsx! { p { class: "text-gray-400 text-center mt-20", "Redirecting to login..." } };
    }

    rsx! {
        div { class: "min-h-screen flex items-center justify-center bg-gray-50",
            div { class: "w-full max-w-sm",
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-8",
                    div { class: "text-center mb-8",
                        h1 { class: "text-2xl font-bold text-gray-900 tracking-tight", "TRMNL" }
                        p { class: "text-gray-500 mt-1 text-sm", "Create your admin account" }
                    }

                    form {
                        action: "/auth/setup",
                        method: "POST",
                        class: "space-y-4",

                        div {
                            label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "username", "Username" }
                            input {
                                r#type: "text",
                                id: "username",
                                name: "username",
                                required: true,
                                autocomplete: "username",
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-2 focus:outline-none focus:ring-1 focus:ring-gray-300",
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "password", "Password" }
                            input {
                                r#type: "password",
                                id: "password",
                                name: "password",
                                required: true,
                                autocomplete: "new-password",
                                class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-2 focus:outline-none focus:ring-1 focus:ring-gray-300",
                            }
                        }

                        button {
                            r#type: "submit",
                            class: "w-full px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                            "Create Account"
                        }
                    }
                }
            }
        }
    }
}
