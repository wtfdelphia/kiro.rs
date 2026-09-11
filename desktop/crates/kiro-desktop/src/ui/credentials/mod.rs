//! 凭据视图（设计文档 §8 / change 4）
//!
//! 筛选栏 + `DataTable` + 分页。同步纯读（查询/筛选面）在 GPUI 线程直调；
//! 写操作与上游交互一律经 [`crate::core::CoreHandle::exec`] 派 tokio，
//! 结果通知 + 表格重拉（§8 状态同步模式）。

pub mod dialogs;
pub mod table;

#[cfg(test)]
mod tests;

use gpui_kit::base::StyledExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::table::{DataTable, TableState};
use gpui_kit::component::{Disableable, Sizable, WindowExt, button::Button, button::ButtonVariants};
use gpui_kit::prelude::*;
use gpui_kit::*;
use kiro_rs::admin::types::{
    CredentialFacetsResponse, CredentialsQuery, CredentialsStatusResponse,
};

use crate::core::CoreHandle;
use table::CredentialsDelegate;

const PER_PAGE: i64 = 12;

/// 凭据管理视图
pub struct CredentialsView {
    handle: CoreHandle,
    table: Entity<TableState<CredentialsDelegate>>,
    /// 当前页数据（与 table delegate 的快照同源）
    response: Option<CredentialsStatusResponse>,
    facets: CredentialFacetsResponse,
    query: CredentialsQuery,
}

impl CredentialsView {
    pub fn new(handle: CoreHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = CredentialsDelegate::new();
        let table = cx.new(|cx| TableState::new(delegate, window, cx));
        let weak_view = cx.entity().downgrade();
        table.update(cx, |t, _| {
            t.delegate_mut().view = Some(weak_view);
        });

        let mut this = Self {
            handle,
            table,
            response: None,
            facets: CredentialFacetsResponse {
                subscription_titles: vec![],
                auth_methods: vec![],
            },
            query: CredentialsQuery::default(),
        };
        this.query.page = Some(1);
        this.query.per_page = Some(PER_PAGE);
        this.reload(window, cx);
        this
    }

