//! `POST /v1/chat/completions` handler

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use serde_json::json;
use tokio::time::interval;
use uuid::Uuid;

use crate::anthropic::types::MessagesRequest;
use crate::anthropic::{
    AppState, convert_request_with_policy, extract_thinking_from_complete_text,
    get_context_window_size, override_thinking_from_model_name, resolution_context_from_state,
};
use crate::kiro::model::events::Event;
use crate::kiro::model::requests::kiro::KiroRequest;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::kiro::provider::KiroProvider;
use crate::token;

use super::converter::to_messages_request;
use super::error::{OpenAiError, map_provider_error};
use super::responses_stream::{ResponsesSseEvent, ResponsesStreamContext};
use super::stream::OpenAiStreamContext;
use super::types::{
    AssistantMessage, ChatCompletion, ChatCompletionRequest, Choice, ResponseFunctionCall,
    ResponseToolCall, Usage,
};

/// SSE 保活间隔（与 Anthropic 侧一致）
const KEEPALIVE_INTERVAL_SECS: u64 = 25;

/// 转换完成后需要交给响应层的状态
///
/// 说明（design D8）：本项目**不抽共享前置层**，OpenAI handler 自行捞取。
/// 以下四项漏任一项都是静默错误，编译器不报：
/// 1. `override_thinking_from_model_name`  -> thinking 后缀失效
/// 2. `tool_name_map`                      -> 超长工具名回显哈希短名
/// 3. `input_tokens`                       -> usage 全 0
/// 4. `thinking_enabled`                   -> 思考内容混进 content
#[derive(Debug)]
struct PreparedRequest {
    body: String,
    /// 回显给客户端的模型名：原始请求值（D9）
    echo_model: String,
    input_tokens: i32,
    thinking_enabled: bool,
    tool_name_map: HashMap<String, String>,
}

pub async fn post_chat_completions(
    State(state): State<AppState>,
    body: Bytes,
) -> Response {
    // 手动反序列化以返回 OpenAI error shape（axum 的 Json 拒绝会返回默认文本）
    let req: ChatCompletionRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return OpenAiError::InvalidRequest(format!("Invalid JSON: {}", e)).into_response();
        }
    };

    tracing::info!(
        model = %req.model,
        stream = %req.stream,
        message_count = %req.messages.len(),
        "Received POST /v1/chat/completions request"
    );

    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            tracing::error!("KiroProvider 未配置");
            return OpenAiError::Unavailable("Kiro API provider not configured".to_string())
                .into_response();
        }
    };

    let prepared = match prepare(&state, &req) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };

    if req.stream {
        handle_stream(provider, prepared, req.include_usage()).await
    } else {
        handle_non_stream(provider, prepared, state.extract_thinking).await
    }
}

/// 前置逻辑：逐项捞齐 design §8 的六步
fn prepare(state: &AppState, req: &ChatCompletionRequest) -> Result<PreparedRequest, OpenAiError> {
    // 1. 协议翻译（model 原样带入）
    let mut msg_req: MessagesRequest = to_messages_request(req)?;

    // 2. thinking 后缀 —— 必须显式调用，漏了会静默降级
    override_thinking_from_model_name(&mut msg_req);

    // 3. 转换（同时拿到 tool_name_map）
    let (policy, catalog_set) = resolution_context_from_state(state);
    let conversion = convert_request_with_policy(&msg_req, &policy, catalog_set.as_ref())?;

    let kiro_request = KiroRequest {
        conversation_state: conversion.conversation_state,
        profile_arn: None,
    };
    // 到达上游的工具清单：工具透传是本服务最易静默失效的环节
    // （客户端可能用 tools 字段、也可能藏在 input 的 additional_tools 里），
    // 只打名字与数量，完整请求体仍走 debug。
    let upstream_tools = &kiro_request
        .conversation_state
        .current_message
        .user_input_message
        .user_input_message_context
        .tools;
    if upstream_tools.is_empty() {
        tracing::info!("上游工具清单为空（客户端未提供工具，或工具在转换中丢失）");
    } else {
        tracing::info!(
            count = upstream_tools.len(),
            names = %upstream_tools
                .iter()
                .map(|t| t.tool_specification.name.as_str())
                .collect::<Vec<_>>()
                .join(","),
            "工具已送达上游"
        );
    }

    let body = serde_json::to_string(&kiro_request)
        .map_err(|e| OpenAiError::Internal(format!("序列化请求失败: {}", e)))?;
    tracing::debug!("Kiro request body: {}", body);

    // 4. thinking_enabled
    let thinking_enabled = msg_req
        .thinking
        .as_ref()
        .map(|t| t.is_enabled())
        .unwrap_or(false);

    // 5. input_tokens —— 必须在 convert 之后（按值消费 msg_req 字段）
    let echo_model = req.model.clone();
    let input_tokens = token::count_all_tokens(
        msg_req.model,
        msg_req.system,
        msg_req.messages,
        msg_req.tools,
    ) as i32;

    Ok(PreparedRequest {
        body,
        echo_model,
        input_tokens,
        thinking_enabled,
        tool_name_map: conversion.tool_name_map,
    })
}

// === 流式 ===

async fn handle_stream(
    provider: Arc<KiroProvider>,
    prepared: PreparedRequest,
    include_usage: bool,
) -> Response {
    let response = match provider.call_api_stream(&prepared.body).await {
        Ok(r) => r,
        Err(e) => return map_provider_error(e).into_response(),
    };

    let ctx = OpenAiStreamContext::new(
        prepared.echo_model,
        prepared.input_tokens,
        prepared.thinking_enabled,
        include_usage,
        prepared.tool_name_map,
    );

    let sse = create_sse_stream(response, ctx);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(sse))
        .expect("构造 SSE 响应失败")
}

