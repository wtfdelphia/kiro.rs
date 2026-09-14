//! 内嵌服务器控制（设计文档 §5.1 状态机）
//!
//! `ServerControl` 在 GPUI 侧只持状态与发信号，所有 bind / serve 工作在
//! tokio 侧。状态变化经 [`CoreEvent::ServerStatus`] 推给视图；停止后的
//! 收敛完成经 [`CoreEvent::ServerDrained`] 与 `request_stop` 的回执
//! 双通道通知（回执供两阶段退出等待，事件供视图与未来托盘消费）。
//!
//! 优雅关闭语义平移自 `src/main.rs`：graceful_shutdown future 收到停止
//! 信号后广播 `ws_shutdown`（活跃 WS 以 1001 关闭），随后 `axum::serve`
//! 等待在途请求收敛，10 秒 `drain_backstop` 兜底防止挂死。

use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc;
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot};

use crate::bridge::CoreEvent;

/// 优雅关闭 drain 的兜底时限（与 `src/main.rs` 的
/// `SHUTDOWN_DRAIN_TIMEOUT` 同值：信号触发后在途请求未收敛的最长等待）
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// 服务器状态（设计文档 §5.1）
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    /// 未启动
    Stopped,
    /// 正在绑定端口
    Starting,
    /// 运行中，携带实际监听地址（端口 0 时为分配后的真实端口）
    Running(String),
    /// 停止信号已发出，等待在途请求收敛
    Stopping,
    /// 启动失败（含端口冲突），携带错误信息，可重试
    Failed(String),
}

impl ServerStatus {
    /// 视图展示文案
    pub fn label(&self) -> &'static str {
        match self {
            ServerStatus::Stopped => "未启动",
            ServerStatus::Starting => "启动中",
            ServerStatus::Running(_) => "运行中",
            ServerStatus::Stopping => "停止中",
            ServerStatus::Failed(_) => "启动失败",
        }
    }
}

struct Inner {
    status: ServerStatus,
    /// 路由器（装配时挂入，每次启动克隆一份给 serve）
    router: Option<axum::Router>,
    /// WS 关闭广播（关闭时活跃会话以 1001 收敛）
    ws_shutdown: Option<broadcast::Sender<()>>,
    /// 最近一次启动地址（restart 未指定新地址时复用）
    last_addr: Option<String>,
    /// 停止信号发送端（运行期存在；发出即取走）
    stop_tx: Option<oneshot::Sender<()>>,
    /// 停止触发器：`stop_tx` 尚未注册的窗口（启动早期）收到的停止/重启
    /// 请求，由 serve 任务绑端口后自查兑现
    pending_stop: bool,
    /// 停止请求（`request_stop` 置位）。优先于 `restart_pending`：
    /// 服务循环在收敛决策点见到即退出，不再重启
    stop_requested: bool,
    /// drain 完成后自动重启（restart 语义，收敛在 tokio 侧）
    restart_pending: bool,
    /// 服务循环任务存活标志。重启间隙状态会短暂回到 `Stopped`，
    /// 仅凭状态无法区分「真停止」与「重启中」，`request_stop` 靠它
    /// 判断回执能否立即兑现（见 [`ServerControl::request_stop`]）。
    loop_active: bool,
}

/// 服务器控制器（Clone 廉价：共享内部状态）
#[derive(Clone)]
pub struct ServerControl {
    rt: tokio::runtime::Handle,
    events: mpsc::UnboundedSender<CoreEvent>,
    inner: Arc<Mutex<Inner>>,
    /// 服务循环真正退出时广播一次；`request_stop` 先订阅再发信号，无竞态。
    /// 重启间隙不发：回执不会在服务器正按新地址重启时被提前兑现
    drained: broadcast::Sender<()>,
}

impl ServerControl {
    pub fn new(rt: tokio::runtime::Handle, events: mpsc::UnboundedSender<CoreEvent>) -> Self {
        let (drained, _) = broadcast::channel(4);
        Self {
            rt,
            events,
            inner: Arc::new(Mutex::new(Inner {
                status: ServerStatus::Stopped,
                router: None,
                ws_shutdown: None,
                last_addr: None,
                stop_tx: None,
                pending_stop: false,
                stop_requested: false,
                restart_pending: false,
                loop_active: false,
            })),
            drained,
        }
    }

    /// 装配期挂入路由与 WS 关闭句柄（启动前调用一次）
    pub fn attach(&self, router: axum::Router, ws_shutdown: broadcast::Sender<()>) {
        let mut inner = self.inner.lock();
        inner.router = Some(router);
        inner.ws_shutdown = Some(ws_shutdown);
    }

    /// 当前状态
    pub fn status(&self) -> ServerStatus {
        self.inner.lock().status.clone()
    }

