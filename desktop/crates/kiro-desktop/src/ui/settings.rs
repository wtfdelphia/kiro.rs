//! 设置视图（设计文档 §8「设置」行）
//!
//! 分区：鉴权 / 代理 / 默认端点 / 负载均衡 / WebSocket / 行为 / 数据。
//! 全部经现有 `AdminService` 设置方法（change 4 起桌面实例已挂 auth /
//! ws 运行时句柄，热更新直达运行中的服务器）。视图持快照，写操作成功
//! 后整体重拉。
//!
//! 行为分区（change 6）：关窗常驻 / 开机自启 / 自动启动服务器三个开关，
//! 直读直写本地 SQLite `preferences` 表（不走 `AdminService`）；开机自启
//! 开关联动平台入口，入口写入成功才落库。
//!
//! 数据分区：JSON 导出（保存对话框）与手动导入（选择对话框）。手动导入
//! 不改名源文件，与首启自动导入（改名 `.bak`）区分。

use gpui_kit::base::StyledExt;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    Disableable, Sizable, WindowExt, button::Button, radio::Radio, switch::Switch,
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use kiro_rs::admin::types::{
    AuthSettingsResponse, EndpointSettingsResponse, LoadBalancingModeResponse,
    ProxySettingsResponse, UpdateAuthSettingsRequest, UpdateProxySettingsRequest,
    UpdateWsSettingsRequest, WsSettingsResponse,
};

use crate::core::CoreHandle;
use crate::store::{json_io, prefs};
use crate::autostart;

/// 各分区快照（视图持快照，写后重拉）
#[derive(Default)]
struct Snapshot {
    auth: Option<AuthSettingsResponse>,
    proxy: Option<ProxySettingsResponse>,
    endpoint: Option<EndpointSettingsResponse>,
    lb: Option<LoadBalancingModeResponse>,
    ws: Option<WsSettingsResponse>,
    credential_count: i64,
    has_config: bool,
    /// 行为偏好（无 SQLite 句柄的调试模式为 `None`，分区禁用）
    behavior: Option<BehaviorSnapshot>,
}

/// 行为分区三键快照
#[derive(Default, Clone)]
struct BehaviorSnapshot {
    close_to_tray: bool,
    launch_at_login: bool,
    auto_start_server: bool,
}

/// 设置视图
pub struct SettingsView {
    handle: CoreHandle,
    snapshot: Snapshot,
    api_key: Entity<InputState>,
    proxy_url: Entity<InputState>,
    proxy_user: Entity<InputState>,
    proxy_pass: Entity<InputState>,
    ws_max_conn: Entity<InputState>,
}

impl SettingsView {
    pub fn new(handle: CoreHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let api_key = cx.new(|cx| InputState::new(window, cx).placeholder("留空保持原值"));
        let proxy_url = cx.new(|cx| InputState::new(window, cx).placeholder("http(s)/socks5 URL，留空清除"));
        let proxy_user = cx.new(|cx| InputState::new(window, cx).placeholder("用户名（可选）"));
        let proxy_pass = cx.new(|cx| InputState::new(window, cx).placeholder("密码（可选）"));
        let ws_max_conn = cx.new(|cx| InputState::new(window, cx).placeholder("如 8"));
        let mut view = Self {
            handle,
            snapshot: Snapshot::default(),
            api_key,
            proxy_url,
            proxy_user,
            proxy_pass,
            ws_max_conn,
        };
        view.reload(cx);
        view
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let s = &self.handle.service;
        self.snapshot = Snapshot {
            auth: Some(s.get_auth_settings()),
            proxy: Some(s.get_proxy_settings()),
            endpoint: Some(s.get_endpoint_settings()),
            lb: Some(s.get_load_balancing_mode()),
            ws: Some(s.get_ws_settings()),
            credential_count: self
                .handle
                .store()
                .and_then(|st| st.credential_count().ok())
                .unwrap_or(0),
            has_config: self
                .handle
                .store()
                .and_then(|st| st.has_config().ok())
                .unwrap_or(false),
            behavior: self.handle.store().map(|st| BehaviorSnapshot {
                close_to_tray: st.get_preference(prefs::CLOSE_TO_TRAY),
                launch_at_login: st.get_preference(prefs::LAUNCH_AT_LOGIN),
                auto_start_server: st.get_preference(prefs::AUTO_START_SERVER),
            }),
        };
        cx.notify();
    }

