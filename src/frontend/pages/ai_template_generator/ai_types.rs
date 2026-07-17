//! Rust types for the subset of the Claude Messages API used by the
//! AI-assisted template generator. Serialized/deserialized directly against
//! `https://api.anthropic.com/v1/messages` from WASM via `gloo-net`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
    },
}

/// A `tool_result` block's `content` is either plain text, or — when the
/// tool has something visual to show (e.g. a rendered preview) — a list of
/// blocks mixing text and images, per the Anthropic Messages API.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ToolResultBlock>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultBlock {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CreateMessageRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub tools: Vec<Tool>,
    pub messages: Vec<AnthropicMessage>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CreateMessageResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_block_wire_tags_match_anthropic_api() {
        assert_eq!(
            serde_json::to_value(ContentBlock::Text { text: "hi".to_string() }).unwrap(),
            serde_json::json!({"type": "text", "text": "hi"})
        );
        assert_eq!(
            serde_json::to_value(ContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "query_prometheus".to_string(),
                input: serde_json::json!({"addr": "http://prom:9090"}),
            })
            .unwrap(),
            serde_json::json!({
                "type": "tool_use",
                "id": "toolu_1",
                "name": "query_prometheus",
                "input": {"addr": "http://prom:9090"}
            })
        );
        assert_eq!(
            serde_json::to_value(ContentBlock::ToolResult {
                tool_use_id: "toolu_1".to_string(),
                content: ToolResultContent::Text("42".to_string()),
            })
            .unwrap(),
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": "toolu_1",
                "content": "42"
            })
        );
    }

    #[test]
    fn test_tool_result_with_image_wire_shape_matches_anthropic_api() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "toolu_2".to_string(),
            content: ToolResultContent::Blocks(vec![
                ToolResultBlock::Text {
                    text: "Rendered successfully.".to_string(),
                },
                ToolResultBlock::Image {
                    source: ImageSource::Base64 {
                        media_type: "image/png".to_string(),
                        data: "AAAA".to_string(),
                    },
                },
            ]),
        };
        assert_eq!(
            serde_json::to_value(block).unwrap(),
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": "toolu_2",
                "content": [
                    {"type": "text", "text": "Rendered successfully."},
                    {"type": "image", "source": {
                        "type": "base64", "media_type": "image/png", "data": "AAAA"
                    }}
                ]
            })
        );
    }

    #[test]
    fn test_request_omits_system_when_none() {
        let req = CreateMessageRequest {
            model: "claude-opus-4-8".to_string(),
            max_tokens: 4096,
            system: None,
            tools: vec![],
            messages: vec![],
        };
        let value = serde_json::to_value(&req).unwrap();
        assert!(
            value.get("system").is_none(),
            "system key should be omitted entirely when None, got: {value:?}"
        );
    }

    #[test]
    fn test_response_ignores_unmodeled_fields() {
        // Real API responses carry id/model/role/usage/etc. we don't model —
        // deserialization must not choke on them.
        let raw = serde_json::json!({
            "id": "msg_1",
            "model": "claude-opus-4-8",
            "role": "assistant",
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "content": [{"type": "text", "text": "HELLO"}],
            "stop_reason": "end_turn"
        });
        let response: CreateMessageResponse = serde_json::from_value(raw).unwrap();
        assert_eq!(response.stop_reason, "end_turn");
        assert_eq!(
            response.content,
            vec![ContentBlock::Text { text: "HELLO".to_string() }]
        );
    }

    #[test]
    fn test_tool_use_response_round_trips() {
        let raw = serde_json::json!({
            "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "toolu_abc", "name": "fetch_url", "input": {"url": "http://x"}}
            ],
            "stop_reason": "tool_use"
        });
        let response: CreateMessageResponse = serde_json::from_value(raw).unwrap();
        assert_eq!(response.stop_reason, "tool_use");
        assert_eq!(response.content.len(), 2);
        match &response.content[1] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_abc");
                assert_eq!(name, "fetch_url");
                assert_eq!(input["url"], "http://x");
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
}
