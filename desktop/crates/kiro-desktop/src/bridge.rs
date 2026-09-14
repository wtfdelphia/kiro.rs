//! 跨运行时事件桥（设计文档 §4.2 的骨架裁剪版）
//!
//! 边界规则：一切核心调用经 tokio 侧 `handle.spawn`，GPUI 任务不直接
//! await 网络 future；事件只传「发生了什么」，UI 收到后重拉数据。
//! change 4 起 `Bootstrapped` 携带 [`crate::core::CoreHandle`]（服务句柄
//! + tokio handle），凭据视图经它读写数据。change 5 扩入服务器状态机
//! 事件与两阶段退出的收敛信号。

use futures::channel::mpsc;
use std::fmt;

use crate::core::CoreHandle;
use crate::core::server::ServerStatus;

/// tokio 侧产生、GPUI 侧消费的核心事件
#[derive(Clone)]
pub enum CoreEvent {
    /// 核心装配完成，携带凭据总数与核心句柄
    Bootstrapped {
        credential_count: usize,
        handle: CoreHandle,
    },
    /// 装配失败（启动错误视图展示）
    BootstrapFailed(String),
    /// 内嵌服务器状态变化（状态机，见 `core::server`）
    ServerStatus(ServerStatus),
    /// 服务器 drain 收敛完成（两阶段退出的回执事件，视图可忽略）
    ServerDrained,
    /// 两阶段退出阶段 1 完成，主事件循环应进入阶段 2（`cx.quit()`）
    ShutdownPrepared,
}

impl fmt::Debug for CoreEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreEvent::Bootstrapped {
                credential_count, ..
            } => f
                .debug_struct("Bootstrapped")
                .field("credential_count", credential_count)
                .finish(),
            CoreEvent::BootstrapFailed(msg) => {
                f.debug_tuple("BootstrapFailed").field(msg).finish()
            }
            CoreEvent::ServerStatus(s) => f.debug_tuple("ServerStatus").field(s).finish(),
            CoreEvent::ServerDrained => f.write_str("ServerDrained"),
            CoreEvent::ShutdownPrepared => f.write_str("ShutdownPrepared"),
        }
    }
}

/// 事件通道（tokio 侧持有 sender，GPUI 侧持有 receiver）
pub fn event_channel() -> (mpsc::UnboundedSender<CoreEvent>, mpsc::UnboundedReceiver<CoreEvent>) {
    mpsc::unbounded()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt};

    fn test_handle() -> CoreHandle {
        let service = std::sync::Arc::new(kiro_rs::admin::AdminService::new_with_runtime(
            std::sync::Arc::new(
                kiro_rs::kiro::token_manager::MultiTokenManager::new(
                    kiro_rs::model::config::Config::default(),
                    vec![],
                    None,
                    None,
                    false,
                )
                .unwrap(),
            ),
            Vec::<String>::new(),
            None,
            None,
        ));
        CoreHandle::new(service, tokio::runtime::Handle::current())
    }

    #[tokio::test]
    async fn event_roundtrip() {
        let (mut tx, mut rx) = event_channel();
        tx.send(CoreEvent::Bootstrapped {
            credential_count: 3,
            handle: test_handle(),
        })
        .await
        .unwrap();
        tx.send(CoreEvent::BootstrapFailed("bad config".into()))
            .await
            .unwrap();

        match rx.next().await {
            Some(CoreEvent::Bootstrapped {
                credential_count, ..
            }) => assert_eq!(credential_count, 3),
            other => panic!("期望 Bootstrapped，实际 {:?}", other),
        }
        match rx.next().await {
            Some(CoreEvent::BootstrapFailed(msg)) => assert_eq!(msg, "bad config"),
            other => panic!("期望 BootstrapFailed，实际 {:?}", other),
        }
        tx.send(CoreEvent::ServerStatus(
            ServerStatus::Running("127.0.0.1:8990".into()),
        ))
        .await
        .unwrap();
        tx.send(CoreEvent::ServerDrained).await.unwrap();
        tx.send(CoreEvent::ShutdownPrepared).await.unwrap();

        match rx.next().await {
            Some(CoreEvent::ServerStatus(ServerStatus::Running(addr))) => {
                assert_eq!(addr, "127.0.0.1:8990")
            }
            other => panic!("期望 ServerStatus，实际 {:?}", other),
        }
        assert!(matches!(rx.next().await, Some(CoreEvent::ServerDrained)));
        assert!(matches!(rx.next().await, Some(CoreEvent::ShutdownPrepared)));
    }
}
