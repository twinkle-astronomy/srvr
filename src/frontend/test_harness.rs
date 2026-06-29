//! Test-only harness for rendering Dioxus components.
//!
//! Two tiers share this module:
//!
//! * **Native tier** (`feature = "server"`): drives the real dioxus-core runtime
//!   and serializes the tree to HTML with dioxus-ssr. SSR is only a *readout* for
//!   assertions — it is not the runtime the app uses. No DOM events.
//! * **Browser tier** (`target_arch = "wasm32"`): mounts a component into a real
//!   DOM via dioxus-web inside a headless browser (run with `wasm-bindgen-test`),
//!   exercising the actual dioxus-web layer, real clicks/inputs, and reactivity.
//!
//! Both tiers reuse [`StoreHost`] to build + provide an [`AppStore`] inside a real
//! scope (signal creation requires one), then render the page under test.
//!
//! See docs/projects/plans/frontend-test-harness.md.

use crate::frontend::store::AppStore;
use dioxus::prelude::*;

// Newtypes around the fn pointers passed into `StoreHost`'s props. The
// `#[component]` macro derives `PartialEq` for memoization; comparing raw fn
// pointers with `==` triggers the `unpredictable_function_pointer_comparisons`
// lint, so we wrap them and compare with `fn_addr_eq` explicitly.
#[derive(Clone, Copy)]
struct StoreBuilder(fn() -> AppStore);
impl PartialEq for StoreBuilder {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.0, other.0)
    }
}

#[derive(Clone, Copy)]
struct PageComponent(fn() -> Element);
impl PartialEq for PageComponent {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.0, other.0)
    }
}

/// Host component: builds + provides an [`AppStore`] inside a real scope (so
/// signal creation is valid) then renders the page under test as a child scope.
#[component]
#[allow(non_snake_case)]
fn StoreHost(make_store: StoreBuilder, page: PageComponent) -> Element {
    use_context_provider(make_store.0);
    let Page = page.0;
    rsx! { Page {} }
}

// ---------------------------------------------------------------------------
// Native tier (SSR readout)
// ---------------------------------------------------------------------------

/// Render a component with the given props to an HTML string.
///
/// Builds a real `VirtualDom`, performs the initial render, and serializes the
/// tree with dioxus-ssr. Components that fetch via `use_resource` will show their
/// loading/`None` branch here (the async task is spawned, not awaited).
#[cfg(feature = "server")]
pub(crate) fn render_with_props<P, M>(
    component: impl dioxus::core::ComponentFunction<P, M>,
    props: P,
) -> String
where
    P: Clone + 'static,
    M: 'static,
{
    let mut dom = VirtualDom::new_with_props(component, props);
    dom.rebuild_in_place();
    dioxus::ssr::render(&dom)
}

/// Render a zero-prop, store-driven page with an [`AppStore`] injected as
/// context, returning the HTML string.
///
/// `make_store` runs inside the Dioxus runtime (so signal creation is valid) and
/// should construct and populate the store the way `NavLayout` would (e.g. set
/// `devices_loaded` / seed `devices`). This is how store-driven pages (`Devices`,
/// `Templates`, `Users`, `Dashboard`) are tested without any server functions.
/// Branches that render `Link` / use the navigator need a router and are covered
/// by the browser tier instead.
#[cfg(feature = "server")]
pub(crate) fn render_with_store(make_store: fn() -> AppStore, page: fn() -> Element) -> String {
    let mut dom = VirtualDom::new_with_props(
        StoreHost,
        StoreHostProps {
            make_store: StoreBuilder(make_store),
            page: PageComponent(page),
        },
    );
    dom.rebuild_in_place();
    dioxus::ssr::render(&dom)
}

// ---------------------------------------------------------------------------
// Browser tier (real DOM via dioxus-web)
// ---------------------------------------------------------------------------

/// Mount a zero-prop, store-driven page into a fresh root `<div>` in the live
/// document and let dioxus-web perform the initial render. Returns the root
/// element so the test can inspect/interact with the real DOM.
///
/// Only available on wasm32; run via `wasm-bindgen-test` in a headless browser.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn mount_with_store(
    make_store: fn() -> AppStore,
    page: fn() -> Element,
) -> web_sys::Element {
    use dioxus::web::{launch::launch_virtual_dom, Config};

    let document = web_sys::window()
        .expect("window")
        .document()
        .expect("document");
    let root = document.create_element("div").expect("create root div");
    let id = unique_root_id();
    root.set_id(&id);
    document
        .body()
        .expect("body")
        .append_child(&root)
        .expect("append root");

    let vdom = VirtualDom::new_with_props(
        StoreHost,
        StoreHostProps {
            make_store: StoreBuilder(make_store),
            page: PageComponent(page),
        },
    );
    launch_virtual_dom(vdom, Config::new().rootname(id.clone()));

    // dioxus-web renders via `spawn_local`; yield so the initial render lands in
    // the DOM before the test inspects it.
    gloo_timers::future::TimeoutFuture::new(50).await;

    document.get_element_by_id(&id).expect("root present in DOM")
}

#[cfg(target_arch = "wasm32")]
fn unique_root_id() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    format!("test-root-{}", N.fetch_add(1, Ordering::Relaxed))
}
