//! kiro-rs 桌面端入口（应用骨架，设计文档 §4.1）
//!
//! 双运行时：tokio 后台运行时先建好，GPUI `run()` 占用主线程。
//! 本骨架完成：单实例锁、数据目录、日志、事件桥、核心装配、窗口 + 四视图。
//! 服务器启停、托盘常驻、两阶段退出由后续 change 实现。

mod bridge;
mod lock;
mod logging;
mod paths;
mod store;
mod ui;

use std::sync::Arc;

use clap::Parser;
use futures::StreamExt;
use gpui_kit::component::Root;
use gpui_kit::*;
use kiro_rs::storage::{ConfigStore, CredentialStore};

use crate::bridge::{CoreEvent, event_channel};
use crate::lock::InstanceLock;
use crate::ui::AppView;

/// kiro-rs 桌面端
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// 配置文件路径（指定时等价 CLI 行为，便于调试）
    #[arg(short, long)]
    config: Option<String>,

    /// 凭据文件路径（指定时等价 CLI 行为，便于调试）
    #[arg(long)]
    credentials: Option<String>,
}

fn main() {
    let args = Args::parse();

    // 数据目录 + 单实例锁 + 日志（都在进 GPUI 前完成）
    let data_dir = paths::data_dir();
    logging::init(&data_dir);

    let _lock = match InstanceLock::acquire(&data_dir) {
        Ok(lock) => {
            tracing::info!("已获取单实例锁: {}", lock.path.display());
            lock
        }
        Err(None) => {
            eprintln!("kiro-rs 已在运行（单实例锁被占用），退出");
            std::process::exit(1);
        }
        Err(Some(e)) => {
            eprintln!("获取单实例锁失败: {}", e);
            std::process::exit(1);
        }
    };

    // tokio 后台运行时（核心装配与服务器都在这一侧）
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("kiro-tokio")
        .build()
        .expect("tokio runtime 创建失败");
    // change 5 起，服务器启停与核心调用经此 handle 进 tokio 侧
    let _rt_handle = rt.handle().clone();

    let (event_tx, mut event_rx) = event_channel();

    // 存储后端选择：显式 --config / --credentials 走 JSON 文件（等价 CLI，
    // 调试用）；否则 SQLite + 系统钥匙串（数据目录，设计文档 §7）
    let json_mode = args.config.is_some() || args.credentials.is_some();

    let sqlite_store = if json_mode {
        None
    } else {
        match store::SqliteStore::open(&data_dir) {
            Ok(s) => {
                tracing::info!(
                    "SQLite 存储就绪（{}），secret 后端: {}",
                    s.data_dir().join("kiro.db").display(),
                    if s.secret_backend_is_system() {
                        "系统钥匙串"
                    } else {
                        "加密文件回退"
                    }
                );
                // 首启（库为空）才导入 JSON，避免覆盖已有数据；成功后备份 .bak
                let empty = s.credential_count().unwrap_or(0) == 0 && !s.has_config().unwrap_or(false);
                if empty {
                    store::json_io::detect_and_import(&data_dir, &s);
                }
                Some(s)
            }
            Err(e) => {
                eprintln!("SQLite 存储初始化失败: {:#}", e);
                std::process::exit(1);
            }
        }
    };

    let (credential_store, config_store): (Arc<dyn CredentialStore>, Arc<dyn ConfigStore>) =
        match &sqlite_store {
            Some(s) => (s.clone(), s.clone()),
            None => {
                let config_path = args.config.clone().unwrap_or_else(|| {
                    kiro_rs::model::config::Config::default_config_path().to_string()
                });
                let credentials_path = args.credentials.clone().unwrap_or_else(|| {
                    kiro_rs::kiro::model::credentials::KiroCredentials::default_credentials_path()
                        .to_string()
                });
                tracing::info!("调试模式：存储走 JSON 文件（--config / --credentials）");
                (
                    Arc::new(kiro_rs::storage::JsonCredentialStore::new(credentials_path)),
                    Arc::new(kiro_rs::storage::JsonConfigStore::new(config_path)),
                )
            }
        };

    let sqlite_store_for_boot = sqlite_store.clone();

    rt.spawn(async move {
        let opts = kiro_rs::bootstrap::BootOptions {
            credential_store: credential_store.clone(),
            config_store: config_store.clone(),
        };
        let event = match kiro_rs::bootstrap::bootstrap(&opts) {
            Ok(b) => {
                // SQLite 模式：回填模型目录 + 后台周期快照（设计文档 D7）
                if let Some(store) = &sqlite_store_for_boot {
                    store::restore_model_catalog(store, &b.token_manager);
                    let store = Arc::clone(store);
                    let token_manager = Arc::clone(&b.token_manager);
                    tokio::spawn(async move {
                        // 首拍延迟 20 秒，等预热刷新的第一批结果落地
                        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                        loop {
                            store::snapshot_model_catalog(&store, &token_manager);
                            tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                        }
                    });
                }
                CoreEvent::Bootstrapped {
                    credential_count: b.token_manager.total_count(),
                }
            }
            Err(e) => CoreEvent::BootstrapFailed(format!("{:#}", e)),
        };
        let _ = event_tx.unbounded_send(event);
    });

    // GPUI 占用主线程。用 AllAssets（完整 Lucide 目录）：默认的
    // `Assets` 只嵌约 100 个组件默认图标，侧边栏用到的
    // box / key-round / server 不在其中。
    let app = gpui_kit::application().with_assets(assets::AllAssets);
    app.run(move |cx| {
        // 必须第一行调用：初始化 gpui-kit 主题与全局设施
        gpui_kit::init(cx);
        cx.set_app_identity("dev.kiro-rs.desktop", "kiro-rs");

        cx.spawn(async move |cx| {
            let view = cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| AppView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            });
            match view {
                Ok(handle) => {
                    tracing::info!("主窗口已创建 (window_id={})", handle.window_id().as_u64());
                }
                Err(e) => {
                    tracing::error!("窗口创建失败: {}", e);
                }
            }

            // 消费事件循环（骨架阶段仅记录装配结果）
            while let Some(event) = event_rx.next().await {
                match &event {
                    CoreEvent::Bootstrapped { credential_count } => {
                        tracing::info!("核心装配完成，凭据数: {}", credential_count);
                    }
                    CoreEvent::BootstrapFailed(msg) => {
                        tracing::error!("核心装配失败: {}", msg);
                        break;
                    }
                }
            }
        })
        .detach();
    });
    // run() 返回即应用已退出；rt 在此 drop（两阶段退出见 change 5）
}
