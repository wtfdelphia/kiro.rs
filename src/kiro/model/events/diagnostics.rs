//! Kiro EventStream 脱敏诊断聚合

use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::base::Event;
use crate::kiro::stream_fault::classify_stream_fault;

/// 请求级事件流诊断聚合器。
#[derive(Debug, Clone, Default)]
pub struct EventStreamDiagnostics {
    event_counts: BTreeMap<String, usize>,
    unknown_event_count: usize,
    unknown_event_types: BTreeMap<String, usize>,
    unknown_payload_bytes: usize,
    context_usage_percentage: Option<f64>,
    metering: Option<MeteringDiagnostic>,
    reasoning: ReasoningDiagnostic,
    tool_uses: BTreeMap<String, ToolUseDiagnostic>,
    /// 流内硬错误按上游 code 计数（不含原始消息）。
    stream_error_codes: BTreeMap<String, usize>,
}

/// 可安全序列化/打印的诊断摘要。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiagnosticSummary {
    pub event_counts: BTreeMap<String, usize>,
    pub unknown_event_count: usize,
    pub unknown_event_types: BTreeMap<String, usize>,
    pub unknown_payload_bytes: usize,
    pub context_usage_percentage: Option<f64>,
    pub metering: Option<MeteringDiagnostic>,
    pub reasoning: ReasoningDiagnostic,
    pub tool_uses: Vec<ToolUseDiagnostic>,
    pub anomalies: Vec<DiagnosticAnomaly>,
    /// 流内硬错误按上游 code 计数（不含原始消息）。
    pub stream_error_codes: BTreeMap<String, usize>,
}

/// 计量诊断元数据。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeteringDiagnostic {
    pub unit: String,
    pub unit_plural: String,
    pub usage: f64,
}

/// reasoning 诊断元数据，只保留长度。
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ReasoningDiagnostic {
    pub event_count: usize,
    pub text_chars: usize,
    pub signature_chars: usize,
}

/// 单个逻辑工具调用的脱敏生命周期摘要。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolUseDiagnostic {
    pub tool_use_id_hash: String,
    pub name: String,
    pub chunk_count: usize,
    pub input_chars: usize,
    pub stop_count: usize,
    pub missing_id: bool,
    pub missing_name: bool,
}

/// 诊断异常。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiagnosticAnomaly {
    pub kind: &'static str,
    pub tool_use_id_hash: Option<String>,
}

impl EventStreamDiagnostics {
    /// 生成可用于日志/诊断的 tool-use id hash。
    pub(crate) fn hash_public_id(id: &str) -> String {
        hash_id(id)
    }

    /// 观察一个事件并更新脱敏诊断摘要。
    pub fn observe(&mut self, event: &Event) {
        let event_name = event.diagnostic_name().to_string();
        *self.event_counts.entry(event_name).or_insert(0) += 1;

        match event {
            Event::ToolUse(tool_use) => {
                let key = tool_use.tool_use_id.clone();
                let id_hash = hash_id(&tool_use.tool_use_id);
                let entry = self
                    .tool_uses
                    .entry(key)
                    .or_insert_with(|| ToolUseDiagnostic {
                        tool_use_id_hash: id_hash,
                        name: tool_use.name.clone(),
                        chunk_count: 0,
                        input_chars: 0,
                        stop_count: 0,
                        missing_id: tool_use.tool_use_id.is_empty(),
                        missing_name: tool_use.name.is_empty(),
                    });

                entry.chunk_count += 1;
                entry.input_chars += tool_use.input.chars().count();
                if tool_use.stop {
                    entry.stop_count += 1;
                }
                entry.missing_id |= tool_use.tool_use_id.is_empty();
                entry.missing_name |= tool_use.name.is_empty();
                if entry.name.is_empty() && !tool_use.name.is_empty() {
                    entry.name = tool_use.name.clone();
                }
            }
            Event::Reasoning(reasoning) => {
                self.reasoning.event_count += 1;
                self.reasoning.text_chars += reasoning.text.chars().count();
                self.reasoning.signature_chars += reasoning.signature.chars().count();
            }
            Event::ContextUsage(context_usage) => {
                self.context_usage_percentage = Some(context_usage.context_usage_percentage);
            }
            Event::Metering(metering) => {
                self.metering = Some(MeteringDiagnostic {
                    unit: metering.unit.clone(),
                    unit_plural: metering.unit_plural.clone(),
                    usage: metering.usage,
                });
            }
            Event::Unknown {
                event_type,
                payload,
            } => {
                self.unknown_event_count += 1;
                *self
                    .unknown_event_types
                    .entry(event_type.clone())
                    .or_insert(0) += 1;
                self.unknown_payload_bytes += payload.len();
            }
            Event::Error { .. } | Event::Exception { .. } => {
                // 只记硬错误 code 与次数；ContentLengthExceededException 保留
                // length 语义不计数；原始消息不入摘要（安全边界）
                if let Some(fault) = classify_stream_fault(event) {
                    *self.stream_error_codes.entry(fault.code).or_insert(0) += 1;
                }
            }
            Event::AssistantResponse(_) => {}
        }
    }

