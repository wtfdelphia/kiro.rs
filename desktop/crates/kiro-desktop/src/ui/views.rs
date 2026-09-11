//! 四个占位视图（设计文档 §8）
//!
//! 本期只有标题与落地 change 说明，无数据交互。
//! 数据视图在 change 4（凭据）与 change 5（设置/服务）实现。

use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::base::StyledExt;

/// 视图枚举（sidebar 项与内容区一一对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Overview,
    Credentials,
    Settings,
    Server,
}

impl ViewKind {
    pub const ALL: [ViewKind; 4] = [
        ViewKind::Overview,
        ViewKind::Credentials,
        ViewKind::Settings,
        ViewKind::Server,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewKind::Overview => "总览",
            ViewKind::Credentials => "凭据",
            ViewKind::Settings => "设置",
            ViewKind::Server => "服务",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ViewKind::Overview => "概览数据面板，change 4 落地",
            ViewKind::Credentials => "凭据管理视图，change 4 落地",
            ViewKind::Settings => "设置面板，change 5 落地",
            ViewKind::Server => "服务器启停视图，change 5 落地",
        }
    }

    pub fn icon(self) -> assets::IconName {
        use assets::IconName;
        match self {
            ViewKind::Overview => IconName::LayoutDashboard,
            ViewKind::Credentials => IconName::KeyRound,
            ViewKind::Settings => IconName::Settings,
            ViewKind::Server => IconName::Server,
        }
    }
}

/// 占位视图：标题 + 说明
#[derive(IntoElement)]
pub struct PlaceholderView {
    kind: ViewKind,
}

impl PlaceholderView {
    pub fn new(kind: ViewKind) -> Self {
        Self { kind }
    }
}

impl RenderOnce for PlaceholderView {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .p_6()
            .gap_2()
            .child(div().text_xl().font_semibold().child(self.kind.label()))
            .child(div().text_sm().child(self.kind.hint()))
            .child(
                div()
                    .text_xs()
                    .mt_4()
                    .child(format!("视图枚举：{:?}", self.kind)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::ViewKind;

    // 显式导入而非 `use super::*`：super 顶层的 `use gpui_kit::*` 会把
    // `gpui::test` 属性宏带进作用域，遮蔽内置 `#[test]`
    #[test]
    fn view_kinds_are_four() {
        assert_eq!(ViewKind::ALL.len(), 4);
        let labels: Vec<&str> = ViewKind::ALL.iter().map(|k| k.label()).collect();
        assert_eq!(labels, vec!["总览", "凭据", "设置", "服务"]);
    }
}