fn create_sse_stream(
    response: reqwest::Response,
    ctx: OpenAiStreamContext,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let body_stream = response.bytes_stream();

    stream::unfold(
        (
            body_stream,
            ctx,
            EventStreamDecoder::new(),
            false,
            interval(Duration::from_secs(KEEPALIVE_INTERVAL_SECS)),
        ),
        |(mut body_stream, mut ctx, mut decoder, finished, mut keepalive)| async move {
            if finished {
                return None;
            }

            loop {
                tokio::select! {
                    _ = keepalive.tick() => {
                        // OpenAI 协议无 ping 事件，用 SSE 注释行保活
                        let bytes: Vec<Result<Bytes, Infallible>> = vec![Ok(Bytes::from(
                            super::stream::OpenAiSseChunk::Keepalive.to_sse_string(),
                        ))];
                        return Some((stream::iter(bytes), (body_stream, ctx, decoder, false, keepalive)));
                    }

                    chunk = body_stream.next() => {
                        match chunk {
                            Some(Ok(bytes_in)) => {
                                if let Err(e) = decoder.feed(&bytes_in) {
                                    tracing::warn!("缓冲区溢出: {}", e);
                                }
                                let mut out: Vec<Result<Bytes, Infallible>> = Vec::new();
                                for result in decoder.decode_iter() {
                                    match result {
                                        Ok(frame) => {
                                            if let Ok(event) = Event::from_frame(frame) {
                                                for c in ctx.process_kiro_event(&event) {
                                                    out.push(Ok(Bytes::from(c.to_sse_string())));
                                                }
                                            }
                                        }
                                        Err(e) => tracing::warn!("解码事件失败: {}", e),
                                    }
                                }
                                if out.is_empty() {
                                    continue;
                                }
                                return Some((stream::iter(out), (body_stream, ctx, decoder, false, keepalive)));
                            }
                            Some(Err(e)) => {
                                tracing::error!("读取响应流失败: {}", e);
                                let out: Vec<Result<Bytes, Infallible>> = ctx
                                    .finish()
                                    .into_iter()
                                    .map(|c| Ok(Bytes::from(c.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(out), (body_stream, ctx, decoder, true, keepalive)));
                            }
                            None => {
                                let out: Vec<Result<Bytes, Infallible>> = ctx
                                    .finish()
                                    .into_iter()
                                    .map(|c| Ok(Bytes::from(c.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(out), (body_stream, ctx, decoder, true, keepalive)));
                            }
                        }
                    }
                }
            }
        },
    )
    .flatten()
}

// === 非流式 ===

async fn handle_non_stream(
    provider: Arc<KiroProvider>,
    prepared: PreparedRequest,
    extract_thinking_cfg: bool,
) -> Response {
    let response = match provider.call_api(&prepared.body).await {
        Ok(r) => r,
        Err(e) => return map_provider_error(e).into_response(),
    };

    let body_bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("读取响应体失败: {}", e);
            return OpenAiError::Upstream(format!("读取响应失败: {}", e)).into_response();
        }
    };

    let aggregated = aggregate(&body_bytes, &prepared);
    let completion = build_completion(
        aggregated,
        &prepared,
        extract_thinking_cfg && prepared.thinking_enabled,
    );

    (StatusCode::OK, Json(completion)).into_response()
}

struct Aggregated {
    text: String,
    tool_calls: Vec<ResponseToolCall>,
    has_tool_use: bool,
    length_limited: bool,
    context_input_tokens: Option<i32>,
}

fn aggregate(body: &[u8], prepared: &PreparedRequest) -> Aggregated {
    let mut decoder = EventStreamDecoder::new();
    if let Err(e) = decoder.feed(body) {
        tracing::warn!("缓冲区溢出: {}", e);
    }

    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut has_tool_use = false;
    let mut length_limited = false;
    let mut context_input_tokens = None;
    let mut buffers: HashMap<String, String> = HashMap::new();

    for result in decoder.decode_iter() {
        let frame = match result {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("解码事件失败: {}", e);
                continue;
            }
        };
        let Ok(event) = Event::from_frame(frame) else {
            continue;
        };

        match event {
            Event::AssistantResponse(resp) => text.push_str(&resp.content),
            Event::ToolUse(tool_use) => {
                has_tool_use = true;
                buffers
                    .entry(tool_use.tool_use_id.clone())
                    .or_default()
                    .push_str(&tool_use.input);

                if tool_use.stop {
                    let args = buffers
                        .get(&tool_use.tool_use_id)
                        .cloned()
                        .unwrap_or_default();
                    // 工具名还原（D8 第二项）
                    let name = prepared
                        .tool_name_map
                        .get(&tool_use.name)
                        .cloned()
                        .unwrap_or_else(|| tool_use.name.clone());
                    tool_calls.push(ResponseToolCall {
                        id: tool_use.tool_use_id.clone(),
                        call_type: "function",
                        function: ResponseFunctionCall {
                            name,
                            arguments: if args.is_empty() {
                                "{}".to_string()
                            } else {
                                args
                            },
                        },
                    });
                }
            }
            Event::ContextUsage(usage) => {
                let window = get_context_window_size(&prepared.echo_model);
                context_input_tokens =
                    Some((usage.context_usage_percentage * (window as f64) / 100.0) as i32);
                if usage.context_usage_percentage >= 100.0 {
                    length_limited = true;
                }
            }
            Event::Exception { exception_type, .. } => {
                if exception_type == "ContentLengthExceededException" {
                    length_limited = true;
                }
            }
            _ => {}
        }
    }

    Aggregated {
        text,
        tool_calls,
        has_tool_use,
        length_limited,
        context_input_tokens,
    }
}

