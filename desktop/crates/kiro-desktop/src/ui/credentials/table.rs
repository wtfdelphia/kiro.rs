//! 凭据表格：`TableDelegate` 实现（设计文档 D4）
//!
//! delegate 持行快照 + 父视图弱引用：单元格交互（启停/删除/测试等）
//! 转发给 `CredentialsView` 统一派 tokio 执行。排序不接：服务端按
//! 优先级排序返回。

use gpui_kit::base::StyledExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::table::{Column, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::*;
use kiro_rs::admin::types::CredentialStatusItem;

use super::CredentialsView;

pub struct CredentialsDelegate {
    pub rows: Vec<CredentialStatusItem>,
    pub view: Option<WeakEntity<CredentialsView>>,
}

impl CredentialsDelegate {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            view: None,
        }
    }
}

const COL_COUNT: usize = 9;

const COL_NAMES: [&str; COL_COUNT] = [
    "#", "账号", "认证", "订阅", "优先级", "状态", "余额", "过期", "操作",
];

impl TableDelegate for CredentialsDelegate {
    fn columns_count(&self, _: &App) -> usize {
        COL_COUNT
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        Column::new(COL_NAMES[col_ix], COL_NAMES[col_ix])
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        let Some(item) = self.rows.get(row_ix) else {
            return String::new();
        };
        match col_ix {
            0 => item.id.to_string(),
            1 => item
                .nickname
                .clone()
                .or_else(|| item.email.clone())
                .or_else(|| item.masked_api_key.clone())
                .unwrap_or_default(),
            2 => item.auth_method.clone().unwrap_or_default(),
            3 => item.subscription_title.clone().unwrap_or_else(|| "未知".into()),
            4 => item.priority.to_string(),
            5 => if item.disabled { "禁用".into() } else { "启用".into() },
            6 => item
                .balance
                .as_ref()
                .map(|b| format!("{:.0} / {:.0}", b.current_usage, b.usage_limit))
                .unwrap_or_default(),
            7 => item.expires_at.clone().unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(item) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let id = item.id;
        let view = self.view.clone();

        // 转发助手：单元格事件 → 父视图方法（弱引用，视图已销毁则忽略）
        // 每次展开先克隆一份弱引用，保证宏被多处调用时不发生移动冲突
        macro_rules! fwd {
            ($method:ident $(, $arg:expr)*) => {
                {
                    let view = view.clone();
                    move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                        let Some(view) = view.clone() else { return };
                        view.update_in(cx, |v, window, cx| v.$method(id $(, $arg)*, window, cx)).ok();
                    }
                }
            };
        }

        let cell: AnyElement = match col_ix {
            0 => div().child(format!("{}", item.id)).into_any_element(),
            1 => {
                let label = item
                    .nickname
                    .clone()
                    .or_else(|| item.email.clone())
                    .or_else(|| item.masked_api_key.clone())
                    .unwrap_or_else(|| "—".to_string());
                div().child(label).into_any_element()
            }
            2 => {
                let label = match &item.provider {
                    Some(p) => format!("{} / {}", item.auth_method.clone().unwrap_or_default(), p),
                    None => item.auth_method.clone().unwrap_or_else(|| "—".into()),
                };
                div().child(label).into_any_element()
            }
            3 => div()
                .child(item.subscription_title.clone().unwrap_or_else(|| "未知".into()))
                .into_any_element(),
            4 => div()
                .h_flex()
                .gap_1()
                .child(format!("{}", item.priority))
                .child(
                    Button::new(("prio-up", row_ix))
                        .icon(assets::IconName::ChevronUp)
                        .small()
                        .on_click(fwd!(bump_priority, true)),
                )
                .child(
                    Button::new(("prio-down", row_ix))
                        .icon(assets::IconName::ChevronDown)
                        .small()
                        .on_click(fwd!(bump_priority, false)),
                )
                .into_any_element(),
            5 => {
                let enabled = !item.disabled;
                let failure = item.failure_count;
                let tooltip = item
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| if enabled { "已启用".into() } else { "已禁用".into() });
                let switch = Switch::new(("disable-switch", row_ix))
                    .checked(enabled)
                    .tooltip(tooltip)
                    .on_click({
                        let view = view.clone();
                        move |_: &bool, _window: &mut Window, cx: &mut App| {
                            let Some(view) = view.clone() else { return };
                            view.update_in(cx, |v, window, cx| {
                                v.toggle_disabled(id, !enabled, window, cx)
                            })
                            .ok();
                        }
                    });
                let mut cell = div().h_flex().gap_1().child(switch);
                if failure > 0 {
                    cell = cell.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child(format!("失败 {}", failure)),
                    );
                }
                cell.into_any_element()
            }
            6 => {
                let label = item
                    .balance
                    .as_ref()
                    .map(|b| format!("{:.0} / {:.0}", b.current_usage, b.usage_limit))
                    .unwrap_or_else(|| "—".into());
                div().child(label).into_any_element()
            }
            7 => div()
                .child(item.expires_at.clone().unwrap_or_else(|| "—".into()))
                .into_any_element(),
            8 => div()
                .h_flex()
                .gap_1()
                .child(
                    Button::new(("test", row_ix))
                        .icon(assets::IconName::Play)
                        .small()
                        .tooltip("测试凭据")
                        .on_click(fwd!(test_credential)),
                )
                .child(
                    Button::new(("balance", row_ix))
                        .icon(assets::IconName::Coins)
                        .small()
                        .tooltip("查询余额")
                        .on_click(fwd!(query_balance)),
                )
                .child(
                    Button::new(("models", row_ix))
                        .icon(assets::IconName::RefreshCcw)
                        .small()
                        .tooltip("刷新模型目录")
                        .on_click(fwd!(refresh_models)),
                )
                .child(
                    Button::new(("delete", row_ix))
                        .icon(assets::IconName::Trash)
                        .small()
                        .danger()
                        .tooltip("删除凭据")
                        .on_click(fwd!(confirm_delete)),
                )
                .into_any_element(),
            _ => div().into_any_element(),
        };

        let _ = window;
        cell
    }
}