    /// 同步重查（白名单内两个纯读方法，GPUI 直调）
    pub fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let resp = self.handle.service.query_credentials(&self.query);
        self.facets = self.handle.service.get_credential_facets();
        // CredentialStatusItem 非 Clone：凭据列表移给 delegate，
        // resp 留作分页元数据（total / page_info）
        let mut resp = resp;
        let credentials = std::mem::take(&mut resp.credentials);
        self.response = Some(resp);
        self.table.update(cx, |t, cx| {
            t.delegate_mut().rows = credentials;
            t.refresh(cx);
        });
        cx.notify();
        let _ = window;
    }

    /// 异步操作完成后的统一收敛：通知 + 重拉
    fn notify_and_reload(
        &mut self,
        message: String,
        ok: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let note = if ok {
            Notification::success(message)
        } else {
            Notification::error(message)
        };
        window.push_notification(note, cx);
        self.reload(window, cx);
    }

    // ============ 行操作（表格转发入口） ============

    pub fn toggle_disabled(
        &mut self,
        id: u64,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = self.handle.service.clone();
        let rx = self.handle.exec(async move {
            service.set_disabled(id, disabled).map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let msg = match rx.await {
                Ok(Ok(())) => {
                    let text = if disabled {
                        format!("凭据 #{} 已禁用", id)
                    } else {
                        format!("凭据 #{} 已启用", id)
                    };
                    (text, true)
                }
                Ok(Err(e)) => (format!("启停失败: {}", e), false),
                Err(_) => ("启停任务被取消".to_string(), false),
            };
            this.update_in(cx, |v, window, cx| {
                v.notify_and_reload(msg.0, msg.1, window, cx)
            })
            .ok();
        })
        .detach();
    }

    pub fn bump_priority(
        &mut self,
        id: u64,
        up: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 当前优先级取自 delegate 快照（reload 后与列表同源）
        let Some(priority) = self
            .table
            .read(cx)
            .delegate()
            .rows
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.priority)
        else {
            return;
        };
        let next = if up {
            priority.saturating_sub(1)
        } else {
            priority.saturating_add(1)
        };
        let service = self.handle.service.clone();
        let rx = self.handle.exec(async move {
            service.set_priority(id, next).map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let msg = match rx.await {
                Ok(Ok(())) => (format!("凭据 #{} 优先级已调整为 {}", id, next), true),
                Ok(Err(e)) => (format!("优先级调整失败: {}", e), false),
                Err(_) => ("优先级任务被取消".to_string(), false),
            };
            this.update_in(cx, |v, window, cx| {
                v.notify_and_reload(msg.0, msg.1, window, cx)
            })
            .ok();
        })
        .detach();
    }

    pub fn confirm_delete(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        dialogs::open_delete_confirm(window, cx, id, self.handle.clone(), view);
    }

    pub fn test_credential(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.handle.service.clone();
        let rx = self.handle.exec(async move {
            service
                .test_credential(id, kiro_rs::admin::types::TestCredentialRequest { model: None })
                .await
                .map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let msg = match rx.await {
                Ok(Ok(resp)) => {
                    if resp.success {
                        (
                            format!("凭据 #{} 测试通过（{} 可用）", id, resp.model),
                            true,
                        )
                    } else {
                        (format!("凭据 #{} 测试失败", id), false)
                    }
                }
                Ok(Err(e)) => (format!("凭据 #{} 测试出错: {}", id, e), false),
                Err(_) => ("测试任务被取消".to_string(), false),
            };
            this.update_in(cx, |v, window, cx| {
                v.notify_and_reload(msg.0, msg.1, window, cx)
            })
            .ok();
        })
        .detach();
    }

    pub fn query_balance(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.handle.service.clone();
        let rx = self
            .handle
            .exec(async move { service.get_balance(id, true).await.map_err(|e| e.to_string()) });
        cx.spawn_in(window, async move |this, cx| {
            let msg = match rx.await {
                Ok(Ok(resp)) => (
                    format!(
                        "凭据 #{} 余额: {:.0} / {:.0}",
                        id, resp.current_usage, resp.usage_limit
                    ),
                    true,
                ),
                Ok(Err(e)) => (format!("余额查询失败: {}", e), false),
                Err(_) => ("余额任务被取消".to_string(), false),
            };
            this.update_in(cx, |v, window, cx| {
                v.notify_and_reload(msg.0, msg.1, window, cx)
            })
            .ok();
        })
        .detach();
    }

    pub fn refresh_models(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.handle.service.clone();
        let rx = self
            .handle
            .exec(async move { service.refresh_models(id).await.map_err(|e| e.to_string()) });
        cx.spawn_in(window, async move |this, cx| {
            let msg = match rx.await {
                Ok(Ok(resp)) => (
                    format!("凭据 #{} 模型目录已刷新（{} 个模型）", id, resp.count),
                    true,
                ),
                Ok(Err(e)) => (format!("模型刷新失败: {}", e), false),
                Err(_) => ("模型刷新任务被取消".to_string(), false),
            };
            this.update_in(cx, |v, window, cx| {
                v.notify_and_reload(msg.0, msg.1, window, cx)
            })
            .ok();
        })
        .detach();
    }
}

impl Render for CredentialsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.response.as_ref().map(|r| r.total).unwrap_or(0);
        let page = self.query.page.unwrap_or(1);
        // 不用 div_ceil（nightly）：向上取整手写
        let pages = ((total as i64 + PER_PAGE - 1) / PER_PAGE).max(1);

        let toolbar = div()
            .h_flex()
            .gap_2()
            .child(
                Button::new("add-credential")
                    .icon(assets::IconName::Plus)
                    .label("添加凭据")
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| {
                        let handle = this.handle.clone();
                        let view = cx.entity().downgrade();
                        dialogs::open_add_credential(window, cx, handle, view);
                    })),
            )
            .child(
                Button::new("import-batch")
                    .icon(assets::IconName::FileBraces)
                    .label("批量导入")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let handle = this.handle.clone();
                        let view = cx.entity().downgrade();
                        dialogs::open_batch_import(window, cx, handle, view);
                    })),
            )
            .child(
                Button::new("import-kam")
                    .icon(assets::IconName::FileUp)
                    .label("KAM 导入")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let handle = this.handle.clone();
                        let view = cx.entity().downgrade();
                        dialogs::open_kam_import(window, cx, handle, view);
                    })),
            )
            .child(
                Button::new("login-builder")
                    .icon(assets::IconName::LogIn)
                    .label("Builder ID 登录")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let handle = this.handle.clone();
                        let view = cx.entity().downgrade();
                        dialogs::open_builder_id_login(window, cx, handle, view);
                    })),
            )
            .child(
                Button::new("login-iam")
                    .icon(assets::IconName::Globe)
                    .label("IAM SSO 登录")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let handle = this.handle.clone();
                        let view = cx.entity().downgrade();
                        dialogs::open_iam_sso_login(window, cx, handle, view);
                    })),
            );

        // 筛选栏在最后构建：render_filter_bar 返回的 impl IntoElement
        // 携带对 cx 的借用，必须晚于所有需要 &cx 的 listener 创建
        let pagination = div()
            .h_flex()
            .gap_2()
            .items_center()
            .child(
                Button::new("page-prev")
                    .icon(assets::IconName::ChevronLeft)
                    .disabled(page <= 1)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if page > 1 {
                            this.query.page = Some(page - 1);
                            this.reload(window, cx);
                        }
                    })),
            )
            .child(format!("第 {} / {} 页，共 {} 条", page, pages, total))
            .child(
                Button::new("page-next")
                    .icon(assets::IconName::ChevronRight)
                    .disabled(page >= pages)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if page < pages {
                            this.query.page = Some(page + 1);
                            this.reload(window, cx);
                        }
                    })),
            );

        let filter_bar = self.render_filter_bar(cx);

        div()
            .v_flex()
            .size_full()
            .gap_2()
            .p_3()
            .child(toolbar)
            .child(filter_bar)
            .child(
                div()
                    .flex_1()
                    .size_full()
                    .child(DataTable::new(&self.table).stripe(true).bordered(true)),
            )
            .child(pagination)
    }
}

