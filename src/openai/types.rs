//! OpenAI Chat Completions 协议类型
//!
//! 与 `crate::anthropic::types` 平行，互不侵入。

use serde::{Deserialize, Serialize};

/// 缺省 max_tokens（与 Anthropic 侧默认上限一致）
pub const DEFAULT_MAX_TOKENS: i32 = 64000;

// === 请求 ===

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub stream_options: Option<StreamOptions>,
    pub max_tokens: Option<i32>,
    pub max_completion_tokens: Option<i32>,
    /// 接受但不透传：Kiro 上游无对应字段
    #[allow(dead_code)]
    pub temperature: Option<f64>,
    /// 接受但不透传
    #[allow(dead_code)]
    pub top_p: Option<f64>,
    pub tools: Option<Vec<OpenAiTool>>,
    /// 接受但不透传：Anthropic 侧的 tool_choice 语义不同，首版不映射
    #[allow(dead_code)]
    pub tool_choice: Option<serde_json::Value>,
}

impl ChatCompletionRequest {
    /// max_tokens / max_completion_tokens 取先出现的非空值，都缺省时用默认上限
    pub fn resolved_max_tokens(&self) -> i32 {
        self.max_tokens
            .or(self.max_completion_tokens)
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_TOKENS)
    }

    /// 客户端是否要求随流返回 usage
    pub fn include_usage(&self) -> bool {
        self.stream_options
            .as_ref()
            .map(|o| o.include_usage)
            .unwrap_or(false)
    }
}

#[derive(Debug, Deserialize)]
pub struct StreamOptions {
    #[serde(default)]
    pub include_usage: bool,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    /// string | parts[] | null
    #[serde(default)]
    pub content: serde_json::Value,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiToolCall {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub function: OpenAiFunctionCall,
}

#[derive(Debug, Default, Deserialize)]
pub struct OpenAiFunctionCall {
    #[serde(default)]
    pub name: String,
    /// JSON 字符串（OpenAI 协议如此定义）
    #[serde(default)]
    pub arguments: String,
}

/// 工具定义
///
/// 手写 `Deserialize` 以同时兼容两种形状（对齐 Kiro-Go `translator.go:1096`）：
/// - Chat Completions：`{"type":"function","function":{"name":..,"parameters":..}}`
/// - Responses：`{"type":"function","name":..,"parameters":..}`
///
/// 漏掉任一形状会导致 name 为空、上游返回 400。
#[derive(Debug, Clone)]
pub struct OpenAiTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    /// 原始 type。Chat 端点不使用（它不提供 server-side 工具，见 D10），
    /// 保留供 Responses 端点（Phase C）判定 web_search 等 server-side tool。
    #[allow(dead_code)]
    pub tool_type: String,
}

impl<'de> Deserialize<'de> for OpenAiTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default, rename = "type")]
            tool_type: String,
            #[serde(default)]
            function: Option<RawFunction>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            parameters: Option<serde_json::Value>,
        }

        #[derive(Deserialize)]
        struct RawFunction {
            #[serde(default)]
            name: String,
            #[serde(default)]
            description: String,
            #[serde(default)]
            parameters: Option<serde_json::Value>,
        }

        let raw = Raw::deserialize(deserializer)?;

        // 嵌套形状优先；缺失时回落顶层
        let (name, description, parameters) = match raw.function {
            Some(f) => (f.name, f.description, f.parameters),
            None => (
                raw.name.unwrap_or_default(),
                raw.description.unwrap_or_default(),
                raw.parameters,
            ),
        };

        Ok(OpenAiTool {
            name,
            description,
            parameters: parameters.unwrap_or_else(|| serde_json::json!({})),
            tool_type: raw.tool_type,
        })
    }
}

// === 响应 ===

#[derive(Debug, Serialize)]
pub struct ChatCompletion {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub index: i32,
    pub message: AssistantMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct AssistantMessage {
    pub role: &'static str,
    /// 无文本时为 null（OpenAI 协议允许）
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ResponseToolCall>>,
}

#[derive(Debug, Serialize)]
pub struct ResponseToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: &'static str,
    pub function: ResponseFunctionCall,
}

#[derive(Debug, Serialize)]
pub struct ResponseFunctionCall {
    pub name: String,
    /// JSON 字符串
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

impl Usage {
    pub fn new(prompt: i32, completion: i32) -> Self {
        Self {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_nested_shape() {
        let tool: OpenAiTool = serde_json::from_str(
            r#"{"type":"function","function":{"name":"get_weather","description":"d",
                "parameters":{"type":"object","properties":{"city":{"type":"string"}}}}}"#,
        )
        .unwrap();
        assert_eq!(tool.name, "get_weather");
        assert_eq!(tool.description, "d");
        assert_eq!(tool.tool_type, "function");
        assert_eq!(tool.parameters["type"], "object");
    }

