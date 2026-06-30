# Plan: AI-Assisted Template Generation

**Branch:** `ai-template-generation`

## What & why

Users describe a template in natural language; Claude iteratively generates,
renders, and refines a Liquid SVG template — including discovering and wiring
up data sources — until the result looks right on the eink canvas. This removes
the barrier of knowing SVG + Liquid to author a useful template from scratch.

## Architecture

- **All AI orchestration runs in WASM.** The server plays no role in the
  agentic loop; it only provides the endpoints the template editor already
  exposes, plus key custody.
- **Server holds the Claude API key.** A new authenticated server function
  returns `CLAUDE_API_KEY` to logged-in sessions. The browser holds the key in
  WASM memory for the session duration — never written to `localStorage` or the
  DOM.
- **Browser calls `api.anthropic.com` directly.** All traffic is outbound.
  Anthropic's servers never reach back to the local server, so the server can
  stay off the public internet.

## Server changes

### New server functions (`src/frontend/server_fns.rs`)

**`get_claude_api_key() -> Result<Option<String>, ServerFnError>`**
Reads the calling user's `claude_api_key` from the `users` table. Requires
auth. Returns `Ok(None)` when the user has not saved a key — the WASM uses
this to decide whether to show or hide the AI panel.

**`save_claude_api_key(key: String) -> Result<(), ServerFnError>`**
Writes the calling user's `claude_api_key` in the `users` table. Requires auth.

**`delete_claude_api_key() -> Result<(), ServerFnError>`**
Sets the calling user's `claude_api_key` to `NULL` in the `users` table.
Requires auth.

**`execute_ad_hoc_http_fetch(url: String) -> Result<String, ServerFnError>`**
Makes an outbound HTTP GET on behalf of the browser and returns the raw
response body (treated as a string). Proxied to avoid CORS issues with
third-party endpoints. Requires auth. No custom header support initially
(headers are not modeled on `HttpSource` either; both can be extended together
later if needed).

### Unchanged server functions — reused as-is

