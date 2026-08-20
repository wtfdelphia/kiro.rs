//! 上游生成流硬错误分类。
//!
//! 硬错误指应以协议错误形式对客户端可见的上游事件；已有语义映射的异常
//!（如 `ContentLengthExceededException` → length/max_tokens）不属于硬错误。
//! 见 openspec/changes/add-stream-error-propagation/。

use super::model::events::Event;

/// 保留既有 length/max_tokens 语义、不作为硬错误处理的异常类型。
pub const CONTENT_LENGTH_EXCEEDED: &str = "ContentLengthExceededException";

/// 上游生成流中出现的硬错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamFault {
    /// 上游错误码：`error_code` 或 `exception_type`。
    pub code: String,
    /// 上游错误消息。
    pub message: String,
}

impl StreamFault {
    /// 客户端可见的错误消息：固定前缀 + 上游 code/message。
    ///
    /// 仅由上游 code 与 message 组成，不附带其他上下文，避免泄漏凭据或请求细节。
    pub fn client_message(&self) -> String {
        let message = self.message.trim();
        if message.is_empty() {
            format!("Kiro upstream error ({})", self.code)
        } else {
            format!("Kiro upstream error ({}): {}", self.code, message)
        }
    }
}

/// 将 Kiro 事件分类为硬错误；`None` 表示非硬错误。
///
/// - `Event::Error` → 硬错误（code = 上游 error_code）
/// - `Event::Exception` → 硬错误（code = exception_type），
///   但 `ContentLengthExceededException` 保留既有语义映射
pub fn classify_stream_fault(event: &Event) -> Option<StreamFault> {
    match event {
        Event::Error {
            error_code,
            error_message,
        } => Some(StreamFault {
            code: error_code.clone(),
            message: error_message.clone(),
        }),
        Event::Exception {
            exception_type,
            message,
        } => {
            if exception_type == CONTENT_LENGTH_EXCEEDED {
                None
            } else {
                Some(StreamFault {
                    code: exception_type.clone(),
                    message: message.clone(),
                })
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiro::model::events::AssistantResponseEvent;

    #[test]
    fn error_event_is_hard_fault() {
        let event = Event::Error {
            error_code: "InternalServerException".to_string(),
            error_message: "upstream exploded".to_string(),
        };
        let fault = classify_stream_fault(&event).expect("Error 事件应分类为硬错误");
        assert_eq!(fault.code, "InternalServerException");
        assert_eq!(fault.message, "upstream exploded");
    }

    #[test]
    fn generic_exception_is_hard_fault() {
        let event = Event::Exception {
            exception_type: "ThrottlingException".to_string(),
            message: "slow down".to_string(),
        };
        let fault = classify_stream_fault(&event).expect("普通异常应分类为硬错误");
        assert_eq!(fault.code, "ThrottlingException");
        assert_eq!(fault.message, "slow down");
    }

    #[test]
    fn content_length_exception_is_not_fault() {
        let event = Event::Exception {
            exception_type: CONTENT_LENGTH_EXCEEDED.to_string(),
            message: "too long".to_string(),
        };
        assert!(
            classify_stream_fault(&event).is_none(),
            "ContentLengthExceededException 应保留既有 length 语义"
        );
    }

    #[test]
    fn normal_events_are_not_faults() {
        let mut payload = AssistantResponseEvent::default();
        payload.content = "hello".to_string();
        let event = Event::AssistantResponse(payload);
        assert!(classify_stream_fault(&event).is_none());
    }

    #[test]
    fn client_message_composes_only_code_and_message() {
        let fault = StreamFault {
            code: "X".to_string(),
            message: "detail".to_string(),
        };
        assert_eq!(fault.client_message(), "Kiro upstream error (X): detail");
    }

    #[test]
    fn client_message_falls_back_when_message_empty() {
        let fault = StreamFault {
            code: "X".to_string(),
            message: "   ".to_string(),
        };
        assert_eq!(fault.client_message(), "Kiro upstream error (X)");
    }
}