fn build_completion(
    agg: Aggregated,
    prepared: &PreparedRequest,
    extract_thinking: bool,
) -> ChatCompletion {
    // thinking 分离：思考内容进 reasoning_content，绝不混进 content
    let (reasoning, content_text) = if extract_thinking {
        let (thinking, remaining) = extract_thinking_from_complete_text(&agg.text);
        (thinking, remaining)
    } else {
        (None, agg.text.clone())
    };

    let finish_reason = if agg.length_limited {
        "length"
    } else if agg.has_tool_use {
        "tool_calls"
    } else {
        "stop"
    };

    let prompt_tokens = agg.context_input_tokens.unwrap_or(prepared.input_tokens);
    let completion_tokens = estimate_completion_tokens(&content_text, &reasoning, &agg.tool_calls);

    ChatCompletion {
        id: format!("chatcmpl-{}", Uuid::new_v4().to_string().replace('-', "")),
        object: "chat.completion",
        created: chrono::Utc::now().timestamp(),
        model: prepared.echo_model.clone(),
        choices: vec![Choice {
            index: 0,
            message: AssistantMessage {
                role: "assistant",
                content: if content_text.is_empty() {
                    None
                } else {
                    Some(content_text.clone())
                },
                reasoning_content: reasoning.filter(|r| !r.is_empty()),
                tool_calls: if agg.tool_calls.is_empty() {
                    None
                } else {
                    Some(agg.tool_calls)
                },
            },
            finish_reason: finish_reason.to_string(),
        }],
        usage: Usage::new(prompt_tokens, completion_tokens),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::types::ContentBlock;

    fn state() -> AppState {
        AppState::new("k", true)
    }

    fn parse(body: &str) -> ChatCompletionRequest {
        serde_json::from_str(body).expect("请求反序列化失败")
    }

    /// 从 prepare 产出的 Kiro 请求体中取出发往上游的 JSON
    fn prepared_body(body: &str) -> serde_json::Value {
        let p = prepare(&state(), &parse(body)).expect("prepare 失败");
        serde_json::from_str(&p.body).expect("Kiro 请求体解析失败")
    }

    #[test]
    fn test_thinking_suffix_reaches_upstream() {
        // D8 第一项锁定测试：漏调 override_thinking_from_model_name 时本测试转红
        let kiro = prepared_body(
            r#"{"model":"claude-sonnet-4.5-thinking",
                "messages":[{"role":"user","content":"hi"}]}"#,
        );
        let serialized = serde_json::to_string(&kiro).expect("序列化失败");
        assert!(
            serialized.contains("<thinking_mode>"),
            "带 -thinking 后缀的请求必须把 thinking 指令带到上游，实际: {}",
            serialized
        );
    }

    #[test]
    fn test_no_thinking_suffix_no_directive() {
        let kiro = prepared_body(
            r#"{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"hi"}]}"#,
        );
        let serialized = serde_json::to_string(&kiro).expect("序列化失败");
        assert!(!serialized.contains("<thinking_mode>"));
    }

    #[test]
    fn test_thinking_enabled_flag_set_from_suffix() {
        let p = prepare(
            &state(),
            &parse(
                r#"{"model":"claude-sonnet-4.5-thinking",
                    "messages":[{"role":"user","content":"hi"}]}"#,
            ),
        )
        .expect("prepare 失败");
        assert!(
            p.thinking_enabled,
            "thinking_enabled 必须由后缀推导出来（D8 第四项）"
        );
    }

    #[test]
    fn test_echo_model_is_original_request_value() {
        // D9：回显原始请求 model，不是 resolve 后的 id
        let p = prepare(
            &state(),
            &parse(r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hi"}]}"#),
        )
        .expect("prepare 失败");
        assert_eq!(p.echo_model, "gpt-4o");

        // 发往上游的 modelId 应是映射后的 Claude id
        let kiro: serde_json::Value = serde_json::from_str(&p.body).expect("解析失败");
        let upstream = serde_json::to_string(&kiro).expect("序列化失败");
        assert!(
            upstream.contains("claude"),
            "上游请求应使用映射后的 Claude 模型: {}",
            upstream
        );
    }

    #[test]
    fn test_input_tokens_computed() {
        // D8 第三项：漏捞时 usage 全 0
        let p = prepare(
            &state(),
            &parse(
                r#"{"model":"claude-sonnet-4.5",
                    "messages":[{"role":"user","content":"hello world this is a test"}]}"#,
            ),
        )
        .expect("prepare 失败");
        assert!(p.input_tokens > 0, "input_tokens 必须被计算");
    }

    #[test]
    fn test_tool_name_map_captured_for_long_names() {
        // D8 第二项：超长工具名会被缩短，map 必须被捞出来供响应层还原
        let long_name = "a".repeat(80);
        let body = format!(
            r#"{{"model":"claude-sonnet-4.5","messages":[{{"role":"user","content":"hi"}}],
                "tools":[{{"type":"function","function":{{"name":"{}","description":"d",
                    "parameters":{{"type":"object"}}}}}}]}}"#,
            long_name
        );
        let p = prepare(&state(), &parse(&body)).expect("prepare 失败");
        assert!(
            !p.tool_name_map.is_empty(),
            "超长工具名应产生 tool_name_map 条目"
        );
        assert!(
            p.tool_name_map.values().any(|v| v == &long_name),
            "map 中必须含原始工具名以供响应还原"
        );
    }

    #[test]
    fn test_short_tool_name_no_map_entry() {
        let p = prepare(
            &state(),
            &parse(
                r#"{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"hi"}],
                    "tools":[{"type":"function","function":{"name":"f","description":"d",
                        "parameters":{"type":"object"}}}]}"#,
            ),
        )
        .expect("prepare 失败");
        assert!(p.tool_name_map.is_empty());
    }

    #[test]
    fn test_unresolvable_model_rejected_with_openai_shape() {
        let err = prepare(
            &state(),
            &parse(
                r#"{"model":"totally-unknown-model-xyz",
                    "messages":[{"role":"user","content":"hi"}]}"#,
            ),
        )
        .unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.error_type(), "invalid_request_error");
        // 不使用「凭据无效」前缀
        assert!(!err.message().starts_with("凭据无效"));
    }

    #[test]
    fn test_web_search_tool_not_intercepted() {
        // D10：Chat 端点不劫持 web_search，它作为普通 tool 进入上游请求
        let p = prepare(
            &state(),
            &parse(
                r#"{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"hi"}],
                    "tools":[{"type":"function","function":{"name":"web_search",
                        "description":"search the web","parameters":{"type":"object"}}}]}"#,
            ),
        )
        .expect("prepare 失败");
        let upstream = p.body;
        assert!(
            upstream.contains("web_search"),
            "web_search 应作为普通工具传给上游，而非被代理拦截执行"
        );
    }

    #[test]
    fn test_finish_reason_length_takes_precedence_over_tool_calls() {
        let agg = Aggregated {
            text: String::new(),
            tool_calls: vec![ResponseToolCall {
                id: "c".into(),
                call_type: "function",
                function: ResponseFunctionCall {
                    name: "f".into(),
                    arguments: "{}".into(),
                },
            }],
            has_tool_use: true,
            length_limited: true,
            context_input_tokens: None,
        };
        let prepared = PreparedRequest {
            body: String::new(),
            echo_model: "m".into(),
            input_tokens: 7,
            thinking_enabled: false,
            tool_name_map: HashMap::new(),
        };
        let c = build_completion(agg, &prepared, false);
        assert_eq!(c.choices[0].finish_reason, "length");
    }

    #[test]
    fn test_non_stream_prompt_tokens_prefers_context_signal() {
        let prepared = PreparedRequest {
            body: String::new(),
            echo_model: "m".into(),
            input_tokens: 999,
            thinking_enabled: false,
            tool_name_map: HashMap::new(),
        };
        let agg = Aggregated {
            text: "hi".into(),
            tool_calls: vec![],
            has_tool_use: false,
            length_limited: false,
            context_input_tokens: Some(42),
        };
        let c = build_completion(agg, &prepared, false);
        assert_eq!(c.usage.prompt_tokens, 42, "应优先使用上游反算值");

        let agg2 = Aggregated {
            text: "hi".into(),
            tool_calls: vec![],
            has_tool_use: false,
            length_limited: false,
            context_input_tokens: None,
        };
        let c2 = build_completion(agg2, &prepared, false);
        assert_eq!(c2.usage.prompt_tokens, 999, "无上游信号时回落估算值");
    }

    #[test]
    fn test_non_stream_thinking_separated_from_content() {
        let prepared = PreparedRequest {
            body: String::new(),
            echo_model: "m".into(),
            input_tokens: 1,
            thinking_enabled: true,
            tool_name_map: HashMap::new(),
        };
        let agg = Aggregated {
            text: "<thinking>思考</thinking>\n\n答案".into(),
            tool_calls: vec![],
            has_tool_use: false,
            length_limited: false,
            context_input_tokens: None,
        };
        let c = build_completion(agg, &prepared, true);
        let msg = &c.choices[0].message;
        assert_eq!(msg.reasoning_content.as_deref(), Some("思考"));
        let content = msg.content.as_deref().unwrap_or("");
        assert_eq!(content, "答案");
        assert!(!content.contains("思考"), "思考内容不得混进 content");
    }

    #[test]
    fn test_non_stream_response_shape() {
        let prepared = PreparedRequest {
            body: String::new(),
            echo_model: "gpt-4o".into(),
            input_tokens: 5,
            thinking_enabled: false,
            tool_name_map: HashMap::new(),
        };
        let agg = Aggregated {
            text: "hello".into(),
            tool_calls: vec![],
            has_tool_use: false,
            length_limited: false,
            context_input_tokens: None,
        };
        let json = serde_json::to_value(build_completion(agg, &prepared, false)).unwrap();
        assert_eq!(json["object"], "chat.completion");
        assert_eq!(json["model"], "gpt-4o");
        assert!(json["id"].as_str().unwrap().starts_with("chatcmpl-"));
        assert_eq!(json["choices"][0]["index"], 0);
        assert_eq!(json["choices"][0]["message"]["role"], "assistant");
        assert_eq!(json["choices"][0]["message"]["content"], "hello");
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
        assert_eq!(json["usage"]["total_tokens"], 5 + json["usage"]["completion_tokens"].as_i64().unwrap());
        // 无工具调用时不应出现该字段
        assert!(json["choices"][0]["message"].get("tool_calls").is_none());
        assert!(json["choices"][0]["message"].get("reasoning_content").is_none());
    }

    #[test]
    fn test_system_message_reaches_upstream() {
        let p = prepare(
            &state(),
            &parse(
                r#"{"model":"claude-sonnet-4.5","messages":[
                    {"role":"system","content":"你是助手"},
                    {"role":"user","content":"hi"}]}"#,
            ),
        )
        .expect("prepare 失败");
        assert!(p.body.contains("你是助手"), "system 内容必须到达上游");
    }

    // === Responses 端点 ===

    fn responses_req(body: &str) -> super::super::responses_types::ResponsesRequest {
        serde_json::from_str(body).expect("Responses 请求反序列化失败")
    }

    /// 走完 Responses 归一 + prepare，返回发往上游的请求体
    fn responses_prepared(body: &str) -> PreparedRequest {
        let req = responses_req(body);
        let (chat_json, _) = super::super::responses::to_chat_request_json(&req).expect("归一失败");
        let chat: ChatCompletionRequest =
            serde_json::from_value(chat_json).expect("归一结果不可解析");
        prepare(&state(), &chat).expect("prepare 失败")
    }

    #[test]
    fn test_responses_thinking_suffix_reaches_upstream() {
        // D8 第一项在 Responses 端点同样适用
        let p = responses_prepared(r#"{"model":"claude-sonnet-4.5-thinking","input":"hi"}"#);
        assert!(
            p.body.contains("<thinking_mode>"),
            "Responses 端点的 thinking 后缀也必须生效: {}",
            p.body
        );
        assert!(p.thinking_enabled);
    }

    #[test]
    fn test_responses_instructions_reaches_upstream() {
        let p = responses_prepared(r#"{"input":"hi","instructions":"你是搜索助手"}"#);
        assert!(
            p.body.contains("你是搜索助手"),
            "instructions 必须到达上游: {}",
            p.body
        );
    }

    #[test]
    fn test_responses_tool_name_map_captured() {
        // D8 第二项
        let long = "b".repeat(80);
        let body = format!(
            r#"{{"input":"hi","tools":[{{"type":"function","name":"{}",
                "description":"d","parameters":{{"type":"object"}}}}]}}"#,
            long
        );
        let p = responses_prepared(&body);
        assert!(p.tool_name_map.values().any(|v| v == &long));
    }

    #[test]
    fn test_responses_input_tokens_computed() {
        let p = responses_prepared(r#"{"input":"hello world this is a longer question"}"#);
        assert!(p.input_tokens > 0);
    }

    #[test]
    fn test_responses_function_call_history_reaches_upstream() {
        let p = responses_prepared(
            r#"{"input":[
                {"role":"user","content":"weather?"},
                {"type":"function_call","call_id":"c1","name":"get_weather","arguments":"{\"c\":\"SH\"}"},
                {"type":"function_call_output","call_id":"c1","output":"sunny"}]}"#,
        );
        assert!(p.body.contains("get_weather"), "工具调用历史应到达上游");
        assert!(p.body.contains("sunny"), "工具结果应到达上游");
    }

    #[test]
    fn test_responses_websearch_gate_respects_flag() {
        use super::super::types::OpenAiTool;
        let tools: Vec<OpenAiTool> =
            vec![serde_json::from_str(r#"{"type":"web_search"}"#).unwrap()];
        assert!(super::super::websearch::should_emulate(Some(&tools), true));
        assert!(
            !super::super::websearch::should_emulate(Some(&tools), false),
            "开关关闭时不得代执行"
        );
    }

    #[test]
    fn test_estimate_chars_never_zero() {
        assert!(estimate_chars("") >= 1);
        assert!(estimate_chars("abcd") >= 1);
    }

    #[test]
    fn test_content_block_type_available() {
        // 保证 anthropic types 导出可用（编译期约束）
        let block: ContentBlock =
            serde_json::from_value(json!({"type":"text","text":"x"})).unwrap();
        assert_eq!(block.block_type, "text");
    }

    // === 响应侧还原：build_tool_call_item ===

    use super::super::responses_tools::ToolRewriteMap;

    fn rewrite_with_freeform(name: &str) -> ToolRewriteMap {
        let mut r = ToolRewriteMap::default();
        r.freeform.insert(name.to_string());
        r
    }

    #[test]
    fn test_plain_tool_stays_function_call() {
        let item = build_tool_call_item("c1", "wait", r#"{"cell_id":"x"}"#, &Default::default());
        assert_eq!(item.item_type, "function_call");
        assert_eq!(item.arguments.as_deref(), Some(r#"{"cell_id":"x"}"#));
        assert!(item.input.is_none());
        assert!(item.namespace.is_none());
    }

    #[test]
    fn test_freeform_tool_becomes_custom_tool_call() {
        let item = build_tool_call_item(
            "c1",
            "exec",
            r#"{"input":"const x = 1;"}"#,
            &rewrite_with_freeform("exec"),
        );
        assert_eq!(item.item_type, "custom_tool_call");
        assert_eq!(item.input.as_deref(), Some("const x = 1;"));
        assert!(
            item.arguments.is_none(),
            "custom_tool_call 不应带 arguments（客户端只读 input）"
        );
        assert_eq!(item.call_id.as_deref(), Some("c1"));
        assert_eq!(item.name.as_deref(), Some("exec"));
    }

    #[test]
    fn test_freeform_tool_raw_source_passthrough() {
        // 模型照 description 要求直接回裸源码（非 JSON）
        let raw = "await tools.exec_command({cmd: \"git status\"});";
        let item = build_tool_call_item("c1", "exec", raw, &rewrite_with_freeform("exec"));
        assert_eq!(item.input.as_deref(), Some(raw));
    }

    #[test]
    fn test_freeform_input_preserves_newlines_and_quotes() {
        let src = "const s = \"a\\nb\";\ntext(\"ok\");";
        let args = serde_json::to_string(&json!({"input": src})).unwrap();
        let item = build_tool_call_item("c1", "exec", &args, &rewrite_with_freeform("exec"));
        assert_eq!(item.input.as_deref(), Some(src), "换行与引号须无损");
    }

    /// 锁定：freeform 集合的 key 是展平/原始名，超长工具名往返后仍能命中
    ///
    /// `tool_name_map` 在 `aggregate` 内已把上游的缩短名还原成原名，
    /// 因此这里查到的必须是原名。若集合存缩短名则失配，
    /// freeform 工具静默退化成 function_call，客户端拒绝执行。
    #[test]
    fn test_long_freeform_tool_name_roundtrip_still_custom_tool_call() {
        let long_name = format!("exec_{}", "x".repeat(80));

        // 请求侧：归一层记入的是原名
        let (_, rewrite) = super::super::responses_tools::normalize_tools(vec![json!({
            "type":"custom","name":long_name.clone(),"description":"d",
            "format":{"type":"grammar","syntax":"lark","definition":"start: X"}
        })])
        .expect("归一应成功");

        // 响应侧：aggregate 已用 tool_name_map 把缩短名还原为原名
        let item = build_tool_call_item("c1", &long_name, r#"{"input":"src"}"#, &rewrite);
        assert_eq!(
            item.item_type, "custom_tool_call",
            "超长 freeform 工具名往返后仍须回 custom_tool_call"
        );
        assert_eq!(item.input.as_deref(), Some("src"));
    }

    #[test]
    fn test_flattened_tool_restored_to_namespace_and_original_name() {
        let (_, rewrite) = super::super::responses_tools::normalize_tools(vec![json!({
            "type":"namespace","name":"collaboration",
            "tools":[{"type":"function","name":"spawn_agent","parameters":{"type":"object"}}]
        })])
        .expect("归一应成功");

        let item = build_tool_call_item("c1", "collaboration__spawn_agent", "{}", &rewrite);
        assert_eq!(item.item_type, "function_call");
        assert_eq!(item.name.as_deref(), Some("spawn_agent"), "须还原为原名");
        assert_eq!(
            item.namespace.as_deref(),
            Some("collaboration"),
            "须补 namespace（客户端按 (namespace, name) 匹配）"
        );
    }

    /// 锁定：两级映射叠加 —— tool_name_map 还原 + 逆映射还原
    ///
    /// 超长展平名经 `tool_name_map` 缩短后回传，`aggregate` 还原为展平名，
    /// 再经逆映射还原为 (namespace, 原名)。两级都漏则工具调用链断。
    #[test]
    fn test_two_level_mapping_long_flattened_name() {
        let long_child = "c".repeat(80);
        let flat = super::super::responses_tools::flatten_namespace_name("ns", &long_child);
        assert!(flat.len() > 63, "展平名须超长以触发缩短");

        let (_, rewrite) = super::super::responses_tools::normalize_tools(vec![json!({
            "type":"namespace","name":"ns",
            "tools":[{"type":"function","name":long_child.clone(),
                      "parameters":{"type":"object"}}]
        })])
        .expect("归一应成功");

        // 第一级由 tool_name_map 完成（既有机制），这里给出其产物：展平名
        let item = build_tool_call_item("c1", &flat, "{}", &rewrite);
        assert_eq!(item.name.as_deref(), Some(long_child.as_str()));
        assert_eq!(item.namespace.as_deref(), Some("ns"));
    }

    #[test]
    fn test_freeform_and_namespace_combined() {
        // 展平自 namespace 的 custom 工具：两项还原同时生效
        let mut rewrite = ToolRewriteMap::default();
        rewrite.freeform.insert("ns__exec".to_string());
        rewrite
            .namespaces
            .insert("ns__exec".to_string(), ("ns".to_string(), "exec".to_string()));

        let item = build_tool_call_item("c1", "ns__exec", r#"{"input":"src"}"#, &rewrite);
        assert_eq!(item.item_type, "custom_tool_call");
        assert_eq!(item.input.as_deref(), Some("src"));
        assert_eq!(item.name.as_deref(), Some("exec"));
        assert_eq!(item.namespace.as_deref(), Some("ns"));
    }

    #[test]
    fn test_custom_tool_call_item_serialization_shape() {
        let item = build_tool_call_item(
            "c1",
            "exec",
            r#"{"input":"x"}"#,
            &rewrite_with_freeform("exec"),
        );
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["type"], "custom_tool_call");
        assert_eq!(v["input"], "x");
        assert!(v.get("arguments").is_none(), "不应序列化 arguments");
        assert!(v.get("namespace").is_none(), "无 namespace 时不应出现该字段");
    }
}

// === Responses 端点 ===

/// Responses 端点专属的响应侧状态
///
/// 与 `PreparedRequest` 分开：后者是 Chat 端点共享的前置产物，
/// 不注入 Responses 概念（design D2/D3）。
struct ResponsesContext {
    /// 回显给客户端的模型名：请求原值（D9）
    echo_model: String,
    instructions: Option<String>,
    metadata: Option<HashMap<String, String>>,
    /// 客户端方言工具的还原映射（design D3）
    tool_rewrite: super::responses_tools::ToolRewriteMap,
}

pub async fn post_responses(State(state): State<AppState>, body: Bytes) -> Response {
    let req: super::responses_types::ResponsesRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return OpenAiError::InvalidRequest(format!("Invalid JSON: {}", e)).into_response();
        }
    };

    tracing::info!(
        model = %req.resolved_model(),
        stream = %req.stream,
        "Received POST /v1/responses request"
    );

    // 归一（含 previous_response_id 报错 —— D2）
    // 放在 provider 检查之前：请求本身不合法时应回 400，而非被 503 掩盖
    let (chat_json, tool_rewrite) = match super::responses::to_chat_request_json(&req) {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };

    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            return OpenAiError::Unavailable("Kiro API provider not configured".to_string())
                .into_response();
        }
    };

    // web_search 代执行分支（D10/D11）：放在转换之前，避免无谓的上游请求构造
    let websearch_enabled = state
        .kiro_provider
        .as_ref()
        .map(|p| p.token_manager().config().web_search_emulation)
        .unwrap_or(true);
    if super::websearch::should_emulate(req.tools.as_deref(), websearch_enabled) {
        let messages = chat_json["messages"].as_array().cloned().unwrap_or_default();
        let Some(query) = super::websearch::extract_query(&messages) else {
            return super::websearch::missing_query_error().into_response();
        };
        return handle_websearch(provider, &req, query).await;
    }

    let chat_req: ChatCompletionRequest = match serde_json::from_value(chat_json) {
        Ok(r) => r,
        Err(e) => {
            return OpenAiError::Internal(format!("归一结果不可解析: {}", e)).into_response();
        }
    };

    let prepared = match prepare(&state, &chat_req) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };

    // Responses 的模型回显用请求原值（D9）
    let echo_model = req.resolved_model();
    let instructions = req
        .instructions
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let ctx = ResponsesContext {
        echo_model,
        instructions,
        metadata: req.metadata,
        tool_rewrite,
    };

    if req.stream {
        handle_responses_stream(provider, prepared, ctx).await
    } else {
        handle_responses_non_stream(provider, prepared, ctx).await
    }
}

