//! WS ingress 错误分类与关闭码映射
//!
//! 移植 sub2api 的三类错误语义（`docs/websocket-support-optimization-design.md` §4.9）：
//! turn 失败阶段（stage）+ 是否已写出下游（wrote_downstream）共同决定能否重试、
//! 以事件还是关闭表达错误。

use axum::extract::ws::close_code;

/// 关闭码：容量压力，稍后再试（IANA 1013，axum `close_code` 未提供该常量）
// 保留原因：spec「连接保护与关闭码」契约常量（design §4.4 容量丢失 → 1013）；
// P0 热缩容不主动断连，无容量丢失关闭路径，供未来缩容/透传路径使用。
#[allow(dead_code)]
pub const CLOSE_TRY_AGAIN_LATER: u16 = 1013;
/// 关闭码：优雅 shutdown / turn 间空闲超时
pub const CLOSE_GOING_AWAY: u16 = close_code::AWAY; // 1001
/// 关闭码：协议违规（首帧契约违规、非法帧）
pub const CLOSE_POLICY_VIOLATION: u16 = close_code::POLICY; // 1008
/// 关闭码：服务端内部错误
pub const CLOSE_INTERNAL_ERROR: u16 = close_code::ERROR; // 1011
/// 关闭码：消息过大
pub const CLOSE_MESSAGE_TOO_BIG: u16 = close_code::SIZE; // 1009

/// turn 失败所处阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStage {
    /// 请求归一 / prepare（请求本身不合法，如 previous_response_id 拒绝）
    Prepare,
    /// 上游调用发起（`call_api_stream` 返回错误）
    CallUpstream,
    /// 上游流读取（流中断 / 读超时）
    ReadUpstream,
    /// 写下游客户端（写出错即客户端已离开）
    WriteClient,
}

impl TurnStage {
    /// 是否属于上游阶段（可换凭据重试的候选）
    pub fn is_upstream(&self) -> bool {
        matches!(self, Self::CallUpstream | Self::ReadUpstream)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::CallUpstream => "call_upstream",
            Self::ReadUpstream => "read_upstream",
            Self::WriteClient => "write_client",
        }
    }
}

/// 客户端侧关闭（含客户端主动断开与服务端判定的关闭语义）
#[derive(Debug)]
pub struct ClientClose {
    pub status: u16,
    pub reason: String,
}

impl ClientClose {
    pub fn new(status: u16, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
        }
    }
}

/// WS turn 级错误
///
/// 判定规则（design §4.9，照抄 sub2api D3/D7）：
/// - `wrote_downstream == false` 且失败发生在上游阶段 → 可换凭据重试一次；
/// - 已写出任何事件 → 错误只能以 `error` / `response.failed` 事件表达，连接存活；
/// - `ClientClose` → 会话结束，不重试。
#[derive(Debug)]
pub enum WsTurnError {
    Turn {
        stage: TurnStage,
        cause: anyhow::Error,
        wrote_downstream: bool,
    },
    ClientClose(ClientClose),
}

impl WsTurnError {
    pub fn turn(stage: TurnStage, cause: anyhow::Error, wrote_downstream: bool) -> Self {
        Self::Turn {
            stage,
            cause,
            wrote_downstream,
        }
    }

    pub fn client_close(status: u16, reason: impl Into<String>) -> Self {
        Self::ClientClose(ClientClose::new(status, reason))
    }

    /// 仅「上游阶段失败且未写出任何下游事件」可重试（design §4.9）
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Turn {
                stage,
                wrote_downstream,
                ..
            } => !wrote_downstream && stage.is_upstream(),
            Self::ClientClose(_) => false,
        }
    }

    /// 失败阶段（ClientClose 返回 None）
    pub fn stage(&self) -> Option<TurnStage> {
        match self {
            Self::Turn { stage, .. } => Some(*stage),
            Self::ClientClose(_) => None,
        }
    }

    /// 错误原因的可读描述
    pub fn message(&self) -> String {
        match self {
            Self::Turn { cause, .. } => cause.to_string(),
            Self::ClientClose(c) => c.reason.clone(),
        }
    }

    /// 需要关闭连接时的关闭码（协议违规 1008 / 内部错误 1011）
    pub fn close_code(&self) -> u16 {
        match self {
            Self::Turn {
                stage: TurnStage::Prepare,
                ..
            } => CLOSE_POLICY_VIOLATION,
            Self::Turn { .. } => CLOSE_INTERNAL_ERROR,
            Self::ClientClose(c) => c.status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    fn turn(stage: TurnStage, wrote: bool) -> WsTurnError {
        WsTurnError::turn(stage, anyhow!("boom"), wrote)
    }

    /// 任务 5.1：wrote_downstream=false 且上游阶段失败才可重试
    #[test]
    fn retryable_matrix() {
        assert!(turn(TurnStage::CallUpstream, false).is_retryable());
        assert!(turn(TurnStage::ReadUpstream, false).is_retryable());
        assert!(!turn(TurnStage::CallUpstream, true).is_retryable());
        assert!(!turn(TurnStage::ReadUpstream, true).is_retryable());
        assert!(!turn(TurnStage::Prepare, false).is_retryable());
        assert!(!turn(TurnStage::WriteClient, false).is_retryable());
        assert!(!WsTurnError::client_close(1000, "bye").is_retryable());
    }

    /// 任务 5.2：关闭码常量取值
    #[test]
    fn close_code_constants() {
        assert_eq!(CLOSE_GOING_AWAY, 1001);
        assert_eq!(CLOSE_POLICY_VIOLATION, 1008);
        assert_eq!(CLOSE_MESSAGE_TOO_BIG, 1009);
        assert_eq!(CLOSE_INTERNAL_ERROR, 1011);
        assert_eq!(CLOSE_TRY_AGAIN_LATER, 1013);
    }

    /// 任务 5.2：错误到关闭码的转换
    #[test]
    fn close_code_conversion() {
        assert_eq!(turn(TurnStage::Prepare, false).close_code(), 1008);
        assert_eq!(turn(TurnStage::CallUpstream, false).close_code(), 1011);
        assert_eq!(turn(TurnStage::ReadUpstream, true).close_code(), 1011);
        assert_eq!(
            WsTurnError::client_close(CLOSE_GOING_AWAY, "shutdown").close_code(),
            1001
        );
    }

    #[test]
    fn stage_and_message_accessors() {
        let e = turn(TurnStage::ReadUpstream, true);
        assert_eq!(e.stage(), Some(TurnStage::ReadUpstream));
        assert_eq!(e.message(), "boom");
        let c = WsTurnError::client_close(1001, "idle");
        assert_eq!(c.stage(), None);
        assert_eq!(c.message(), "idle");
    }
}
