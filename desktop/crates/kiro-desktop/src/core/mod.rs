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

use std::future::Future;
use std::sync::Arc;

use kiro_rs::admin::AdminService;
use tokio::sync::oneshot;

/// 装配成功后 GPUI 侧持有的核心句柄
#[derive(Clone)]
pub struct CoreHandle {
    pub service: Arc<AdminService>,
    rt: tokio::runtime::Handle,
}

impl CoreHandle {
    pub fn new(service: Arc<AdminService>, rt: tokio::runtime::Handle) -> Self {
        Self { service, rt }
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