/// web_search 代执行：不经上游 generate
async fn handle_websearch(
    provider: Arc<KiroProvider>,
    req: &super::responses_types::ResponsesRequest,
    query: String,
) -> Response {
    use super::responses_types::{ResponsesObject, ResponsesUsage, response_id};

    let (query, results) = super::websearch::run_search(&provider, &query).await;
    let resp_id = response_id();
    let model = req.resolved_model();
    let created_at = chrono::Utc::now().timestamp();
    let instructions = req
        .instructions
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if req.stream {
        let (events, items, summary) = super::websearch::build_stream_events(&query, &results);
        // usage 用本地估算：该路径未调用上游，没有 contextUsageEvent
        let usage = ResponsesUsage::new(
            estimate_chars(&query),
            estimate_chars(&summary),
        );
        let final_obj = ResponsesObject {
            id: resp_id.clone(),
            object: "response",
            created_at,
            status: "completed",
            model: model.clone(),
            output: items,
            usage,
            instructions: instructions.clone(),
            metadata: req.metadata.clone(),
            error: None,
        };

        let in_progress = ResponsesObject {
            output: Vec::new(),
            status: "in_progress",
            ..final_obj.clone()
        };
        let snap = |t: &str, obj: &ResponsesObject| {
            ResponsesSseEvent::named(
                t,
                json!({"type": t, "response": serde_json::to_value(obj).expect("序列化失败")}),
            )
        };

        let mut all = vec![
            snap("response.created", &in_progress),
            snap("response.in_progress", &in_progress),
        ];
        all.extend(events);
        all.push(snap("response.completed", &final_obj));
        all.push(ResponsesSseEvent::Done);

        let bytes: Vec<Result<Bytes, Infallible>> = all
            .into_iter()
            .map(|e| Ok(Bytes::from(e.to_sse_string())))
            .collect();

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(Body::from_stream(stream::iter(bytes)))
            .expect("构造 SSE 响应失败");
    }

    let (items, summary) = super::websearch::build_output_items(&query, &results);
    let obj = ResponsesObject {
        id: resp_id,
        object: "response",
        created_at,
        status: "completed",
        model,
        output: items,
        usage: ResponsesUsage::new(estimate_chars(&query), estimate_chars(&summary)),
        instructions,
        metadata: req.metadata.clone(),
        error: None,
    };
    (StatusCode::OK, Json(obj)).into_response()
}

