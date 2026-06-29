//! Test-only harness for rendering Dioxus components natively (no browser).
//!
//! These helpers drive the *real* dioxus-core reactive runtime (signals, hooks,
//! effects) and serialize the resulting tree to an HTML string via dioxus-ssr.
//! SSR is used purely as a *readout* for assertions — it is not the runtime the
//! app uses (the app runs as client-side WASM). There is deliberately no DOM
//! event dispatch here; genuine click/input interaction would belong to a
//! browser tier (see docs/projects/completed/20260628-frontend-test-harness.md).
//!
//! [`StoreHost`] builds + provides an [`AppStore`] inside a real scope (signal
//! creation requires one), then renders the page under test.

use crate::frontend::store::AppStore;
use dioxus::core::ComponentFunction;
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

/// Render a component with the given props to an HTML string.
///
/// Builds a real `VirtualDom`, performs the initial render, and serializes the
/// tree with dioxus-ssr. Components that fetch via `use_resource` will show their
/// loading/`None` branch here (the async task is spawned, not awaited).
pub(crate) fn render_with_props<P, M>(
    component: impl ComponentFunction<P, M>,
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
/// Branches that render `Link` / use the navigator need a router and can't be
/// rendered by this native tier.
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
