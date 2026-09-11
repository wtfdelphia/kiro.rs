//! 凭据相关对话框与在线登录（设计文档 D6 / D7）
//!
//! 全部经 `window.open_dialog` 打开；确认/取消文案与变体走
//! [`DialogButtonProps`]。写操作与上游交互派
//! [`crate::core::CoreHandle::exec`]（tokio 侧），结果以通知收敛并经
//! 父视图弱引用触发重拉。对话框构建闭包是 `Fn`，捕获的实体句柄在
//! 每个内部闭包使用前各自克隆，避免所有权移动。

use gpui_kit::base::StyledExt;
use gpui_kit::component::WindowExt;
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use kiro_rs::admin::types::{AddCredentialRequest, BatchImportRequest, KamImportRequest};

use super::CredentialsView;
use crate::core::CoreHandle;

/// 打开「删除凭据」确认对话框
pub fn open_delete_confirm(
    window: &mut Window,
    cx: &mut App,
    id: u64,
    handle: CoreHandle,
    view: WeakEntity<CredentialsView>,
) {
    window.open_alert_dialog(cx, move |dialog, _, _| {
        dialog
            .title("删除凭据")
            .description(format!(
                "确认删除凭据 #{}？此操作会同步清理本地存储与钥匙串条目。",
                id
            ))
            .button_props(
                DialogButtonProps::default()
                    .ok_text("删除")
                    .cancel_text("取消")
                    .show_cancel(true),
            )
            .on_ok({
                let exec = handle.clone();
                let view = view.clone();
                move |_, window, cx| {
                    // on_ok 是 Fn：捕获值先在闭包体内克隆再移入异步任务
                    let view = view.clone();
                    let service = exec.service.clone();
                    let rx = exec.exec(async move {
                        service.delete_credential(id).map_err(|e| e.to_string())
                    });
                    window
                        .spawn(cx, async move |cx| match rx.await {
                            Ok(Ok(())) => {
                                notify_and_reload(true, format!("凭据 #{} 已删除", id), &view, cx)
                            }
                            Ok(Err(e)) => notify_and_reload(false, format!("删除失败: {}", e), &view, cx),
                            Err(_) => notify_and_reload(false, "删除任务被取消".into(), &view, cx),
                        })
                        .detach();
                    true
                }
            })
    });
}