    /// 构造可安全打印的摘要。
    pub fn summary(&self) -> DiagnosticSummary {
        let mut tool_uses: Vec<ToolUseDiagnostic> = self.tool_uses.values().cloned().collect();
        tool_uses.sort_by(|a, b| a.tool_use_id_hash.cmp(&b.tool_use_id_hash));

        let mut anomalies = Vec::new();
        for tool in &tool_uses {
            if tool.missing_id {
                anomalies.push(DiagnosticAnomaly {
                    kind: "tool_use_missing_id",
                    tool_use_id_hash: Some(tool.tool_use_id_hash.clone()),
                });
            }
            if tool.missing_name {
                anomalies.push(DiagnosticAnomaly {
                    kind: "tool_use_missing_name",
                    tool_use_id_hash: Some(tool.tool_use_id_hash.clone()),
                });
            }
            if tool.stop_count == 0 {
                anomalies.push(DiagnosticAnomaly {
                    kind: "tool_use_missing_stop",
                    tool_use_id_hash: Some(tool.tool_use_id_hash.clone()),
                });
            } else if tool.stop_count > 1 {
                anomalies.push(DiagnosticAnomaly {
                    kind: "tool_use_duplicate_stop",
                    tool_use_id_hash: Some(tool.tool_use_id_hash.clone()),
                });
            }
        }

        DiagnosticSummary {
            event_counts: self.event_counts.clone(),
            unknown_event_count: self.unknown_event_count,
            unknown_event_types: self.unknown_event_types.clone(),
            unknown_payload_bytes: self.unknown_payload_bytes,
            context_usage_percentage: self.context_usage_percentage,
            metering: self.metering.clone(),
            reasoning: self.reasoning.clone(),
            tool_uses,
            anomalies,
            stream_error_codes: self.stream_error_codes.clone(),
        }
    }

    /// 输出脱敏摘要。正常情况走 debug，异常走 warn。
    pub fn log_summary(&self, protocol: &'static str) {
        let summary = self.summary();
        if summary.is_empty() {
            return;
        }
        if summary.anomalies.is_empty() {
            tracing::debug!(
                protocol,
                summary = ?summary,
                "Kiro EventStream diagnostic summary"
            );
        } else {
            tracing::warn!(
                protocol,
                summary = ?summary,
                "Kiro EventStream diagnostic anomalies"
            );
        }
    }
}

impl DiagnosticSummary {
    fn is_empty(&self) -> bool {
        self.event_counts.is_empty()
            && self.tool_uses.is_empty()
            && self.unknown_event_types.is_empty()
            && self.reasoning.event_count == 0
            && self.metering.is_none()
            && self.context_usage_percentage.is_none()
            && self.unknown_event_count == 0
            && self.unknown_payload_bytes == 0
            && self.stream_error_codes.is_empty()
    }
}

impl Event {
    /// 脱敏诊断使用的事件名。
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::AssistantResponse(_) => "assistantResponseEvent",
            Self::ToolUse(_) => "toolUseEvent",
            Self::Reasoning(_) => "reasoningContentEvent",
            Self::Metering(_) => "meteringEvent",
            Self::ContextUsage(_) => "contextUsageEvent",
            Self::Unknown { .. } => "unknown",
            Self::Error { .. } => "error",
            Self::Exception { .. } => "exception",
        }
    }
}

