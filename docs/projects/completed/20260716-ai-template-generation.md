# AI-Assisted Template Generation

Users describe a template in natural language on a dedicated chat page
(`/template/:id/generate`); Claude iteratively generates, renders, and refines
a Liquid SVG template — exploring Prometheus/HTTP data sources with tools and
*seeing* its own renders as images — until the result looks right, then the
user saves it in place. Originated from
[ideas/ai-template-generation](../ideas/) (idea file removed on completion).

## What shipped

- **All AI orchestration in WASM.** The browser calls `api.anthropic.com`
  directly (`anthropic-dangerous-direct-browser-access`); the server only
  provides key custody and the render/query endpoints the manual editor
  already had. No server-side agent loop, no public exposure required.
- **Per-user Claude API key custody** (`claude_api_key` column on `users`,
  `GET/PUT/DELETE /dashboard/claude-api-key` in
  [src/api/auth.rs](../../../src/api/auth.rs)), managed from the Users page
  with instructions for getting a key from the Anthropic console. Held in WASM
  memory only — never `localStorage`.
- **Agentic tool loop**
  ([ai_generator.rs](../../../src/frontend/pages/ai_template_generator/ai_generator.rs)):
  `run_conversation_loop` is pure and dependency-injected (API call, tool
  executor, per-turn callback all passed in), so the whole loop is unit-tested
  natively without a browser. Tools: `query_prometheus`,
  `query_prometheus_range`, `fetch_url` (proxied via new
  `POST /dashboard/ad-hoc-fetch` to dodge CORS), and `render_template`.
- **Claude sees its renders.** `render_template` returns the rendered screen
  as a PNG image block in the `tool_result` (Claude's vision input doesn't
  accept BMP), via a new `render_screen_png` sharing the BMP renderer's
  rasterize+threshold stage and a `POST /dashboard/preview/png` endpoint.
- **Chat UX**: Enter sends / Shift+Enter newline, per-chunk incremental
  message display (not token streaming), stick-to-bottom auto-scroll,
  subtle per-message timestamps, Stop button for in-flight turns, 3-state
  Save button (no changes / saving / unsaved), full-resolution scrollable
  preview panel. The AI editor is a peer of the manual editor (equal-weight
  links on template cards, cross-links between both editors).

## Hard-won correctness lessons

The feature worked in tests but repeatedly failed in real use; each failure
mode is now guarded:

- **`tool_use`/`tool_result` pairing is an API invariant.** Any turn that dies
  between pushing an assistant `tool_use` and pushing its `tool_result`
  (Stop click, superseded task, timeout) poisons the history — the next
  request 400s. `close_dangling_tool_uses` repairs the tail of the history
  before every send.
- **Process-exit boundaries need timeouts.** The server's shared `reqwest`
  client got a 20s timeout (it previously had none — a dead Prometheus target
  hung the tool call forever); the browser's Claude API call races a
  `gloo-timers` timeout (`gloo-net` has none built in). One bound per
  boundary; interior awaits inherit it.
- **Concurrent sends corrupt shared history.** A reentrancy guard on
  `send_message` plus a generation counter (bumped on every send/Stop;
  checked before every write-back) keep a superseded turn's late results
  from interleaving into a newer conversation.
- **`stop_reason == "max_tokens"` is not success.** Truncation was silently
  treated as a normal end of turn — Claude would be cut off mid-`tool_use`
  (before ever calling `render_template`) and the UI just went idle, looking
  exactly like a hang. Now surfaced as `LoopOutcome::Truncated` with a
  visible error, and `MAX_TOKENS` raised to 128k (Opus 4.8's max output)
  with a client timeout sized to match.

## Retrospective

**What worked**

- The dependency-injected loop design made the trickiest logic (tool
  dispatch, truncation, dangling-tool_use repair, image threading) natively
  unit-testable; every production bug fix landed with a failing test first.
- Live browser verification (WebDriver against the compose `chrome` service,
  and one full end-to-end run with a real API key) caught bugs that static
  reasoning and the native test tiers could not: the flexbox `min-h-0`
  scroll bug, centered-overflow clipping in the preview, and the truncation
  stall.

**What caused friction**

- Multiple rounds of opaque in-the-wild failures (stalls, 400s) that each
  looked like the previous one. The root causes were all *non-success paths
  treated as success or silence*: no timeouts, no truncation handling,
  silent generation-guard bails. Instrumentation (tracing around every await
  point) was only added after the third bug report — adding it when the
  feature was first built would have collapsed three debugging sessions into
  one console read.
- The Anthropic wire types were hand-rolled and initially too narrow
  (`tool_result.content: String`), requiring a mid-project reshape to carry
  image blocks.

**What to change**

- Bound the boundaries, not every await: put a timeout once at each shared
  client or wrapper where control leaves the process, handle every
  non-success terminal state there explicitly, give the user a cancel for
  bounded-but-slow work, and trace the seam — as part of the initial build,
  not as a debugging afterthought. All three field failures here were
  unbounded or silently-swallowed boundary outcomes. Promoted to a rule in
  [development-process.md](../../development-process.md#rules).