/// 打开「添加凭据」对话框（social / api_key / idc，按填写字段推断）
pub fn open_add_credential(
    window: &mut Window,
    cx: &mut App,
    handle: CoreHandle,
    view: WeakEntity<CredentialsView>,
) {
    let refresh = cx.new(|cx| InputState::new(window, cx).placeholder("refreshToken（social/idc 必填）"));
    let api_key = cx.new(|cx| InputState::new(window, cx).placeholder("ksk_ 开头（api_key 必填）"));
    let client_id = cx.new(|cx| InputState::new(window, cx).placeholder("clientId（仅 idc）"));
    let client_secret = cx.new(|cx| InputState::new(window, cx).placeholder("clientSecret（仅 idc）"));

    window.open_dialog(cx, move |dialog, _, _| {
        // content 与 on_ok 各持一份克隆，避免移动捕获
        let (refresh_c, refresh_o) = (refresh.clone(), refresh.clone());
        let (api_key_c, api_key_o) = (api_key.clone(), api_key.clone());
        let (client_id_c, client_id_o) = (client_id.clone(), client_id.clone());
        let (client_secret_c, client_secret_o) = (client_secret.clone(), client_secret.clone());
        dialog
            .title("添加凭据")
            .w(px(520.))
            .button_props(
                DialogButtonProps::default()
                    .ok_text("添加")
                    .cancel_text("取消")
                    .show_cancel(true),
            )
            .content(move |content, _, _| {
                content.child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_2()
                        .child(field_hint(
                            "认证方式",
                            "填 kiroApiKey 走 api_key；填 clientId 走 idc；否则 social",
                        ))
                        .child(labeled_row("refreshToken", Input::new(&refresh_c)))
                        .child(labeled_row("kiroApiKey", Input::new(&api_key_c)))
                        .child(labeled_row("clientId", Input::new(&client_id_c)))
                        .child(labeled_row("clientSecret", Input::new(&client_secret_c))),
                )
            })
            .on_ok({
                let handle = handle.clone();
                let view = view.clone();
                move |_, window, cx| {
                    let rt = refresh_o.read(cx).value().to_string();
                    let ak = api_key_o.read(cx).value().to_string();
                    let cid = client_id_o.read(cx).value().to_string();
                    let cs = client_secret_o.read(cx).value().to_string();

                    let auth_method = match validate_add_input(&rt, &ak, &cid) {
                        Ok(m) => m,
                        Err(msg) => {
                            push_err(window, cx, &msg);
                            return false;
                        }
                    };

                    let req = AddCredentialRequest {
                        refresh_token: non_empty(rt),
                        auth_method: auth_method.to_string(),
                        provider: None,
                        profile_arn: None,
                        client_id: non_empty(cid),
                        client_secret: non_empty(cs),
                        priority: 0,
                        region: None,
                        auth_region: None,
                        api_region: None,
                        machine_id: None,
                        email: None,
                        user_id: None,
                        nickname: None,
                        start_url: None,
                        on_conflict: None,
                        proxy_url: None,
                        proxy_username: None,
                        proxy_password: None,
                        kiro_api_key: non_empty(ak),
                        endpoint: None,
                        token_endpoint: None,
                        issuer_url: None,
                        scopes: None,
                    };

                    let service = handle.service.clone();
                    let rx =
                        handle.exec(async move { service.add_credential(req).await.map_err(|e| e.to_string()) });
                    let view = view.clone();
                    window
                        .spawn(cx, async move |cx| match rx.await {
                            Ok(Ok(resp)) => notify_and_reload(
                                true,
                                format!("凭据已添加（#{}）{}", resp.credential_id, resp.message),
                                &view,
                                cx,
                            ),
                            Ok(Err(e)) => notify_and_reload(false, format!("添加失败: {}", e), &view, cx),
                            Err(_) => notify_and_reload(false, "添加任务被取消".into(), &view, cx),
                        })
                        .detach();
                    true
                }
            })
    });
}

/// 打开「批量导入」对话框（JSON 数组）
pub fn open_batch_import(
    window: &mut Window,
    cx: &mut App,
    handle: CoreHandle,
    view: WeakEntity<CredentialsView>,
) {
    let textarea = cx.new(|cx| {
        TextareaState::new(window, cx)
            .placeholder("粘贴凭据 JSON 数组")
            .rows(8)
    });

    window.open_dialog(cx, move |dialog, _, _| {
        let (textarea_c, textarea_o) = (textarea.clone(), textarea.clone());
        dialog
            .title("批量导入凭据")
            .w(px(560.))
            .button_props(
                DialogButtonProps::default()
                    .ok_text("导入")
                    .cancel_text("取消")
                    .show_cancel(true),
            )
            .content(move |content, _, _| {
                content.child(div().v_flex().p_2().child(Textarea::new(&textarea_c)))
            })
            .on_ok({
                let handle = handle.clone();
                let view = view.clone();
                move |_, window, cx| {
                    let raw = textarea_o.read(cx).value().to_string();
                    // 本地预解析：解析失败就地报错，不发请求
                    let items: Vec<AddCredentialRequest> = match serde_json::from_str(&raw) {
                        Ok(v) => v,
                        Err(e) => {
                            push_err(window, cx, &format!("JSON 解析失败: {}", e));
                            return false;
                        }
                    };
                    if items.is_empty() {
                        push_err(window, cx, "导入列表为空");
                        return false;
                    }

                    let req = BatchImportRequest { items, options: None };
                    let service = handle.service.clone();
                    let rx = handle.exec(async move {
                        service.import_credentials_batch(req).await.map_err(|e| e.to_string())
                    });
                    let view = view.clone();
                    window
                        .spawn(cx, async move |cx| match rx.await {
                            Ok(Ok(resp)) => notify_and_reload(
                                resp.success,
                                format!(
                                    "批量导入完成：{} 成功 / {} 跳过 / {} 失败",
                                    resp.summary.created, resp.summary.duplicate, resp.summary.failed
                                ),
                                &view,
                                cx,
                            ),
                            Ok(Err(e)) => notify_and_reload(false, format!("批量导入失败: {}", e), &view, cx),
                            Err(_) => notify_and_reload(false, "导入任务被取消".into(), &view, cx),
                        })
                        .detach();
                    true
                }
            })
    });
}

