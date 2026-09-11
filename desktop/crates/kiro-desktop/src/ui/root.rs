//! 应用主视图：标题栏 + sidebar + 内容区（设计文档 §8）
//!
//! change 4 起：装配完成后持有 [`crate::core::CoreHandle`]，「凭据」
//! 项渲染真实数据视图；装配完成前显示加载态。总览/设置/服务仍是占位。

use gpui_kit::base::StyledExt;
use gpui_kit::component::{
    ActiveTheme, Icon,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::credentials::CredentialsView;
use super::views::{PlaceholderView, ViewKind};
use crate::core::CoreHandle;

/// 应用主视图（窗口第一层内容，外层由 `Root` 包裹）
pub struct AppView {
    current: ViewKind,
    /// 装配完成后由事件循环注入
    handle: Option<CoreHandle>,
    /// 凭据视图实体（首次进入凭据页时懒创建）
    credentials: Option<Entity<CredentialsView>>,
    /// 装配进度文案（加载态 / 失败态）
    boot_status: String,
}

impl AppView {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            current: ViewKind::Overview,
            handle: None,
            credentials: None,
            boot_status: "核心装配中…".to_string(),
        }
    }

    fn select(&mut self, kind: ViewKind, cx: &mut Context<Self>) {
        self.current = kind;
        cx.notify();
    }

    /// 装配成功：记录句柄并默认切到凭据页
    pub fn bootstrapped(&mut self, handle: CoreHandle, cx: &mut Context<Self>) {
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
            (ViewKind::Credentials, None) => {
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

        div()
            .v_flex()
            .size_full()
            // 标题栏（服务器状态徽标为占位，change 5 接真实状态）
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
                            .bg(cx.theme().muted)
                            .child("服务器: 未启动"),
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