fn estimate_chars(text: &str) -> i32 {
    (((text.len() + 3) / 4) as i32).max(1)
}

async fn handle_responses_stream(
    provider: Arc<KiroProvider>,
    prepared: PreparedRequest,
    rctx: ResponsesContext,
) -> Response {
    let response = match provider.call_api_stream(&prepared.body).await {
        Ok(r) => r,
        Err(e) => return map_provider_error(e).into_response(),
    };

    let ctx = ResponsesStreamContext::new(
        super::responses_types::response_id(),
        rctx.echo_model,
        rctx.instructions,
        rctx.metadata,
        prepared.input_tokens,
        prepared.thinking_enabled,
        prepared.tool_name_map,
        rctx.tool_rewrite,
    );

    let sse = create_responses_sse_stream(response, ctx);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(sse))
        .expect("构造 SSE 响应失败")
}

fn create_responses_sse_stream(
    response: reqwest::Response,
    ctx: ResponsesStreamContext,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    // 先发 created + in_progress
    let initial: Vec<Result<Bytes, Infallible>> = ctx
        .initial_events()
        .into_iter()
        .map(|e| Ok(Bytes::from(e.to_sse_string())))
        .collect();

    let body_stream = response.bytes_stream();

    let processing = stream::unfold(
        (
            body_stream,
            ctx,
            EventStreamDecoder::new(),
            false,
            interval(Duration::from_secs(KEEPALIVE_INTERVAL_SECS)),
        ),
        |(mut body_stream, mut ctx, mut decoder, finished, mut keepalive)| async move {
            if finished {
                return None;
            }

            loop {
                tokio::select! {
                    _ = keepalive.tick() => {
                        let bytes: Vec<Result<Bytes, Infallible>> = vec![Ok(Bytes::from(
                            ResponsesSseEvent::Keepalive.to_sse_string(),
                        ))];
                        return Some((stream::iter(bytes), (body_stream, ctx, decoder, false, keepalive)));
                    }

                    chunk = body_stream.next() => {
                        match chunk {
                            Some(Ok(bytes_in)) => {
                                if let Err(e) = decoder.feed(&bytes_in) {
                                    tracing::warn!("缓冲区溢出: {}", e);
                                }
                                let mut out: Vec<Result<Bytes, Infallible>> = Vec::new();
                                for result in decoder.decode_iter() {
                                    match result {
                                        Ok(frame) => {
                                            if let Ok(event) = Event::from_frame(frame) {
                                                for e in ctx.process_kiro_event(&event) {
                                                    out.push(Ok(Bytes::from(e.to_sse_string())));
                                                }
                                            }
                                        }
                                        Err(e) => tracing::warn!("解码事件失败: {}", e),
                                    }
                                }
                                if out.is_empty() {
                                    continue;
                                }
                                return Some((stream::iter(out), (body_stream, ctx, decoder, false, keepalive)));
                            }
                            Some(Err(e)) => {
                                tracing::error!("读取响应流失败: {}", e);
                                // 已开始输出则走 failed 事件，否则也只能在流中报错
                                let events = ctx.fail(format!("upstream stream error: {}", e));
                                let out: Vec<Result<Bytes, Infallible>> = events
                                    .into_iter()
                                    .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(out), (body_stream, ctx, decoder, true, keepalive)));
                            }
                            None => {
                                let out: Vec<Result<Bytes, Infallible>> = ctx
                                    .finish()
                                    .into_iter()
                                    .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(out), (body_stream, ctx, decoder, true, keepalive)));
                            }
                        }
                    }
                }
            }
        },
    )
    .flatten();

    stream::iter(initial).chain(processing)
}