/// 打开「KAM 文档导入」对话框
pub fn open_kam_import(
    window: &mut Window,
    cx: &mut App,
    handle: CoreHandle,
    view: WeakEntity<CredentialsView>,
) {
    let textarea = cx.new(|cx| {
        TextareaState::new(window, cx)
            .placeholder("粘贴 KAM 导出文档（JSON）")
            .rows(8)
    });

    window.open_dialog(cx, move |dialog, _, _| {
        let (textarea_c, textarea_o) = (textarea.clone(), textarea.clone());
        dialog
            .title("KAM 文档导入")
            .w(px(560.))
            .button_props(
                DialogButtonProps::default()
                    .ok_text("导入")
                    .cancel_text("取消")
                    .show_cancel(true),
            )
            .content(move |content, _, _| {
                content.child(div().v_flex().p_2().child(Textarea::new(&textarea_c)))
            })
            .on_ok({
                let handle = handle.clone();
                let view = view.clone();
                move |_, window, cx| {
                    let raw = textarea_o.read(cx).value().to_string();
                    let document: serde_json::Value = match serde_json::from_str(&raw) {
                        Ok(v) => v,
                        Err(e) => {
                            push_err(window, cx, &format!("JSON 解析失败: {}", e));
                            return false;
                        }
                    };
                    let req = KamImportRequest { document, options: None, dry_run: false };
                    let service = handle.service.clone();
                    let rx = handle.exec(async move {
                        service.import_kam_document(req).await.map_err(|e| e.to_string())
                    });
                    let view = view.clone();
                    window
                        .spawn(cx, async move |cx| match rx.await {
                            Ok(Ok(resp)) => {
                                let msg = match &resp.summary {
                                    Some(s) => format!(
                                        "KAM 导入完成（{}）：{} 成功 / {} 跳过 / {} 失败",
                                        resp.container, s.created, s.duplicate, s.failed
                                    ),
                                    None => format!("KAM 导入完成（{}）", resp.container),
                                };
                                notify_and_reload(resp.success, msg, &view, cx)
                            }
                            Ok(Err(e)) => notify_and_reload(false, format!("KAM 导入失败: {}", e), &view, cx),
                            Err(_) => notify_and_reload(false, "导入任务被取消".into(), &view, cx),
                        })
                        .detach();
                    true
                }
            })
    });
}