| Server function | How Claude uses it |
|---|---|
| `execute_prometheus_query(PrometheusQuery)` | exploration: WASM passes `template_id: 0`, no DB write |
| `get_template_preview(RenderContext)` | render tool: WASM builds an inline `RenderContext` (virtual device, Claude's SVG as template content, proposed fetcher lists) — no DB read or write |
| `save_prometheus_query`, `delete_prometheus_query` | commit phase: apply final fetcher config |
| `save_http_source`, `delete_http_source` | commit phase: apply final fetcher config |
| `save_template` | commit phase: persist the final SVG |

### Migration

`migrations/YYYYMMDDHHMMSS_add_claude_api_key_to_users.sql` — adds a nullable
`claude_api_key TEXT` column to `users`. No backfill needed; existing rows get
`NULL`, which maps to `Option<String>` cleanly.

```sql
ALTER TABLE users ADD COLUMN claude_api_key TEXT;
```

### New DB functions (`src/db.rs`)

- `get_claude_api_key(user_id: i64) -> Result<Option<String>, sqlx::Error>`
- `set_claude_api_key(user_id: i64, key: &str) -> Result<(), sqlx::Error>`
- `clear_claude_api_key(user_id: i64) -> Result<(), sqlx::Error>` — sets column to `NULL`

The key is stored as plaintext. This is consistent with how passwords are
handled prior to hashing and with the threat model of a self-hosted,
single-binary deployment where DB access is already equivalent to root.

### Users page — key management UI (`src/frontend/pages/users.rs`)

A new "Claude API Key" card (styled like the existing "Change Password" card):
- A password-type input and a "Save" button
- A "Delete" button when a key is already set (shown as `sk-ant-••••••••••••`)
- On save: calls `save_claude_api_key(key)` server fn
- On delete: calls `delete_claude_api_key()` server fn
- Status feedback: "Saved" / "Deleted" / error message inline

## WASM frontend changes

### New: Anthropic API types (`src/frontend/pages/template_editor/ai_types.rs`)

Rust structs (all `derive(Serialize, Deserialize)`) for the subset of the
Claude Messages API we use:

```
ContentBlock (enum)
  Text { text: String }
  ToolUse { id: String, name: String, input: serde_json::Value }
  ToolResult { tool_use_id: String, content: String }

AnthropicMessage { role: String, content: Vec<ContentBlock> }

Tool { name: String, description: String, input_schema: serde_json::Value }

CreateMessageRequest {
    model: String,
    max_tokens: u32,
    system: Option<String>,
    tools: Vec<Tool>,
    messages: Vec<AnthropicMessage>,
}

CreateMessageResponse {
    content: Vec<ContentBlock>,
    stop_reason: String,   // "end_turn" | "tool_use"
}
```

### Tool suite (defined in `ai_generator.rs`, dispatched to server functions)

**Exploration tools** (stateless, no DB writes):

| Tool name | Arguments | Dispatches to |
|---|---|---|
| `query_prometheus` | `addr`, `expr` | `execute_prometheus_query` (template_id=0) |
| `query_prometheus_range` | `addr`, `expr`, `duration`, `step` | `execute_range_query` (template_id=0) |
| `fetch_url` | `url` | `execute_ad_hoc_http_fetch` |

**Render tool** (stateless, no DB writes):

| Tool name | Arguments | Dispatches to |
|---|---|---|
| `render_template` | `svg`, `prometheus_queries: []`, `range_queries: []`, `http_sources: []` | `get_template_preview` with inline `RenderContext` (virtual device, proposed SVG + fetcher lists) |

The render tool accepts the full proposed fetcher config inline, so Claude can
add, remove, or modify fetchers and immediately preview the effect without
touching the database. The WASM tracks the last arguments Claude passed to
`render_template` as the "current proposal."

There is no explicit "save" or "finish" tool. The session ends when Claude
returns `stop_reason == "end_turn"`. At that point the WASM surfaces an
"Apply to template" button that commits the last proposal.

### Conversation loop (`src/frontend/pages/template_editor/ai_generator.rs`)

State:
```rust
messages: Signal<Vec<AnthropicMessage>>
proposal: Signal<Option<TemplateProposal>>   // last render_template args
log: Signal<Vec<LogEntry>>                   // shown to user
status: Signal<AiStatus>                     // Idle | Thinking | ToolCalling(name) | Done | Error
api_key: Signal<Option<String>>
iterations: Signal<u8>
```

Loop:
1. On mount: call `get_claude_api_key()`. Store in `api_key` signal. If absent
   (`None`), render an empty state: "No Claude API key configured — add yours
   on the [Users](/users) page." Input and Send button are hidden in this state.
2. On user submit: append user message, set status=`Thinking`, POST to
   `https://api.anthropic.com/v1/messages` via `gloo-net` with the full
   messages array plus system prompt and tool definitions.
3. On response:
   - Append assistant message to `messages`.
   - If `stop_reason == "tool_use"`: for each `ToolUse` block, dispatch to the
     appropriate server function, collect the result, append a `tool_result`
     message, increment `iterations`. If `iterations >= 20`, stop with an error.
     Loop back to step 2.
   - If `stop_reason == "end_turn"`: set status=`Done`. Surface the "Apply"
     button.
4. On "Apply": call `save_template`, then the CRUD functions to reconcile
   committed fetchers with the proposal (add new, delete removed) — covering
   Prometheus queries, range queries, and HTTP sources. Redirect to the
   template editor with the saved template id.

System prompt given to Claude:
- Role: SVG + Liquid template author for an eink display; canvas dimensions
  taken from `render_context.device.width` × `render_context.device.height`
- Available template variables format (mirrors the "Template Variables" panel)
- List of currently committed fetchers (for edit sessions)
- Instruction to use `render_template` to verify the SVG renders before finishing
- Instruction to keep SVG within the device's width × height and use only black/white

### New page: `src/frontend/pages/ai_template_generator.rs`

A dedicated full-page experience at `/template/:id/generate`, separate from
the existing template editor. Two-column layout within the standard
`NavLayout`:

```
┌─ Nav header ─────────────────────────────────────────────────────────┐
├──────────────────────────────────────────────────────────────────────┤
│ ← Back to Editor   "My Template"                       [Apply]       │
├──────────────────────────┬───────────────────────────────────────────┤
│  CONVERSATION            │  PREVIEW                                  │
│                          │                                           │
│  Scrollable log:         │  Most recent render_template result,      │
│  • user turns            │  displayed as a scaled <img> centered     │
│  • Claude text replies   │  in the panel. Shows a placeholder        │
│  • tool call summaries   │  ("no preview yet") until Claude calls    │
│    ↳ queried prometheus  │  render_template for the first time.      │
│    ↳ rendered (thumb)    │                                           │
│                          │                                           │
│  ──────────────────────  │                                           │
│  [ describe your         │                                           │
│    template...         ] │                                           │
│  [Send ▶]                │                                           │
└──────────────────────────┴───────────────────────────────────────────┘
```

- **Top bar**: breadcrumb back to `/template/:id`, template name, and an
  "Apply to template" button (enabled only when `status == Done` and a
  proposal exists).
- **Left column**: scrollable conversation log + fixed-bottom input area.
  Log entries: user messages (right-aligned), Claude text (left-aligned),
  tool call summaries as muted one-liners. Input is a textarea + Send button,
  disabled while `status != Idle`.
- **Right column**: the latest BMP preview rendered as `<img
  src="data:image/bmp;base64,...">`, scaled to fit. Placeholder text when
  no preview exists yet.
- **Empty state** (no API key): left column shows "No Claude API key
  configured — add yours on the [Users](/users) page." Input is hidden.

### Route + registration

New route `/template/:id/generate` added to the `Route` enum in
`src/frontend/mod.rs`:
```rust
#[route("/template/:id/generate")]
AiTemplateGenerator { id: i64 },
```

Entry point from the existing template editor: an "Generate with AI" link
button in the template editor's header, next to "Back to Templates". The
link navigates to `Route::AiTemplateGenerator { id }` rather than opening
a panel in-place.

### New files for the page

- `src/frontend/pages/ai_template_generator.rs` — page component, conversation
  state, agentic loop, tool dispatch
- Register in `src/frontend/pages/mod.rs`: `mod ai_template_generator; pub use ai_template_generator::AiTemplateGenerator;`

## New dependency

`gloo-net` — WASM-idiomatic HTTP client (no Node.js, no `wasm-bindgen-futures`
ceremony):
```toml
gloo-net = { version = "0.6", features = ["http"], optional = true }
```
Added under `[features] web`.

## Scope exclusions

- **Custom headers on `fetch_url`** — deferred; `HttpSource` doesn't have them
  either. Extend both together.
- **Streaming responses** — non-streaming simplifies the loop: tool dispatch
  requires the full response before it can proceed, so streaming adds complexity
  with no benefit for the agentic case.
- **Multi-turn session memory across page loads** — each AI session is ephemeral
  WASM state. No persistence of conversation history.

## TDD order

Each step: failing test first → implementation → confirm passing.

1. **DB functions + server fns for key management** — three cases: (a) save a
   key, read it back, assert equality; (b) delete it, read back, assert `None`;
   (c) read with no key set, assert `None`. Mirror the `update_user_password` /
   `get_user_by_id` test pattern.

2. **`execute_ad_hoc_http_fetch` server fn** — starts a local `hyper`/`axum`
   mock server, calls the server fn with its URL, asserts the response body
   matches. Mirror the pattern used in existing `dev-dependencies`.

3. **Tool dispatch routing** — a pure `dispatch_tool(name: &str, input: Value,
   ...) -> Result<String, ToolError>` function (if extracted as a unit). Test
   that `"query_prometheus"` routes to the prometheus handler, `"fetch_url"`
   routes to the HTTP handler, `"render_template"` routes to preview, and an
   unknown name returns an error. No WASM needed; native test.

4. **Spike** *(gates Phase 3)* — a minimal WASM component (not the real UI)
   that calls `api.anthropic.com/v1/messages` with a hardcoded single-turn
   "reply with the word HELLO" prompt using `gloo-net`. Goal: confirm CORS
   headers allow the request and that authentication works. Run in browser.
   Spike outcome: either (a) it works, proceed; or (b) CORS is blocked, pivot
   to a server-side proxy for Anthropic calls.

5. **Conversation loop logic** — the cycling logic (`tool_use` → dispatch →
   append `tool_result` → loop) is extracted into a pure/injectable function
   that takes a closure for "call the API." Unit-test by injecting a fake API
   response sequence and asserting the resulting messages array and dispatched
   tool calls are correct.

6. **AI generator page** — manual browser testing: navigate to
   `/template/:id/generate`, submit a prompt, verify the conversation log
   populates on the left, confirm the BMP preview appears on the right after
   Claude calls `render_template`, and confirm "Apply" commits correctly.

## Files

| Path | Action |
|---|---|
| `migrations/YYYYMMDDHHMMSS_add_claude_api_key_to_users.sql` | New — nullable `claude_api_key` column |
| `src/db.rs` | Add `get_claude_api_key`, `update_claude_api_key` |
| `src/frontend/server_fns.rs` | Add `get_claude_api_key`, `save_claude_api_key`, `delete_claude_api_key`, `execute_ad_hoc_http_fetch` |
| `src/frontend/pages/users.rs` | Add Claude API Key management card |
| `src/frontend/pages/ai_template_generator.rs` | New — page component, loop, tool dispatch |
| `src/frontend/pages/ai_types.rs` | New — Anthropic API structs |
| `src/frontend/pages/mod.rs` | Register `AiTemplateGenerator` |
| `src/frontend/mod.rs` | Add `Route::AiTemplateGenerator { id }` at `/template/:id/generate` |
| `src/frontend/pages/template_editor/mod.rs` | Add "Generate with AI" link button |
| `Cargo.toml` | Add `gloo-net` under `web` feature |

## Definition of done

- `cargo test --features server` passes (server fn tests + dispatch logic test)
- `cargo check --no-default-features --features web --target wasm32-unknown-unknown` compiles
- CORS spike confirms `api.anthropic.com` is reachable from browser WASM
- End-to-end manual test: describe a template → see Claude's iteration log →
  preview renders in the panel → "Apply" saves the template and its fetchers

## Resolved decisions

1. **Per-user API key stored in the database.** Users enter their own Anthropic
   key on the Users page; it is stored in the `users` table as plaintext (same
   threat model as the rest of the SQLite DB — local, self-hosted). No env var.
2. **Commit is user-triggered, not Claude-triggered.** Claude ends its turn;
   the user clicks "Apply." Prevents accidental overwrites of a working template.
3. **Last `render_template` call is the proposal.** No special `finish` tool or
   structured end_turn message parsing. If the user is satisfied with the last
   preview, they apply it.
4. **Non-streaming.** Tool dispatch needs the full response before proceeding;
   streaming adds WASM complexity for no UX gain in the agentic loop case.
6. **`gloo-net` for WASM HTTP.** Already a standard choice for browser WASM;
   `reqwest`'s WASM target requires additional plumbing that isn't worth adding
   for a single fetch call.

## Open questions to verify during implementation

1. Does `api.anthropic.com` send CORS headers permitting requests from
   arbitrary origins? (Spike — step 4 gates the rest of Phase 3.)
2. What `max_tokens` budget is appropriate? Too low and Claude can't fit a full
   SVG; too high and a runaway loop burns the key. Calibrate during manual
   testing; start at 4096.
3. Does `gloo-net` correctly serialize the `anthropic-version: 2023-06-01`
   header required by the Messages API? Verify in the spike.
