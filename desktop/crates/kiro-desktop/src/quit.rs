//! 两阶段退出协调（设计文档 §5.2）
//!
//! `gpui-pre` 的 `App::shutdown()` 对 `on_app_quit` 回调只等
//! `SHUTDOWN_TIMEOUT`（200ms），直接 `cx.quit()` 会掐掉在途请求与活跃
//! WS。因此一切等待必须发生在 `cx.quit()` 之前：
//!
//! ```text
//! 阶段 1（上限 12 秒）：请求停止内嵌服务器 → 等收敛回执 →
//!   统计强制落盘 → SQLite WAL checkpoint
//! 阶段 2：主事件循环收到 ShutdownPrepared 后 cx.quit()
//! ```
//!
//! 退出请求的入口有三处：`RequestQuit` action（覆写 cmd-q / alt-f4）、
//! 窗口关闭拦截（`on_window_should_close`）、（change 6）托盘菜单。
//! 全部收敛到 [`begin_phase1`]，由全局 `QUITTING` 标志去重。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::channel::mpsc;
use gpui_kit::*;

use crate::bridge::CoreEvent;
use crate::core::CoreHandle;
use crate::store::SqliteStore;

/// 阶段 1 上限 = drain 兜底 10 秒 + 2 秒余量（设计文档 §5.2）
pub const QUIT_PHASE1_TIMEOUT: Duration = Duration::from_secs(12);

gpui_kit::actions!(kiro_desktop, [RequestQuit]);

/// 全局退出标志（多入口去重 + 标题栏徽标）
static QUITTING: AtomicBool = AtomicBool::new(false);

/// 是否已进入退出流程（视图渲染用）
pub fn is_quitting() -> bool {
    QUITTING.load(Ordering::SeqCst)
}

/// 启动阶段 1。已在退出流程中时返回 `false`，不重复进入。
///
/// 编排放在 tokio 侧（`CoreHandle::exec`）：等回执与超时计时都是
/// tokio 原生能力；GPUI 侧只负责在收敛完成后发 `ShutdownPrepared`。
/// 服务器未启动（或装配未完成）时回执立即就绪，阶段 1 只剩落盘。
pub fn begin_phase1(
    handle: CoreHandle,
    events: mpsc::UnboundedSender<CoreEvent>,
    cx: &mut App,
) -> bool {
    if QUITTING.swap(true, Ordering::SeqCst) {
        return false;
    }
    tracing::info!("退出阶段 1 开始：等待内嵌服务器收敛");
    cx.refresh_windows();

    let drain_rx = handle.server().map(|s| s.request_stop());
    let store = handle.store().cloned();
    let wait_done = handle.exec(async move {
        match drain_rx {
            None => {}
            Some(rx) => {
                let timer = tokio::time::sleep(QUIT_PHASE1_TIMEOUT);
                tokio::select! {
                    _ = rx => tracing::info!("服务器收敛完成"),
                    _ = timer => tracing::warn!(
                        timeout_secs = QUIT_PHASE1_TIMEOUT.as_secs(),
                        "退出阶段 1 等待服务器收敛超时，继续退出（可能有在途请求未收敛）"
                    ),
                }
            }
        }
    });

    cx.spawn(async move |cx| {
        let _ = wait_done.await;
        // 尾部落盘：统计防抖未刷部分 + WAL 收敛
        handle.flush_stats();
        if let Some(store) = &store {
            SqliteStore::wal_checkpoint(store);
        }
        tracing::info!("退出阶段 1 完成，进入阶段 2");
        let _ = events.unbounded_send(CoreEvent::ShutdownPrepared);
        cx.refresh();
    })
    .detach();
    true
}