fn hash_id(id: &str) -> String {
    if id.is_empty() {
        return "missing".to_string();
    }

    let mut hasher = Sha256::new();
    hasher.update(id.as_bytes());
    hex::encode(&hasher.finalize()[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiro::model::events::{
        ContextUsageEvent, Event, MeteringEvent, ReasoningContentEvent, ToolUseEvent,
    };
    use crate::kiro::parser::frame::Frame;
    use crate::kiro::parser::header::{HeaderValue, Headers};

    fn event_frame(event_type: &str, payload: &[u8]) -> Frame {
        let mut headers = Headers::new();
        headers.insert(
            ":message-type".to_string(),
            HeaderValue::String("event".to_string()),
        );
        headers.insert(
            ":event-type".to_string(),
            HeaderValue::String(event_type.to_string()),
        );
        Frame {
            headers,
            payload: payload.to_vec(),
        }
    }

    fn observe_frame(diagnostics: &mut EventStreamDiagnostics, event_type: &str, payload: &[u8]) {
        let event = Event::from_frame(event_frame(event_type, payload)).unwrap();
        diagnostics.observe(&event);
    }

    #[test]
    fn summarizes_synthetic_frame_stream() {
        let mut diagnostics = EventStreamDiagnostics::default();

        observe_frame(
            &mut diagnostics,
            "assistantResponseEvent",
            br#"{"content":"hello"}"#,
        );
        observe_frame(
            &mut diagnostics,
            "toolUseEvent",
            br#"{"name":"Read","toolUseId":"toolu_fixture_id","input":"{\"path\":","stop":false}"#,
        );
        observe_frame(
            &mut diagnostics,
            "toolUseEvent",
            br#"{"name":"Read","toolUseId":"toolu_fixture_id","input":"\"/fixture/file\"}","stop":true}"#,
        );
        observe_frame(
            &mut diagnostics,
            "reasoningContentEvent",
            br#"{"text":"abc","signature":"fixture_signature_value"}"#,
        );
        observe_frame(
            &mut diagnostics,
            "contextUsageEvent",
            br#"{"contextUsagePercentage":42.5}"#,
        );
        observe_frame(
            &mut diagnostics,
            "meteringEvent",
            br#"{"unit":"request","unitPlural":"requests","usage":1.0}"#,
        );

        let summary = diagnostics.summary();
        assert_eq!(
            summary.event_counts["assistantResponseEvent"], 1,
            "assistant text event should be counted"
        );
        assert_eq!(summary.event_counts["toolUseEvent"], 2);
        assert_eq!(summary.event_counts["reasoningContentEvent"], 1);
        assert_eq!(summary.event_counts["contextUsageEvent"], 1);
        assert_eq!(summary.event_counts["meteringEvent"], 1);
        assert_eq!(summary.context_usage_percentage, Some(42.5));
        assert_eq!(summary.metering.as_ref().unwrap().usage, 1.0);
        assert_eq!(
            summary.reasoning.signature_chars,
            "fixture_signature_value".len()
        );
        assert_eq!(summary.tool_uses.len(), 1);
        assert_eq!(summary.tool_uses[0].chunk_count, 2);
        assert_eq!(summary.tool_uses[0].stop_count, 1);
        assert!(summary.anomalies.is_empty());

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("/fixture/file"));
        assert!(!serialized.contains("toolu_fixture_id"));
        assert!(!serialized.contains("fixture_signature_value"));
    }

    #[test]
    fn summarizes_multichunk_tool_without_raw_input() {
        let mut diagnostics = EventStreamDiagnostics::default();
        diagnostics.observe(&Event::ToolUse(ToolUseEvent {
            name: "Read".to_string(),
            tool_use_id: "toolu_fixture_id".to_string(),
            input: r#"{"path":"/fixture/file"}"#.to_string(),
            stop: false,
        }));
        diagnostics.observe(&Event::ToolUse(ToolUseEvent {
            name: "Read".to_string(),
            tool_use_id: "toolu_fixture_id".to_string(),
            input: "}".to_string(),
            stop: true,
        }));

        let summary = diagnostics.summary();
        assert_eq!(summary.tool_uses.len(), 1);
        assert_eq!(summary.tool_uses[0].chunk_count, 2);
        assert_eq!(summary.tool_uses[0].stop_count, 1);
        assert!(summary.anomalies.is_empty());

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("/fixture/file"));
        assert!(!serialized.contains("toolu_fixture_id"));
        assert!(serialized.contains("input_chars"));
    }

    #[test]
    fn reports_tool_lifecycle_anomalies() {
        let mut diagnostics = EventStreamDiagnostics::default();
        diagnostics.observe(&Event::ToolUse(ToolUseEvent {
            name: String::new(),
            tool_use_id: String::new(),
            input: "fixture input".to_string(),
            stop: false,
        }));
        diagnostics.observe(&Event::ToolUse(ToolUseEvent {
            name: String::new(),
            tool_use_id: String::new(),
            input: String::new(),
            stop: true,
        }));
        diagnostics.observe(&Event::ToolUse(ToolUseEvent {
            name: String::new(),
            tool_use_id: String::new(),
            input: String::new(),
            stop: true,
        }));
        diagnostics.observe(&Event::ToolUse(ToolUseEvent {
            name: "Read".to_string(),
            tool_use_id: "toolu_missing_stop".to_string(),
            input: "{}".to_string(),
            stop: false,
        }));

        let summary = diagnostics.summary();
        let kinds: Vec<_> = summary.anomalies.iter().map(|a| a.kind).collect();
        assert!(kinds.contains(&"tool_use_missing_id"));
        assert!(kinds.contains(&"tool_use_missing_name"));
        assert!(kinds.contains(&"tool_use_missing_stop"));
        assert!(kinds.contains(&"tool_use_duplicate_stop"));

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("fixture input"));
        assert!(!serialized.contains("toolu_missing_stop"));
    }

    #[test]
    fn summarizes_reasoning_without_signature() {
        let mut diagnostics = EventStreamDiagnostics::default();
        diagnostics.observe(&Event::Reasoning(ReasoningContentEvent {
            text: "abc".to_string(),
            signature: "fixture_signature_value".to_string(),
            ..Default::default()
        }));

        let summary = diagnostics.summary();
        assert_eq!(summary.reasoning.event_count, 1);
        assert_eq!(summary.reasoning.text_chars, 3);
        assert_eq!(
            summary.reasoning.signature_chars,
            "fixture_signature_value".len()
        );

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("fixture_signature_value"));
        assert!(!serialized.contains("abc"));
    }

    #[test]
    fn summarizes_usage_signals_and_unknowns() {
        let mut diagnostics = EventStreamDiagnostics::default();
        diagnostics.observe(&Event::ContextUsage(ContextUsageEvent {
            context_usage_percentage: 21.5,
        }));
        diagnostics.observe(&Event::Metering(MeteringEvent {
            unit: "request".to_string(),
            unit_plural: "requests".to_string(),
            usage: 0.5,
            ..Default::default()
        }));
        diagnostics.observe(&Event::Unknown {
            event_type: "futureEvent".to_string(),
            payload: b"fixture future payload".to_vec(),
        });

        let summary = diagnostics.summary();
        assert_eq!(summary.unknown_event_count, 1);
        assert_eq!(summary.unknown_event_types["futureEvent"], 1);
        assert_eq!(
            summary.unknown_payload_bytes,
            b"fixture future payload".len()
        );
        assert_eq!(summary.context_usage_percentage, Some(21.5));
        assert_eq!(summary.metering.as_ref().unwrap().usage, 0.5);

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("fixture future payload"));
    }

    #[test]
    fn counts_stream_error_codes_without_messages() {
        let mut diagnostics = EventStreamDiagnostics::default();
        diagnostics.observe(&Event::Error {
            error_code: "InternalServerException".to_string(),
            error_message: "fixture secret message".to_string(),
        });
        diagnostics.observe(&Event::Error {
            error_code: "InternalServerException".to_string(),
            error_message: "another message".to_string(),
        });
        diagnostics.observe(&Event::Exception {
            exception_type: "ValidationException".to_string(),
            message: "fixture exception detail".to_string(),
        });

        let summary = diagnostics.summary();
        assert_eq!(summary.stream_error_codes["InternalServerException"], 2);
        assert_eq!(summary.stream_error_codes["ValidationException"], 1);

        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("fixture secret message"));
        assert!(!serialized.contains("another message"));
        assert!(!serialized.contains("fixture exception detail"));
    }

    #[test]
    fn content_length_exception_not_counted_as_stream_error() {
        let mut diagnostics = EventStreamDiagnostics::default();
        diagnostics.observe(&Event::Exception {
            exception_type: "ContentLengthExceededException".to_string(),
            message: "too long".to_string(),
        });

        let summary = diagnostics.summary();
        assert!(
            summary.stream_error_codes.is_empty(),
            "保留 length 语义的异常不是硬错误"
        );
    }
}
