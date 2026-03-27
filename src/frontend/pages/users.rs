use dioxus::prelude::*;

use crate::frontend::server_fns::{check_auth, delete_user, get_all_users};
use crate::models::AuthenticatedUser;

#[component]
pub fn Users() -> Element {
    let mut users = use_server_future(move || get_all_users())?;
    let current_user = use_server_future(move || check_auth())?;

    let current_user_id = match current_user() {
        Some(Ok(Some(u))) => Some(u.id),
        _ => None,
    };

    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Users" }
            p { class: "text-gray-500 mt-1", "Manage user accounts" }
        }

        // Change password form
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6 mb-6",
            h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4", "Change Password" }
            form {
                action: "/auth/change-password",
                method: "POST",
                class: "flex items-end gap-3",

                div { class: "flex-1",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "current_password", "Current Password" }
                    input {
                        r#type: "password",
                        id: "current_password",
                        name: "current_password",
                        required: true,
                        class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                    }
                }

                div { class: "flex-1",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "new_password", "New Password" }
                    input {
                        r#type: "password",
                        id: "new_password",
                        name: "new_password",
                        required: true,
                        class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                    }
                }

                button {
                    r#type: "submit",
                    class: "px-4 py-1.5 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                    "Change"
                }
            }
        }

        // Create user form
        div { class: "bg-white rounded-xl shadow-sm border border-gray-100 p-6 mb-6",
            h2 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider mb-4", "Create User" }
            form {
                action: "/auth/create-user",
                method: "POST",
                class: "flex items-end gap-3",

                div { class: "flex-1",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "username", "Username" }
                    input {
                        r#type: "text",
                        id: "username",
                        name: "username",
                        required: true,
                        class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                    }
                }

                div { class: "flex-1",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", r#for: "password", "Password" }
                    input {
                        r#type: "password",
                        id: "password",
                        name: "password",
                        required: true,
                        class: "w-full text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-1 focus:ring-gray-300",
                    }
                }

                button {
                    r#type: "submit",
                    class: "px-4 py-1.5 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                    "Create"
                }
            }
        }

        // User list
        match users() {
            Some(Ok(user_list)) => rsx! {
                div { class: "bg-white rounded-xl shadow-sm border border-gray-100",
                    div { class: "divide-y divide-gray-100",
                        for user in user_list {
                            UserRow {
                                key: "{user.id}",
                                user: user.clone(),
                                is_current: current_user_id == Some(user.id),
                                on_delete: move |_| async move {
                                    users.restart();
                                },
                            }
                        }
                    }
                }
            },
            Some(Err(e)) => rsx! {
                p { class: "text-red-400 text-sm", "Error loading users: {e}" }
            },
            None => rsx! {
                div { class: "flex flex-col items-center justify-center py-12 gap-3",
                    div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                    p { class: "text-sm text-gray-400", "Loading..." }
                }
            },
        }
    }
}

#[component]
fn UserRow(user: AuthenticatedUser, is_current: bool, on_delete: EventHandler) -> Element {
    let mut deleting = use_signal(|| false);
    let user_id = user.id;

    rsx! {
        div { class: "flex items-center justify-between px-6 py-4",
            div { class: "flex items-center gap-3",
                div { class: "w-8 h-8 rounded-full bg-gray-100 flex items-center justify-center",
                    span { class: "text-sm font-medium text-gray-600",
                        "{user.username.chars().next().unwrap_or('?').to_uppercase()}"
                    }
                }
                div {
                    p { class: "text-sm font-medium text-gray-900", "{user.username}" }
                    if is_current {
                        span { class: "text-xs text-gray-400", "You" }
                    }
                }
            }
            if !is_current {
                button {
                    class: "px-3 py-1 text-xs font-medium text-red-600 hover:bg-red-50 rounded-lg transition-colors",
                    disabled: deleting(),
                    onclick: move |_| async move {
                        deleting.set(true);
                        if let Ok(()) = delete_user(user_id).await {
                            on_delete.call(());
                        }
                        deleting.set(false);
                    },
                    if deleting() { "Deleting..." } else { "Delete" }
                }
            }
        }
    }
}
