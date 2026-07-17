//! Tool dispatch and the agentic conversation loop for the AI template
//! generator. Tool *routing* (this file's `dispatch_tool`) is a pure,
//! synchronous parse of Claude's tool-call JSON into a typed call — it does
//! no I/O, so it's unit-testable natively without WASM or a live server.
//! Actually executing a parsed call (hitting `execute_prometheus_query`,
//! `execute_ad_hoc_http_fetch`, etc.) is a separate, browser-only step.

use std::future::Future;

use serde_json::Value;

use super::ai_types::{AnthropicMessage, ContentBlock, CreateMessageResponse, ToolResultContent};

#[derive(Clone, Debug, PartialEq)]
pub enum ToolError {
    UnknownTool(String),
    InvalidInput { tool: String, reason: String },
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::UnknownTool(name) => write!(f, "unknown tool '{name}'"),
            ToolError::InvalidInput { tool, reason } => {
                write!(f, "invalid input for tool '{tool}': {reason}")
            }
        }
    }
}

impl std::error::Error for ToolError {}

/// A tool call parsed from Claude's `tool_use` block, with its arguments
/// extracted into typed fields. Produced by [`dispatch_tool`]; executing one
/// of these (the actual `fetch`/DB call) happens elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub enum ParsedToolCall {
    QueryPrometheus {
        addr: String,
        expr: String,
    },
    QueryPrometheusRange {
        addr: String,
        expr: String,
        duration: String,
        step: String,
    },
    FetchUrl {
        url: String,
    },
    RenderTemplate {
        svg: String,
        prometheus_queries: Vec<Value>,
        range_queries: Vec<Value>,
        http_sources: Vec<Value>,
    },
}

fn required_str(input: &Value, tool: &str, field: &str) -> Result<String, ToolError> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::InvalidInput {
            tool: tool.to_string(),
            reason: format!("missing or non-string field '{field}'"),
        })
}

