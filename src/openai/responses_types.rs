//! OpenAI Responses 协议类型
//!
//! 对齐 Kiro-Go `proxy/responses_types.go`。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::OpenAiTool;

/// model 缺省时使用（对齐 Go 的 defaultResponsesModel）
pub const DEFAULT_RESPONSES_MODEL: &str = "claude-sonnet-4.5";

/// 缺省 max_output_tokens
pub const DEFAULT_MAX_OUTPUT_TOKENS: i32 = 64000;

// === 请求 ===

#[derive(Debug, Deserialize)]
pub struct ResponsesRequest {
    #[serde(default)]
    pub model: Option<String>,
    /// string | array | object
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub tools: Option<Vec<OpenAiTool>>,
    /// 仅在客户端方言（`custom` / `namespace`）时改写后透传，其余形状忽略
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default)]
    pub previous_response_id: Option<String>,
    /// 读取但忽略（首版无状态，见 D2）
    #[allow(dead_code)]
    #[serde(default)]
    pub store: Option<bool>,
    /// 接受但不透传
    #[allow(dead_code)]
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_output_tokens: Option<i32>,
    #[serde(default)]
    pub metadata: Option<HashMap<String, String>>,
    /// 结构化输出请求（`text.format`）。**读取仅为可观测性**：
    /// Kiro 上游 `userInputMessageContext` 只有 `toolResults` / `tools`，
    /// 没有 response format 概念，无处透传（见 design D10）。收到时打 warn，不参与转换。
    #[serde(default)]
    pub text: Option<serde_json::Value>,
}

impl ResponsesRequest {
    /// 回显给客户端的模型名（缺省时用默认模型）
    pub fn resolved_model(&self) -> String {
        self.model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_RESPONSES_MODEL)
            .to_string()
    }

    pub fn resolved_max_tokens(&self) -> i32 {
        self.max_output_tokens
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
    }

    /// 是否请求了有状态续接（首版不支持）
    pub fn wants_stateful(&self) -> bool {
        self.previous_response_id
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    }
}

// === 响应 ===

#[derive(Debug, Clone, Serialize)]
pub struct ResponsesObject {
    pub id: String,
    pub object: &'static str,
    pub created_at: i64,
    pub status: &'static str,
    pub model: String,
    pub output: Vec<ResponseOutputItem>,
    pub usage: ResponsesUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponsesError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseOutputItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ResponseContentPart>>,
    /// function_call item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    /// web_search_call item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<serde_json::Value>,
    /// custom_tool_call item：裸文本输入（非 JSON）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// 展平自 namespace 的工具：客户端按 (namespace, name) 匹配，缺此字段会匹配失败
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

impl ResponseOutputItem {
    pub fn message(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            item_type: "message".to_string(),
            role: Some("assistant"),
            status: Some("completed"),
            content: Some(vec![ResponseContentPart::output_text(text)]),
            call_id: None,
            name: None,
            arguments: None,
            action: None,
            input: None,
            namespace: None,
        }
    }

