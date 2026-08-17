//! 推理内容事件
//!
//! 处理 reasoningContentEvent 类型的事件。当前仅用于安全诊断，
//! 不直接暴露到 Anthropic/OpenAI 响应契约。

use serde::{Deserialize, Serialize};

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// 推理内容事件
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningContentEvent {
    /// 推理文本片段
    #[serde(default)]
    pub text: String,

    /// 上游签名。仅允许用于长度诊断，禁止写入日志或响应。
    #[serde(default)]
    pub signature: String,

    /// 捕获其他未使用字段，保持向前兼容。
    #[serde(flatten)]
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    pub(crate) extra: serde_json::Value,
}

impl EventPayload for ReasoningContentEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_reasoning_payload() {
        let event: ReasoningContentEvent =
            serde_json::from_str(r#"{"text":"ok","signature":"fixture_signature_value"}"#).unwrap();

        assert_eq!(event.text, "ok");
        assert_eq!(event.signature, "fixture_signature_value");
    }

    #[test]
    fn deserialize_reasoning_payload_missing_fields() {
        let event: ReasoningContentEvent = serde_json::from_str(r#"{}"#).unwrap();

        assert!(event.text.is_empty());
        assert!(event.signature.is_empty());
    }
}