async fn handle_responses_non_stream(
    provider: Arc<KiroProvider>,
    prepared: PreparedRequest,
    ctx: ResponsesContext,
) -> Response {
    use super::responses_types::{
        ResponseOutputItem, ResponsesObject, ResponsesUsage, output_item_id, response_id,
    };

    let response = match provider.call_api(&prepared.body).await {
        Ok(r) => r,
        Err(e) => return map_provider_error(e).into_response(),
    };

    let body_bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return OpenAiError::Upstream(format!("读取响应失败: {}", e)).into_response();
        }
    };

    let agg = aggregate(&body_bytes, &prepared);

    // thinking 内容不进 output（Responses 无稳定的 reasoning part 契约）
    let text = if prepared.thinking_enabled {
        let (thinking, remaining) = extract_thinking_from_complete_text(&agg.text);
        if thinking.is_some() {
            tracing::debug!("Responses 端点丢弃 thinking 内容（首版无 reasoning part）");
        }
        remaining
    } else {
        agg.text.clone()
    };

    let mut output: Vec<ResponseOutputItem> = Vec::new();
    if !text.is_empty() {
        output.push(ResponseOutputItem::message(output_item_id("msg"), &text));
    }
    for call in &agg.tool_calls {
        output.push(build_tool_call_item(
            &call.id,
            &call.function.name,
            &call.function.arguments,
            &ctx.tool_rewrite,
        ));
    }

    let input_tokens = agg.context_input_tokens.unwrap_or(prepared.input_tokens);
    let output_tokens = estimate_chars(&text)
        + agg
            .tool_calls
            .iter()
            .map(|c| estimate_chars(&c.function.arguments))
            .sum::<i32>();

    let obj = ResponsesObject {
        id: response_id(),
        object: "response",
        created_at: chrono::Utc::now().timestamp(),
        status: "completed",
        model: ctx.echo_model,
        output,
        usage: ResponsesUsage::new(input_tokens, output_tokens.max(1)),
        instructions: ctx.instructions,
        metadata: ctx.metadata,
        error: None,
    };

    (StatusCode::OK, Json(obj)).into_response()
}

