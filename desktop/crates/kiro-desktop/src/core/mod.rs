//! 核心句柄：GPUI 侧对服务端核心的访问入口（设计文档 §4.2 事件桥的延伸）
//!
//! `CoreHandle` 打包 `AdminService` 与 tokio handle：
//! - 同步纯读（`query_credentials` / `get_credential_facets`）在 GPUI
//!   线程直调，锁短无网络。
//! - 其余操作（含 `set_disabled` / `set_priority`：内部
//!   `spawn_refresh_models_arc` 需要 runtime 上下文）一律经 [`exec`]
//!   派到 tokio 侧，oneshot 回传结果。
//!
//! 白名单之外在 GPUI 线程直调含 `tokio::spawn` 的方法会 panic，
//! 这是该分界的直接依据（`src/admin/service.rs:292,310`）。
//!
//! change 5 起额外携带 [`ServerControl`]（内嵌服务器状态机）与
//! [`SqliteStore`] 句柄（退出前 WAL checkpoint、JSON 导入导出）。

use std::future::Future;
use std::sync::Arc;

use kiro_rs::admin::AdminService;
use tokio::sync::oneshot;

pub mod server;
#[cfg(test)]
mod server_tests;

use self::server::ServerControl;
use crate::store::SqliteStore;

/// 装配成功后 GPUI 侧持有的核心句柄
#[derive(Clone)]
pub struct CoreHandle {
    pub service: Arc<AdminService>,
    rt: tokio::runtime::Handle,
    server: Option<ServerControl>,
    store: Option<Arc<SqliteStore>>,
}

impl CoreHandle {
    pub fn new(service: Arc<AdminService>, rt: tokio::runtime::Handle) -> Self {
        Self {
            service,
            rt,
            server: None,
            store: None,
        }
    }

    /// 挂入服务器控制器（装配完成后调用）
    pub fn with_server(mut self, server: ServerControl) -> Self {
        self.server = Some(server);
        self
    }

    /// 挂入存储句柄（SQLite 模式；`--config`/`--credentials` 调试模式为 `None`）
    pub fn with_store(mut self, store: Arc<SqliteStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// 服务器控制器（装配完成前为 `None`）
    pub fn server(&self) -> Option<&ServerControl> {
        self.server.as_ref()
    }

    /// 存储句柄（调试模式为 `None`）
    pub fn store(&self) -> Option<&Arc<SqliteStore>> {
        self.store.as_ref()
    }

    /// 退出前统计落盘（两阶段退出阶段 1；防抖未刷的尾部强制刷盘）
    pub fn flush_stats(&self) {
        self.service.flush_stats();
    }

    /// 把 future 派到 tokio 侧执行，oneshot 回传结果。
    ///
    /// receiver 被 drop 时 future 结果随之丢弃（调用方取消语义）。
    pub fn exec<R: Send + 'static>(
        &self,
        fut: impl Future<Output = R> + Send + 'static,
    ) -> oneshot::Receiver<R> {
        let (tx, rx) = oneshot::channel();
        self.rt.spawn(async move {
            let out = fut.await;
            let _ = tx.send(out);
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> Arc<AdminService> {
        Arc::new(AdminService::new_with_runtime(
            Arc::new(
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
        ))
    }

    #[tokio::test]
    async fn exec_returns_result() {
        let handle = CoreHandle::new(test_service(), tokio::runtime::Handle::current());
        let rx = handle.exec(async { 21 + 21 });
        assert_eq!(rx.await.unwrap(), 42);
    }

    #[tokio::test]
    async fn exec_receiver_drop_is_cancel_semantics() {
        let handle = CoreHandle::new(test_service(), tokio::runtime::Handle::current());
        let rx = handle.exec(async { "dropped" });
        drop(rx);
        // 无 panic 即通过：发送端在接收端缺席时静默丢弃
    }
}