    /// 通用写操作派发：exec → 结果通知 → 成功重拉快照
    fn exec_write<F>(&mut self, action_name: &'static str, fut: F, window: &mut Window, cx: &mut Context<Self>)
    where
        F: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        let view = cx.entity().downgrade();
        let rx = self.handle.exec(fut);
        window
            .spawn(cx, async move |cx| match rx.await {
                Ok(Ok(())) => done(&view, true, format!("{}已保存", action_name), cx),
                Ok(Err(e)) => done(&view, false, format!("{}保存失败: {}", action_name, e), cx),
                Err(_) => done(&view, false, format!("{}任务被取消", action_name), cx),
            })
            .detach();
    }

    fn save_auth(&mut self, require: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
        let api_key_raw = self.api_key.read(cx).value().trim().to_string();
        let api_key = if api_key_raw.is_empty() { None } else { Some(api_key_raw) };
        let service = self.handle.service.clone();
        self.exec_write(
            "鉴权设置",
            async move {
                service
                    .update_auth_settings(UpdateAuthSettingsRequest {
                        require_api_key: require,
                        api_key,
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            window,
            cx,
        );
    }

    fn save_proxy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.proxy_url.read(cx).value().trim().to_string();
        let user = self.proxy_user.read(cx).value().trim().to_string();
        let pass = self.proxy_pass.read(cx).value().trim().to_string();
        let service = self.handle.service.clone();
        self.exec_write(
            "代理设置",
            async move {
                service
                    .update_proxy_settings(UpdateProxySettingsRequest {
                        proxy_url: Some(url),
                        proxy_username: if user.is_empty() { None } else { Some(user) },
                        proxy_password: if pass.is_empty() { None } else { Some(pass) },
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            window,
            cx,
        );
    }

    /// 行为偏好写（change 6）：直写本地 `preferences` 表，成功重拉快照。
    /// 与服务器设置不同，这三键不经 `AdminService`、无需网络。
    fn save_preference(&mut self, key: &'static str, value: bool, cx: &mut Context<Self>) {
        let Some(store) = self.handle.store().cloned() else {
            return;
        };
        if let Err(e) = store.set_preference(key, value) {
            tracing::warn!("写入偏好失败 ({}): {}", key, e);
        }
        self.reload(cx);
    }

    /// 开机自启开关联动平台入口（design D4/D5）：先写入口，成功才落库；
    /// 失败则通知且偏好不变。
    fn toggle_launch_at_login(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = autostart::set_enabled(next) {
            tracing::warn!("设置开机自启失败（入口写入失败，偏好不变）: {}", e);
            window.push_notification(
                Notification::error(format!("开机自启设置失败: {}", e)),
                cx,
            );
            return;
        }
        self.save_preference(prefs::LAUNCH_AT_LOGIN, next, cx);
    }

    fn save_endpoint(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.handle.service.clone();
        self.exec_write(
            "默认端点",
            async move {
                service
                    .update_endpoint_settings(
                        kiro_rs::admin::types::UpdateEndpointSettingsRequest {
                            default_endpoint: name,
                        },
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            window,
            cx,
        );
    }

    fn save_lb(&mut self, mode: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.handle.service.clone();
        self.exec_write(
            "负载均衡模式",
            async move {
                service
                    .set_load_balancing_mode(kiro_rs::admin::types::SetLoadBalancingModeRequest {
                        mode: mode.to_string(),
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            window,
            cx,
        );
    }

    fn save_ws(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.ws_max_conn.read(cx).value().trim().to_string();
        let max_connections: Option<usize> = if raw.is_empty() {
            None
        } else {
            match raw.parse() {
                Ok(n) if n > 0 => Some(n),
                _ => {
                    window.push_notification(Notification::error(format!("连接数非法: {}", raw)), cx);
                    return;
                }
            }
        };
        let service = self.handle.service.clone();
        self.exec_write(
            "WebSocket 设置",
            async move {
                service
                    .update_ws_settings(UpdateWsSettingsRequest {
                        enabled: None,
                        mode: None,
                        max_connections,
                        client_first_message_timeout_seconds: None,
                        inter_turn_idle_timeout_seconds: None,
                        max_message_bytes: None,
                        upstream_read_timeout_seconds: None,
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            window,
            cx,
        );
    }

    fn toggle_ws(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.handle.service.clone();
        self.exec_write(
            "WebSocket 开关",
            async move {
                service
                    .update_ws_settings(UpdateWsSettingsRequest {
                        enabled: Some(enabled),
                        mode: None,
                        max_connections: None,
                        client_first_message_timeout_seconds: None,
                        inter_turn_idle_timeout_seconds: None,
                        max_message_bytes: None,
                        upstream_read_timeout_seconds: None,
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            window,
            cx,
        );
    }

    /// JSON 导出：保存对话框 → `json_io::export_*`（不改名，不覆盖确认由系统对话框承担）
    fn export(&mut self, kind: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.handle.store().cloned() else {
            window.push_notification(
                Notification::error("调试模式（--config/--credentials）下无 SQLite 存储，不支持导出".to_string()),
                cx,
            );
            return;
        };
        let dir = store.data_dir().to_path_buf();
        let suggested = match kind {
            "credentials" => "credentials.json",
            _ => "config.json",
        };
        let rx = cx.prompt_for_new_path(&dir, Some(suggested));
        let view = cx.entity().downgrade();
        let exec = self.handle.clone();
        window
            .spawn(cx, async move |cx| {
                let path = match rx.await {
                    Ok(Ok(Some(p))) => p,
                    _ => return, // 取消
                };
                let path_display = path.display().to_string();
                let store2 = store.clone();
                let out = exec.exec(async move {
                    match kind {
                        "credentials" => json_io::export_credentials(&store2, &path),
                        _ => json_io::export_config(&store2, &path),
                    }
                    .map_err(|e| e.to_string())
                });
                let msg = match out.await {
                    Ok(Ok(())) => (true, format!("已导出到 {}", path_display)),
                    Ok(Err(e)) => (false, format!("导出失败: {}", e)),
                    Err(_) => (false, "导出任务被取消".to_string()),
                };
                done(&view, msg.0, msg.1, cx);
            })
            .detach();
    }

    /// JSON 手动导入：选择对话框 → `json_io::import_*`（不改名源文件）
    fn import(&mut self, kind: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.handle.store().cloned() else {
            window.push_notification(
                Notification::error("调试模式（--config/--credentials）下无 SQLite 存储，不支持导入".to_string()),
                cx,
            );
            return;
        };
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let view = cx.entity().downgrade();
        let exec = self.handle.clone();
        window
            .spawn(cx, async move |cx| {
                let paths = match rx.await {
                    Ok(Ok(Some(p))) if !p.is_empty() => p,
                    _ => return, // 取消
                };
                let path = paths[0].clone();
                let store2 = store.clone();
                let out = exec.exec(async move {
                    match kind {
                        "credentials" => json_io::import_credentials(&store2, &path)
                            .map(|n| format!("已导入 {} 条凭据（源文件未改名）", n)),
                        _ => json_io::import_config(&store2, &path)
                            .map(|_| "配置已导入（源文件未改名）".to_string()),
                    }
                    .map_err(|e| e.to_string())
                });
                let msg = match out.await {
                    Ok(Ok(m)) => (true, m),
                    Ok(Err(e)) => (false, format!("导入失败: {}", e)),
                    Err(_) => (false, "导入任务被取消".to_string()),
                };
                done(&view, msg.0, msg.1, cx);
            })
            .detach();
    }
}

/// 写操作完成收敛：通知 + 成功重拉快照
fn done(view: &WeakEntity<SettingsView>, ok: bool, msg: String, cx: &mut AsyncWindowContext) {
    let view = view.clone();
    cx.update(move |window, cx| {
        let note = if ok { Notification::success(msg) } else { Notification::error(msg) };
        window.push_notification(note, cx);
        if ok {
            view.update(cx, |v, cx| v.reload(cx)).ok();
        }
    })
    .ok();
}

/// 分区标题
fn section(title: &'static str) -> impl IntoElement {
    div()
        .id(gpui::ElementId::Name(format!("sec-{}", title).into()))
        .test_support()
        .aria_label(title)
        .text_sm()
        .font_semibold()
        .mt_2()
        .child(title)
}

/// 字段行：标签 + 控件
fn row(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    div()
        .h_flex()
        .gap_2()
        .items_center()
        .child(div().text_sm().w(px(120.)).child(label))
        .child(control)
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = &self.snapshot;

        let mut page = div()
            .id("settings-page")
            .v_flex()
            .size_full()
            .p_4()
            .gap_2()
            .overflow_y_scroll();

        // 鉴权
        page = page
            .child(section("鉴权"))
            .child(row(
                "requireApiKey",
                Switch::new("auth-require")
                    .checked(snap.auth.as_ref().is_some_and(|a| a.require_api_key))
                    .on_click(cx.listener(|this, checked: &bool, window, cx| {
                        this.save_auth(Some(*checked), window, cx);
                    })),
            ))
            .child(row(
                "apiKey",
                div().w(px(320.)).h_flex().gap_2().items_center().child(
                    div().w(px(240.)).child(Input::new(&self.api_key)),
                ).child(
                    Button::new("auth-save")
                        .small()
                        .label("保存")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_auth(None, window, cx);
                        })),
                ),
            ))
            .child(div().text_xs().child(format!(
                "当前 apiKey: {}",
                snap.auth
                    .as_ref()
                    .and_then(|a| a.api_key_mask.clone())
                    .unwrap_or_else(|| "未配置".into())
            )));

        // 代理
        page = page
            .child(section("代理"))
            .child(row("URL", div().w(px(320.)).child(Input::new(&self.proxy_url))))
            .child(row("用户名", div().w(px(320.)).child(Input::new(&self.proxy_user))))
            .child(row("密码", div().w(px(320.)).child(Input::new(&self.proxy_pass))))
            .child(
                Button::new("proxy-save")
                    .small()
                    .label("保存代理")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.save_proxy(window, cx);
                    })),
            )
            .child(div().text_xs().child(format!(
                "当前代理: {}",
                snap.proxy
                    .as_ref()
                    .and_then(|p| p.proxy_url.clone())
                    .unwrap_or_else(|| "未配置".into())
            )));

        // 默认端点
        if let Some(ep) = &snap.endpoint {
            let mut ep_row = div().v_flex().gap_1();
            for name in ep.registered_endpoints.clone() {
                let current = ep.default_endpoint.clone();
                let selected = name == current;
                ep_row = ep_row.child(
                    Radio::new(SharedString::from(format!("ep-{}", name)))
                        .label(name.clone())
                        .checked(selected)
                        .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                            if *checked {
                                this.save_endpoint(name.clone(), window, cx);
                            }
                        })),
                );
            }
            page = page.child(section("默认端点")).child(ep_row);
        }

        // 负载均衡
        if let Some(lb) = &snap.lb {
            let cur = lb.mode.clone();
            page = page.child(section("负载均衡")).child(
                div().v_flex().gap_1()
                    .child(
                        Radio::new("lb-priority")
                            .label("priority（按优先级）")
                            .checked(cur == "priority")
                            .on_click(cx.listener(|this, checked: &bool, window, cx| {
                                if *checked {
                                    this.save_lb("priority", window, cx);
                                }
                            })),
                    )
                    .child(
                        Radio::new("lb-balanced")
                            .label("balanced（均衡分配）")
                            .checked(cur == "balanced")
                            .on_click(cx.listener(|this, checked: &bool, window, cx| {
                                if *checked {
                                    this.save_lb("balanced", window, cx);
                                }
                            })),
                    ),
            );
        }

        // WebSocket
        if let Some(ws) = &snap.ws {
            page = page
                .child(section("WebSocket"))
                .child(row(
                    "启用",
                    Switch::new("ws-enabled")
                        .checked(ws.enabled)
                        .on_click(cx.listener(|this, checked: &bool, window, cx| {
                            this.toggle_ws(*checked, window, cx);
                        })),
                ))
                .child(row(
                    "最大连接数",
                    div().w(px(160.)).h_flex().gap_2().items_center().child(
                        div().w(px(100.)).child(Input::new(&self.ws_max_conn)),
                    ).child(
                        Button::new("ws-save")
                            .small()
                            .label("保存")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_ws(window, cx);
                            })),
                    ),
                ))
                .child(div().text_xs().child(format!(
                    "模式 {}，活跃连接 {}",
                    ws.mode, ws.active_connections
                )));
        }

        // 行为（change 6：托盘常驻 / 开机自启 / 自动启动服务器）
        // 直读直写本地 preferences，不走 AdminService；无 SQLite 句柄
        //（调试模式）时分区禁用
        if let Some(behavior) = &snap.behavior {
            page = page
                .child(section("行为"))
                .child(row(
                    "关窗时最小化常驻",
                    Switch::new("beh-close-tray")
                        .checked(behavior.close_to_tray)
                        .on_click(cx.listener(|this, checked: &bool, _window, cx| {
                            this.save_preference(prefs::CLOSE_TO_TRAY, *checked, cx);
                        })),
                ))
                .child(row(
                    "开机自启",
                    Switch::new("beh-launch-login")
                        .checked(behavior.launch_at_login)
                        .on_click(cx.listener(|this, checked: &bool, window, cx| {
                            this.toggle_launch_at_login(*checked, window, cx);
                        })),
                ))
                .child(row(
                    "启动时自动运行服务器",
                    Switch::new("beh-auto-start")
                        .checked(behavior.auto_start_server)
                        .on_click(cx.listener(|this, checked: &bool, _window, cx| {
                            this.save_preference(prefs::AUTO_START_SERVER, *checked, cx);
                        })),
                ))
                .child(div().text_xs().child(
                    "关窗常驻仅最小化窗口，服务器与进程继续运行；退出经托盘「退出」或快捷键",
                ));
        }

        // 数据（JSON 导入导出）
        let store_available = self.handle.store().is_some();
        page = page
            .child(section("数据"))
            .child(div().text_xs().child(format!(
                "SQLite 凭据 {} 条，配置{}；凭据视图另有批量导入入口",
                snap.credential_count,
                if snap.has_config { "已保存" } else { "未保存" }
            )))
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        Button::new("export-creds")
                            .small()
                            .label("导出凭据")
                            .disabled(!store_available)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.export("credentials", window, cx);
                            })),
                    )
                    .child(
                        Button::new("export-config")
                            .small()
                            .label("导出配置")
                            .disabled(!store_available)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.export("config", window, cx);
                            })),
                    )
                    .child(
                        Button::new("import-creds")
                            .small()
                            .label("导入凭据")
                            .disabled(!store_available)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.import("credentials", window, cx);
                            })),
                    )
                    .child(
                        Button::new("import-config")
                            .small()
                            .label("导入配置")
                            .disabled(!store_available)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.import("config", window, cx);
                            })),
                    ),
            )
            .child(div().text_xs().child("手动导入不改名源文件；首启自动导入会改名 .bak"));

        page
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::AppContext;
    use gpui_kit::test::TestWindowExt;

    use super::SettingsView;
    use crate::core::CoreHandle;

    /// 任务 5.5：六分区标题渲染（headless，经 a11y 标签断言）
    ///
    /// 数据分区在无 SQLite 句柄时仍渲染（导出/导入按钮禁用），
    /// 断言六个分区全部在场。change 6 起「行为」分区仅在持有 SQLite
    /// 句柄时渲染，本用例（无句柄）断言其缺席、其余六分区在场。
    #[gpui::test]
    fn all_sections_render_headless(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_kit::init);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let service = Arc::new(kiro_rs::admin::AdminService::new_with_runtime(
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
        ));
        let handle = CoreHandle::new(service, rt.handle().clone());
        let window = cx.add_window(move |window, cx| SettingsView::new(handle, window, cx));
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            for title in ["鉴权", "代理", "默认端点", "负载均衡", "WebSocket", "数据"] {
                let snap = window.find(format!("sec-{}", title));
                assert_eq!(snap.label(), Some(title), "分区 {} 应渲染", title);
                assert!(snap.visible(), "分区 {} 应可见", title);
            }
            // 无 SQLite 句柄：行为分区不渲染
            assert!(
                window.try_find("sec-行为").is_none(),
                "行为分区应缺席（无句柄）"
            );
            // 无 SQLite 句柄：数据导出/导入按钮在场但禁用
            for id in ["export-creds", "export-config", "import-creds", "import-config"] {
                assert!(window.find(id).visible(), "按钮 {} 应渲染", id);
            }
        })
        .unwrap();
    }

    /// 任务 5.2：行为分区在持有 SQLite 句柄时渲染，三个开关在场
    #[gpui::test]
    fn behavior_section_renders_with_store(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_kit::init);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let service = Arc::new(kiro_rs::admin::AdminService::new_with_runtime(
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
        ));
        let dir = tempfile::tempdir().unwrap();
        // 无钥匙串的测试环境自动回退加密文件后端（`select_backend`）
        let store = crate::store::SqliteStore::open(dir.path()).unwrap();
        let handle = CoreHandle::new(service, rt.handle().clone()).with_store(store);
        let window = cx.add_window(move |window, cx| SettingsView::new(handle, window, cx));
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            let sec = window.find("sec-行为");
            assert_eq!(sec.label(), Some("行为"), "行为分区应渲染");
            assert!(sec.visible(), "行为分区应可见");
            for id in ["beh-close-tray", "beh-launch-login", "beh-auto-start"] {
                assert!(window.find(id).visible(), "开关 {} 应渲染", id);
            }
            // 缺省：关窗常驻开、自动启动服务器开、开机自启关
            assert_eq!(window.find("beh-close-tray").checked(), Some(true));
            assert_eq!(window.find("beh-auto-start").checked(), Some(true));
            assert_eq!(window.find("beh-launch-login").checked(), Some(false));
        })
        .unwrap();
    }
}