/// Builder ID 在线登录（设计文档 D7）
///
/// 对话框只收可选 region；确认后派 `start_builder_id_login`，成功即拉起
/// 浏览器并启动独立轮询任务（不依赖对话框生命周期），验证码以通知呈现。
pub fn open_builder_id_login(
    window: &mut Window,
    cx: &mut App,
    handle: CoreHandle,
    view: WeakEntity<CredentialsView>,
) {
    let region = cx.new(|cx| InputState::new(window, cx).placeholder("region（可选，如 us-east-1）"));

    window.open_dialog(cx, move |dialog, _, _| {
        let (region_c, region_o) = (region.clone(), region.clone());
        dialog
            .title("Builder ID 登录")
            .w(px(520.))
            .button_props(
                DialogButtonProps::default()
                    .ok_text("开始登录")
                    .cancel_text("取消")
                    .show_cancel(true),
            )
            .content(move |content, _, _| {
                content.child(div().v_flex().gap_2().p_2().child(labeled_row("region", Input::new(&region_c))))
            })
            .on_ok({
                let exec = handle.clone();
                let view = view.clone();
                move |_, window, cx| {
                    let region_value = non_empty(region_o.read(cx).value().to_string());
                    let service = exec.service.clone();
                    let rx = exec.exec(async move {
                        service.start_builder_id_login(region_value).await.map_err(|e| e.to_string())
                    });
                    let exec_poll = exec.clone();
                    let view = view.clone();
                    window
                        .spawn(cx, async move |cx| match rx.await {
                            Ok(Ok(start)) => {
                                // 拉起浏览器；失败时 URL 已在通知中可复制
                                if let Err(e) = open::that(&start.verification_uri) {
                                    tracing::warn!("打开浏览器失败: {}，请手动访问 {}", e, start.verification_uri);
                                }
                                let user_code = start.user_code.clone();
                                cx.update(|window, cx| {
                                    window.push_notification(
                                        Notification::info(format!(
                                            "已打开浏览器完成授权（验证码 {}），登录中…",
                                            user_code
                                        )),
                                        cx,
                                    );
                                })
                                .ok();
                                let session_id = start.session_id.clone();
                                let interval = (start.interval.max(1)) as u64;
                                let expires = start.expires_in.max(60);
                                // 轮询任务独立于对话框生命周期
                                cx.spawn(async move |cx| {
                                    poll_builder_id(exec_poll, session_id, interval, expires, user_code, view, cx)
                                        .await;
                                })
                                .detach();
                            }
                            Ok(Err(e)) => {
                                notify_and_reload(false, format!("Builder ID 登录启动失败: {}", e), &view, cx)
                            }
                            Err(_) => notify_and_reload(false, "Builder ID 登录任务被取消".into(), &view, cx),
                        })
                        .detach();
                    true
                }
            })
    });
}

/// IAM SSO 登录（设计文档 D7，两段式）
///
/// 第一段：收 startUrl → `start_iam_sso_login` → 拉起浏览器 → 打开第二段
/// 对话框。第二段：粘贴回调 URL → `complete_iam_sso_login`。会话过期/回调
/// 解析失败的文案透传 `AdminServiceError` 的 Display。
pub fn open_iam_sso_login(
    window: &mut Window,
    cx: &mut App,
    handle: CoreHandle,
    view: WeakEntity<CredentialsView>,
) {
    let start_url = cx.new(|cx| InputState::new(window, cx).placeholder("IAM SSO startUrl（必填）"));

    window.open_dialog(cx, move |dialog, _, _| {
        let (start_url_c, start_url_o) = (start_url.clone(), start_url.clone());
        dialog
            .title("IAM SSO 登录")
            .w(px(520.))
            .button_props(
                DialogButtonProps::default()
                    .ok_text("开始登录")
                    .cancel_text("取消")
                    .show_cancel(true),
            )
            .content(move |content, _, _| {
                content.child(div().v_flex().gap_2().p_2().child(labeled_row("startUrl", Input::new(&start_url_c))))
            })
            .on_ok({
                let exec = handle.clone();
                let view = view.clone();
                move |_, window, cx| {
                    let url = start_url_o.read(cx).value().to_string();
                    if url.trim().is_empty() {
                        push_err(window, cx, "startUrl 不能为空");
                        return false;
                    }
                    let service = exec.service.clone();
                    let rx = exec.exec(async move {
                        service.start_iam_sso_login(url, None).await.map_err(|e| e.to_string())
                    });
                    let handle_d2 = exec.clone();
                    let view_d2 = view.clone();
                    let view = view.clone();
                    window
                        .spawn(cx, async move |cx| match rx.await {
                            Ok(Ok(start)) => {
                                if let Err(e) = open::that(&start.authorize_url) {
                                    tracing::warn!("打开浏览器失败: {}，请手动访问 {}", e, start.authorize_url);
                                }
                                let session_id = start.session_id.clone();
                                cx.update(|window, cx| {
                                    window.push_notification(
                                        Notification::info(format!(
                                            "已打开授权页（会话 {}），完成后回来粘贴回调",
                                            session_id
                                        )),
                                        cx,
                                    );
                                })
                                .ok();
                                // 第二段：回调输入
                                cx.update(move |window, cx| {
                                    open_iam_callback_dialog(window, cx, session_id, handle_d2, view_d2);
                                })
                                .ok();
                            }
                            Ok(Err(e)) => notify_and_reload(false, format!("IAM SSO 启动失败: {}", e), &view, cx),
                            Err(_) => notify_and_reload(false, "IAM SSO 任务被取消".into(), &view, cx),
                        })
                        .detach();
                    true
                }
            })
    });
}

