//! CLI 二进制：解析参数 → `bootstrap` → `build_routes` → `axum::serve`。
//! 装配链与存储接缝在 lib（`kiro_rs::bootstrap` / `kiro_rs::storage`），
//! 信号处理与优雅关闭是 CLI 运行时语义，留在本文件。

use std::sync::Arc;

use clap::Parser;

use kiro_rs::bootstrap::{self, BootOptions};
use kiro_rs::kiro::model::credentials::KiroCredentials;
use kiro_rs::model::arg::Args;
use kiro_rs::model::config::Config;
use kiro_rs::public_api;
use kiro_rs::storage::{JsonConfigStore, JsonCredentialStore};

#[tokio::main]
async fn main() {
    // 解析命令行参数
    let args = Args::parse();

    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    tracing::info!("kiro-rs v{}", env!("CARGO_PKG_VERSION"));

    let config_path = args
        .config
        .unwrap_or_else(|| Config::default_config_path().to_string());
    let credentials_path = args
        .credentials
        .unwrap_or_else(|| KiroCredentials::default_credentials_path().to_string());

    // 装配：核心（配置/凭据/token manager/provider）→ 全量路由
    let opts = BootOptions {
        credential_store: Arc::new(JsonCredentialStore::new(&credentials_path)),
        config_store: Arc::new(JsonConfigStore::new(&config_path)),
    };
    let b = match bootstrap::bootstrap(&opts) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("启动失败: {}", e);
            std::process::exit(1);
        }
    };
    let (app, app_state) = bootstrap::build_routes(&b);

    let admin_key_valid = b
        .config
        .admin_api_key
        .as_ref()
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);

    // 启动服务器
    let addr = format!("{}:{}", b.config.host, b.config.port);
    tracing::info!("启动 Anthropic API 端点: {}", addr);
    let api_key = b.config.api_key.clone().unwrap_or_default();
    if api_key.is_empty() {
        tracing::info!("API Key: <empty>");
    } else {
        tracing::info!("API Key: {}***", &api_key[..(api_key.len() / 2).max(1)]);
    }
    tracing::info!("requireApiKey: {}", b.config.require_api_key);
    // 对外 API 清单来自 public_api catalog（单一事实源），勿在此手写第二份
    tracing::info!("可用 API:");
    for endpoint in public_api::live_endpoints() {
        tracing::info!("  {:<4} {}", endpoint.method, endpoint.path);
    }
    if admin_key_valid {
        // Admin API 不属于 Public Client API，catalog 不覆盖，此处保持手写
        tracing::info!("Admin API:");
        tracing::info!("  GET  /api/admin/credentials");
        tracing::info!("  POST /api/admin/credentials/:index/disabled");
        tracing::info!("  POST /api/admin/credentials/:index/priority");
        tracing::info!("  POST /api/admin/credentials/:index/reset");
        tracing::info!("  GET  /api/admin/credentials/:index/balance");
        tracing::info!("  POST /api/admin/credentials/models/refresh");
        tracing::info!("  POST /api/admin/credentials/:id/models/refresh");
        tracing::info!("  GET  /api/admin/credentials/:id/models");
        tracing::info!("  POST /api/admin/credentials/:id/test");
        tracing::info!("  GET  /api/admin/models/catalog");
        tracing::info!("  GET/PUT /api/admin/settings/proxy");
        tracing::info!("  GET/PUT /api/admin/settings/endpoint");
        tracing::info!("  GET/PUT /api/admin/settings/auth");
        tracing::info!("  GET/PUT /api/admin/settings/websocket");
        tracing::info!("Admin UI:");
        tracing::info!("  GET  /admin");
    }

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    // 优雅关闭：收到信号后广播 ws_shutdown，活跃 WS 会话以 1001 关闭，
    // 随后 hyper 等待在途请求收敛（WS 连接关闭即收敛）。
    // drain 有兜底时限：对端停止读取导致 Close 写不出去、或在途长请求不收敛时，
    // 超限即放弃等待强制结束，防止进程在关闭阶段无限挂起。
    let ws_shutdown_tx = app_state.ws_shutdown.clone();
    let (drain_signal_tx, drain_signal_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        shutdown_signal().await;
        tracing::info!("收到 shutdown 信号，开始优雅关闭（活跃 WS 将以 1001 关闭）");
        let _ = ws_shutdown_tx.send(());
        let _ = drain_signal_tx.send(());
    });

    tokio::select! {
        res = server => res.unwrap(),
        _ = drain_backstop(drain_signal_rx) => {
            tracing::warn!(
                timeout_secs = SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                "优雅关闭 drain 超过兜底时限，强制结束（部分在途请求未收敛）"
            );
        }
    }
}

/// 优雅关闭 drain 的兜底时限：信号触发后在途请求仍未收敛的最长等待
const SHUTDOWN_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// drain 兜底：shutdown 信号触发后才开始计时，超限返回；
/// 信号未触发（serve 提前结束）则永不返回，不影响正常退出路径。
async fn drain_backstop(drain_signal_rx: tokio::sync::oneshot::Receiver<()>) {
    if drain_signal_rx.await.is_err() {
        std::future::pending::<()>().await;
    }
    tokio::time::sleep(SHUTDOWN_DRAIN_TIMEOUT).await;
}

/// 监听进程退出信号（Ctrl-C / SIGTERM）
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut s) => {
                let _ = s.recv().await;
            }
            Err(e) => tracing::warn!("注册 SIGTERM 处理失败: {}", e),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 兜底计时只在 shutdown 信号触发后启动；信号前永不返回
    #[tokio::test(start_paused = true)]
    async fn drain_backstop_only_arms_after_signal() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let backstop = tokio::spawn(drain_backstop(rx));

        // 信号未触发：即使越过兜底时限也不返回
        tokio::time::advance(SHUTDOWN_DRAIN_TIMEOUT * 2).await;
        tokio::task::yield_now().await;
        assert!(!backstop.is_finished(), "信号触发前兜底不得启动");

        // 信号触发后：到达兜底时限即返回
        tx.send(()).expect("发送 drain 信号失败");
        tokio::time::timeout(SHUTDOWN_DRAIN_TIMEOUT + std::time::Duration::from_secs(1), backstop)
            .await
            .expect("信号触发后兜底必须在时限内返回")
            .expect("backstop 任务异常退出");
    }
}