impl CredentialsView {
    /// 筛选栏：认证方式 / 禁用状态 / 订阅等级
    fn render_filter_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let auth_methods = self.facets.auth_methods.clone();
        let subscription_titles = self.facets.subscription_titles.clone();
        let current_auth = self.query.auth_method.clone();
        let current_sub = self.query.subscription_title.clone();
        let current_disabled = self.query.disabled;

        let mut bar = div().h_flex().gap_2().items_center();

        // 认证方式筛选
        bar = bar.child(div().text_sm().child("认证:"));
        bar = bar.child(filter_chip(
            "全部",
            current_auth.is_none(),
            cx.listener(|this, _, window, cx| {
                this.query.auth_method = None;
                this.query.page = Some(1);
                this.reload(window, cx);
            }),
        ));
        for m in auth_methods {
            let active = current_auth.as_deref() == Some(m.as_str());
            bar = bar.child(filter_chip(
                m.clone(),
                active,
                cx.listener(move |this, _, window, cx| {
                    this.query.auth_method = Some(m.clone());
                    this.query.page = Some(1);
                    this.reload(window, cx);
                }),
            ));
        }

        // 禁用状态筛选
        bar = bar.child(div().text_sm().ml_2().child("状态:"));
        bar = bar.child(filter_chip(
            "全部",
            current_disabled.is_none(),
            cx.listener(|this, _, window, cx| {
                this.query.disabled = None;
                this.query.page = Some(1);
                this.reload(window, cx);
            }),
        ));
        for (label, value) in [("启用", false), ("禁用", true)] {
            let active = current_disabled == Some(value);
            bar = bar.child(filter_chip(
                label,
                active,
                cx.listener(move |this, _, window, cx| {
                    this.query.disabled = Some(value);
                    this.query.page = Some(1);
                    this.reload(window, cx);
                }),
            ));
        }

        // 订阅等级筛选
        if !subscription_titles.is_empty() {
            bar = bar.child(div().text_sm().ml_2().child("订阅:"));
            bar = bar.child(filter_chip(
                "全部",
                current_sub.is_none(),
                cx.listener(|this, _, window, cx| {
                    this.query.subscription_title = None;
                    this.query.page = Some(1);
                    this.reload(window, cx);
                }),
            ));
            bar = bar.child(filter_chip(
                "未知",
                current_sub.as_deref()
                    == Some(kiro_rs::admin::types::SUBSCRIPTION_TITLE_UNKNOWN),
                cx.listener(|this, _, window, cx| {
                    this.query.subscription_title =
                        Some(kiro_rs::admin::types::SUBSCRIPTION_TITLE_UNKNOWN.to_string());
                    this.query.page = Some(1);
                    this.reload(window, cx);
                }),
            ));
            for s in subscription_titles {
                let active = current_sub.as_deref() == Some(s.as_str());
                bar = bar.child(filter_chip(
                    s.clone(),
                    active,
                    cx.listener(move |this, _, window, cx| {
                        this.query.subscription_title = Some(s.clone());
                        this.query.page = Some(1);
                        this.reload(window, cx);
                    }),
                ));
            }
        }

        bar
    }
}

/// 筛选 chip：激活态高亮的小按钮
fn filter_chip(
    label: impl Into<SharedString>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let b = Button::new(label.clone()).small().on_click(on_click);
    if active {
        b.primary()
    } else {
        b
    }
}
