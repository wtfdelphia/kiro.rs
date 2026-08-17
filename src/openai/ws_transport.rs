//! WS 传输模式路由与传输抽象缝
//!
//! P0 只有 `HttpBridgeTransport`（客户端保持 WS，每个 turn 翻译为一次上游
//! HTTP/SSE 调用）。`PassthroughTransport`（WS→WS 帧中继）为预留分支：
//! 选中时在 upgrade 之前返回 501，真正实现另立 change
//! （`docs/websocket-support-optimization-design.md` §4.6）。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::extract::ws::WebSocket;
use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::anthropic::AppState;
use crate::model::config::WsSettings;

pub use crate::model::config::WsTransportMode;

/// 全局 WS 准入计数器
///
/// 用 `AtomicUsize` CAS 而非 Semaphore：Semaphore 容量运行时不可变，
/// 挡不住 `max_connections` 热改（design §2.2）。
/// 热改只影响后续准入判定，存量连接不受影响（§4.7 语义矩阵）。
#[derive(Debug, Default)]
pub struct WsAdmission {
    active: AtomicUsize,
}

impl WsAdmission {
    pub fn new() -> Self {
        Self::default()
    }

    /// 尝试占用一个名额；`active < max` 时 CAS 成功
    pub fn try_acquire(self: &Arc<Self>, max: usize) -> Option<WsAdmissionGuard> {
        let ok = self
            .active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |cur| {
                (cur < max).then_some(cur + 1)
            })
            .is_ok();
        ok.then(|| WsAdmissionGuard {
            admission: Arc::clone(self),
        })
    }

    /// 当前活跃连接数（Admin GET 展示用）
    pub fn active(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }

    fn release(&self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

/// 准入名额 RAII 守卫：会话无论以何种方式结束都归还计数（task 6.7）
#[derive(Debug)]
pub struct WsAdmissionGuard {
    admission: Arc<WsAdmission>,
}

impl Drop for WsAdmissionGuard {
    fn drop(&mut self) {
        self.admission.release();
    }
}

/// 握手期拒绝原因（全部在 upgrade 之前返回 HTTP 错误）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsHandshakeReject {
    /// `mode=passthrough`：预留未实现，501
    PassthroughNotImplemented,
}

/// 从最新设置快照解析传输模式
///
/// 未知 mode 值在反序列化阶段已回落 `http_bridge` 并告警（`WsTransportMode`
/// 的手动 Deserialize），此处是模式路由的唯一缝隙：未来新增模式在这里分支。
pub fn resolve_mode(settings: &WsSettings) -> WsTransportMode {
    settings.mode
}

/// 按模式解析传输实现；passthrough 在握手前显式拒绝（design §4.6）
pub fn resolve_transport(settings: &WsSettings) -> Result<Arc<dyn WsTransport>, WsHandshakeReject> {
    match resolve_mode(settings) {
        WsTransportMode::HttpBridge => Ok(Arc::new(HttpBridgeTransport)),
        WsTransportMode::Passthrough => Err(WsHandshakeReject::PassthroughNotImplemented),
    }
}

/// 握手时冻结的会话上下文
///
/// 模式随连接冻结；超时 / 帧上限等字段在每个等待边界重新读快照（§4.7）。
pub struct WsSessionContext {
    /// 建连时解析并冻结的传输模式
    pub mode: WsTransportMode,
    /// 复用既有鉴权 / provider / 转换核
    pub app_state: AppState,
    /// 热加载设置句柄（会话循环每个边界读最新快照）
    pub ws_settings: Arc<RwLock<WsSettings>>,
    /// 优雅 shutdown 信号（1001 关闭活跃连接）
    pub shutdown_rx: broadcast::Receiver<()>,
}

impl WsSessionContext {
    /// 读取最新设置快照
    pub fn settings_snapshot(&self) -> WsSettings {
        self.ws_settings.read().clone()
    }
}

/// 传输抽象缝：一个 WS 连接交给一个 transport 驱动到关闭
///
/// 对象安全（boxed future），便于未来按模式路由到不同实现。
pub trait WsTransport: Send + Sync {
    fn run_session(
        &self,
        socket: WebSocket,
        ctx: WsSessionContext,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// http_bridge 传输：会话循环本体在 `ws_ingress::run_http_bridge_session`
pub struct HttpBridgeTransport;

impl WsTransport for HttpBridgeTransport {
    fn run_session(
        &self,
        socket: WebSocket,
        ctx: WsSessionContext,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(super::ws_ingress::run_http_bridge_session(socket, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::WsTransportMode as Mode;

    fn settings(mode: Mode) -> WsSettings {
        WsSettings {
            mode,
            ..Default::default()
        }
    }

    /// 任务 4.1：http_bridge 解析为桥接传输，passthrough 握手前拒绝
    #[test]
    fn resolve_transport_routes_by_mode() {
        assert!(resolve_transport(&settings(Mode::HttpBridge)).is_ok());
        assert!(
            matches!(
                resolve_transport(&settings(Mode::Passthrough)),
                Err(WsHandshakeReject::PassthroughNotImplemented)
            ),
            "passthrough 必须在握手前被拒绝"
        );
    }

    #[test]
    fn resolve_mode_reads_snapshot() {
        assert_eq!(resolve_mode(&settings(Mode::Passthrough)), Mode::Passthrough);
        assert_eq!(resolve_mode(&settings(Mode::HttpBridge)), Mode::HttpBridge);
    }

    /// 任务 7.2：CAS 准入，满员拒绝，归还后可再入
    #[test]
    fn admission_cas_semantics() {
        let adm = Arc::new(WsAdmission::new());
        let g1 = adm.try_acquire(2);
        let g2 = adm.try_acquire(2);
        assert!(g1.is_some() && g2.is_some());
        assert!(adm.try_acquire(2).is_none(), "满员必须拒绝");
        assert_eq!(adm.active(), 2);
        drop(g1);
        assert_eq!(adm.active(), 1, "Drop 必须归还计数");
        assert!(adm.try_acquire(2).is_some(), "归还后应可再入");
    }

    /// 任务 7.2：max_connections 热缩减只拦新连接，不影响存量
    #[test]
    fn admission_hot_shrink_only_affects_new_connections() {
        let adm = Arc::new(WsAdmission::new());
        let existing = (0..4)
            .map(|_| adm.try_acquire(8).expect("准入失败"))
            .collect::<Vec<_>>();
        assert_eq!(adm.active(), 4);

        // 热缩减到 2：存量 4 个连接不受影响，新连接按新上限被拒
        let new_limit = 2;
        assert!(adm.try_acquire(new_limit).is_none());
        assert_eq!(adm.active(), 4, "存量连接不得被热缩减断开");

        // 自然回落到新上限以下后恢复准入
        drop(existing);
        assert!(adm.try_acquire(new_limit).is_some());
    }
}