/// 按还原映射产出工具调用 item
///
/// 两项还原都是**功能性必需**，不是格式偏好：
/// - freeform 工具收到 `function_call` 会被客户端自身拒绝（客户端为其登记的
///   payload 类型只接受裸文本 `input`），模型陷入重试
/// - 展平自 namespace 的工具若不带 `namespace` 字段，客户端按
///   `(namespace, name)` 查注册表会找不到工具
///
/// `name` 已由 `tool_name_map` 还原为展平名（见 design D3.1），直接用作 key。
fn build_tool_call_item(
    call_id: &str,
    name: &str,
    arguments: &str,
    rewrite: &super::responses_tools::ToolRewriteMap,
) -> super::responses_types::ResponseOutputItem {
    use super::responses_types::{ResponseOutputItem, output_item_id};

    let is_freeform = rewrite.freeform.contains(name);
    let item = if is_freeform {
        ResponseOutputItem::custom_tool_call(
            output_item_id("ctc"),
            call_id,
            name,
            super::responses_tools::extract_custom_input(arguments),
        )
    } else {
        ResponseOutputItem::function_call(output_item_id("fc"), call_id, name, arguments)
    };

    let restored = match rewrite.namespaces.get(name) {
        Some((namespace, original)) => item.with_namespace(namespace, original),
        None => item,
    };

    // 分派结果直接决定客户端能否执行该调用：freeform 工具收到 function_call
    // 会被客户端拒绝，展平工具缺 namespace 会匹配失败。
    tracing::info!(
        upstream_name = %name,
        item_type = %restored.item_type,
        namespace = restored.namespace.as_deref().unwrap_or("-"),
        "工具调用已分派"
    );

    restored
}

fn estimate_completion_tokens(
    content: &str,
    reasoning: &Option<String>,
    tool_calls: &[ResponseToolCall],
) -> i32 {
    let mut blocks: Vec<serde_json::Value> = Vec::new();
    if !content.is_empty() {
        blocks.push(json!({"type": "text", "text": content}));
    }
    if let Some(r) = reasoning {
        blocks.push(json!({"type": "text", "text": r}));
    }
    for call in tool_calls {
        blocks.push(json!({
            "type": "tool_use",
            "input": serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                .unwrap_or_else(|_| json!({})),
        }));
    }
    token::estimate_output_tokens(&blocks).max(1)
}