/// IAM SSO 第二段：粘贴回调完成登录
fn open_iam_callback_dialog(
    window: &mut Window,
    cx: &mut App,
    session_id: String,
    handle: CoreHandle,
    view: WeakEntity<CredentialsView>,
) {
    let callback = cx.new(|cx| InputState::new(window, cx).placeholder("浏览器授权后的回调 URL"));

    window.open_dialog(cx, move |dialog, _, _| {
        let (callback_c, callback_o) = (callback.clone(), callback.clone());
        // 外层是 Fn 闭包：session_id 给两个内部闭包各备一份克隆
        let (session_c, session_o) = (session_id.clone(), session_id.clone());
        dialog
            .title("IAM SSO：粘贴回调")
            .w(px(520.))
            .button_props(
                DialogButtonProps::default()
                    .ok_text("完成登录")
                    .cancel_text("取消")
                    .show_cancel(true),
            )
            .content(move |content, _, _| {
                content.child(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_2()
                        .child(div().text_xs().child(format!("会话 {} 已就绪，完成浏览器授权后粘贴回调", session_c)))
                        .child(labeled_row("回调 URL", Input::new(&callback_c))),
                )
            })
            .on_ok({
                let exec = handle.clone();
                let view = view.clone();
                move |_, window, cx| {
                    let cb = callback_o.read(cx).value().to_string();
                    if cb.trim().is_empty() {
                        push_err(window, cx, "请粘贴浏览器授权后的回调 URL");
                        return false;
                    }
                    let sid = session_o.clone();
                    let service = exec.service.clone();
                    let rx = exec.exec(async move {
                        service.complete_iam_sso_login(sid, cb).await.map_err(|e| e.to_string())
                    });
                    let view = view.clone();
                    window
                        .spawn(cx, async move |cx| match rx.await {
                            Ok(Ok(resp)) => notify_and_reload(
                                true,
                                format!("IAM SSO 登录成功（凭据 #{}）{}", resp.credential_id, resp.message),
                                &view,
                                cx,
                            ),
                            Ok(Err(e)) => notify_and_reload(false, format!("IAM SSO 完成失败: {}", e), &view, cx),
                            Err(_) => notify_and_reload(false, "IAM SSO 任务被取消".into(), &view, cx),
                        })
                        .detach();
                    true
                }
            })
    });
}

// ============ 辅助 ============

