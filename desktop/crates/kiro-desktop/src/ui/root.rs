//! 应用主视图：标题栏 + sidebar + 内容区（设计文档 §8）
//!
//! 本期无数据交互：标题栏显示服务器状态占位徽标，
//! sidebar 四项切换内容区的占位视图。

use gpui_kit::component::{
    ActiveTheme, Icon,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::base::StyledExt;

use super::views::{PlaceholderView, ViewKind};

/// 应用主视图（窗口第一层内容，外层由 `Root` 包裹）
pub struct AppView {
    current: ViewKind,
}

impl AppView {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            current: ViewKind::Overview,
        }
    }

    fn select(&mut self, kind: ViewKind, cx: &mut Context<Self>) {
        self.current = kind;
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    SidebarMenu::new()
                        .children(ViewKind::ALL.iter().map(|kind| {
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
            .child(PlaceholderView::new(self.current));

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