fn optional_array(input: &Value, field: &str) -> Vec<Value> {
    input
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Route a tool-use call by name, validating and extracting its arguments.
/// Pure and synchronous: no network or DB access happens here.
pub fn dispatch_tool(name: &str, input: &Value) -> Result<ParsedToolCall, ToolError> {
    match name {
        "query_prometheus" => Ok(ParsedToolCall::QueryPrometheus {
            addr: required_str(input, name, "addr")?,
            expr: required_str(input, name, "expr")?,
        }),
        "query_prometheus_range" => Ok(ParsedToolCall::QueryPrometheusRange {
            addr: required_str(input, name, "addr")?,
            expr: required_str(input, name, "expr")?,
            duration: required_str(input, name, "duration")?,
            step: required_str(input, name, "step")?,
        }),
        "fetch_url" => Ok(ParsedToolCall::FetchUrl {
            url: required_str(input, name, "url")?,
        }),
        "render_template" => Ok(ParsedToolCall::RenderTemplate {
            svg: required_str(input, name, "svg")?,
            prometheus_queries: optional_array(input, "prometheus_queries"),
            range_queries: optional_array(input, "range_queries"),
            http_sources: optional_array(input, "http_sources"),
        }),
        other => Err(ToolError::UnknownTool(other.to_string())),
    }
}

/// If the tail of `messages` is an assistant turn with `tool_use` blocks that
/// never got a matching `tool_result` — because the turn was cancelled via
/// Stop mid-tool-call, or a superseded generation's task was blocked from
/// writing its result (see the generation-counter comments in `mod.rs`) —
/// synthesize a closing `tool_result` for each dangling id. Without this, the
/// next request to the API is rejected with "tool_use ids were found without
/// tool_result blocks immediately after."
pub fn close_dangling_tool_uses(messages: &mut Vec<AnthropicMessage>) {
    let Some(last) = messages.last() else {
        return;
    };
    if last.role != "assistant" {
        return;
    }

    let pending_ids: Vec<String> = last
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    if pending_ids.is_empty() {
        return;
    }

    let content = pending_ids
        .into_iter()
        .map(|tool_use_id| ContentBlock::ToolResult {
            tool_use_id,
            content: ToolResultContent::Text(
                "Cancelled — no result was recorded before the conversation moved on."
                    .to_string(),
            ),
        })
        .collect();
    messages.push(AnthropicMessage {
        role: "user".to_string(),
        content,
    });
}

/// Outcome of running the conversation loop to completion.
#[derive(Clone, Debug, PartialEq)]
pub enum LoopOutcome {
    /// Claude returned `stop_reason == "end_turn"` (or another non-`tool_use`,
    /// non-`max_tokens` reason) — a normal end of turn.
    Done { messages: Vec<AnthropicMessage> },
    /// Claude returned `stop_reason == "max_tokens"`: its response was cut off
    /// mid-generation, possibly mid-`tool_use`, before it finished (or before
    /// it ever got to calling a tool at all). Distinct from `Done` because the
    /// caller must not treat this as a successful, intentional stop — the
    /// content that came back may be incomplete or unusable.
    Truncated { messages: Vec<AnthropicMessage> },
    /// Hit `max_iterations` while Claude kept calling tools.
    MaxIterationsExceeded { messages: Vec<AnthropicMessage> },
}

/// Drive the tool-use conversation loop: call the API, and while Claude keeps
/// asking for tools, dispatch + execute each one and feed the results back.
/// `call_api` and `execute_tool` are injected so this stays testable without
/// a live network call — the browser build passes real `gloo-net` calls.
/// `on_message` fires the moment each turn (Claude's reply, then the tool
/// results sent back) is appended, so a caller can display it immediately —
/// this is chunk-at-a-time (per API response), not token streaming.
pub async fn run_conversation_loop<CallApi, CallApiFut, ExecuteTool, ExecuteToolFut, OnMessage>(
    mut messages: Vec<AnthropicMessage>,
    max_iterations: u8,
    call_api: CallApi,
    execute_tool: ExecuteTool,
    mut on_message: OnMessage,
) -> LoopOutcome
where
    CallApi: Fn(Vec<AnthropicMessage>) -> CallApiFut,
    CallApiFut: Future<Output = CreateMessageResponse>,
    ExecuteTool: Fn(ParsedToolCall) -> ExecuteToolFut,
    ExecuteToolFut: Future<Output = ToolResultContent>,
    OnMessage: FnMut(&AnthropicMessage),
{
    for iteration in 0..max_iterations {
        tracing::debug!(
            "conversation loop: iteration {iteration}/{max_iterations}, {} messages so far — calling API",
            messages.len(),
        );
        let response = call_api(messages.clone()).await;
        tracing::debug!(
            "conversation loop: iteration {iteration} response arrived, stop_reason={}, {} content blocks",
            response.stop_reason,
            response.content.len(),
        );
        let assistant_message = AnthropicMessage {
            role: "assistant".to_string(),
            content: response.content.clone(),
        };
        on_message(&assistant_message);
        messages.push(assistant_message);

        if response.stop_reason == "max_tokens" {
            tracing::warn!(
                "conversation loop: TRUNCATED — Claude hit the output token limit before finishing (possibly mid-tool_use)",
            );
            return LoopOutcome::Truncated { messages };
        }

        if response.stop_reason != "tool_use" {
            tracing::debug!("conversation loop: done, stop_reason={}", response.stop_reason);
            return LoopOutcome::Done { messages };
        }

        let mut tool_results = Vec::new();
        for block in &response.content {
            if let ContentBlock::ToolUse { id, name, input } = block {
                tracing::debug!("conversation loop: dispatching tool_use id={id} name={name} input={input}");
                let content = match dispatch_tool(name, input) {
                    Ok(call) => {
                        tracing::debug!("conversation loop: awaiting execute_tool for id={id} name={name}");
                        let result = execute_tool(call).await;
                        tracing::debug!("conversation loop: execute_tool RETURNED for id={id} name={name}");
                        result
                    }
                    Err(e) => {
                        tracing::warn!("conversation loop: dispatch_tool failed for id={id} name={name}: {e}");
                        ToolResultContent::Text(format!("Error: {e}"))
                    }
                };
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content,
                });
            }
        }
        let tool_result_message = AnthropicMessage {
            role: "user".to_string(),
            content: tool_results,
        };
        on_message(&tool_result_message);
        messages.push(tool_result_message);
    }

    tracing::warn!("conversation loop: hit max_iterations ({max_iterations}) without stop_reason != tool_use");
    LoopOutcome::MaxIterationsExceeded { messages }
}

