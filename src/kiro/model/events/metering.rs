//! 计量事件
//!
//! 处理 meteringEvent 类型的事件。当前仅用于安全诊断，
//! 不改变对外 usage 响应契约。

use serde::{Deserialize, Serialize};

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// 计量事件
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MeteringEvent {
    /// 计量单位
    #[serde(default)]
    pub unit: String,

    /// 计量单位复数
    #[serde(default)]
    pub unit_plural: String,

    /// 使用量
    #[serde(default)]
    pub usage: f64,

    /// 捕获其他未使用字段，保持向前兼容。
    #[serde(flatten)]
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    pub(crate) extra: serde_json::Value,
}

impl EventPayload for MeteringEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

impl std::fmt::Display for MeteringEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unit.is_empty() {
            write!(f, "{}", self.usage)
        } else {
            write!(f, "{} {}", self.usage, self.unit)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_metering_payload() {
        let event: MeteringEvent =
            serde_json::from_str(r#"{"unit":"request","unitPlural":"requests","usage":1.25}"#)
                .unwrap();

        assert_eq!(event.unit, "request");
        assert_eq!(event.unit_plural, "requests");
        assert_eq!(event.usage, 1.25);
    }

    #[test]
    fn deserialize_metering_payload_missing_fields() {
        let event: MeteringEvent = serde_json::from_str(r#"{}"#).unwrap();

        assert!(event.unit.is_empty());
        assert!(event.unit_plural.is_empty());
        assert_eq!(event.usage, 0.0);
    }
}