    /// 启动。仅 `Stopped` / `Failed` 有效；其余状态忽略并返回 `false`
    pub fn start(&self, addr: &str) -> bool {
        let mut inner = self.inner.lock();
        if !matches!(
            inner.status,
            ServerStatus::Stopped | ServerStatus::Failed(_)
        ) {
            return false;
        }
        // 重启间隙状态会短暂回到 `Stopped`，此时循环仍存活，禁止二次启动
        if inner.loop_active {
            return false;
        }
        if inner.router.is_none() {
            inner.status = ServerStatus::Failed("服务器装配未完成".to_string());
            self.emit_status(&inner.status);
            return false;
        }
        inner.status = ServerStatus::Starting;
        inner.last_addr = Some(addr.to_string());
        inner.loop_active = true;
        self.emit_status(&inner.status);
        drop(inner);
        self.spawn_service_loop(addr.to_string());
        true
    }

    /// 停止。立即返回；回执在 drain 完成（或本就未运行）后就绪
    ///
    /// 先订阅收敛广播再判状态/发信号：避免「读到 Running 后 serve 恰好
    /// 完成、广播已发出」导致订阅者永远等不到。
    ///
    /// 回执只在服务循环真正退出后才兑现：重启间隙状态短暂为 `Stopped`
    /// 但循环仍存活（`loop_active`），此时按在途路径处理（置
    /// `pending_stop`、等收敛广播），杜绝「退出协调在服务器正按新地址
    /// 重启时放行 `cx.quit()`」。
    pub fn request_stop(&self) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let mut sub = self.drained.subscribe();
        {
            let mut inner = self.inner.lock();
            match &inner.status {
                ServerStatus::Stopped | ServerStatus::Failed(_) if !inner.loop_active => {
                    let _ = tx.send(());
                    return rx;
                }
                _ => {
                    inner.stop_requested = true;
                    inner.pending_stop = true;
                    if let Some(stop_tx) = inner.stop_tx.take() {
                        let _ = stop_tx.send(());
                    }
                }
            }
        }
        self.rt.spawn(async move {
            let _ = sub.recv().await;
            let _ = tx.send(());
        });
        rx
    }

    /// 重启：运行中则停止后自动按新地址重启；未运行则直接启动
    ///
    /// `addr` 为 `None` 时复用最近一次启动地址。
    /// 停止与重启在 drain 窗口内交织时停止优先（`pending_stop` 否决重启）。
    pub fn restart(&self, addr: Option<&str>) -> bool {
        let mut inner = self.inner.lock();
        let addr = addr.map(str::to_string).or_else(|| inner.last_addr.clone());
        let Some(addr) = addr else {
            return false;
        };
        match &inner.status {
            ServerStatus::Stopped | ServerStatus::Failed(_) if !inner.loop_active => {
                drop(inner);
                self.start(&addr)
            }
            _ => {
                inner.restart_pending = true;
                inner.last_addr = Some(addr);
                let stop_tx = inner.stop_tx.take();
                if stop_tx.is_none() {
                    // Starting 窗口（停止通道未注册）：改经 `pending_stop`，
                    // serve_once 绑端口完成后自查兑现
                    inner.pending_stop = true;
                }
                drop(inner);
                if let Some(tx) = stop_tx {
                    let _ = tx.send(());
                }
                true
            }
        }
    }

    fn emit_status(&self, status: &ServerStatus) {
        let _ = self.events.unbounded_send(CoreEvent::ServerStatus(status.clone()));
    }

    /// 服务循环：反复 `serve_once`，直到其与退出复查都返回 `None`。
    ///
    /// 收敛广播只在循环真正退出时发一次：重启间隙不发，回执因此不会
    /// 在服务器正按新地址重启时被提前兑现。
    fn spawn_service_loop(&self, addr: String) {
        let inner_arc = self.inner.clone();
        let events = self.events.clone();
        let drained = self.drained.clone();
        self.rt.spawn(async move {
            let mut addr = addr;
            loop {
                match serve_once(addr, inner_arc.clone(), events.clone()).await {
                    Some(next) => {
                        addr = next;
                    }
                    None => {
                        // 重启请求可能晚于最后一次收敛决策到达：在同一把锁内
                        // 复查，有则继续循环，无则原子地结束生命周期。
                        // 退出方（`start` / `restart` / `request_stop`）以
                        // `loop_active` 分界，本锁之后不再走在途路径
                        let next = {
                            let mut inner = inner_arc.lock();
                            if inner.restart_pending && !inner.stop_requested {
                                inner.restart_pending = false;
                                // 旧周期残留的停止触发器对新一轮无意义：
                                // 间隙内的 `restart` 语义是「按新地址启动」，
                                // 不是「起来再立刻停」
                                inner.pending_stop = false;
                                inner.status = ServerStatus::Starting;
                                let next = inner.last_addr.clone();
                                drop(inner);
                                let _ = events.unbounded_send(CoreEvent::ServerStatus(
                                    ServerStatus::Starting,
                                ));
                                next
                            } else {
                                inner.loop_active = false;
                                inner.stop_requested = false;
                                inner.pending_stop = false;
                                inner.restart_pending = false;
                                None
                            }
                        };
                        match next {
                            Some(next) => addr = next,
                            None => break,
                        }
                    }
                }
            }
            let _ = events.unbounded_send(CoreEvent::ServerDrained);
            let _ = drained.send(());
        });
    }
}

