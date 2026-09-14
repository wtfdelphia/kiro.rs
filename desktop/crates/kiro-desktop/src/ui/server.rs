//! 服务视图（设计文档 §8「服务」行）
//!
//! 状态卡 + 启停/重启按钮 + `host` / `port` 修改。状态经
//! [`crate::bridge::CoreEvent::ServerStatus`] 推送，由 `AppView` 转发
//! [`ServerView::on_status`]；修改监听地址落配置后自动触发重启（新地址
//! 立即生效，不需要用户手动重启）。写操作经 `CoreHandle::exec`，
//! 结果经弱引用回调收敛，与凭据对话框同构。

use gpui_kit::base::StyledExt;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt, button::Button, button::ButtonVariants,
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use kiro_rs::admin::types::UpdateServerSettingsRequest;

use crate::core::CoreHandle;
use crate::core::server::ServerStatus;

/// 服务视图
pub struct ServerView {
    handle: CoreHandle,
    status: ServerStatus,
    host: Entity<InputState>,
    port: Entity<InputState>,
}

impl ServerView {
    pub fn new(handle: CoreHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = handle.service.get_server_settings();
        let host_text = settings.host.clone();
        let port_text = settings.port.to_string();
        let host = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(host_text, window, cx);
            s
        });
        let port = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(port_text, window, cx);
            s
        });
        let status = handle
            .server()
            .map(|s| s.status())
            .unwrap_or(ServerStatus::Stopped);
        Self { handle, status, host, port }
    }

    /// 服务器状态事件转发入口（主事件循环 → AppView → 本视图）
    pub fn on_status(&mut self, status: ServerStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    /// 应用地址修改：校验 → `update_server_settings` → 按新地址重启
    fn apply_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.host.read(cx).value().trim().to_string();
        let port_raw = self.port.read(cx).value().trim().to_string();
        if host.is_empty() {
            push_note(window, cx, false, "host 不能为空".into());
            return;
        }
        let port: u16 = match port_raw.parse() {
            Ok(p) if p > 0 => p,
            _ => {
                push_note(window, cx, false, format!("port 非法: {}", port_raw));
                return;
            }
        };

        let service = self.handle.service.clone();
        let server = self.handle.server().cloned();
        let rx = self.handle.exec(async move {
            service
                .update_server_settings(UpdateServerSettingsRequest {
                    host: Some(host.clone()),
                    port: Some(port),
                })
                .map_err(|e| e.to_string())?;
            // 落盘成功即按新地址重启（运行中先停再起；未运行直接起）
            if let Some(srv) = &server {
                srv.restart(Some(&format!("{}:{}", host, port)));
            }
            Ok::<(), String>(())
        });
        window
            .spawn(cx, async move |cx| match rx.await {
                Ok(Ok(())) => notify_view(true, "监听地址已更新，服务器重启中".into(), cx),
                Ok(Err(e)) => notify_view(false, format!("更新失败: {}", e), cx),
                Err(_) => notify_view(false, "更新任务被取消".into(), cx),
            })
            .detach();
    }

    fn button_row(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let can_start = matches!(self.status, ServerStatus::Stopped | ServerStatus::Failed(_));
        let can_stop = matches!(
            self.status,
            ServerStatus::Running(_) | ServerStatus::Starting | ServerStatus::Stopping
        );
        let server_start = self.handle.server().cloned();
        let server_stop = self.handle.server().cloned();
        let server_restart = self.handle.server().cloned();
        let host_val = self.host.read(cx).value().trim().to_string();
        let port_val = self.port.read(cx).value().trim().to_string();
        // 三个按钮闭包各需一份地址
        let addr = format!("{}:{}", host_val, port_val);

        let addr_restart = addr.clone();

        div()
            .h_flex()
            .gap_2()
            .child(
                Button::new("srv-start")
                    .label("启动")
                    .primary()
                    .disabled(!can_start)
                    .on_click(move |_, _, _| {
                        if let Some(srv) = &server_start {
                            srv.start(&addr);
                        }
                    }),
            )
            .child(
                Button::new("srv-stop")
                    .label("停止")
                    .disabled(!can_stop)
                    .on_click(move |_, _, _| {
                        if let Some(srv) = &server_stop {
                            srv.request_stop();
                        }
                    }),
            )
            .child(
                Button::new("srv-restart")
                    .label("重启")
                    .disabled(!can_stop)
                    .on_click(move |_, _, _| {
                        if let Some(srv) = &server_restart {
                            srv.restart(Some(&addr_restart));
                        }
                    }),
            )
            .into_any_element()
    }
}

/// 同步推送一条通知（输入校验等就地场景）
fn push_note(window: &mut Window, cx: &mut App, ok: bool, msg: String) {
    let note = if ok { Notification::success(msg) } else { Notification::error(msg) };
    window.push_notification(note, cx);
}

