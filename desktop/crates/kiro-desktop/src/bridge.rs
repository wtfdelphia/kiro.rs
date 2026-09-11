//! 跨运行时事件桥（设计文档 §4.2 的骨架裁剪版）
//!
//! 边界规则：一切核心调用经 tokio 侧 `handle.spawn`，GPUI 任务不直接
//! await 网络 future；事件只传「发生了什么」，UI 收到后重拉数据。
//! 本期事件面只有装配结果，`ServerStatus` / `ServerDrained` 等由后续
//! change 扩 [`CoreEvent`]。

use futures::channel::mpsc;

/// tokio 侧产生、GPUI 侧消费的核心事件
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    /// 核心装配完成，携带凭据总数
    Bootstrapped { credential_count: usize },
    /// 装配失败（启动错误视图展示）
    BootstrapFailed(String),
}

/// 事件通道（tokio 侧持有 sender，GPUI 侧持有 receiver）
pub fn event_channel() -> (mpsc::UnboundedSender<CoreEvent>, mpsc::UnboundedReceiver<CoreEvent>) {
    mpsc::unbounded()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt};

    #[tokio::test]
    async fn event_roundtrip() {
        let (mut tx, mut rx) = event_channel();
        tx.send(CoreEvent::Bootstrapped { credential_count: 3 })
            .await
            .unwrap();
        tx.send(CoreEvent::BootstrapFailed("bad config".into()))
            .await
            .unwrap();

        assert_eq!(
            rx.next().await,
            Some(CoreEvent::Bootstrapped { credential_count: 3 })
        );
        assert_eq!(
            rx.next().await,
            Some(CoreEvent::BootstrapFailed("bad config".into()))
        );
    }
}
