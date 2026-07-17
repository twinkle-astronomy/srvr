pub mod ai_generator;
pub mod ai_types;

use chrono::Utc;
use dioxus::prelude::*;

use ai_generator::{ParsedToolCall, close_dangling_tool_uses, run_conversation_loop, LoopOutcome};
use ai_types::{
    AnthropicMessage, ContentBlock, CreateMessageRequest, CreateMessageResponse, ImageSource, Tool,
    ToolResultBlock, ToolResultContent,
};

use crate::frontend::server_fns::{
    delete_http_source, delete_prometheus_query, delete_range_query, execute_ad_hoc_http_fetch,
    execute_prometheus_query, execute_range_query, get_claude_api_key, get_template_context,
    get_template_preview, get_template_preview_png, get_virtual_render_context, save_http_source,
    save_prometheus_query, save_range_query,
};
use crate::frontend::server_fns::TemplateVar;
use crate::frontend::store::AppStore;
use crate::models::{Device, HttpSource, PrometheusQuery, RangeQuery, RenderContext, Template};

const CLAUDE_MODEL: &str = "claude-opus-4-8";
const MAX_TOKENS: u32 = 128000;
const MAX_ITERATIONS: u8 = 20;

#[derive(Clone, Debug, PartialEq)]
struct TemplateProposal {
    svg: String,
    prometheus_queries: Vec<serde_json::Value>,
    range_queries: Vec<serde_json::Value>,
    http_sources: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq)]
enum AiStatus {
    Idle,
    Thinking,
    ToolCalling(String),
    Done,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SaveState {
    /// Nothing to save — either no proposal yet, or the last proposal is saved.
    NoChanges,
    Saving,
    /// A rendered proposal exists and hasn't been saved yet.
    UnsavedChanges,
}

fn parse_prometheus_queries(template_id: i64, values: &[serde_json::Value]) -> Vec<PrometheusQuery> {
    values
        .iter()
        .filter_map(|v| {
            Some(PrometheusQuery {
                id: None,
                template_id,
                name: v.get("name")?.as_str()?.to_string(),
                addr: v.get("addr")?.as_str()?.to_string(),
                query: v.get("query")?.as_str()?.to_string(),
                created_at: Utc::now().naive_utc(),
                updated_at: Utc::now().naive_utc(),
            })
        })
        .collect()
}

fn parse_range_queries(template_id: i64, values: &[serde_json::Value]) -> Vec<RangeQuery> {
    values
        .iter()
        .filter_map(|v| {
            Some(RangeQuery {
                id: None,
                template_id,
                name: v.get("name")?.as_str()?.to_string(),
                addr: v.get("addr")?.as_str()?.to_string(),
                query: v.get("query")?.as_str()?.to_string(),
                duration: v.get("duration")?.as_str()?.to_string(),
                step: v.get("step")?.as_str()?.to_string(),
                created_at: Utc::now().naive_utc(),
                updated_at: Utc::now().naive_utc(),
            })
        })
        .collect()
}

fn parse_http_sources(template_id: i64, values: &[serde_json::Value]) -> Vec<HttpSource> {
    values
        .iter()
        .filter_map(|v| {
            Some(HttpSource {
                id: None,
                template_id,
                name: v.get("name")?.as_str()?.to_string(),
                url: v.get("url")?.as_str()?.to_string(),
                created_at: Utc::now().naive_utc(),
                updated_at: Utc::now().naive_utc(),
            })
        })
        .collect()
}

fn build_inline_render_context(
    template_id: i64,
    device: Device,
    svg: String,
    prometheus_queries: &[serde_json::Value],
    range_queries: &[serde_json::Value],
    http_sources: &[serde_json::Value],
) -> RenderContext {
    RenderContext {
        device,
        template: Template {
            id: template_id,
            name: "AI proposal".to_string(),
            content: svg,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        },
        prometheus_queries: parse_prometheus_queries(template_id, prometheus_queries),
        range_queries: parse_range_queries(template_id, range_queries),
        http_sources: parse_http_sources(template_id, http_sources),
    }
}

fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool {
            name: "query_prometheus".to_string(),
            description: "Run an instant Prometheus query to explore what data is available. \
                Use this before wiring a query into the template."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {"type": "string", "description": "Prometheus server address, e.g. http://prometheus:9090"},
                    "expr": {"type": "string", "description": "PromQL expression"}
                },
                "required": ["addr", "expr"]
            }),
        },
        Tool {
            name: "query_prometheus_range".to_string(),
            description: "Run a Prometheus range query (a time series) to explore what data is \
                available for a sparkline or chart."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "addr": {"type": "string", "description": "Prometheus server address"},
                    "expr": {"type": "string", "description": "PromQL expression"},
                    "duration": {"type": "string", "description": "Window length back from now, e.g. 1h"},
                    "step": {"type": "string", "description": "Resolution between points, e.g. 60s"}
                },
                "required": ["addr", "expr", "duration", "step"]
            }),
        },
        Tool {
            name: "fetch_url".to_string(),
            description: "Fetch a URL and return its raw response body, to explore a third-party \
                JSON data source before wiring it into the template."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"]
            }),
        },
        Tool {
            name: "render_template".to_string(),
            description: "Render the proposed SVG Liquid template (with the given data sources) \
                on the device canvas and return whether it succeeded. Always call this to verify \
                the template renders before finishing."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "svg": {"type": "string", "description": "Full SVG Liquid template source"},
                    "prometheus_queries": {
                        "type": "array",
                        "description": "Instant queries to wire in: [{name, addr, query}]",
                        "items": {"type": "object"}
                    },
                    "range_queries": {
                        "type": "array",
                        "description": "Range queries to wire in: [{name, addr, query, duration, step}]",
                        "items": {"type": "object"}
                    },
                    "http_sources": {
                        "type": "array",
                        "description": "HTTP sources to wire in: [{name, url}]",
                        "items": {"type": "object"}
                    }
                },
                "required": ["svg"]
            }),
        },
    ]
}

