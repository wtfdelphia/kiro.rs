//! `GET /v1/responses` WebSocket ingress（http_bridge 模式）
//!
//! 客户端保持一条 WS 连接完成多轮对话，每个 `response.create` 翻译为对既有
//! 上游 HTTP/SSE 链路的一次调用。设计：`docs/websocket-support-optimization-design.md`
//! §4；规格：`openspec/changes/add-responses-websocket-ingress/specs/openai-responses-websocket/spec.md`。
//!
//! 关键不变量：
//! - 所有握手拒绝（426/401/503/429/501）发生在 upgrade 之前；
//! - `wrote_downstream` 是「事件表达 vs 关闭连接」的唯一分界；
//! - 超时 / 帧上限在每个等待边界重读最新设置快照（热加载语义）。

use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, State};
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::anthropic::AppState;

use super::error::{OpenAiErrorBody, OpenAiErrorDetail};
use super::handlers::{ResponsesTurnStart, start_responses_stream_turn};
use super::responses_stream::ResponsesSseEvent;
use super::responses_types::ResponsesRequest;
use super::ws_error::{
    CLOSE_GOING_AWAY, CLOSE_MESSAGE_TOO_BIG, TurnStage, WsTurnError,
};
use super::ws_transport::{
    WsHandshakeReject, WsSessionContext, resolve_mode, resolve_transport,
};

type WsTx = SplitSink<WebSocket, Message>;
type WsRx = SplitStream<WebSocket>;

// === 握手与准入（全部在 upgrade 之前拒绝）===

/// `GET /v1/responses`：OpenAI Responses WebSocket ingress
pub async fn get_responses_ws(State(state): State<AppState>, req: Request<Body>) -> Response {
    let (mut parts, _body) = req.into_parts();

    // 非 upgrade 请求 → 426（spec：WS 端点与握手准入）
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(u) => u,
        Err(_) => {
            return ws_reject(
                StatusCode::UPGRADE_REQUIRED,
                "invalid_request_error",
                "GET /v1/responses requires a WebSocket upgrade (send `Upgrade: websocket`)",
                None,
            );
        }
    };

    // beta 头观测日志（design §5.1）：Codex 可能带 openai-beta: responses_websockets=...
    if let Some(beta) = parts.headers.get("openai-beta").and_then(|v| v.to_str().ok()) {
        tracing::debug!(openai_beta = %beta, "WS ingress: 收到 openai-beta 头");
    }

    // 设置快照：enabled → 模式路由 → 准入（顺序保证 passthrough 不占名额）
    let snapshot = state.ws_settings_snapshot();
    if !snapshot.enabled {
        return ws_reject(
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "WebSocket ingress is disabled (websocket.enabled=false)",
            Some(5),
        );
    }
    let transport = match resolve_transport(&snapshot) {
        Ok(t) => t,
        Err(WsHandshakeReject::PassthroughNotImplemented) => {
            return ws_reject(
                StatusCode::NOT_IMPLEMENTED,
                "server_error",
                "websocket.mode=passthrough is reserved and not implemented; switch to http_bridge",
                None,
            );
        }
    };
    let Some(guard) = state.ws_admission_guard(snapshot.max_connections) else {
        return ws_reject(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit_error",
            "WebSocket connection limit reached",
            Some(5),
        );
    };

    // 模式在建连时冻结，后续热加载不影响本连接（§4.7）
    let mode = resolve_mode(&snapshot);
    let ctx = WsSessionContext {
        mode,
        app_state: state.clone(),
        ws_settings: state.ws_settings.clone(),
        shutdown_rx: state.ws_shutdown.subscribe(),
    };
    let max_message_bytes = snapshot.max_message_bytes;

    tracing::info!(mode = ?mode, "WS ingress: 新连接升级");

    upgrade
        // codec 层背停：握手时冻结；热加载的帧上限在会话循环里按快照复查
        .max_message_size(max_message_bytes)
        .on_upgrade(move |socket| async move {
            // guard 无论会话如何结束都归还准入计数（Drop，task 6.7）
            let _guard = guard;
            transport.run_session(socket, ctx).await;
            tracing::debug!("WS ingress: 连接关闭");
        })
}

/// 握手期 JSON 错误响应（OpenAI error shape + 可选 Retry-After）
fn ws_reject(
    status: StatusCode,
    error_type: &'static str,
    message: &str,
    retry_after_secs: Option<u64>,
) -> Response {
    let body = OpenAiErrorBody {
        error: OpenAiErrorDetail {
            message: message.to_string(),
            error_type,
            code: None,
        },
    };
    let mut resp = (status, Json(body)).into_response();
    if let Some(secs) = retry_after_secs {
        if let Ok(v) = secs.to_string().try_into() {
            resp.headers_mut().insert(header::RETRY_AFTER, v);
        }
    }
    resp
}

// === 会话循环 ===

/// 等待一帧客户端消息的结局
enum FrameWait {
    /// 数据帧（Text 直接取；Binary 按 UTF-8 解析成功）
    Payload(String),
    /// Binary 帧非合法 UTF-8
    BadEncoding,
    /// 客户端发 Close 或连接已结束
    Ended,
    Timeout,
    Shutdown,
}