/// 异步任务完成后的收敛：通知（窗口已销毁则静默）
fn notify_view(ok: bool, msg: String, cx: &mut AsyncWindowContext) {
    cx.update(move |window, cx| {
        let note = if ok { Notification::success(msg) } else { Notification::error(msg) };
        window.push_notification(note, cx);
    })
    .ok();
}

impl Render for ServerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.status.clone();
        let status_color = match &status {
            ServerStatus::Running(_) => gpui::hsla(142., 0.7, 0.4, 1.),
            ServerStatus::Failed(_) => gpui::hsla(0., 0.75, 0.5, 1.),
            ServerStatus::Stopping | ServerStatus::Starting => gpui::hsla(45., 0.9, 0.5, 1.),
            ServerStatus::Stopped => cx.theme().muted_foreground,
        };
        let addr_text = match &status {
            ServerStatus::Running(a) => a.clone(),
            _ => "—".to_string(),
        };

        let mut content = div().v_flex().size_full().p_4().gap_3().child(
            div()
                .h_flex()
                .items_center()
                .gap_2()
                .child(div().size_2().rounded_full().bg(status_color))
                .child(
                    div()
                        .id("srv-status")
                        .test_support()
                        .aria_label(status.label())
                        .text_sm()
                        .font_semibold()
                        .child(status.label()),
                )
                .child(
                    div()
                        .id("srv-addr")
                        .test_support()
                        .aria_label(addr_text.clone())
                        .text_sm()
                        .child(addr_text),
                ),
        );

        if let ServerStatus::Failed(msg) = &status {
            content = content.child(
                div()
                    .id("srv-fail-reason")
                    .test_support()
                    .aria_label(msg.clone())
                    .text_xs()
                    .p_2()
                    .rounded_sm()
                    .bg(cx.theme().danger)
                    .child(format!("失败原因: {}", msg)),
            );
        }

        content
            .child(self.button_row(window, cx))
            .child(div().border_t_1().border_color(cx.theme().border))
            .child(div().text_sm().font_semibold().child("监听地址"))
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().w(px(40.)).child("host"))
                    .child(div().w(px(220.)).child(Input::new(&self.host)))
                    .child(div().text_sm().w(px(40.)).child("port"))
                    .child(div().w(px(100.)).child(Input::new(&self.port)))
                    .child(
                        Button::new("srv-apply")
                            .label("应用并重启")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_address(window, cx);
                            })),
                    ),
            )
            .child(div().text_xs().child(
                "修改后立即按新地址重启服务器；端口冲突会显示在失败原因里。",
            ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::AppContext;
    use gpui_kit::test::TestWindowExt;

    use super::ServerView;
    use crate::core::CoreHandle;
    use crate::core::server::ServerStatus;

    /// 最小可装配句柄：默认配置的 AdminService（与生产同一装配路径）
    fn test_handle(rt: tokio::runtime::Handle) -> CoreHandle {
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
        CoreHandle::new(service, rt)
    }

    /// 任务 5.5：五态文案与地址展示（headless 渲染，经 a11y 标签断言）
    #[gpui::test]
    fn status_labels_and_address_follow_state(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_kit::init);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = test_handle(rt.handle().clone());
        let window = cx.add_window(move |window, cx| ServerView::new(handle, window, cx));
        let any = window.into();

        let set_status = |cx: &mut gpui::TestAppContext, status: ServerStatus| {
            window.update(cx, |view, _, cx| view.on_status(status, cx)).unwrap();
        };
        let draw = |cx: &mut gpui::TestAppContext| {
            cx.update_window(any, |_, window, cx| {
                window.draw(cx).clear(cx);
            })
            .unwrap();
        };

        // Stopped（初始：未挂服务器控制器）
        draw(cx);
        cx.update_window(any, |_, window, _| {
            assert_eq!(window.find("srv-status").label(), Some("未启动"));
            assert_eq!(window.find("srv-addr").label(), Some("—"));
        })
        .unwrap();

        set_status(cx, ServerStatus::Starting);
        draw(cx);
        cx.update_window(any, |_, window, _| {
            assert_eq!(window.find("srv-status").label(), Some("启动中"));
        })
        .unwrap();

        set_status(cx, ServerStatus::Running("127.0.0.1:8080".into()));
        draw(cx);
        cx.update_window(any, |_, window, _| {
            assert_eq!(window.find("srv-status").label(), Some("运行中"));
            assert_eq!(window.find("srv-addr").label(), Some("127.0.0.1:8080"));
        })
        .unwrap();

        set_status(cx, ServerStatus::Stopping);
        draw(cx);
        cx.update_window(any, |_, window, _| {
            assert_eq!(window.find("srv-status").label(), Some("停止中"));
        })
        .unwrap();

        set_status(cx, ServerStatus::Failed("端口 8080 被占用".into()));
        draw(cx);
        cx.update_window(any, |_, window, _| {
            assert_eq!(window.find("srv-status").label(), Some("启动失败"));
            assert_eq!(
                window.find("srv-fail-reason").label(),
                Some("端口 8080 被占用")
            );
        })
        .unwrap();
    }
}