    #[test]
    fn test_tool_top_level_shape() {
        let tool: OpenAiTool = serde_json::from_str(
            r#"{"type":"function","name":"get_weather","description":"d",
                "parameters":{"type":"object","properties":{"city":{"type":"string"}}}}"#,
        )
        .unwrap();
        assert_eq!(tool.name, "get_weather");
        assert_eq!(tool.description, "d");
        assert_eq!(tool.parameters["type"], "object");
    }

    #[test]
    fn test_tool_both_shapes_equivalent() {
        let nested: OpenAiTool = serde_json::from_str(
            r#"{"type":"function","function":{"name":"f","description":"d","parameters":{"a":1}}}"#,
        )
        .unwrap();
        let top: OpenAiTool =
            serde_json::from_str(r#"{"type":"function","name":"f","description":"d","parameters":{"a":1}}"#)
                .unwrap();
        assert_eq!(nested.name, top.name);
        assert_eq!(nested.description, top.description);
        assert_eq!(nested.parameters, top.parameters);
    }

    #[test]
    fn test_tool_missing_parameters_defaults_to_empty_object() {
        let tool: OpenAiTool =
            serde_json::from_str(r#"{"type":"function","name":"f"}"#).unwrap();
        assert_eq!(tool.parameters, serde_json::json!({}));
    }

    #[test]
    fn test_web_search_tool_type_preserved() {
        // Responses 端点（Phase C）需要 type 来判定 server-side tool
        let tool: OpenAiTool = serde_json::from_str(r#"{"type":"web_search"}"#).unwrap();
        assert_eq!(tool.tool_type, "web_search");
        assert!(tool.name.is_empty());
    }

    #[test]
    fn test_request_ignores_unknown_fields() {
        let req: ChatCompletionRequest = serde_json::from_str(
            r#"{"model":"m","messages":[],"seed":42,"logprobs":true,"n":2}"#,
        )
        .unwrap();
        assert_eq!(req.model, "m");
    }

    #[test]
    fn test_content_three_shapes() {
        for body in [
            r#"{"role":"user","content":"hi"}"#,
            r#"{"role":"user","content":[{"type":"text","text":"hi"}]}"#,
            r#"{"role":"assistant","content":null}"#,
        ] {
            let msg: ChatMessage = serde_json::from_str(body).unwrap();
            assert!(!msg.role.is_empty());
        }
    }

    #[test]
    fn test_max_tokens_precedence() {
        let with_max: ChatCompletionRequest =
            serde_json::from_str(r#"{"model":"m","messages":[],"max_tokens":100}"#).unwrap();
        assert_eq!(with_max.resolved_max_tokens(), 100);

        let with_completion: ChatCompletionRequest = serde_json::from_str(
            r#"{"model":"m","messages":[],"max_completion_tokens":200}"#,
        )
        .unwrap();
        assert_eq!(with_completion.resolved_max_tokens(), 200);

        // max_tokens 优先
        let both: ChatCompletionRequest = serde_json::from_str(
            r#"{"model":"m","messages":[],"max_tokens":100,"max_completion_tokens":200}"#,
        )
        .unwrap();
        assert_eq!(both.resolved_max_tokens(), 100);

        let neither: ChatCompletionRequest =
            serde_json::from_str(r#"{"model":"m","messages":[]}"#).unwrap();
        assert_eq!(neither.resolved_max_tokens(), DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn test_include_usage_default_false() {
        let plain: ChatCompletionRequest =
            serde_json::from_str(r#"{"model":"m","messages":[],"stream":true}"#).unwrap();
        assert!(!plain.include_usage());

        let opted: ChatCompletionRequest = serde_json::from_str(
            r#"{"model":"m","messages":[],"stream":true,"stream_options":{"include_usage":true}}"#,
        )
        .unwrap();
        assert!(opted.include_usage());

        let explicit_false: ChatCompletionRequest = serde_json::from_str(
            r#"{"model":"m","messages":[],"stream":true,"stream_options":{"include_usage":false}}"#,
        )
        .unwrap();
        assert!(!explicit_false.include_usage());
    }

    #[test]
    fn test_usage_total() {
        let u = Usage::new(10, 5);
        assert_eq!(u.total_tokens, 15);
    }

    #[test]
    fn test_tool_calls_parsed() {
        let msg: ChatMessage = serde_json::from_str(
            r#"{"role":"assistant","content":null,"tool_calls":[
                {"id":"call_1","type":"function",
                 "function":{"name":"f","arguments":"{\"a\":1}"}}]}"#,
        )
        .unwrap();
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "f");
        assert_eq!(calls[0].function.arguments, r#"{"a":1}"#);
    }
}
