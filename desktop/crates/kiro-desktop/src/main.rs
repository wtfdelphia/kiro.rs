//! kiro-rs 桌面端入口（应用骨架，设计文档 §4.1）
//!
//! 双运行时：tokio 后台运行时先建好，GPUI `run()` 占用主线程。
//! 装配完成后自动启动内嵌服务器（端口冲突进 Failed 态，界面可重试）；
//! 退出走两阶段协调（`quit.rs`：先收敛服务器再 `cx.quit()`）。
//! 托盘常驻由 change 6 实现。

mod bridge;
mod core;
mod lock;
mod logging;
mod paths;
mod quit;
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

    // GPUI 侧留一份事件发送端（装配任务持有另一份）
    let events_gpui = event_tx.clone();

    rt.spawn(async move {
        // 服务器控制器：事件发送端先克隆一份（Bootstrapped 事件之后继续用）
        let server = crate::core::server::ServerControl::new(
            tokio::runtime::Handle::current(),
            event_tx.clone(),
        );
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
                // 构建全量路由：顺带拿到 AppState 的运行时句柄，挂给桌面
                // AdminService（鉴权热更新 + WebSocket 设置/准入），
                // 设置面板的写操作因此能直达运行中的服务器
                let (router, app_state) = kiro_rs::bootstrap::build_routes(&b);
                // 桌面专用 AdminService：不带 admin_api_key 门槛
                //（门槛只管 HTTP Admin API，本地界面不受限）
                let admin_service = Arc::new(
                    kiro_rs::admin::AdminService::new_with_runtime(
                        b.token_manager.clone(),
                        b.endpoint_names.clone(),
                        Some(app_state.auth.clone()),
                        Some(b.kiro_provider.clone()),
                    )
                    .with_ws_runtime(app_state.ws_settings.clone(), app_state.ws_admission.clone()),
                );
                server.attach(router, app_state.ws_shutdown.clone());
                // 默认行为：装配成功即按配置地址启动；端口冲突就是
                // bind 失败，进 Failed 态（服务视图可见原因并重试）
                let addr = format!("{}:{}", b.config.host, b.config.port);
                tracing::info!("自动启动内嵌服务器: {}", addr);
                server.start(&addr);
                let mut handle = crate::core::CoreHandle::new(
                    admin_service,
                    tokio::runtime::Handle::current(),
                )
                .with_server(server);
                if let Some(store) = sqlite_store_for_boot.clone() {
                    handle = handle.with_store(store);
                }
                CoreEvent::Bootstrapped {
                    credential_count: b.token_manager.total_count(),
                    handle,
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

        // 主视图句柄槽：键绑定与关窗拦截在窗口创建前注册，经槽位取句柄
        let view_slot: Arc<parking_lot::RwLock<Option<gpui::Entity<AppView>>>> =
            Arc::new(Default::default());
        let events_for_quit = events_gpui.clone();
        let slot_for_quit = view_slot.clone();

        // 覆写退出键：cmd-q / alt-f4 进阶段 1，不允许任何路径直达
        // cx.quit()（设计文档 §5.2）
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-q", quit::RequestQuit, None),
            gpui::KeyBinding::new("alt-f4", quit::RequestQuit, None),
        ]);
        cx.on_action(move |_: &quit::RequestQuit, cx: &mut gpui::App| {
            if quit::is_quitting() {
                return;
            }
            let Some(view) = slot_for_quit.read().clone() else {
                return;
            };
            let Some(handle) = view.read(cx).core_handle() else {
                // 引导未完成：无服务器可收敛、无统计可落盘，记录后忽略
                //（关窗拦截同窗口期放行真实关闭，窗口已关则自然退出）
                tracing::warn!("退出请求到达时核心尚未装配完成，忽略本次退出键");
                return;
            };
            quit::begin_phase1(handle, events_for_quit.clone(), cx);
        });

        cx.spawn(async move |cx| {
            let slot = view_slot.clone();
            let events_for_close = events_gpui.clone();
            let view = cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| AppView::new(window, cx));
                *slot.write() = Some(view.clone());
                // 关窗拦截：未退出时进阶段 1 并保留窗口（阶段 2 随进程
                // 退出）；已进入退出流程则放行。change 6 会把这里改成
                // 「常驻时最小化」。
                let slot2 = slot.clone();
                let events2 = events_for_close.clone();
                window.on_window_should_close(cx, move |_window, cx| {
                    if quit::is_quitting() {
                        return true;
                    }
                    let Some(v) = slot2.read().clone() else {
                        return true;
                    };
                    let Some(handle) = v.read(cx).core_handle() else {
                        return true;
                    };
                    quit::begin_phase1(handle, events2.clone(), cx);
                    false
                });
                cx.new(|cx| Root::new(view, window, cx))
            });
            let window_handle = match view {
                Ok(handle) => {
                    tracing::info!("主窗口已创建 (window_id={})", handle.window_id().as_u64());
                    handle
                }
                Err(e) => {
                    tracing::error!("窗口创建失败: {}", e);
                    return;
                }
            };

            // 消费事件循环：装配句柄、服务器状态、退出协调
            while let Some(event) = event_rx.next().await {
                match event {
                    CoreEvent::Bootstrapped {
                        credential_count,
                        handle,
                    } => {
                        tracing::info!("核心装配完成，凭据数: {}", credential_count);
                        let _ = window_handle.update(cx, move |root, _, cx| {
                            let root_view = root.view().clone();
                            if let Ok(app_view) = root_view.downcast::<AppView>() {
                                app_view.update(cx, |v, cx| v.bootstrapped(handle, cx));
                            }
                        });
                    }
                    CoreEvent::BootstrapFailed(msg) => {
                        tracing::error!("核心装配失败: {}", msg);
                        let _ = window_handle.update(cx, move |root, _, cx| {
                            let root_view = root.view().clone();
                            if let Ok(app_view) = root_view.downcast::<AppView>() {
                                app_view.update(cx, |v, cx| v.bootstrap_failed(msg.clone(), cx));
                            }
                        });
                        break;
                    }
                    CoreEvent::ServerStatus(status) => {
                        tracing::info!("服务器状态: {:?}", status);
                        let _ = window_handle.update(cx, move |root, _, cx| {
                            let root_view = root.view().clone();
                            if let Ok(app_view) = root_view.downcast::<AppView>() {
                                app_view.update(cx, |v, cx| {
                                    v.on_server_status(status.clone(), cx)
                                });
                            }
                        });
                    }
                    CoreEvent::ServerDrained => {
                        tracing::info!("服务器收敛完成（ServerDrained）");
                    }
                    CoreEvent::ShutdownPrepared => {
                        // 阶段 2：一切等待已在阶段 1 完成，直接退出
                        tracing::info!("退出阶段 2：quit");
                        let _ = cx.update(|cx| cx.quit());
                        break;
                    }
                }
            }
        })
        .detach();
    });
    // run() 返回即应用已退出（两阶段退出已在 quit 前完成）；rt 在此 drop
}