/// 单次服务：bind → serve → 优雅关闭 → drain 兜底。
///
/// 返回 `Some(addr)` 表示需要按该地址重启；`None` 表示服务循环结束。
async fn serve_once(
    addr: String,
    inner: Arc<Mutex<Inner>>,
    events: mpsc::UnboundedSender<CoreEvent>,
) -> Option<String> {
    let emit = |status: &ServerStatus| {
        let _ = events.unbounded_send(CoreEvent::ServerStatus(status.clone()));
    };

    // 停止请求先于本轮 bind（上一轮收敛窗口内到达）：落停止态并结束循环
    {
        let mut inner = inner.lock();
        if inner.stop_requested {
            inner.status = ServerStatus::Stopped;
            emit(&inner.status);
            return None;
        }
    }

    let (router, ws_shutdown) = {
        let inner = inner.lock();
        (inner.router.clone(), inner.ws_shutdown.clone())
    };
    let Some(router) = router else {
        let mut inner = inner.lock();
        inner.status = ServerStatus::Failed("服务器装配未完成".to_string());
        inner.restart_pending = false;
        emit(&inner.status);
        return None;
    };

    // 停止通道先建好再 bind：restart/stop 在 Starting 阶段也能送达
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    inner.lock().stop_tx = Some(stop_tx);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            let msg = format!("绑定 {} 失败: {}", addr, e);
            let mut inner = inner.lock();
            inner.stop_tx = None;
            inner.restart_pending = false;
            inner.status = ServerStatus::Failed(msg);
            emit(&inner.status);
            return None;
        }
    };
    let local_addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| addr.clone());

    // Starting 阶段收到的停止请求在此兑现
    let immediate_stop = {
        let mut inner = inner.lock();
        let pending = inner.pending_stop;
        inner.pending_stop = false;
        inner.status = ServerStatus::Running(local_addr.clone());
        emit(&inner.status);
        pending
    };

    let (drain_signal_tx, drain_signal_rx) = oneshot::channel::<()>();
    let inner_for_close = inner.clone();
    let events_for_close = events.clone();
    let server = axum::serve(listener, router).with_graceful_shutdown(async move {
        if !immediate_stop {
            let _ = stop_rx.await;
        }
        {
            let mut inner = inner_for_close.lock();
            inner.status = ServerStatus::Stopping;
            let _ = events_for_close.unbounded_send(CoreEvent::ServerStatus(inner.status.clone()));
        }
        if let Some(tx) = &ws_shutdown {
            let _ = tx.send(());
        }
        let _ = drain_signal_tx.send(());
    });

    tokio::select! {
        res = server => {
            if let Err(e) = res {
                tracing::warn!("内嵌服务器异常退出: {}", e);
            }
        }
        _ = drain_backstop(drain_signal_rx) => {
            tracing::warn!(
                timeout_secs = SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                "优雅关闭 drain 超过兜底时限，强制结束（部分在途请求未收敛）"
            );
        }
    }

    let next = {
        let mut inner = inner.lock();
        inner.stop_tx = None;
        // 停止优先于重启：收敛窗口内交织时按停止收敛
        let restart = inner.restart_pending && !inner.stop_requested;
        inner.restart_pending = false;
        if restart {
            // 锁内直接切 Starting：不经过 Stopped 中间态，重启间隙
            // 也不发收敛广播（循环真正退出时才发）
            inner.status = ServerStatus::Starting;
            emit(&inner.status);
            let next = inner.last_addr.clone();
            drop(inner);
            tracing::info!("服务器重启中（目标地址 {:?}）", next);
            next
        } else {
            inner.status = ServerStatus::Stopped;
            emit(&inner.status);
            None
        }
    };
    next
}

/// drain 兜底：停止信号触发后才开始计时；信号前永不返回
async fn drain_backstop(drain_signal_rx: oneshot::Receiver<()>) {
    if drain_signal_rx.await.is_err() {
        std::future::pending::<()>().await;
    }
    tokio::time::sleep(SHUTDOWN_DRAIN_TIMEOUT).await;
}