/// Bound on how long a single Claude API round trip is allowed to take.
/// `gloo-net`'s fetch has no built-in timeout, so without this a hung or
/// silently-dropped connection to api.anthropic.com stalls the conversation
/// forever — the browser gives no error, `status` never leaves `Thinking`,
/// and the only way out was previously the Stop button (see
/// `close_dangling_tool_uses` for what a Stop mid-tool-call also needed).
/// Must be sized to `MAX_TOKENS` in mod.rs: a long non-streaming response
/// (a big multi-series chart SVG) legitimately takes minutes to generate,
/// and the user always has the Stop button to bail out sooner. Anthropic
/// caps non-streaming requests at ~10 minutes.
#[cfg(feature = "web")]
const CLAUDE_API_TIMEOUT_MS: u32 = 300_000;

/// Call the real Claude Messages API directly from the browser. Only
/// available in the WASM build — `gloo-net`'s HTTP client doesn't exist
/// natively, and this is never reached from the native component-test tier
/// (this page can't render there anyway; it depends on the router).
#[cfg(feature = "web")]
pub async fn call_anthropic_api(
    api_key: &str,
    request: &super::ai_types::CreateMessageRequest,
) -> Result<CreateMessageResponse, String> {
    tracing::debug!(
        "claude api: sending request, {} messages, {} tools",
        request.messages.len(),
        request.tools.len(),
    );

    let send_and_parse = async {
        let response = gloo_net::http::Request::post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            // Anthropic blocks direct browser-origin requests by default; this
            // header is the documented opt-in for calling the API straight from
            // WASM instead of through a server-side proxy.
            .header("anthropic-dangerous-direct-browser-access", "true")
            .json(request)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| {
                tracing::warn!("claude api: send() rejected before a response arrived: {e}");
                e.to_string()
            })?;

        tracing::debug!("claude api: response headers arrived, status={}", response.status());

        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("claude api: error response {status}: {body}");
            return Err(format!("Anthropic API error {status}: {body}"));
        }

        let parsed = response
            .json::<CreateMessageResponse>()
            .await
            .map_err(|e| e.to_string());
        match &parsed {
            Ok(resp) => tracing::debug!(
                "claude api: parsed ok, stop_reason={}, {} content blocks",
                resp.stop_reason,
                resp.content.len(),
            ),
            Err(e) => tracing::warn!("claude api: failed to parse response body: {e}"),
        }
        parsed
    };

    let timeout = gloo_timers::future::TimeoutFuture::new(CLAUDE_API_TIMEOUT_MS);

    match futures_util::future::select(std::pin::pin!(send_and_parse), std::pin::pin!(timeout))
        .await
    {
        futures_util::future::Either::Left((result, _)) => result,
        futures_util::future::Either::Right(((), _)) => {
            tracing::warn!("claude api: timed out after {}s", CLAUDE_API_TIMEOUT_MS / 1000);
            Err(format!(
                "request to the Claude API timed out after {}s",
                CLAUDE_API_TIMEOUT_MS / 1000
            ))
        }
    }
}

