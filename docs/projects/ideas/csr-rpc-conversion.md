# Client-Side Rendering + Explicit API

Make the dashboard a client-side-rendered (CSR) app talking to the backend over a
plain HTTP API, instead of a server-side-rendered Dioxus fullstack app that
hydrates on load.

Two things this unlocks:

- **Fast, isolated frontend tests.** Today a top-level page can't be mounted in a
  test browser because the fullstack build always tries to hydrate against
  server-rendered HTML, so an isolated mount panics before any component runs (see
  [20260628-frontend-test-harness](../completed/20260628-frontend-test-harness.md)).
  A CSR build removes forced hydration, so pages can be mounted and exercised
  directly — real clicks, real DOM, no running server — closing the gap the native
  test tier can't reach (router-dependent rendering, interactions).
- **A clearer, inspectable client/server boundary.** The data layer becomes an
  ordinary HTTP API you can call with `curl`, log, and reason about, rather than
  opaque framework-generated RPC.

The friction this removes: the frontend's only test signal stops at "renders the
expected HTML in a headless runtime," and the client/server contract is implicit.

The main effort is not the number of endpoints — a single shared wrapper makes the
existing server functions convert mechanically — but the switch from SSR to CSR
serving and the loss of the fullstack integration. The technical analysis and a
preserved WIP spike live on the `csr-rpc-conversion` branch and in the completed
test-harness doc.
