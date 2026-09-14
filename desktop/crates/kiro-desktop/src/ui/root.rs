//! 应用主视图：标题栏 + sidebar + 内容区（设计文档 §8）
//!
//! change 4 起：装配完成后持有 [`crate::core::CoreHandle`]，「凭据」
//! 项渲染真实数据视图。change 5 起：设置与服务视图接真实数据，标题栏
//! 徽标接服务器状态机与退出状态；装配完成前显示加载态。总览仍是占位。

use gpui_kit::base::StyledExt;
use gpui_kit::component::{
    ActiveTheme, Icon,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::credentials::CredentialsView;
use super::server::ServerView;
use super::settings::SettingsView;
use super::views::{PlaceholderView, ViewKind};
use crate::core::CoreHandle;
use crate::core::server::ServerStatus;

/// 应用主视图（窗口第一层内容，外层由 `Root` 包裹）
pub struct AppView {
    current: ViewKind,
    /// 装配完成后由事件循环注入
    handle: Option<CoreHandle>,
    /// 凭据视图实体（首次进入凭据页时懒创建）
    credentials: Option<Entity<CredentialsView>>,
    /// 服务视图实体（懒创建）
    server: Option<Entity<ServerView>>,
    /// 设置视图实体（懒创建）
    settings: Option<Entity<SettingsView>>,
    /// 服务器状态（事件驱动刷新）
    server_status: ServerStatus,
    /// 装配进度文案（加载态 / 失败态）
    boot_status: String,
}

impl AppView {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            current: ViewKind::Overview,
            handle: None,
            credentials: None,
            server: None,
            settings: None,
            server_status: ServerStatus::Stopped,
            boot_status: "核心装配中…".to_string(),
        }
    }

    /// 核心句柄（键绑定与关窗拦截经此触发退出协调；装配前为 `None`）
    pub fn core_handle(&self) -> Option<CoreHandle> {
        self.handle.clone()
    }

    fn select(&mut self, kind: ViewKind, cx: &mut Context<Self>) {
        self.current = kind;
        cx.notify();
    }

    /// 装配成功：记录句柄并默认切到凭据页
    pub fn bootstrapped(&mut self, handle: CoreHandle, cx: &mut Context<Self>) {
        // 句柄携带的服务器状态为准（装配任务可能已自动启动）
        if let Some(srv) = handle.server() {
            self.server_status = srv.status();
        }
        self.handle = Some(handle);
        self.boot_status = "就绪".to_string();
        self.current = ViewKind::Credentials;
        cx.notify();
    }

    /// 装配失败：显示错误信息
    pub fn bootstrap_failed(&mut self, msg: String, cx: &mut Context<Self>) {
        self.boot_status = format!("装配失败: {}", msg);
        cx.notify();
    }

    /// 服务器状态事件：刷新徽标并转发给服务视图
    pub fn on_server_status(&mut self, status: ServerStatus, cx: &mut Context<Self>) {
        self.server_status = status.clone();
        if let Some(v) = &self.server {
            v.update(cx, |v, cx| v.on_status(status, cx));
        }
        cx.notify();
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match (self.current, self.handle.clone()) {
            (ViewKind::Credentials, Some(handle)) => {
                let view = match &self.credentials {
                    Some(v) => v.clone(),
                    None => {
                        let v = cx.new(|cx| CredentialsView::new(handle, window, cx));
                        self.credentials = Some(v.clone());
                        v
                    }
                };
                view.into_any_element()
            }
            (ViewKind::Settings, Some(handle)) => {
                let view = match &self.settings {
                    Some(v) => v.clone(),
                    None => {
                        let v = cx.new(|cx| SettingsView::new(handle, window, cx));
                        self.settings = Some(v.clone());
                        v
                    }
                };
                view.into_any_element()
            }
            (ViewKind::Server, Some(handle)) => {
                let view = match &self.server {
                    Some(v) => v.clone(),
                    None => {
                        let v = cx.new(|cx| ServerView::new(handle, window, cx));
                        self.server = Some(v.clone());
                        v
                    }
                };
                view.into_any_element()
            }
            (_, None) => {
                let status = self.boot_status.clone();
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_sm().child(status))
                    .into_any_element()
            }
            (kind, _) => PlaceholderView::new(kind).into_any_element(),
        }
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar = Sidebar::new("main-sidebar")
            .collapsible(true)
            .header(
                div()
                    .h_flex()
                    .px_2()
                    .py_2()
                    .gap_2()
                    .child(Icon::new(assets::IconName::Box))
                    .child(div().font_semibold().child("kiro-rs")),
            )
            .child(
                SidebarGroup::new("导航").child(
                    SidebarMenu::new().children(ViewKind::ALL.iter().map(|kind| {
                        let kind = *kind;
                        SidebarMenuItem::new(kind.label())
                            .icon(kind.icon())
                            .active(self.current == kind)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select(kind, cx);
                            }))
                    })),
                ),
            );

        let content = div()
            .v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_content(window, cx));

        // 标题栏徽标：服务器状态机实时值；退出中覆盖为退出指示
        let (badge_text, quitting) = if crate::quit::is_quitting() {
            ("正在退出…".to_string(), true)
        } else {
            (format!("服务器: {}", self.server_status.label()), false)
        };
        let badge_bg = if quitting {
            cx.theme().warning
        } else {
            match &self.server_status {
                ServerStatus::Running(_) => cx.theme().success,
                ServerStatus::Failed(_) => cx.theme().danger,
                _ => cx.theme().muted,
            }
        };

        div()
            .v_flex()
            .size_full()
            // 标题栏
            .child(
                div()
                    .h_flex()
                    .id("titlebar")
                    .h_8()
                    .px_3()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().text_sm().font_semibold().child("kiro-rs"))
                    .child(
                        div()
                            .text_xs()
                            .px_1()
                            .rounded_sm()
                            .bg(badge_bg)
                            .child(badge_text),
                    )
                    .child(
                        div()
                            .text_xs()
                            .px_1()
                            .rounded_sm()
                            .bg(cx.theme().muted)
                            .child(format!("核心: {}", self.boot_status)),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .flex_1()
                    .size_full()
                    .child(sidebar)
                    .child(content),
            )
    }
}
