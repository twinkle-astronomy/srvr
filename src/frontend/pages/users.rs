use dioxus::prelude::*;

use crate::frontend::store::AppStore;
use crate::models::AuthenticatedUser;

#[component]
pub fn Users() -> Element {
    let store = use_context::<AppStore>();
    let users = store.users;
    let users_loaded = store.users_loaded;
    let current_user = store.current_user;

    let current_user_id = current_user().map(|u| u.id);

    rsx! {
        div { class: "mb-8",
            h1 { class: "text-3xl font-bold text-gray-900 tracking-tight", "Users" }
            p { class: "text-gray-500 mt-1", "Manage user accounts" }
        }

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

        if !users_loaded() {
            div { class: "flex flex-col items-center justify-center py-12 gap-3",
                div { class: "w-6 h-6 border-2 border-gray-200 border-t-gray-900 rounded-full animate-spin" }
                p { class: "text-sm text-gray-400", "Loading..." }
            }
        } else {
            div { class: "bg-white rounded-xl shadow-sm border border-gray-100",
                div { class: "divide-y divide-gray-100",
                    for user in users() {
                        UserRow {
                            key: "{user.id}",
                            user: user.clone(),
                            is_current: current_user_id == Some(user.id),
                        }
                    }
                }
            }
        }
    }
}

// Characterization tests for the store-driven Users page and UserRow logic.
// Users has no router dependency, so its loaded list renders natively.
#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::frontend::store::AppStore;
    use crate::frontend::test_harness::render_with_store;

    fn au(id: i64, username: &str) -> AuthenticatedUser {
        AuthenticatedUser {
            id,
            username: username.to_string(),
        }
    }

    fn store_users_loading() -> AppStore {
        AppStore::new()
    }

    fn store_users_two() -> AppStore {
        let mut s = AppStore::new();
        s.users.set(vec![au(1, "alice"), au(2, "bob")]);
        s.current_user.set(Some(au(1, "alice")));
        s.users_loaded.set(true);
        s
    }

    fn store_users_only_self() -> AppStore {
        let mut s = AppStore::new();
        s.users.set(vec![au(1, "alice")]);
        s.current_user.set(Some(au(1, "alice")));
        s.users_loaded.set(true);
        s
    }

    #[test]
    fn users_page_shows_spinner_before_load() {
        let html = render_with_store(store_users_loading, Users);
        assert!(
            html.contains("Loading..."),
            "expected spinner before users load, got: {html:?}"
        );
    }

    #[test]
    fn loaded_users_are_listed_by_username() {
        let html = render_with_store(store_users_two, Users);
        assert!(html.contains("alice"), "expected alice listed, got: {html:?}");
        assert!(html.contains("bob"), "expected bob listed, got: {html:?}");
        assert!(
            !html.contains("Loading..."),
            "must not show spinner once loaded, got: {html:?}"
        );
    }

    #[test]
    fn current_user_is_marked_and_not_deletable() {
        // Only the current user present: the "You" badge shows and there is no
        // Delete button (the `if !is_current` branch is skipped).
        let html = render_with_store(store_users_only_self, Users);
        assert!(
            html.contains("You"),
            "expected the current-user 'You' badge, got: {html:?}"
        );
        assert!(
            !html.contains("Delete"),
            "current user must not have a Delete button, got: {html:?}"
        );
    }

    #[test]
    fn other_users_have_a_delete_button() {
        // alice (current) + bob (other): bob is deletable.
        let html = render_with_store(store_users_two, Users);
        assert!(
            html.contains("Delete"),
            "expected a Delete button for the non-current user, got: {html:?}"
        );
    }
}

// Browser-tier test: mounts the Users page in a real headless browser via
// dioxus-web and asserts on the real DOM. Run with:
//   cargo test --no-default-features --features web --target wasm32-unknown-unknown
#[cfg(all(test, target_arch = "wasm32"))]
mod web_tests {
    use super::*;
    use crate::frontend::store::AppStore;
    use crate::frontend::test_harness::mount_with_store;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    fn au(id: i64, username: &str) -> AuthenticatedUser {
        AuthenticatedUser {
            id,
            username: username.to_string(),
        }
    }

    fn store_two() -> AppStore {
        let mut s = AppStore::new();
        s.users.set(vec![au(1, "alice"), au(2, "bob")]);
        s.current_user.set(Some(au(1, "alice")));
        s.users_loaded.set(true);
        s
    }

    #[wasm_bindgen_test]
    async fn users_page_renders_in_real_browser() {
        let root = mount_with_store(store_two, Users).await;
        let text = root.text_content().unwrap_or_default();
        assert!(text.contains("alice"), "expected alice in DOM, got: {text:?}");
        assert!(text.contains("bob"), "expected bob in DOM, got: {text:?}");
        assert!(
            text.contains("You"),
            "expected the current-user 'You' badge in DOM, got: {text:?}"
        );
    }
}

#[component]
fn UserRow(user: AuthenticatedUser, is_current: bool) -> Element {
    let store = use_context::<AppStore>();
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
                        if let Err(e) = store.delete_user(user_id).await {
                            tracing::error!("Failed to delete user: {e}");
                        }
                        deleting.set(false);
                    },
                    if deleting() { "Deleting..." } else { "Delete" }
                }
            }
        }
    }
}