#[cfg(not(feature = "web"))]
pub async fn call_anthropic_api(
    _api_key: &str,
    _request: &super::ai_types::CreateMessageRequest,
) -> Result<CreateMessageResponse, String> {
    Err("the Claude API is only reachable from the browser build".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::ai_types::{ImageSource, ToolResultBlock};
    use std::cell::{Cell, RefCell};

    #[test]
    fn test_query_prometheus_routes_and_extracts_args() {
        let input = serde_json::json!({"addr": "http://prom:9090", "expr": "up"});
        let call = dispatch_tool("query_prometheus", &input).expect("should route");
        assert_eq!(
            call,
            ParsedToolCall::QueryPrometheus {
                addr: "http://prom:9090".to_string(),
                expr: "up".to_string(),
            }
        );
    }

    #[test]
    fn test_query_prometheus_range_routes_and_extracts_args() {
        let input = serde_json::json!({
            "addr": "http://prom:9090",
            "expr": "rate(cpu[5m])",
            "duration": "1h",
            "step": "60s"
        });
        let call = dispatch_tool("query_prometheus_range", &input).expect("should route");
        assert_eq!(
            call,
            ParsedToolCall::QueryPrometheusRange {
                addr: "http://prom:9090".to_string(),
                expr: "rate(cpu[5m])".to_string(),
                duration: "1h".to_string(),
                step: "60s".to_string(),
            }
        );
    }

    #[test]
    fn test_fetch_url_routes_to_http_handler() {
        let input = serde_json::json!({"url": "https://example.com/data.json"});
        let call = dispatch_tool("fetch_url", &input).expect("should route");
        assert_eq!(
            call,
            ParsedToolCall::FetchUrl {
                url: "https://example.com/data.json".to_string(),
            }
        );
    }

    #[test]
    fn test_render_template_routes_to_preview() {
        let input = serde_json::json!({
            "svg": "<svg/>",
            "prometheus_queries": [{"name": "cpu", "addr": "http://prom:9090", "query": "up"}],
            "range_queries": [],
            "http_sources": []
        });
        let call = dispatch_tool("render_template", &input).expect("should route");
        match call {
            ParsedToolCall::RenderTemplate {
                svg,
                prometheus_queries,
                range_queries,
                http_sources,
            } => {
                assert_eq!(svg, "<svg/>");
                assert_eq!(prometheus_queries.len(), 1);
                assert!(range_queries.is_empty());
                assert!(http_sources.is_empty());
            }
            other => panic!("expected RenderTemplate, got {other:?}"),
        }
    }

    #[test]
    fn test_render_template_defaults_missing_fetcher_lists_to_empty() {
        let input = serde_json::json!({"svg": "<svg/>"});
        let call = dispatch_tool("render_template", &input).expect("should route");
        assert_eq!(
            call,
            ParsedToolCall::RenderTemplate {
                svg: "<svg/>".to_string(),
                prometheus_queries: vec![],
                range_queries: vec![],
                http_sources: vec![],
            }
        );
    }

    #[test]
    fn test_unknown_tool_name_returns_error() {
        let err = dispatch_tool("delete_everything", &serde_json::json!({})).unwrap_err();
        assert_eq!(err, ToolError::UnknownTool("delete_everything".to_string()));
    }

    #[tokio::test]
    async fn test_loop_dispatches_tool_then_stops_on_end_turn() {
        let responses = vec![
            CreateMessageResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "toolu_1".to_string(),
                    name: "fetch_url".to_string(),
                    input: serde_json::json!({"url": "http://x"}),
                }],
                stop_reason: "tool_use".to_string(),
            },
            CreateMessageResponse {
                content: vec![ContentBlock::Text {
                    text: "Done!".to_string(),
                }],
                stop_reason: "end_turn".to_string(),
            },
        ];
        let call_index = Cell::new(0usize);
        let dispatched: RefCell<Vec<ParsedToolCall>> = RefCell::new(vec![]);

        let initial_messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "hi".to_string(),
            }],
        }];

        let on_message_calls: RefCell<Vec<AnthropicMessage>> = RefCell::new(vec![]);

        let outcome = run_conversation_loop(
            initial_messages,
            20,
            |_msgs| {
                let i = call_index.get();
                call_index.set(i + 1);
                let response = responses[i].clone();
                async move { response }
            },
            |call| {
                dispatched.borrow_mut().push(call);
                async move { ToolResultContent::Text("fetched!".to_string()) }
            },
            |msg: &AnthropicMessage| on_message_calls.borrow_mut().push(msg.clone()),
        )
        .await;

        let messages = match outcome {
            LoopOutcome::Done { messages } => messages,
            other => panic!("expected Done, got {other:?}"),
        };

        // on_message should fire once per appended turn, in order, and match
        // what ends up in the final messages array (everything but the
        // pre-existing initial user turn).
        assert_eq!(on_message_calls.into_inner(), messages[1..].to_vec());

        // user(hi) -> assistant(tool_use) -> user(tool_result) -> assistant(text)
        assert_eq!(messages.len(), 4, "unexpected message sequence: {messages:?}");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(
            messages[1].content,
            vec![ContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "fetch_url".to_string(),
                input: serde_json::json!({"url": "http://x"}),
            }]
        );
        assert_eq!(messages[2].role, "user");
        assert_eq!(
            messages[2].content,
            vec![ContentBlock::ToolResult {
                tool_use_id: "toolu_1".to_string(),
                content: ToolResultContent::Text("fetched!".to_string()),
            }]
        );
        assert_eq!(messages[3].role, "assistant");
        assert_eq!(
            messages[3].content,
            vec![ContentBlock::Text {
                text: "Done!".to_string()
            }]
        );

        assert_eq!(
            dispatched.into_inner(),
            vec![ParsedToolCall::FetchUrl {
                url: "http://x".to_string()
            }]
        );
    }

    #[tokio::test]
    async fn test_loop_reports_truncated_outcome_when_max_tokens_hit() {
        // Claude can be cut off by the token limit mid-generation — including
        // mid-tool_use, before it ever gets to calling a tool. That must be
        // distinguishable from a normal end-of-turn `Done`, or the caller has
        // no way to tell the user their request silently failed instead of
        // finishing successfully.
        let outcome = run_conversation_loop(
            vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![],
            }],
            20,
            |_msgs| async move {
                CreateMessageResponse {
                    content: vec![ContentBlock::Text {
                        text: "I'll build a chart with both...".to_string(),
                    }],
                    stop_reason: "max_tokens".to_string(),
                }
            },
            |_call| async move { unreachable!("no tool_use block, so no tool should run") },
            |_msg: &AnthropicMessage| {},
        )
        .await;

        match outcome {
            LoopOutcome::Truncated { messages } => {
                assert_eq!(messages.len(), 2, "got: {messages:?}");
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_loop_stops_after_max_iterations() {
        let initial_messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: vec![],
        }];

        // Always asks for another tool call — the loop must not run forever.
        let outcome = run_conversation_loop(
            initial_messages,
            3,
            |_msgs| async move {
                CreateMessageResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "toolu_x".to_string(),
                        name: "fetch_url".to_string(),
                        input: serde_json::json!({"url": "http://x"}),
                    }],
                    stop_reason: "tool_use".to_string(),
                }
            },
            |_call| async move { ToolResultContent::Text("ok".to_string()) },
            |_msg: &AnthropicMessage| {},
        )
        .await;

        match outcome {
            LoopOutcome::MaxIterationsExceeded { messages } => {
                // 1 initial + 3 iterations * 2 messages (assistant + user tool_result)
                assert_eq!(messages.len(), 1 + 3 * 2, "got: {messages:?}");
            }
            other => panic!("expected MaxIterationsExceeded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_loop_reports_unknown_tool_as_error_result_and_continues() {
        let responses = vec![
            CreateMessageResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "toolu_1".to_string(),
                    name: "delete_everything".to_string(),
                    input: serde_json::json!({}),
                }],
                stop_reason: "tool_use".to_string(),
            },
            CreateMessageResponse {
                content: vec![ContentBlock::Text {
                    text: "Sorry, I can't do that.".to_string(),
                }],
                stop_reason: "end_turn".to_string(),
            },
        ];
        let call_index = Cell::new(0usize);

        let outcome = run_conversation_loop(
            vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![],
            }],
            20,
            |_msgs| {
                let i = call_index.get();
                call_index.set(i + 1);
                let response = responses[i].clone();
                async move { response }
            },
            |_call| async move { unreachable!("unknown tool should never be executed") },
            |_msg: &AnthropicMessage| {},
        )
        .await;

        let messages = match outcome {
            LoopOutcome::Done { messages } => messages,
            other => panic!("expected Done, got {other:?}"),
        };
        match &messages[2].content[0] {
            ContentBlock::ToolResult { content, .. } => match content {
                ToolResultContent::Text(text) => assert!(
                    text.contains("delete_everything"),
                    "error tool_result should name the unknown tool, got: {text}"
                ),
                other => panic!("expected text content, got {other:?}"),
            },
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_loop_threads_image_tool_result_through_to_final_messages() {
        // Simulates the render_template tool returning a rendered preview
        // image alongside its confirmation text.
        let responses = vec![
            CreateMessageResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "toolu_1".to_string(),
                    name: "render_template".to_string(),
                    input: serde_json::json!({"svg": "<svg/>"}),
                }],
                stop_reason: "tool_use".to_string(),
            },
            CreateMessageResponse {
                content: vec![ContentBlock::Text {
                    text: "Looks good!".to_string(),
                }],
                stop_reason: "end_turn".to_string(),
            },
        ];
        let call_index = Cell::new(0usize);

        let outcome = run_conversation_loop(
            vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![],
            }],
            20,
            move |_msgs| {
                let i = call_index.get();
                call_index.set(i + 1);
                let response = responses[i].clone();
                async move { response }
            },
            |_call| async move {
                ToolResultContent::Blocks(vec![
                    ToolResultBlock::Text {
                        text: "Rendered successfully.".to_string(),
                    },
                    ToolResultBlock::Image {
                        source: ImageSource::Base64 {
                            media_type: "image/png".to_string(),
                            data: "AAAA".to_string(),
                        },
                    },
                ])
            },
            |_msg: &AnthropicMessage| {},
        )
        .await;

        let messages = match outcome {
            LoopOutcome::Done { messages } => messages,
            other => panic!("expected Done, got {other:?}"),
        };
        match &messages[2].content[0] {
            ContentBlock::ToolResult { content, .. } => match content {
                ToolResultContent::Blocks(blocks) => {
                    assert_eq!(blocks.len(), 2);
                    assert!(matches!(blocks[0], ToolResultBlock::Text { .. }));
                    assert!(matches!(blocks[1], ToolResultBlock::Image { .. }));
                }
                other => panic!("expected block content carrying the image, got {other:?}"),
            },
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn test_close_dangling_tool_uses_is_noop_on_empty_history() {
        let mut messages: Vec<AnthropicMessage> = vec![];
        close_dangling_tool_uses(&mut messages);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_close_dangling_tool_uses_is_noop_when_last_turn_is_user() {
        let mut messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "hi".to_string(),
            }],
        }];
        let before = messages.clone();
        close_dangling_tool_uses(&mut messages);
        assert_eq!(messages, before, "should not touch a history that already ends on a user turn");
    }

    #[test]
    fn test_close_dangling_tool_uses_is_noop_when_last_assistant_turn_is_text_only() {
        let mut messages = vec![AnthropicMessage {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text {
                text: "All done!".to_string(),
            }],
        }];
        let before = messages.clone();
        close_dangling_tool_uses(&mut messages);
        assert_eq!(messages, before, "a text-only assistant turn needs no tool_result");
    }

    #[test]
    fn test_close_dangling_tool_uses_appends_result_for_each_pending_tool_use() {
        let mut messages = vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "render it".to_string(),
                }],
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: vec![
                    ContentBlock::Text {
                        text: "Let me check a couple things.".to_string(),
                    },
                    ContentBlock::ToolUse {
                        id: "toolu_1".to_string(),
                        name: "query_prometheus".to_string(),
                        input: serde_json::json!({}),
                    },
                    ContentBlock::ToolUse {
                        id: "toolu_2".to_string(),
                        name: "render_template".to_string(),
                        input: serde_json::json!({}),
                    },
                ],
            },
        ];

        close_dangling_tool_uses(&mut messages);

        assert_eq!(messages.len(), 3, "should append exactly one closing turn");
        let closing = &messages[2];
        assert_eq!(closing.role, "user");
        assert_eq!(
            closing.content,
            vec![
                ContentBlock::ToolResult {
                    tool_use_id: "toolu_1".to_string(),
                    content: ToolResultContent::Text(
                        "Cancelled — no result was recorded before the conversation moved on."
                            .to_string()
                    ),
                },
                ContentBlock::ToolResult {
                    tool_use_id: "toolu_2".to_string(),
                    content: ToolResultContent::Text(
                        "Cancelled — no result was recorded before the conversation moved on."
                            .to_string()
                    ),
                },
            ]
        );
    }

    #[test]
    fn test_missing_required_field_returns_invalid_input_error() {
        let err = dispatch_tool("query_prometheus", &serde_json::json!({"addr": "http://x"}))
            .unwrap_err();
        assert_eq!(
            err,
            ToolError::InvalidInput {
                tool: "query_prometheus".to_string(),
                reason: "missing or non-string field 'expr'".to_string(),
            }
        );
    }
}