    pub fn function_call(
        id: impl Into<String>,
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            item_type: "function_call".to_string(),
            role: None,
            status: Some("completed"),
            content: None,
            call_id: Some(call_id.into()),
            name: Some(name.into()),
            arguments: Some(arguments.into()),
            action: None,
            input: None,
            namespace: None,
        }
    }

    /// custom_tool_call item：freeform（wire `type: "custom"`）工具的调用
    ///
    /// 客户端为 freeform 工具登记的 payload 类型只接受裸文本 `input`，
    /// 回 `function_call` 会被客户端自身拒绝并触发模型重试（见 design D9）。
    pub fn custom_tool_call(
        id: impl Into<String>,
        call_id: impl Into<String>,
        name: impl Into<String>,
        input: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            item_type: "custom_tool_call".to_string(),
            role: None,
            status: Some("completed"),
            content: None,
            call_id: Some(call_id.into()),
            name: Some(name.into()),
            arguments: None,
            action: None,
            input: Some(input.into()),
            namespace: None,
        }
    }

    /// 展平自 namespace 的工具：还原 `name` 为原名并补 `namespace`
    pub fn with_namespace(mut self, namespace: impl Into<String>, name: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self.name = Some(name.into());
        self
    }

    pub fn web_search_call(id: impl Into<String>, query: &str) -> Self {
        Self {
            id: id.into(),
            item_type: "web_search_call".to_string(),
            role: None,
            status: Some("completed"),
            content: None,
            call_id: None,
            name: None,
            arguments: None,
            action: Some(serde_json::json!({"type": "search", "query": query})),
            input: None,
            namespace: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseContentPart {
    #[serde(rename = "type")]
    pub part_type: &'static str,
    pub text: String,
}

impl ResponseContentPart {
    pub fn output_text(text: impl Into<String>) -> Self {
        Self {
            part_type: "output_text",
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ResponsesUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub total_tokens: i32,
}

impl ResponsesUsage {
    pub fn new(input: i32, output: i32) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponsesError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

/// 生成 output item id
pub fn output_item_id(prefix: &str) -> String {
    format!(
        "{}_{}",
        prefix,
        uuid::Uuid::new_v4().to_string().replace('-', "")
    )
}

/// 生成 response id
pub fn response_id() -> String {
    format!(
        "resp_{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_default_when_absent() {
        let req: ResponsesRequest = serde_json::from_str(r#"{"input":"hi"}"#).unwrap();
        assert_eq!(req.resolved_model(), DEFAULT_RESPONSES_MODEL);
    }

    #[test]
    fn test_model_default_when_blank() {
        let req: ResponsesRequest = serde_json::from_str(r#"{"model":"  ","input":"hi"}"#).unwrap();
        assert_eq!(req.resolved_model(), DEFAULT_RESPONSES_MODEL);
    }

    #[test]
    fn test_model_echoed_as_given() {
        let req: ResponsesRequest =
            serde_json::from_str(r#"{"model":"gpt-4o","input":"hi"}"#).unwrap();
        assert_eq!(req.resolved_model(), "gpt-4o");
    }

    #[test]
    fn test_wants_stateful() {
        let none: ResponsesRequest = serde_json::from_str(r#"{"input":"hi"}"#).unwrap();
        assert!(!none.wants_stateful());

        let blank: ResponsesRequest =
            serde_json::from_str(r#"{"input":"hi","previous_response_id":"  "}"#).unwrap();
        assert!(!blank.wants_stateful());

        let set: ResponsesRequest =
            serde_json::from_str(r#"{"input":"hi","previous_response_id":"resp_1"}"#).unwrap();
        assert!(set.wants_stateful());
    }

    #[test]
    fn test_max_output_tokens() {
        let req: ResponsesRequest =
            serde_json::from_str(r#"{"input":"hi","max_output_tokens":50}"#).unwrap();
        assert_eq!(req.resolved_max_tokens(), 50);

        let dflt: ResponsesRequest = serde_json::from_str(r#"{"input":"hi"}"#).unwrap();
        assert_eq!(dflt.resolved_max_tokens(), DEFAULT_MAX_OUTPUT_TOKENS);
    }

    #[test]
    fn test_store_accepted_but_ignored() {
        let req: ResponsesRequest =
            serde_json::from_str(r#"{"input":"hi","store":true}"#).unwrap();
        assert_eq!(req.store, Some(true));
    }

    #[test]
    fn test_tools_use_top_level_shape() {
        // Responses 的工具是顶层形状，复用 Phase B 的双形状 Deserialize
        let req: ResponsesRequest = serde_json::from_str(
            r#"{"input":"hi","tools":[{"type":"function","name":"f","parameters":{"a":1}}]}"#,
        )
        .unwrap();
        let tools = req.tools.unwrap();
        assert_eq!(tools[0].name, "f");
        assert_eq!(tools[0].parameters["a"], 1);
    }

    #[test]
    fn test_web_search_tool_type_preserved() {
        let req: ResponsesRequest =
            serde_json::from_str(r#"{"input":"hi","tools":[{"type":"web_search"}]}"#).unwrap();
        assert_eq!(req.tools.unwrap()[0].tool_type, "web_search");
    }

    #[test]
    fn test_message_item_shape() {
        let json = serde_json::to_value(ResponseOutputItem::message("msg_1", "hello")).unwrap();
        assert_eq!(json["type"], "message");
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["content"][0]["type"], "output_text");
        assert_eq!(json["content"][0]["text"], "hello");
        // function_call 专属字段不应出现
        assert!(json.get("call_id").is_none());
        assert!(json.get("arguments").is_none());
        assert!(json.get("action").is_none());
    }

    #[test]
    fn test_function_call_item_shape() {
        let json =
            serde_json::to_value(ResponseOutputItem::function_call("fc_1", "c1", "f", "{}"))
                .unwrap();
        assert_eq!(json["type"], "function_call");
        assert_eq!(json["call_id"], "c1");
        assert_eq!(json["name"], "f");
        assert_eq!(json["arguments"], "{}");
        assert!(json.get("role").is_none());
        assert!(json.get("content").is_none());
    }

    #[test]
    fn test_web_search_call_item_shape() {
        let json =
            serde_json::to_value(ResponseOutputItem::web_search_call("ws_1", "rust news")).unwrap();
        assert_eq!(json["type"], "web_search_call");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["action"]["type"], "search");
        assert_eq!(json["action"]["query"], "rust news");
    }

    #[test]
    fn test_usage_total() {
        assert_eq!(ResponsesUsage::new(10, 5).total_tokens, 15);
    }

    #[test]
    fn test_id_prefixes() {
        assert!(response_id().starts_with("resp_"));
        assert!(output_item_id("msg").starts_with("msg_"));
        assert!(output_item_id("fc").starts_with("fc_"));
    }

    #[test]
    fn test_unknown_fields_ignored() {
        let req: ResponsesRequest = serde_json::from_str(
            r#"{"input":"hi","include":["x"],"truncation":"auto","parallel_tool_calls":true}"#,
        )
        .unwrap();
        assert_eq!(req.resolved_model(), DEFAULT_RESPONSES_MODEL);
    }
}