/// A small, muted timestamp for chat bubbles — local time, not sent to Claude.
fn format_now() -> String {
    chrono::Local::now().format("%-I:%M %p").to_string()
}

fn build_system_prompt(ctx: &RenderContext, vars: &[TemplateVar]) -> String {
    let mut s = String::new();
    s.push_str(
        "You are an expert SVG + Liquid template author for a TRMNL eink display. \
         Templates are SVG rendered through Liquid, then converted to a 1-bit black/white BMP.\n\n",
    );
    s.push_str(&format!(
        "Canvas size: {}x{} — keep the SVG within these dimensions, and use only black and white \
         (no grayscale or color; the display is 1-bit).\n\n",
        ctx.device.width, ctx.device.height
    ));

    if ctx.template.content.trim().is_empty() {
        s.push_str("This template is currently empty — you're starting from scratch.\n\n");
    } else {
        s.push_str(
            "Current template source — this is what the user is looking at right now. Treat it \
             as the starting point: preserve it and make the requested change, rather than \
             rewriting it from scratch, unless the user asks for something new.\n\
             ```liquid\n",
        );
        s.push_str(&ctx.template.content);
        s.push_str("\n```\n\n");
    }

    s.push_str("Available template variables (path — current value):\n");
    for v in vars {
        s.push_str(&format!("  {{{{ {} }}}} — {}\n", v.path, v.value));
    }
    s.push('\n');

    s.push_str(
        "Custom Liquid filters (not shown above — use these for QR codes):\n\
         \x20 {{ value | qrcode }}\n\
         \x20 {{ value | qrcode: module_size: 3 }}\n\
         \x20 {{ ssid | qrcode_wifi: password: \"pw\" }}\n\
         \x20 {{ ssid | qrcode_wifi: password: \"pw\", security: \"WEP\", module_size: 3 }}\n\
         `qrcode` renders any string as an inline QR SVG fragment. `qrcode_wifi` takes an SSID as \
         input and emits a WiFi-join QR code; `password` is required, `security` defaults to `WPA` \
         (`WEP`/`nopass` also accepted). Both accept optional `module_size` (default 1).\n\n",
    );

    if !ctx.prometheus_queries.is_empty()
        || !ctx.range_queries.is_empty()
        || !ctx.http_sources.is_empty()
    {
        s.push_str("Currently committed data sources on this template:\n");
        for pq in &ctx.prometheus_queries {
            s.push_str(&format!("  prometheus.{}: {} @ {}\n", pq.name, pq.query, pq.addr));
        }
        for rq in &ctx.range_queries {
            s.push_str(&format!(
                "  prometheus_range.{}: {} @ {} (last {}, step {})\n",
                rq.name, rq.query, rq.addr, rq.duration, rq.step
            ));
        }
        for hs in &ctx.http_sources {
            s.push_str(&format!("  http.{}: {}\n", hs.name, hs.url));
        }
        s.push('\n');
    }

    s.push_str(
        "Use render_template to verify your SVG renders correctly before ending your turn. \
         Iterate on the render error message if it fails.",
    );
    s
}