/// 等待下一帧；Ping/Pong 由 tungstenite 在读时自动处理，这里跳过。
/// `timeout=None` 表示不限时（活跃 turn 内的等待）。
async fn wait_client_frame(
    rx: &mut WsRx,
    shutdown_rx: &mut broadcast::Receiver<()>,
    timeout: Option<Duration>,
) -> FrameWait {
    // 截止时间不随 ping/pong 重置：用绝对 instant 而非每次重建 sleep
    let deadline = timeout.map(|d| tokio::time::Instant::now() + d);
    loop {
        let sleep = async {
            match deadline {
                Some(dl) => tokio::time::sleep_until(dl).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            biased;
            res = shutdown_rx.recv() => {
                // Ok / Lagged / Closed 都按进入 shutdown 处理（信号只发一次）
                let _ = res;
                return FrameWait::Shutdown;
            }
            frame = rx.next() => {
                return match frame {
                    Some(Ok(Message::Text(t))) => FrameWait::Payload(t.as_str().to_owned()),
                    Some(Ok(Message::Binary(b))) => match String::from_utf8(b.to_vec()) {
                        Ok(s) => FrameWait::Payload(s),
                        Err(_) => FrameWait::BadEncoding,
                    },
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
                    Some(Ok(Message::Close(_))) | None => FrameWait::Ended,
                    Some(Err(e)) => {
                        tracing::debug!(error = %e, "WS ingress: 读帧错误，结束会话");
                        FrameWait::Ended
                    }
                };
            }
            _ = sleep, if deadline.is_some() => return FrameWait::Timeout,
        }
    }
}

/// http_bridge 会话循环：首帧契约 → turn 循环 → 连接保护
///
/// 由 `HttpBridgeTransport::run_session` 调用（模式已在握手时冻结）。
pub(super) async fn run_http_bridge_session(socket: WebSocket, ctx: WsSessionContext) {
    let (mut tx, mut rx) = socket.split();
    let mut shutdown_rx = ctx.shutdown_rx.resubscribe();
    let mut session_model: Option<String> = None;
    tracing::debug!(mode = ?ctx.mode, "WS ingress: 会话开始（模式建连冻结）");

    // --- 首帧契约（独立超时窗口，读最新快照；0 保护下限 1s，防误配立即断开新连接）---
    let first_timeout = Duration::from_secs(
        ctx.settings_snapshot()
            .client_first_message_timeout_seconds
            .max(1),
    );
    let first = wait_client_frame(&mut rx, &mut shutdown_rx, Some(first_timeout)).await;
    let first_payload = match first {
        FrameWait::Payload(p) => p,
        FrameWait::BadEncoding => {
            protocol_violation(&mut tx, "first frame must be valid UTF-8").await;
            return;
        }
        FrameWait::Timeout => {
            protocol_violation(&mut tx, "client first message timeout").await;
            return;
        }
        FrameWait::Shutdown => {
            close_with(&mut tx, CLOSE_GOING_AWAY, "server shutting down").await;
            return;
        }
        FrameWait::Ended => return,
    };

    if dispatch_and_check(
        &mut tx,
        &mut rx,
        &mut shutdown_rx,
        &ctx,
        &mut session_model,
        &first_payload,
        true,
    )
    .await
    {
        return;
    }

    // --- turn 循环：终态后等待下一帧 ---
    loop {
        // 空闲超时在每个等待边界重读快照（热加载语义，0=关闭）
        let idle_secs = ctx.settings_snapshot().inter_turn_idle_timeout_seconds;
        let idle = (idle_secs > 0).then_some(Duration::from_secs(idle_secs));
        match wait_client_frame(&mut rx, &mut shutdown_rx, idle).await {
            FrameWait::Payload(p) => {
                // 帧上限热复查（codec 层另有握手时冻结的背停上限）
                if p.len() > ctx.settings_snapshot().max_message_bytes {
                    close_with(&mut tx, CLOSE_MESSAGE_TOO_BIG, "message too large").await;
                    return;
                }
                if dispatch_and_check(
                    &mut tx,
                    &mut rx,
                    &mut shutdown_rx,
                    &ctx,
                    &mut session_model,
                    &p,
                    false,
                )
                .await
                {
                    return;
                }
            }
            FrameWait::BadEncoding => {
                protocol_violation(&mut tx, "frame must be valid UTF-8").await;
                return;
            }
            FrameWait::Timeout => {
                close_with(&mut tx, CLOSE_GOING_AWAY, "inter-turn idle timeout").await;
                return;
            }
            FrameWait::Shutdown => {
                close_with(&mut tx, CLOSE_GOING_AWAY, "server shutting down").await;
                return;
            }
            FrameWait::Ended => return,
        }
    }
}

/// 协议违规的统一出口：先写一条 `error` 事件，再以 1008 关闭（design §4.4）
async fn protocol_violation(tx: &mut WsTx, message: &str) {
    let _ = write_events(tx, vec![error_event("invalid_request_error", message)]).await;
    // 关闭码经 WsTurnError 分类派生（Prepare 阶段 → 1008）
    let classified = WsTurnError::turn(TurnStage::Prepare, anyhow::anyhow!("{}", message), false);
    close_with(tx, classified.close_code(), message).await;
}

/// 执行帧分派并把结局折算为会话级动作；返回 true 表示会话应结束。
/// shutdown 结局在退出前以 1001 关闭（spec：优雅关闭）。
async fn dispatch_and_check(
    tx: &mut WsTx,
    rx: &mut WsRx,
    shutdown_rx: &mut broadcast::Receiver<()>,
    ctx: &WsSessionContext,
    session_model: &mut Option<String>,
    payload: &str,
    is_first: bool,
) -> bool {
    match dispatch_frame(tx, rx, shutdown_rx, ctx, session_model, payload, is_first).await {
        SessionFlow::Continue => false,
        SessionFlow::End => true,
        SessionFlow::Shutdown => {
            close_with(tx, CLOSE_GOING_AWAY, "server shutting down").await;
            true
        }
    }
}

// === 帧分派 ===

#[derive(Clone, Copy, PartialEq)]
enum SessionFlow {
    Continue,
    End,
    /// 进程优雅 shutdown：结束前以 1001 关闭连接
    Shutdown,
}


/// 解析并处理一帧；首帧与后续帧共用，`is_first` 决定违规时的严格程度
async fn dispatch_frame(
    tx: &mut WsTx,
    rx: &mut WsRx,
    shutdown_rx: &mut broadcast::Receiver<()>,
    ctx: &WsSessionContext,
    session_model: &mut Option<String>,
    payload: &str,
    is_first: bool,
) -> SessionFlow {
    // 帧上限热复查（首帧同样适用）
    if payload.len() > ctx.settings_snapshot().max_message_bytes {
        close_with(tx, CLOSE_MESSAGE_TOO_BIG, "message too large").await;
        return SessionFlow::End;
    }

    let value: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => {
            protocol_violation(tx, "frame is not valid JSON").await;
            return SessionFlow::End;
        }
    };
    let obj = match value.as_object() {
        Some(o) => o,
        None => {
            protocol_violation(tx, "frame must be a JSON object").await;
            return SessionFlow::End;
        }
    };

    // type 缺省按 response.create 处理（spec：首帧契约）
    let frame_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("response.create");

    match frame_type {
        "response.create" => {
            // model 解析：帧内 model → session.update 覆盖 → 拒绝
            let frame_model = obj
                .get("model")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let model = frame_model.or_else(|| session_model.clone());
            let Some(model) = model else {
                if is_first {
                    protocol_violation(tx, "first frame must carry a non-empty `model`").await;
                    return SessionFlow::End;
                }
                let _ = write_events(
                    tx,
                    vec![error_event(
                        "invalid_request_error",
                        "response.create missing `model` (no session.update override set)",
                    )],
                )
                .await;
                return SessionFlow::Continue;
            };

            // 组装 turn 请求：去掉 type，强制 stream=true，注入 model
            let mut request = value.clone();
            let obj_mut = request.as_object_mut().expect("上面已校验为对象");
            obj_mut.remove("type");
            obj_mut.insert("model".to_string(), json!(model));
            obj_mut.insert("stream".to_string(), json!(true));

            run_turn(tx, rx, shutdown_rx, ctx, session_model, request).await
        }
        "session.update" => {
            // 会话级 model 覆盖：兼容 {session:{model}} 与扁平 {model} 两种形状
            let new_model = obj
                .get("session")
                .and_then(|s| s.get("model"))
                .or_else(|| obj.get("model"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            if let Some(m) = new_model {
                tracing::debug!(model = %m, "WS ingress: session.update 记录 model 覆盖");
                *session_model = Some(m);
            }
            SessionFlow::Continue
        }
        "response.cancel" => {
            // 会话循环层不存在活跃 turn（turn 内的 cancel 在 pump 里处理）
            let _ = write_events(
                tx,
                vec![error_event("invalid_request_error", "no active turn to cancel")],
            )
            .await;
            SessionFlow::Continue
        }
        other => {
            if is_first {
                protocol_violation(
                    tx,
                    &format!("first frame must be response.create, got `{}`", other),
                )
                .await;
                return SessionFlow::End;
            }
            let _ = write_events(
                tx,
                vec![error_event(
                    "invalid_request_error",
                    format!("unsupported frame type `{}`", other),
                )],
            )
            .await;
            SessionFlow::Continue
        }
    }
}

// === turn 执行 ===

#[derive(Clone, Copy, PartialEq)]
enum TurnOutcome {
    /// turn 到达终态（事件已表达），连接继续
    Done,
    /// 进程优雅 shutdown
    Shutdown,
}

/// 执行一个 turn：上游阶段失败且未写出任何事件时重试一次（design §4）
async fn run_turn(
    tx: &mut WsTx,
    rx: &mut WsRx,
    shutdown_rx: &mut broadcast::Receiver<()>,
    ctx: &WsSessionContext,
    session_model: &mut Option<String>,
    request_value: Value,
) -> SessionFlow {
    let mut retried = false;
    loop {
        match run_turn_attempt(tx, rx, shutdown_rx, ctx, session_model, request_value.clone()).await
        {
            Ok(outcome) => {
                return match outcome {
                    TurnOutcome::Done => SessionFlow::Continue,
                    TurnOutcome::Shutdown => SessionFlow::Shutdown,
                };
            }
            Err(e) if e.is_retryable() && !retried => {
                retried = true;
                tracing::warn!(
                    stage = e.stage().map(|s| s.as_str()).unwrap_or("?"),
                    error = %e.message(),
                    "WS turn: 上游失败且未写出下游事件，换凭据重试一次"
                );
                continue;
            }
            Err(WsTurnError::ClientClose(close)) => {
                tracing::debug!(status = close.status, reason = %close.reason, "WS turn: 客户端已离开");
                return SessionFlow::End;
            }
            Err(e) => {
                // 不可重试（或重试后仍失败）：以 error 事件表达，连接存活
                tracing::warn!(
                    stage = e.stage().map(|s| s.as_str()).unwrap_or("?"),
                    error = %e.message(),
                    "WS turn: 失败，以 error 事件表达"
                );
                if write_events(tx, vec![error_event("server_error", e.message())])
                    .await
                    .is_err()
                {
                    return SessionFlow::End;
                }
                return SessionFlow::Continue;
            }
        }
    }
}

/// 单次 turn 尝试：仅在「未写出任何下游事件」时以 Err 返回（交给外层决定重试）；
/// 一旦写出事件，所有失败都在内部以事件表达并返回 Ok。
async fn run_turn_attempt(
    tx: &mut WsTx,
    rx: &mut WsRx,
    shutdown_rx: &mut broadcast::Receiver<()>,
    ctx: &WsSessionContext,
    session_model: &mut Option<String>,
    request_value: Value,
) -> Result<TurnOutcome, WsTurnError> {
    let req: ResponsesRequest = serde_json::from_value(request_value)
        .map_err(|e| WsTurnError::turn(TurnStage::Prepare, e.into(), false))?;

    // 与 POST /v1/responses（stream=true）共用 turn 构建（tasks 3.1）
    let turn = start_responses_stream_turn(&ctx.app_state, req)
        .await
        .map_err(|e| {
            WsTurnError::turn(
                TurnStage::Prepare,
                anyhow::anyhow!("{}", e.message()),
                false,
            )
        })?;

    match turn {
        ResponsesTurnStart::WebSearch { events } => {
            write_events(tx, events).await.map_err(|_| {
                WsTurnError::turn(
                    TurnStage::WriteClient,
                    anyhow::anyhow!("client write failed"),
                    false,
                )
            })?;
            Ok(TurnOutcome::Done)
        }
        ResponsesTurnStart::Upstream {
            mut source,
            body,
            provider,
        } => {
            // 上游调用发起：失败时未写任何事件，外层可重试一次（MultiTokenManager 换凭据）
            let response = provider
                .call_api_stream(&body)
                .await
                .map_err(|e| WsTurnError::turn(TurnStage::CallUpstream, e, false))?;

            // 上游直接返回非 2xx：尚未写出任何事件，按上游阶段失败处理（外层可重试）
            if !response.status().is_success() {
                return Err(WsTurnError::turn(
                    TurnStage::ReadUpstream,
                    anyhow::anyhow!("upstream returned HTTP {}", response.status()),
                    false,
                ));
            }

            // 开场事件写出后，downstream 视为已开始
            let initial = source.initial_events();
            write_events(tx, initial).await.map_err(|_| {
                WsTurnError::turn(
                    TurnStage::WriteClient,
                    anyhow::anyhow!("client write failed"),
                    false,
                )
            })?;

            // 事件泵：失败全部事件化，连接存活
            pump_upstream(tx, rx, shutdown_rx, ctx, session_model, source, response).await
        }
    }
}

/// 事件泵：上游流 / 客户端帧（cancel、重叠 create、session.update）/ 读超时 / shutdown
///
/// `Err(WsTurnError::ClientClose)` 表示客户端已离开；其余失败一律事件化后返回 `Ok`。
async fn pump_upstream(
    tx: &mut WsTx,
    rx: &mut WsRx,
    shutdown_rx: &mut broadcast::Receiver<()>,
    ctx: &WsSessionContext,
    session_model: &mut Option<String>,
    mut source: super::handlers::ResponsesEventSource,
    response: reqwest::Response,
) -> Result<TurnOutcome, WsTurnError> {
    let mut body_stream = response.bytes_stream();

    // 客户端离开的统一表达（1005 仅作诊断标记，不会用于发送）
    let gone = |reason: &str| WsTurnError::client_close(1005, reason.to_string());

    // 读超时只计「无上游 chunk 到达」：客户端帧（session.update / 重叠 create 等）
    // 不得重置计时，否则卡死的 turn 可被客户端流量无限续命
    let mut last_chunk_at = tokio::time::Instant::now();

    loop {
        // 上游读超时在每个等待边界重读最新快照（0 保护下限 1s）；
        // deadline 由最近一次上游 chunk 时间推导，热更新即时生效
        let deadline = last_chunk_at
            + Duration::from_secs(
                ctx.settings_snapshot()
                    .upstream_read_timeout_seconds
                    .max(1),
            );
        tokio::select! {
            biased;
            res = shutdown_rx.recv() => {
                let _ = res;
                return Ok(TurnOutcome::Shutdown);
            }
            frame = rx.next() => {
                match frame {
                    Some(Ok(Message::Text(t))) => {
                        match handle_in_turn_frame(tx, session_model, &mut source, t.as_str()).await {
                            Ok(InTurnControl::Continue) => {}
                            Ok(InTurnControl::TurnCancelled) => return Ok(TurnOutcome::Done),
                            Err(()) => return Err(gone("client write failed during turn")),
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        match String::from_utf8(b.to_vec()) {
                            Ok(s) => {
                                match handle_in_turn_frame(tx, session_model, &mut source, &s).await {
                                    Ok(InTurnControl::Continue) => {}
                                    Ok(InTurnControl::TurnCancelled) => return Ok(TurnOutcome::Done),
                                    Err(()) => return Err(gone("client write failed during turn")),
                                }
                            }
                            Err(_) => {
                                // 活跃 turn 内的非法编码帧：报 error 事件不关连接
                                if write_events(tx, vec![error_event("invalid_request_error", "frame must be valid UTF-8")]).await.is_err() {
                                    return Err(gone("client write failed during turn"));
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(cf))) => {
                        let (status, reason) = match cf {
                            Some(f) => (f.code, f.reason.as_str().to_string()),
                            None => (1005, "client closed".to_string()),
                        };
                        return Err(WsTurnError::client_close(status, reason));
                    }
                    None => return Err(gone("client stream ended")),
                    Some(Err(e)) => return Err(gone(&format!("client read error: {}", e))),
                }
            }
            chunk = body_stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        last_chunk_at = tokio::time::Instant::now();
                        let events = source.feed(&bytes);
                        if !events.is_empty() && write_events(tx, events).await.is_err() {
                            return Err(gone("client write failed during turn"));
                        }
                    }
                    Some(Err(e)) => {
                        // 已开始输出则走 failed 事件（与 SSE 侧同语义）
                        let events = source.fail(format!("upstream stream error: {}", e));
                        if write_events(tx, events).await.is_err() {
                            return Err(gone("client write failed during turn"));
                        }
                        return Ok(TurnOutcome::Done);
                    }
                    None => {
                        let events = source.finish();
                        if write_events(tx, events).await.is_err() {
                            return Err(gone("client write failed during turn"));
                        }
                        return Ok(TurnOutcome::Done);
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                tracing::warn!("WS turn: 上游读超时，以 response.failed 事件结束本 turn");
                let events = source.fail("upstream read timeout".to_string());
                if write_events(tx, events).await.is_err() {
                    return Err(gone("client write failed during turn"));
                }
                return Ok(TurnOutcome::Done);
            }
        }
    }
}

/// 活跃 turn 内客户端帧的处理结果
enum InTurnControl {
    Continue,
    TurnCancelled,
}

/// 活跃 turn 内的客户端帧：cancel / 重叠 create / session.update / 其他
///
/// 返回 `Err(())` 表示客户端已离开。
async fn handle_in_turn_frame(
    tx: &mut WsTx,
    session_model: &mut Option<String>,
    source: &mut super::handlers::ResponsesEventSource,
    payload: &str,
) -> Result<InTurnControl, ()> {
    let value: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => {
            write_events(
                tx,
                vec![error_event("invalid_request_error", "frame is not valid JSON")],
            )
            .await
            .map_err(|_| ())?;
            return Ok(InTurnControl::Continue);
        }
    };
    let frame_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("response.create");

    match frame_type {
        "response.cancel" => {
            // 停止当前 turn：cancelled 终态事件写回后 drop 上游流
            let events = source.cancel();
            write_events(tx, events).await.map_err(|_| ())?;
            Ok(InTurnControl::TurnCancelled)
        }
        "response.create" => {
            // 重叠 create：回 error 事件，不关连接（与 sub2api 文案一致）
            write_events(
                tx,
                vec![error_event(
                    "invalid_request_error",
                    "overlapping response.create is not supported",
                )],
            )
            .await
            .map_err(|_| ())?;
            Ok(InTurnControl::Continue)
        }
        "session.update" => {
            let new_model = value
                .get("session")
                .and_then(|s| s.get("model"))
                .or_else(|| value.get("model"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            if let Some(m) = new_model {
                *session_model = Some(m);
            }
            Ok(InTurnControl::Continue)
        }
        other => {
            write_events(
                tx,
                vec![error_event(
                    "invalid_request_error",
                    format!("unsupported frame type `{}` during active turn", other),
                )],
            )
            .await
            .map_err(|_| ())?;
            Ok(InTurnControl::Continue)
        }
    }
}

// === 输出辅助 ===

/// WS sink：事件 JSON（SSE 的 data 部分）作为 text 帧发送；
/// Done / Keepalive 不下发（协议级 ping/pong 替代，design §4.5）
async fn write_events(tx: &mut WsTx, events: Vec<ResponsesSseEvent>) -> Result<(), ()> {
    for event in events {
        let ResponsesSseEvent::Named { data, .. } = event else {
            continue;
        };
        if tx.send(Message::Text(data.to_string().into())).await.is_err() {
            return Err(());
        }
    }
    Ok(())
}

/// 协议层 `error` 事件（区别于 turn 内的 `response.failed`）
fn error_event(code: &str, message: impl std::fmt::Display) -> ResponsesSseEvent {
    ResponsesSseEvent::named(
        "error",
        json!({
            "type": "error",
            "code": code,
            "message": message.to_string(),
        }),
    )
}

/// 发送 Close 帧并关闭 sink；对端可能已离开，错误忽略
async fn close_with(tx: &mut WsTx, code: u16, reason: &str) {
    let frame = CloseFrame {
        code,
        reason: reason.to_string().into(),
    };
    let _ = tx.send(Message::Close(Some(frame))).await;
    let _ = SinkExt::close(tx).await;
}

#[cfg(test)]
mod ws_integration {
    //! WS ingress 集成测试：真实 TCP + tokio-tungstenite 客户端 + mock 上游
    //!
    //! mock 上游返回合法 AWS event-stream：先发一条 assistantResponseEvent，
    //! 再挂起 `hold_ms` 后结束——挂起窗口内 turn 保持活跃，可测 cancel/重叠 create。

    use super::*;
    use crate::anthropic::AppState;
    use crate::kiro::endpoint::{KiroEndpoint, RequestContext};
    use crate::kiro::model::credentials::KiroCredentials;
    use crate::kiro::parser::crc::crc32;
    use crate::kiro::provider::KiroProvider;
    use crate::kiro::token_manager::MultiTokenManager;
    use crate::model::config::{Config, WsSettings, WsTransportMode};
    use axum::body::Body as AxumBody;
    use axum::response::Response as AxumResponse;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio_tungstenite::tungstenite::Message as TMessage;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::{MaybeTlsStream, connect_async};

    type WsClient = tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

    // === mock 上游 ===

    /// 构造合法的 AWS event-stream 帧（与 handlers::ws_parity_tests 同一编码）
    fn encode_event_frame(event_type: &str, payload_json: &str) -> bytes::Bytes {
        fn push_str_header(buf: &mut Vec<u8>, name: &str, value: &str) {
            buf.push(name.len() as u8);
            buf.extend_from_slice(name.as_bytes());
            buf.push(7); // HeaderValueType::String
            buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
            buf.extend_from_slice(value.as_bytes());
        }
        let mut headers = Vec::new();
        push_str_header(&mut headers, ":message-type", "event");
        push_str_header(&mut headers, ":event-type", event_type);
        push_str_header(&mut headers, ":content-type", "application/json");
        let payload = payload_json.as_bytes();
        let total = 12 + headers.len() + payload.len() + 4;
        let mut msg = Vec::new();
        msg.extend_from_slice(&(total as u32).to_be_bytes());
        msg.extend_from_slice(&(headers.len() as u32).to_be_bytes());
        let prelude_crc = crc32(&msg[..8]);
        msg.extend_from_slice(&prelude_crc.to_be_bytes());
        msg.extend_from_slice(&headers);
        msg.extend_from_slice(payload);
        let message_crc = crc32(&msg);
        msg.extend_from_slice(&message_crc.to_be_bytes());
        bytes::Bytes::from(msg)
    }

    /// 启动 mock 上游：首个事件后立即可解码，`hold_ms` 后流结束
    async fn spawn_mock_upstream(hold_ms: u64) -> String {
        use axum::routing::post;
        let app = axum::Router::new().route(
            "/generate",
            post(move || async move {
                let first = encode_event_frame("assistantResponseEvent", r#"{"content":"hi"}"#);
                // 首帧放进 unfold 状态里，避免 FnMut 闭包捕获移动
                let stream =
                    futures::stream::unfold((0u8, Some(first)), move |(state, frame)| async move {
                        match state {
                            0 => Some((
                                Ok::<_, std::convert::Infallible>(frame.expect("状态机保证")),
                                (1u8, None),
                            )),
                            1 => {
                                tokio::time::sleep(Duration::from_millis(hold_ms)).await;
                                None
                            }
                            _ => None,
                        }
                    });
                AxumResponse::builder()
                    .status(200)
                    .header("content-type", "application/vnd.amazon.eventstream")
                    .body(AxumBody::from_stream(stream))
                    .expect("构造 mock 响应失败")
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind 失败");
        let addr = listener.local_addr().expect("local_addr 失败");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://127.0.0.1:{}", addr.port())
    }

    /// 指向 mock 上游的测试端点
    struct MockEndpoint {
        base: String,
    }

    impl KiroEndpoint for MockEndpoint {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn api_url(&self, _ctx: &RequestContext<'_>) -> String {
            format!("{}/generate", self.base)
        }
        fn mcp_url(&self, _ctx: &RequestContext<'_>) -> String {
            format!("{}/mcp", self.base)
        }
        fn decorate_api(
            &self,
            req: reqwest::RequestBuilder,
            ctx: &RequestContext<'_>,
        ) -> reqwest::RequestBuilder {
            req.bearer_auth(ctx.token)
        }
        fn decorate_mcp(
            &self,
            req: reqwest::RequestBuilder,
            ctx: &RequestContext<'_>,
        ) -> reqwest::RequestBuilder {
            req.bearer_auth(ctx.token)
        }
        fn transform_api_body(&self, body: &str, _ctx: &RequestContext<'_>) -> String {
            body.to_string()
        }
    }

    /// api_key 凭据 + 预置 profileArn：token 解析与 profileArn 均不发真实请求
    fn local_credential() -> KiroCredentials {
        serde_json::from_value(json!({
            "authMethod": "api_key",
            "kiroApiKey": "test-key",
            "profileArn": "arn:aws:codewhisperer:us-east-1:123456789012:profile/test"
        }))
        .expect("测试凭据构造失败")
    }

    async fn state_with_mock_upstream(ws: WsSettings, hold_ms: u64) -> AppState {
        let base = spawn_mock_upstream(hold_ms).await;
        let tm = MultiTokenManager::new(
            Config::default(),
            vec![local_credential()],
            None,
            None,
            false,
        )
        .expect("token manager 构造失败");
        let tm = Arc::new(tm);
        let mut endpoints: HashMap<String, Arc<dyn KiroEndpoint>> = HashMap::new();
        endpoints.insert("mock".to_string(), Arc::new(MockEndpoint { base }));
        let provider = KiroProvider::with_proxy(tm, None, endpoints, "mock".to_string());
        let state = AppState::new("test-key", true)
            .with_auth_runtime(false, "test-key")
            .with_kiro_provider_arc(Arc::new(provider));
        state.set_ws_settings(ws);
        state
    }

    fn state_without_provider(ws: WsSettings) -> AppState {
        let state = AppState::new("test-key", true).with_auth_runtime(false, "test-key");
        state.set_ws_settings(ws);
        state
    }

    async fn spawn_ws_server(state: AppState) -> String {
        let app = crate::openai::create_openai_routes(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind 失败");
        let addr = listener.local_addr().expect("local_addr 失败");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("ws://127.0.0.1:{}/v1/responses", addr.port())
    }

    // === 客户端辅助 ===

    async fn ws_connect(url: &str) -> WsClient {
        let req = url.into_client_request().expect("构造 WS 请求失败");
        let (ws, resp) = connect_async(req).await.expect("WS 连接失败");
        assert_eq!(resp.status().as_u16(), 101, "必须升级成功");
        ws
    }

    /// 带 upgrade 头的普通 HTTP GET（用于断言 upgrade 前的拒绝）
    async fn http_upgrade_attempt(ws_url: &str) -> reqwest::Response {
        let http_url = ws_url.replacen("ws://", "http://", 1);
        reqwest::Client::new()
            .get(&http_url)
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .expect("HTTP 请求失败")
    }

    /// 读下一个 text 帧并解析为 JSON（跳过 ping/pong）
    async fn next_json(ws: &mut WsClient) -> Value {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .expect("等待帧超时")
                .expect("连接已结束")
                .expect("读帧错误");
            match frame {
                TMessage::Text(t) => return serde_json::from_str(&t).expect("帧非合法 JSON"),
                TMessage::Ping(_) | TMessage::Pong(_) => continue,
                other => panic!("意外的帧类型: {:?}", other),
            }
        }
    }

    /// 读到 Close 帧并返回关闭码；连接被直接重置/断开时按 1006（异常关闭）计
    async fn next_close_code(ws: &mut WsClient) -> u16 {
        loop {
            let item = tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .expect("等待关闭帧超时");
            let frame = match item {
                Some(Ok(f)) => f,
                Some(Err(_)) | None => return 1006,
            };
            match frame {
                TMessage::Close(cf) => return cf.map(|f| f.code.into()).unwrap_or(1005),
                TMessage::Ping(_) | TMessage::Pong(_) => continue,
                TMessage::Text(_) | TMessage::Binary(_) => continue,
                other => panic!("意外的帧类型: {:?}", other),
            }
        }
    }

    /// 标准 response.create 帧
    fn create_frame(model: &str) -> String {
        json!({
            "type": "response.create",
            "model": model,
            "input": "hello"
        })
        .to_string()
    }

    /// 断言事件序列到达终态（completed），返回事件类型序列
    async fn drain_until_terminal(ws: &mut WsClient) -> Vec<String> {
        let mut types = Vec::new();
        loop {
            let v = next_json(ws).await;
            let t = v["type"].as_str().unwrap_or("").to_string();
            types.push(t.clone());
            if matches!(
                t.as_str(),
                "response.completed" | "response.failed" | "response.incomplete" | "response.cancelled"
            ) {
                return types;
            }
        }
    }

    /// 从拆分后的读半部读帧直至终态，返回终态事件类型
    async fn drain_split_until_terminal(rx: &mut SplitStream<WsClient>) -> String {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(8), rx.next())
                .await
                .expect("等待终态超时")
                .expect("连接已结束")
                .expect("读帧错误");
            match frame {
                TMessage::Text(t) => {
                    let v: Value = serde_json::from_str(&t).expect("帧非合法 JSON");
                    let ty = v["type"].as_str().unwrap_or("").to_string();
                    if matches!(
                        ty.as_str(),
                        "response.completed"
                            | "response.failed"
                            | "response.incomplete"
                            | "response.cancelled"
                    ) {
                        return ty;
                    }
                }
                TMessage::Ping(_) | TMessage::Pong(_) => {}
                other => panic!("意外的帧类型: {:?}", other),
            }
        }
    }

    // === 握手准入（任务 6.2 / 4.3，全部在 upgrade 前）===

    /// 任务 6.2：enabled=false → 503 + Retry-After，不升级
    #[tokio::test]
    async fn disabled_rejected_503_before_upgrade() {
        let ws = WsSettings {
            enabled: false,
            ..Default::default()
        };
        let url = spawn_ws_server(state_without_provider(ws)).await;
        let resp = http_upgrade_attempt(&url).await;
        assert_eq!(resp.status().as_u16(), 503);
        assert!(resp.headers().contains_key("retry-after"), "应带 Retry-After");
    }

    /// 任务 4.3：mode=passthrough → upgrade 前 501 JSON 错误
    #[tokio::test]
    async fn passthrough_rejected_501_before_upgrade() {
        let ws = WsSettings {
            mode: WsTransportMode::Passthrough,
            ..Default::default()
        };
        let url = spawn_ws_server(state_without_provider(ws)).await;
        let resp = http_upgrade_attempt(&url).await;
        assert_eq!(resp.status().as_u16(), 501);
        let body: Value = resp.json().await.expect("响应非 JSON");
        assert_eq!(body["error"]["type"], "server_error");
        assert!(body["error"]["message"].as_str().unwrap().contains("passthrough"));
    }

    /// 任务 6.2 + 6.7：容量满 → 429；客户端断开后名额归还、可再入
    #[tokio::test]
    async fn capacity_full_rejected_429_and_released_on_disconnect() {
        let ws = WsSettings {
            max_connections: 1,
            ..Default::default()
        };
        let state = state_with_mock_upstream(ws, 0).await;
        let admission = state.ws_admission.clone();
        let url = spawn_ws_server(state).await;

        let client = ws_connect(&url).await;
        assert_eq!(admission.active(), 1, "连接建立后计数应为 1");

        let resp = http_upgrade_attempt(&url).await;
        assert_eq!(resp.status().as_u16(), 429, "容量满必须 429");
        assert!(resp.headers().contains_key("retry-after"));

        drop(client);
        // 等待服务端感知断开并归还名额
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(admission.active(), 0, "客户端断开必须归还计数");

        let _again = ws_connect(&url).await;
        assert_eq!(admission.active(), 1, "归还后应可再入");
    }

    // === 首帧契约（任务 6.3）===

    /// 首帧超时：error 事件 + 1008 关闭
    #[tokio::test]
    async fn first_frame_timeout_closes_1008() {
        let ws = WsSettings {
            client_first_message_timeout_seconds: 1,
            ..Default::default()
        };
        let url = spawn_ws_server(state_without_provider(ws)).await;
        let mut client = ws_connect(&url).await;
        // 不发首帧：先收到 error 事件，再收到 1008 关闭
        let err = next_json(&mut client).await;
        assert_eq!(err["type"], "error", "超时前应先写 error 事件");
        assert_eq!(next_close_code(&mut client).await, 1008);
    }

    /// 首帧非法 JSON：error 事件 + 1008 关闭
    #[tokio::test]
    async fn invalid_json_first_frame_error_then_1008() {
        let url = spawn_ws_server(state_without_provider(WsSettings::default())).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text("not json".into()))
            .await
            .expect("发送失败");
        let err = next_json(&mut client).await;
        assert_eq!(err["type"], "error");
        assert!(err["message"].as_str().unwrap().contains("JSON"));
        assert_eq!(next_close_code(&mut client).await, 1008);
    }

    /// 首帧缺 model：error 事件 + 1008 关闭
    #[tokio::test]
    async fn missing_model_first_frame_error_then_1008() {
        let url = spawn_ws_server(state_without_provider(WsSettings::default())).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(r#"{"input":"hi"}"#.into()))
            .await
            .expect("发送失败");
        let err = next_json(&mut client).await;
        assert_eq!(err["type"], "error");
        assert!(err["message"].as_str().unwrap().contains("model"));
        assert_eq!(next_close_code(&mut client).await, 1008);
    }

    /// type 缺省按 response.create 处理：完整 turn 直至 completed
    #[tokio::test]
    async fn missing_type_defaults_to_response_create() {
        let url = spawn_ws_server(state_with_mock_upstream(WsSettings::default(), 0).await).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(
                json!({"model": "claude-sonnet-4.5", "input": "hi"}).to_string().into(),
            ))
            .await
            .expect("发送失败");
        let types = drain_until_terminal(&mut client).await;
        assert!(types.contains(&"response.created".to_string()), "{:?}", types);
        assert_eq!(types.last().map(String::as_str), Some("response.completed"));
    }

    // === turn 循环（任务 6.4 / 6.5）===

    /// 任务 6.4：多 turn 复用同一连接，各自完整到终态
    #[tokio::test]
    async fn multi_turn_reuses_connection() {
        let url = spawn_ws_server(state_with_mock_upstream(WsSettings::default(), 0).await).await;
        let mut client = ws_connect(&url).await;

        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        let first = drain_until_terminal(&mut client).await;
        assert_eq!(first.last().map(String::as_str), Some("response.completed"));

        // 第一个终态后连接必须仍然可用
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("第二个 turn 发送失败（连接已断？）");
        let second = drain_until_terminal(&mut client).await;
        assert_eq!(second.last().map(String::as_str), Some("response.completed"));

        // 模型回显必须是客户端请求名（spec：http_bridge 事件等价）
        assert!(second.iter().any(|t| t == "response.created"));
    }

    /// 任务 6.5：重叠 response.create → error 事件，不关连接
    #[tokio::test]
    async fn overlapping_create_rejected_without_closing() {
        // hold 600ms：turn 活跃窗口内发第二个 create
        let url = spawn_ws_server(state_with_mock_upstream(WsSettings::default(), 600).await).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        // 等到开场事件出现（turn 已进泵），再发重叠 create
        let first = next_json(&mut client).await;
        assert_eq!(first["type"], "response.created");
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");

        // 随后应收到重叠 create 的 error 事件，且当前 turn 继续走完
        let mut saw_overlap_error = false;
        loop {
            let v = next_json(&mut client).await;
            match v["type"].as_str() {
                Some("error") => {
                    assert!(v["message"]
                        .as_str()
                        .unwrap()
                        .contains("overlapping response.create"));
                    saw_overlap_error = true;
                }
                Some("response.completed") => break,
                _ => {}
            }
        }
        assert!(saw_overlap_error, "必须收到重叠 create 的 error 事件");

        // 连接仍然存活：再跑一个 turn
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("连接应存活");
        let after = drain_until_terminal(&mut client).await;
        assert_eq!(after.last().map(String::as_str), Some("response.completed"));
    }

    /// 任务 6.5：response.cancel → 停止 turn 并回 response.cancelled，连接存活
    #[tokio::test]
    async fn cancel_stops_turn_and_connection_survives() {
        // hold 3s：足够长，确保 cancel 落在 turn 活跃期内
        let url = spawn_ws_server(state_with_mock_upstream(WsSettings::default(), 3000).await).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        let first = next_json(&mut client).await;
        assert_eq!(first["type"], "response.created");
        client
            .send(TMessage::Text(r#"{"type":"response.cancel"}"#.into()))
            .await
            .expect("发送失败");

        let types = drain_until_terminal(&mut client).await;
        assert_eq!(
            types.last().map(String::as_str),
            Some("response.cancelled"),
            "cancel 必须以 response.cancelled 收尾: {:?}",
            types
        );

        // 连接存活：下一个 turn 正常完成
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("连接应存活");
        let after = drain_until_terminal(&mut client).await;
        assert_eq!(after.last().map(String::as_str), Some("response.completed"));
    }

    /// 任务 6.5：session.update 的 model 覆盖被后续省略 model 的 create 使用
    #[tokio::test]
    async fn session_update_model_override_used_by_create() {
        let url = spawn_ws_server(state_with_mock_upstream(WsSettings::default(), 0).await).await;
        let mut client = ws_connect(&url).await;

        // 首帧必须带 model：先跑一个普通 turn
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        drain_until_terminal(&mut client).await;

        // session.update 记录 model 覆盖
        client
            .send(TMessage::Text(
                json!({"type":"session.update","session":{"model":"claude-haiku-4.5"}})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("发送失败");

        // 省略 model 的 create 使用 session model，且事件回显该名
        client
            .send(TMessage::Text(
                json!({"type":"response.create","input":"hi"}).to_string().into(),
            ))
            .await
            .expect("发送失败");
        let created = next_json(&mut client).await;
        assert_eq!(created["type"], "response.created", "实际帧: {}", created);
        assert_eq!(
            created["response"]["model"], "claude-haiku-4.5",
            "model 必须回显 session.update 的覆盖值（D9 请求原值）"
        );
        drain_until_terminal(&mut client).await;
    }

    /// turn 失败不毁掉会话：无 provider 时 turn 报 error 事件，连接存活
    #[tokio::test]
    async fn turn_failure_keeps_connection_alive() {
        let url = spawn_ws_server(state_without_provider(WsSettings::default())).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        let err = next_json(&mut client).await;
        assert_eq!(err["type"], "error", "失败必须以 error 事件表达");

        // 连接存活：再发一个 turn 仍能得到响应（同样是 error 事件）
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("连接应存活");
        let err2 = next_json(&mut client).await;
        assert_eq!(err2["type"], "error");
    }

    // === 连接保护（任务 6.6）===

    /// turn 间空闲超时 → 1001 关闭
    #[tokio::test]
    async fn inter_turn_idle_timeout_closes_1001() {
        let ws = WsSettings {
            inter_turn_idle_timeout_seconds: 1,
            ..Default::default()
        };
        let url = spawn_ws_server(state_with_mock_upstream(ws, 0).await).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        drain_until_terminal(&mut client).await;
        // 终态后不发新帧：1s 空闲后应收到 1001
        assert_eq!(next_close_code(&mut client).await, 1001);
    }

    /// 超大帧被拒绝（codec 上限握手时冻结）
    #[tokio::test]
    async fn oversized_frame_rejected() {
        let ws = WsSettings {
            max_message_bytes: 64,
            ..Default::default()
        };
        let url = spawn_ws_server(state_without_provider(ws)).await;
        let mut client = ws_connect(&url).await;
        let big = "x".repeat(200);
        let _ = client.send(TMessage::Text(big.into())).await;
        // tungstenite 服务端以 1009 关闭（或连接直接断）
        let code = next_close_code(&mut client).await;
        assert!(code == 1009 || code == 1006, "超大帧应触发 1009/断开，实际 {}", code);
    }

    /// 优雅 shutdown：活跃 WS 以 1001 关闭
    #[tokio::test]
    async fn graceful_shutdown_closes_active_ws_1001() {
        let state = state_with_mock_upstream(WsSettings::default(), 3000).await;
        let shutdown_tx = state.ws_shutdown.clone();
        let url = spawn_ws_server(state).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        let first = next_json(&mut client).await;
        assert_eq!(first["type"], "response.created");

        shutdown_tx.send(()).expect("广播 shutdown 失败");
        assert_eq!(next_close_code(&mut client).await, 1001);
    }

    // === 热加载语义（任务 7.4）===

    /// enabled=false 只拦新连接，不杀存量会话
    #[tokio::test]
    async fn hot_disable_does_not_kill_existing_session() {
        let state = state_with_mock_upstream(WsSettings::default(), 0).await;
        let settings = state.ws_settings.clone();
        let url = spawn_ws_server(state).await;

        let mut client = ws_connect(&url).await;
        // 热关闭
        settings.write().enabled = false;

        // 存量会话照常完成 turn
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        let types = drain_until_terminal(&mut client).await;
        assert_eq!(types.last().map(String::as_str), Some("response.completed"));

        // 新连接被 503 拒绝
        let resp = http_upgrade_attempt(&url).await;
        assert_eq!(resp.status().as_u16(), 503);
    }

    /// 首帧超时误配为 0 时不得立即断开新连接（0 保护下限 1s）
    #[tokio::test]
    async fn zero_first_message_timeout_is_protected() {
        let ws = WsSettings {
            client_first_message_timeout_seconds: 0,
            ..Default::default()
        };
        let url = spawn_ws_server(state_with_mock_upstream(ws, 0).await).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        let types = drain_until_terminal(&mut client).await;
        assert_eq!(
            types.last().map(String::as_str),
            Some("response.completed"),
            "0 值首帧超时必须按 1s 下限保护，首个 turn 应正常完成"
        );
    }

    /// 上游读超时只计上游 chunk：客户端帧不得重置计时（审查修复）
    ///
    /// 上游首帧后卡死 5s；`upstream_read_timeout_seconds=1` 期间客户端每 100ms
    /// 发一条 session.update。正确语义下 turn 应在约 1s 以 response.failed 终结；
    /// 若客户端帧能重置计时，turn 会存活到 5s 上游自然结束（response.completed）。
    /// 超时后连接必须存活（spec：连接 MUST 存活），并可再跑一轮完整 turn。
    #[tokio::test]
    async fn client_frames_do_not_extend_stalled_upstream() {
        let ws = WsSettings {
            upstream_read_timeout_seconds: 1,
            ..Default::default()
        };
        let url = spawn_ws_server(state_with_mock_upstream(ws, 5000).await).await;
        let mut client = ws_connect(&url).await;
        client
            .send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("发送失败");
        // 等开场事件出现，确认 turn 已进泵
        let first = next_json(&mut client).await;
        assert_eq!(first["type"], "response.created");

        // 拆分连接：后台持续发 session.update 试图重置读超时
        let (mut tx, mut rx) = client.split();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
        let pinger = tokio::spawn(async move {
            let frame = TMessage::Text(
                json!({"type":"session.update","session":{"model":"claude-haiku-4.5"}})
                    .to_string()
                    .into(),
            );
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stop_rx => break,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                }
                if tx.send(frame.clone()).await.is_err() {
                    break;
                }
            }
            tx
        });

        let started = std::time::Instant::now();
        let terminal = drain_split_until_terminal(&mut rx).await;
        let elapsed = started.elapsed();

        assert_eq!(
            terminal, "response.failed",
            "卡死的 turn 必须以上游读超时终结，而不是等上游 5s 自然结束"
        );
        assert!(
            elapsed < Duration::from_millis(3500),
            "客户端帧不得续命读超时（实际 {:?}）",
            elapsed
        );

        // 停止 ping 流量并收回写半部
        let _ = stop_tx.send(());
        let mut tx = pinger.await.expect("pinger 任务异常退出");

        // 连接存活（spec：连接 MUST 存活）：超时后再跑一轮完整 turn。
        // mock 上游每个请求都先吐首帧再卡死，第二轮同样以读超时终结
        tx.send(TMessage::Text(create_frame("claude-sonnet-4.5").into()))
            .await
            .expect("连接应存活（第二轮 turn 发送失败）");
        let terminal2 = drain_split_until_terminal(&mut rx).await;
        assert_eq!(
            terminal2, "response.failed",
            "超时存活后的第二轮 turn 仍须走完完整事件序列"
        );
    }
}
