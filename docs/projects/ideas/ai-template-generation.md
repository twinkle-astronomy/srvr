# AI-Assisted Template Generation

Use Claude to iteratively generate and refine Liquid SVG templates from a
natural-language description. The user describes what they want; Claude proposes
an SVG, sees it rendered on the eink canvas, and revises until satisfied.

## Architecture

**The AI integration lives entirely in the frontend (WASM).** The server plays
no role in orchestrating Claude — it only provides the endpoints the template
editor already exposes. This keeps the server simple and avoids coupling the
core app to any AI provider.

**The server holds the Claude API key.** An authenticated endpoint returns the
key to a logged-in session. The frontend holds it in memory for the duration of
the session and uses it to call `api.anthropic.com` directly from the browser.
The key is never stored in `localStorage` or embedded in the client bundle.

**No public server exposure required.** All traffic is outbound — browser to
Anthropic, browser to the local server. Anthropic's servers never need to reach
back in. This rules out remote MCP and webhook patterns, but the chosen approach
doesn't need them.

## Context Claude Receives Upfront

Before the first exchange, Claude is given the current state of the template's
data sources: the list of configured Prometheus queries and HTTP fetchers, and
the variable names each one exposes (mirroring what the "Available Template
Variables" panel shows the user in the template editor). This lets Claude write
templates that reference real variables rather than invented placeholders, and
reason about what data is already available before deciding whether to add more.

## The Tool Suite

Claude interacts with the template through a set of tools, not free-text
generation. The WASM handler dispatches each tool call to the appropriate
existing server endpoint and returns the result to Claude.

**Exploration tools** (stateless — no database writes, no template variables created):
- `query_prometheus(expr, step?, duration?)` — executes an ad-hoc PromQL query
  and returns the raw result; lets Claude verify a metric exists, understand its
  shape, and try different expressions before committing to anything
- `fetch_url(url, headers?)` — makes an ad-hoc HTTP request and returns the
  response body; lets Claude inspect a JSON structure and decide what fields are
  worth exposing as variables

**Data source tools** (mirroring the CRUD the template editor already exposes):
- `add_prometheus_query(name, expr, step, duration)` — creates a Prometheus
  fetcher and returns the variable prefix it will expose (e.g. `prometheus.cpu`)
- `add_http_fetcher(name, url, headers?)` — creates an HTTP fetcher and returns
  its variable name
- `update_fetcher(id, ...)` — modifies an existing fetcher on an already-saved
  template
- `remove_fetcher(id)` — removes an existing fetcher from an already-saved
  template
- `list_fetchers()` — re-inspects the committed fetchers if Claude needs to
  refresh its view mid-session

**Render tool:**
- `render_template(svg, prometheus_queries=[], http_fetchers=[])` — renders the
  SVG against an inline fetcher configuration supplied in the call itself, with
  no database involvement. Claude passes its proposed fetchers alongside the SVG;
  the server executes them live and renders the result. This mirrors how the
  template editor lets the user modify fetchers and preview the result before
  committing to the database.

The exploration tools give Claude a sandbox: it can probe Prometheus for
available metrics, inspect an HTTP endpoint's JSON schema, and experiment with
PromQL expressions before deciding what to commit as template variables. The
natural flow is explore → decide → draft SVG with proposed fetchers → render →
iterate on both → commit only when satisfied.

During iteration Claude always works with proposed (uncommitted) state. The
render tool accepts the full fetcher configuration inline, so Claude can modify,
add, or remove fetchers from its working set and immediately see the effect in a
preview — without touching the database. This mirrors `execute_prometheus_query`
and `execute_http_source` in the template editor, which run live against local
(unsaved) state.

The only difference between new and existing templates is what Claude receives
as upfront context: empty for a new template, the current committed fetchers for
an existing one. In both cases, all iteration is stateless.

The CRUD tools are called once at the end to commit the final configuration:
add new fetchers, update changed ones, remove deleted ones. This is the
equivalent of the editor's Save button.

Termination is when Claude stops calling tools and returns its final SVG and
fetcher configuration in the `end_turn` message. A max-iteration cap on the
WASM side prevents runaway loops regardless.

The growing messages array is the full session state and can be surfaced to the
user as a log of what Claude considered during generation.

## Server Changes

Two categories of server work:

**New endpoints:**
- **PNG preview render** — accepts raw Liquid SVG content plus an inline fetcher
  configuration (Prometheus queries and HTTP fetchers as request body fields).
  Executes the fetchers live, evaluates the template, and returns a PNG. Entirely
  stateless — no database reads or writes, no saved template or device required.
  resvg already supports PNG output.
- **Prometheus exploration** — executes an ad-hoc PromQL query (instant or
  range) via the configured Prometheus backend and returns raw results. No
  database involvement. The app already queries Prometheus for rendering, so the
  connection infrastructure is reusable.
- **HTTP exploration** — makes an ad-hoc outbound HTTP request on behalf of the
  browser and returns the response body. Proxied through the server rather than
  called directly from WASM to avoid CORS issues with arbitrary third-party
  endpoints.

**Reused endpoints:** The Prometheus query and HTTP fetcher CRUD endpoints the
template editor already uses. No changes needed — Claude calls the same server
functions the UI does, via the same authenticated session.

## What Was Ruled Out

- **MCP server**: powerful iteration loop, but claude.ai's remote MCP requires
  Anthropic's servers to reach your server — incompatible with keeping the server
  off the public internet. A local MCP (stdio or LAN) would work but requires
  Claude Desktop or Claude Code CLI as the client, not claude.ai.

- **OAuth with Anthropic**: Anthropic doesn't currently offer an OAuth provider
  for third-party API access. API key is the only option.

- **AI calls from the Rust server**: straightforward, but the server would be
  blind to rendered output unless it also calls its own render pipeline in a
  loop — more complexity for no UX gain over the frontend approach.

- **User-managed API key**: storing the key in `localStorage` or prompting the
  user to paste it in. Server-side custody is cleaner and removes key management
  from the user entirely.