async fn execute_tool_call(
    call: ParsedToolCall,
    template_id: i64,
    device: Device,
    mut proposal: Signal<Option<TemplateProposal>>,
    mut preview_image: Signal<Option<String>>,
    mut status: Signal<AiStatus>,
    mut save_state: Signal<SaveState>,
    generation: Signal<u64>,
    my_generation: u64,
) -> ToolResultContent {
    tracing::debug!(
        "execute_tool_call: entry, generation={} mine={my_generation}, call={call:?}",
        generation(),
    );

    // A newer send (or a Stop click) has already superseded this turn — skip
    // the work entirely rather than let a stale tool call flash its result
    // into the UI (or make a network call nobody's waiting on anymore).
    if generation() != my_generation {
        tracing::warn!(
            "execute_tool_call: superseded before starting (generation {} != mine {my_generation}) — returning Cancelled without any network call",
            generation(),
        );
        return ToolResultContent::Text("Cancelled.".to_string());
    }

    match call {
        ParsedToolCall::QueryPrometheus { addr, expr } => {
            status.set(AiStatus::ToolCalling("Querying Prometheus".to_string()));
            let query = PrometheusQuery {
                id: None,
                template_id: 0,
                name: String::new(),
                addr,
                query: expr,
                created_at: Utc::now().naive_utc(),
                updated_at: Utc::now().naive_utc(),
            };
            tracing::debug!("execute_tool_call: awaiting execute_prometheus_query");
            let text = match execute_prometheus_query(query).await {
                Ok(result) => {
                    tracing::debug!("execute_tool_call: execute_prometheus_query RETURNED ok");
                    serde_json::to_string(&result).unwrap_or_default()
                }
                Err(e) => {
                    tracing::warn!("execute_tool_call: execute_prometheus_query RETURNED err: {e}");
                    format!("Query failed: {e}")
                }
            };
            ToolResultContent::Text(text)
        }
        ParsedToolCall::QueryPrometheusRange { addr, expr, duration, step } => {
            status.set(AiStatus::ToolCalling("Querying Prometheus range".to_string()));
            let query = RangeQuery {
                id: None,
                template_id: 0,
                name: String::new(),
                addr,
                query: expr,
                duration,
                step,
                created_at: Utc::now().naive_utc(),
                updated_at: Utc::now().naive_utc(),
            };
            tracing::debug!("execute_tool_call: awaiting execute_range_query");
            let text = match execute_range_query(query).await {
                Ok(result) => {
                    tracing::debug!("execute_tool_call: execute_range_query RETURNED ok");
                    serde_json::to_string(&result).unwrap_or_default()
                }
                Err(e) => {
                    tracing::warn!("execute_tool_call: execute_range_query RETURNED err: {e}");
                    format!("Query failed: {e}")
                }
            };
            ToolResultContent::Text(text)
        }
        ParsedToolCall::FetchUrl { url } => {
            status.set(AiStatus::ToolCalling(format!("Fetching {url}")));
            tracing::debug!("execute_tool_call: awaiting execute_ad_hoc_http_fetch url={url}");
            let text = match execute_ad_hoc_http_fetch(url).await {
                Ok(body) => {
                    tracing::debug!("execute_tool_call: execute_ad_hoc_http_fetch RETURNED ok, {} bytes", body.len());
                    body
                }
                Err(e) => {
                    tracing::warn!("execute_tool_call: execute_ad_hoc_http_fetch RETURNED err: {e}");
                    format!("Fetch failed: {e}")
                }
            };
            ToolResultContent::Text(text)
        }
        ParsedToolCall::RenderTemplate { svg, prometheus_queries, range_queries, http_sources } => {
            status.set(AiStatus::ToolCalling("Rendering preview".to_string()));
            let ctx = build_inline_render_context(
                template_id,
                device,
                svg.clone(),
                &prometheus_queries,
                &range_queries,
                &http_sources,
            );
            tracing::debug!(
                "execute_tool_call: render_template, svg={} bytes — awaiting get_template_preview",
                svg.len(),
            );
            match get_template_preview(ctx.clone()).await {
                Ok(image) => {
                    tracing::debug!("execute_tool_call: get_template_preview RETURNED ok, {} bytes b64", image.len());
                    // Re-check: this render was already in flight when Stop
                    // (or a newer send) superseded it. Don't let a stale
                    // render clobber the preview/proposal a newer turn may
                    // already be building.
                    if generation() != my_generation {
                        tracing::warn!(
                            "execute_tool_call: superseded mid-render (generation {} != mine {my_generation}) — dropping this render, skipping the PNG fetch",
                            generation(),
                        );
                        return ToolResultContent::Text("Cancelled.".to_string());
                    }
                    preview_image.set(Some(image));
                    proposal.set(Some(TemplateProposal {
                        svg,
                        prometheus_queries,
                        range_queries,
                        http_sources,
                    }));
                    save_state.set(SaveState::UnsavedChanges);

                    // Let Claude actually see what it rendered — the on-screen
                    // preview above is BMP (for the <img> tag), but Claude's
                    // vision input only accepts jpeg/png/gif/webp, so fetch
                    // the same render as PNG for the tool result.
                    tracing::debug!("execute_tool_call: awaiting get_template_preview_png");
                    match get_template_preview_png(ctx).await {
                        Ok(png_base64) => {
                            tracing::debug!(
                                "execute_tool_call: get_template_preview_png RETURNED ok, {} bytes b64",
                                png_base64.len(),
                            );
                            ToolResultContent::Blocks(vec![
                                ToolResultBlock::Text {
                                    text: "Rendered successfully. Here is what it looks like:"
                                        .to_string(),
                                },
                                ToolResultBlock::Image {
                                    source: ImageSource::Base64 {
                                        media_type: "image/png".to_string(),
                                        data: png_base64,
                                    },
                                },
                            ])
                        }
                        Err(e) => {
                            tracing::warn!("execute_tool_call: get_template_preview_png RETURNED err: {e}");
                            ToolResultContent::Text(format!(
                                "Rendered successfully, but couldn't generate an image of it for you to see: {e}"
                            ))
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("execute_tool_call: get_template_preview RETURNED err: {e}");
                    ToolResultContent::Text(format!("Render failed: {e}"))
                }
            }
        }
    }
}

/// Send the current input as a user turn and drive the conversation loop.
/// Shared by the form's `onsubmit` and the textarea's Enter-to-send handler.
fn send_message(
    key: String,
    render_context: Signal<Option<RenderContext>>,
    template_vars: Signal<Vec<TemplateVar>>,
    mut input: Signal<String>,
    mut messages: Signal<Vec<AnthropicMessage>>,
    mut message_times: Signal<Vec<String>>,
    mut status: Signal<AiStatus>,
    proposal: Signal<Option<TemplateProposal>>,
    preview_image: Signal<Option<String>>,
    save_state: Signal<SaveState>,
    mut generation: Signal<u64>,
) {
    // Guard against re-entrancy: the UI disables the input while busy, but
    // that's a rendering hint, not a lock — e.g. Enter-key auto-repeat can
    // fire several keydown events before the disabled state visually lands.
    // Two concurrent run_conversation_loop calls writing into the same
    // `messages` signal via `on_message` can interleave and clobber each
    // other's in-flight tool_use/tool_result pairing, which is exactly what
    // produces the "tool_use ids were found without tool_result" 400 from
    // the API on the next turn. Bail out here, synchronously, before any
    // `.await` point, so a second trigger can never slip through.
    if matches!(status(), AiStatus::Thinking | AiStatus::ToolCalling(_)) {
        tracing::warn!("send_message: reentrancy guard tripped, status={:?} — ignoring this call", status());
        return;
    }

    let Some(ctx) = render_context() else {
        tracing::warn!("send_message: no render_context yet — ignoring this call");
        return;
    };
    let prompt = input();
    if prompt.trim().is_empty() {
        return;
    }
    input.set(String::new());

    // A prior turn may have been cut short (Stop mid-tool-call, or a
    // superseded generation whose task never got to write its tool_result —
    // see the generation-counter comment below). Repair the existing history
    // *before* appending this new turn, or the next API call 400s on the
    // leftover dangling tool_use.
    let before_close = messages.read().len();
    close_dangling_tool_uses(&mut messages.write());
    if messages.read().len() != before_close {
        tracing::warn!(
            "send_message: close_dangling_tool_uses appended a closing turn — a prior turn's tool_use never got a tool_result",
        );
    }

    tracing::debug!("send_message: sending prompt ({} chars)", prompt.len());
    messages.write().push(AnthropicMessage {
        role: "user".to_string(),
        content: vec![ContentBlock::Text { text: prompt }],
    });
    message_times.write().push(format_now());
    status.set(AiStatus::Thinking);

    // Every send (and every Stop click) bumps the generation counter. The
    // spawned task below captures its own generation and checks it before
    // every write back to shared state — if a stall leads the user to hit
    // Stop, or somehow two turns still end up racing, a superseded task's
    // eventual results are silently dropped instead of corrupting the
    // conversation or clobbering a newer turn already in progress.
    let my_generation = generation() + 1;
    generation.set(my_generation);
    tracing::debug!("send_message: generation bumped to {my_generation}, spawning conversation loop");

    let system = build_system_prompt(&ctx, &template_vars());
    let device = ctx.device.clone();
    let template_id = ctx.template.id;
    let current_messages = messages();

    spawn(async move {
        let outcome = run_conversation_loop(
            current_messages,
            MAX_ITERATIONS,
            {
                let key = key.clone();
                let system = system.clone();
                move |msgs| {
                    let key = key.clone();
                    let system = system.clone();
                    async move {
                        let request = CreateMessageRequest {
                            model: CLAUDE_MODEL.to_string(),
                            max_tokens: MAX_TOKENS,
                            system: Some(system),
                            tools: tool_definitions(),
                            messages: msgs,
                        };
                        match ai_generator::call_anthropic_api(&key, &request).await {
                            Ok(resp) => resp,
                            Err(e) => CreateMessageResponse {
                                content: vec![ContentBlock::Text {
                                    text: format!("(request failed: {e})"),
                                }],
                                stop_reason: "end_turn".to_string(),
                            },
                        }
                    }
                }
            },
            move |call| {
                let device = device.clone();
                async move {
                    execute_tool_call(
                        call, template_id, device, proposal, preview_image, status, save_state,
                        generation, my_generation,
                    )
                    .await
                }
            },
            // Push each turn into the live signal as soon as it arrives, so
            // the log fills in chunk-by-chunk instead of all at once at the
            // end of a (possibly multi-tool-call) turn. Skipped once this
            // task has been superseded (see the generation comment above).
            move |msg: &AnthropicMessage| {
                if generation() == my_generation {
                    messages.write().push(msg.clone());
                    message_times.write().push(format_now());
                } else {
                    tracing::warn!(
                        "send_message: on_message dropped a turn — generation {} != mine {my_generation}",
                        generation(),
                    );
                }
            },
        )
        .await;

        tracing::debug!(
            "send_message: run_conversation_loop RETURNED (generation now={}, mine={my_generation}), outcome={:?}",
            generation(),
            match &outcome {
                LoopOutcome::Done { .. } => "Done",
                LoopOutcome::Truncated { .. } => "Truncated",
                LoopOutcome::MaxIterationsExceeded { .. } => "MaxIterationsExceeded",
            },
        );

        if generation() != my_generation {
            tracing::warn!(
                "send_message: superseded by the time the loop finished (generation {} != mine {my_generation}) — dropping the final result",
                generation(),
            );
            return;
        }

        let final_messages = match outcome {
            LoopOutcome::Done { messages } => messages,
            LoopOutcome::Truncated { messages } => {
                status.set(AiStatus::Error(
                    "Claude's response was cut off after hitting the output token limit \
                     (possibly mid tool-call) — try asking for a smaller change, or continue \
                     the conversation to prompt it to pick back up."
                        .to_string(),
                ));
                messages
            }
            LoopOutcome::MaxIterationsExceeded { messages } => {
                status.set(AiStatus::Error(
                    "Stopped after 20 iterations without finishing.".to_string(),
                ));
                messages
            }
        };
        messages.set(final_messages);
        if !matches!(status(), AiStatus::Error(_)) {
            status.set(AiStatus::Done);
        }
        tracing::debug!("send_message: done, status={:?}", status());
    });
}

fn render_log_entry(
    role: String,
    block: ContentBlock,
    key: String,
    timestamp: Option<String>,
) -> Element {
    match block {
        ContentBlock::Text { text } => {
            let align = if role == "user" { "items-end" } else { "items-start" };
            let bubble = if role == "user" {
                "bg-gray-900 text-white"
            } else {
                "bg-gray-100 text-gray-900"
            };
            rsx! {
                div { key: "{key}", class: "flex flex-col {align}",
                    span { class: "inline-block max-w-[85%] px-3 py-2 rounded-lg text-sm whitespace-pre-wrap {bubble}",
                        "{text}"
                    }
                    if let Some(ts) = timestamp {
                        span { class: "text-[10px] text-gray-300 mt-0.5 px-1", "{ts}" }
                    }
                }
            }
        }
        ContentBlock::ToolUse { name, .. } => rsx! {
            div { key: "{key}", class: "text-xs text-gray-400 italic", "\u{21b3} called {name}" }
        },
        ContentBlock::ToolResult { .. } => rsx! {},
    }
}

#[component]
pub fn AiTemplateGenerator(id: i64) -> Element {
    let store = use_context::<AppStore>();
    let templates = store.templates;

    let mut api_key = use_signal(|| None::<Option<String>>);
    let mut render_context = use_signal(|| None::<RenderContext>);
    let mut template_vars = use_signal(Vec::<TemplateVar>::new);
    let messages = use_signal(Vec::<AnthropicMessage>::new);
    // Index-aligned with `messages` — kept separate rather than added as a
    // field on `AnthropicMessage`, since that struct is also the literal
    // Anthropic API wire type (serialized into requests, compared for exact
    // equality in ai_generator's unit tests). Purely a rendering concern.
    let message_times = use_signal(Vec::<String>::new);
    let mut status = use_signal(|| AiStatus::Idle);
    let proposal = use_signal(|| None::<TemplateProposal>);
    let mut preview_image = use_signal(|| None::<String>);
    let mut input = use_signal(String::new);
    let mut save_error = use_signal(|| None::<String>);
    let mut save_state = use_signal(|| SaveState::NoChanges);
    // Bumped on every send and on Stop — lets an in-flight turn's spawned
    // task recognize it's been superseded and skip writing back stale
    // results. See the comment in `send_message`.
    let mut generation = use_signal(|| 0u64);

    // Auto-scroll the conversation log: stick to the bottom as new content
    // arrives, but stop once the user scrolls up to read earlier messages.
    let mut stick_to_bottom = use_signal(|| true);
    let mut bottom_anchor = use_signal(|| None::<std::rc::Rc<MountedData>>);
    use_effect(move || {
        // Subscribe to whatever changes the log's content/height.
        let _ = messages();
        let _ = status();
        if stick_to_bottom() {
            if let Some(anchor) = bottom_anchor() {
                spawn(async move {
                    let _ = anchor.scroll_to(ScrollBehavior::Instant).await;
                });
            }
        }
    });

    use_resource(move || async move {
        api_key.set(Some(get_claude_api_key().await.unwrap_or(None)));
        if let Ok(ctx) = get_virtual_render_context(id).await {
            if let Ok(vars) = get_template_context(ctx.clone()).await {
                template_vars.set(vars);
            }
            // Render the template's current state up front, so the page opens
            // showing what's actually there instead of a blank "no preview yet".
            if let Ok(image) = get_template_preview(ctx.clone()).await {
                preview_image.set(Some(image));
            }
            render_context.set(Some(ctx));
        }
    });

    let template_name = templates()
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.name.clone())
        .unwrap_or_default();

    let busy = matches!(status(), AiStatus::Thinking | AiStatus::ToolCalling(_));

    rsx! {
        div { class: "mb-4 flex items-center justify-between",
            div {
                Link {
                    to: super::super::Route::Templates {},
                    class: "inline-flex items-center gap-1.5 text-sm text-gray-500 hover:text-gray-900 transition-colors mb-1",
                    "\u{2190} Back to Templates"
                }
                h1 { class: "text-2xl font-bold text-gray-900 tracking-tight", "{template_name}" }
            }
            div { class: "flex items-center gap-3",
                Link {
                    to: super::super::Route::TemplateEditor { id },
                    class: "inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-gray-700 border border-gray-200 rounded-lg hover:bg-gray-50 transition-colors",
                    "Manual Editor"
                }
                button {
                    class: match save_state() {
                        SaveState::UnsavedChanges => "px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                        SaveState::NoChanges | SaveState::Saving => "px-4 py-2 bg-gray-100 text-gray-400 text-sm font-medium rounded-lg cursor-not-allowed",
                    },
                    disabled: save_state() != SaveState::UnsavedChanges,
                    onclick: move |_| {
                        let Some(prop) = proposal() else { return; };
                        let Some(ctx) = render_context() else { return; };
                        save_error.set(None);
                        save_state.set(SaveState::Saving);
                        spawn(async move {
                            let name = ctx.template.name.clone();
                            if let Err(e) = store.save_template(ctx.template.id, name, prop.svg.clone()).await {
                                save_error.set(Some(format!("Failed to save template: {e}")));
                                save_state.set(SaveState::UnsavedChanges);
                                return;
                            }
                            for pq in &ctx.prometheus_queries {
                                if let Some(pid) = pq.id {
                                    let _ = delete_prometheus_query(pid).await;
                                }
                            }
                            for rq in &ctx.range_queries {
                                if let Some(rid) = rq.id {
                                    let _ = delete_range_query(rid).await;
                                }
                            }
                            for hs in &ctx.http_sources {
                                if let Some(hid) = hs.id {
                                    let _ = delete_http_source(hid).await;
                                }
                            }
                            for pq in parse_prometheus_queries(ctx.template.id, &prop.prometheus_queries) {
                                let _ = save_prometheus_query(pq).await;
                            }
                            for rq in parse_range_queries(ctx.template.id, &prop.range_queries) {
                                let _ = save_range_query(rq).await;
                            }
                            for hs in parse_http_sources(ctx.template.id, &prop.http_sources) {
                                let _ = save_http_source(hs).await;
                            }
                            // Refresh in place — stay on this page instead of
                            // handing off to the manual editor. Re-fetching
                            // picks up the real ids the fetchers above were
                            // just saved under.
                            if let Ok(fresh) = get_virtual_render_context(ctx.template.id).await {
                                if let Ok(vars) = get_template_context(fresh.clone()).await {
                                    template_vars.set(vars);
                                }
                                render_context.set(Some(fresh));
                            }
                            save_state.set(SaveState::NoChanges);
                        });
                    },
                    {match save_state() {
                        SaveState::NoChanges => "Saved",
                        SaveState::Saving => "Saving...",
                        SaveState::UnsavedChanges => "Save",
                    }}
                }
            }
        }

        if let Some(ref msg) = save_error() {
            div { class: "mb-4 p-3 bg-red-50 border border-red-200 rounded-lg",
                p { class: "text-sm text-red-600", "{msg}" }
            }
        }

        match api_key() {
            None => rsx! {
                p { class: "text-gray-400 text-sm", "Loading..." }
            },
            Some(None) => rsx! {
                p { class: "text-gray-500 text-sm",
                    "No Claude API key configured — add yours on the "
                    Link { to: super::super::Route::Users {}, class: "text-blue-600 hover:underline", "Users" }
                    " page."
                }
            },
            Some(Some(key)) => rsx! {
                div { class: "flex flex-wrap gap-6",
                    div {
                        class: "flex-1 min-w-[400px] flex flex-col bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden",
                        style: "height: 70vh;",
                        div {
                            // min-h-0 is load-bearing: a flex child's default min-height is
                            // "auto" (fits content), which lets this div grow past the parent's
                            // fixed height instead of developing real internal scroll — the
                            // outer overflow-hidden then just clips it, so nothing scrolls.
                            class: "flex-1 min-h-0 overflow-y-auto p-4 space-y-3",
                            onscroll: move |event| {
                                let data = event.data();
                                let distance_from_bottom = data.scroll_height() as f64
                                    - data.scroll_top()
                                    - data.client_height() as f64;
                                stick_to_bottom.set(distance_from_bottom < 40.0);
                            },
                            for (i, msg) in messages().into_iter().enumerate() {
                                for (j, block) in msg.content.clone().into_iter().enumerate() {
                                    {render_log_entry(
                                        msg.role.clone(),
                                        block,
                                        format!("{i}-{j}"),
                                        message_times().get(i).cloned(),
                                    )}
                                }
                            }
                            if busy {
                                div { class: "text-sm text-gray-400 italic",
                                    {match status() { AiStatus::ToolCalling(name) => name, _ => "Thinking...".to_string() }}
                                }
                            }
                            if let AiStatus::Error(e) = status() {
                                div { class: "text-sm text-red-500", "Error: {e}" }
                            }
                            div { onmounted: move |element| bottom_anchor.set(Some(element.data())) }
                        }
                        form {
                            class: "border-t border-gray-100 p-3 flex gap-2",
                            onsubmit: {
                                let key = key.clone();
                                move |event: FormEvent| {
                                    event.prevent_default();
                                    send_message(
                                        key.clone(), render_context, template_vars, input, messages,
                                        message_times, status, proposal, preview_image, save_state,
                                        generation,
                                    );
                                }
                            },
                            textarea {
                                class: "flex-1 text-sm border border-gray-200 rounded-lg px-3 py-2 focus:outline-none focus:ring-1 focus:ring-gray-300 resize-none",
                                rows: 2,
                                placeholder: "Describe your template... (Enter to send, Shift+Enter for a new line)",
                                disabled: busy,
                                value: "{input()}",
                                oninput: move |e| input.set(e.value()),
                                onkeydown: move |event: KeyboardEvent| {
                                    if event.key() == Key::Enter
                                        && !event.modifiers().shift()
                                        && !event.is_auto_repeating()
                                    {
                                        event.prevent_default();
                                        send_message(
                                            key.clone(), render_context, template_vars, input, messages,
                                            message_times, status, proposal, preview_image, save_state,
                                            generation,
                                        );
                                    }
                                },
                            }
                            if busy {
                                button {
                                    r#type: "button",
                                    class: "px-4 py-2 text-sm font-medium text-red-600 border border-red-200 rounded-lg hover:bg-red-50 transition-colors",
                                    onclick: move |_| {
                                        // Unstick a stalled turn: mark it superseded
                                        // (so its eventual result, if any, is
                                        // ignored — see `send_message`) and hand
                                        // control back to the user immediately.
                                        generation.set(generation() + 1);
                                        status.set(AiStatus::Idle);
                                    },
                                    "Stop"
                                }
                            } else {
                                button {
                                    r#type: "submit",
                                    class: "px-4 py-2 bg-gray-900 text-white text-sm font-medium rounded-lg hover:bg-gray-700 transition-colors",
                                    "Send"
                                }
                            }
                        }
                    }

                    div { class: "flex-1 min-w-[300px] bg-white rounded-xl shadow-sm border border-gray-100 p-4 overflow-auto",
                        match (preview_image(), render_context()) {
                            (Some(b64), Some(ctx)) => rsx! {
                                // Sized to the device's actual pixel dimensions —
                                // matches the manual editor's preview — so the
                                // image renders at native resolution instead of
                                // being scaled down to fit this column. If the
                                // column is narrower than the device, this panel
                                // scrolls rather than shrinking the image.
                                //
                                // Deliberately NOT centered: centering an
                                // overflowing scroll container clips content on
                                // the start side and makes it unreachable by
                                // scrolling in most browsers — with an 800px-wide
                                // render in a column that's often narrower, that
                                // silently cropped both edges of the image.
                                div {
                                    style: "width: {ctx.device.width}px; height: {ctx.device.height}px;",
                                    img {
                                        src: "data:image/bmp;base64,{b64}",
                                        alt: "Template preview",
                                        class: "max-w-none",
                                        style: "image-rendering: pixelated;",
                                    }
                                }
                            },
                            _ => rsx! {
                                div { class: "h-full flex items-center justify-center",
                                    p { class: "text-gray-300 text-sm", "No preview yet" }
                                }
                            },
                        }
                    }
                }
            },
        }
    }
}