/// 轮询 Builder ID 直到完成、失败或过期
#[allow(clippy::too_many_arguments)]
async fn poll_builder_id(
    exec: CoreHandle,
    session_id: String,
    interval: u64,
    expires_secs: u64,
    user_code: String,
    view: WeakEntity<CredentialsView>,
    cx: &mut AsyncWindowContext,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(expires_secs);
    loop {
        let sid = session_id.clone();
        let svc = exec.service.clone();
        let rx = exec.exec(async move { svc.poll_builder_id_login(sid).await.map_err(|e| e.to_string()) });
        match rx.await {
            Ok(Ok(value)) => {
                let completed = value.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);
                if completed {
                    let email = value.get("email").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let msg = if email.is_empty() {
                        format!("Builder ID 登录成功（{}）", user_code)
                    } else {
                        format!("Builder ID 登录成功：{}", email)
                    };
                    notify_and_reload(true, msg, &view, cx);
                    return;
                }
            }
            Ok(Err(e)) => {
                notify_and_reload(false, format!("Builder ID 登录失败: {}", e), &view, cx);
                return;
            }
            Err(_) => return, // 接收端被取消（退出路径），静默结束
        }
        if std::time::Instant::now() >= deadline {
            notify_and_reload(false, "Builder ID 登录已过期，请重新发起".into(), &view, cx);
            return;
        }
        cx.background_executor()
            .timer(std::time::Duration::from_secs(interval))
            .await;
    }
}

/// 异步侧通知收敛 + 成功时重拉列表
fn notify_and_reload(ok: bool, msg: String, view: &WeakEntity<CredentialsView>, cx: &mut AsyncWindowContext) {
    let note = if ok { Notification::success(msg) } else { Notification::error(msg) };
    let view = view.clone();
    cx.update(move |window, cx| {
        window.push_notification(note, cx);
        if ok {
            view.update_in(cx, |v, window, cx| v.reload(window, cx)).ok();
        }
    })
    .ok();
}

/// 添加凭据本地校验：返回推断出的认证方式或错误文案（纯函数，见测试）
fn validate_add_input(refresh_token: &str, api_key: &str, client_id: &str) -> Result<&'static str, String> {
    let rt = refresh_token.trim();
    let ak = api_key.trim();
    if ak.is_empty() && rt.is_empty() {
        return Err("请提供 refreshToken 或 kiroApiKey".to_string());
    }
    if !ak.is_empty() && !ak.starts_with("ksk_") {
        return Err("kiroApiKey 必须以 ksk_ 开头".to_string());
    }
    if !ak.is_empty() {
        Ok("api_key")
    } else if !client_id.trim().is_empty() {
        Ok("idc")
    } else {
        Ok("social")
    }
}

/// 空白串归一为 None
fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

/// 字段说明行
fn field_hint(label: &'static str, hint: &'static str) -> impl IntoElement {
    div()
        .v_flex()
        .child(div().text_sm().font_semibold().child(label))
        .child(div().text_xs().child(hint))
}

/// 带标签的输入行
fn labeled_row(label: &'static str, input: impl IntoElement) -> impl IntoElement {
    div().v_flex().gap_1().child(div().text_sm().child(label)).child(input)
}

fn push_err(window: &mut Window, cx: &mut App, msg: &str) {
    window.push_notification(Notification::error(msg.to_string()), cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    // 任务 5.4：表单校验（纯函数）

    #[::core::prelude::v1::test]
    fn validate_requires_token_or_key() {
        assert_eq!(validate_add_input("", "", "").unwrap_err(), "请提供 refreshToken 或 kiroApiKey");
    }

    #[::core::prelude::v1::test]
    fn validate_api_key_needs_ksk_prefix() {
        assert_eq!(validate_add_input("", "abc123", "").unwrap_err(), "kiroApiKey 必须以 ksk_ 开头");
        assert_eq!(validate_add_input("", "ksk_abc", "").unwrap(), "api_key");
    }

    #[::core::prelude::v1::test]
    fn validate_infers_auth_method() {
        assert_eq!(validate_add_input("rt-x", "", "").unwrap(), "social");
        assert_eq!(validate_add_input("rt-x", "", "cid").unwrap(), "idc");
        // 同时给了 key 时 api_key 优先
        assert_eq!(validate_add_input("rt-x", "ksk_a", "cid").unwrap(), "api_key");
    }

    #[::core::prelude::v1::test]
    fn non_empty_trims() {
        assert_eq!(non_empty("  ".into()), None);
        assert_eq!(non_empty(" a ".into()), Some("a".into()));
    }
}
