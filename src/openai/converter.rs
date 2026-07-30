//! OpenAI Chat Completions -> 内存 `MessagesRequest`
//!
//! 设计依据 `docs/multi-protocol-api-design.md` D1：只做协议结构翻译，
//! Kiro 侧的全部约束（prefill、工具名缩短、system 分块、thinking 前缀、
//! tool_use/tool_result 配对校验、孤儿清理）交给
//! `convert_request_with_policy` 处理，不在此重复实现。

use serde_json::{Value, json};

use crate::anthropic::types::{ImageSource, MessagesRequest, Message, SystemMessage, Tool};

use super::error::OpenAiError;
use super::types::{ChatCompletionRequest, ChatMessage, OpenAiTool};

/// 把 OpenAI 请求翻译成内存中的 Anthropic 请求结构
pub fn to_messages_request(req: &ChatCompletionRequest) -> Result<MessagesRequest, OpenAiError> {
    if req.messages.is_empty() {
        return Err(OpenAiError::InvalidRequest(
            "messages must not be empty".to_string(),
        ));
    }

    let mut system_parts: Vec<SystemMessage> = Vec::new();
    let mut messages: Vec<Message> = Vec::new();
    // 连续 tool 消息归集缓冲：它们要合并进同一条 user 消息
    let mut pending_tool_results: Vec<Value> = Vec::new();
    let mut has_user = false;

    for msg in &req.messages {
        let role = msg.role.trim();

        // system / developer 都进 system 段（developer 是新 SDK 的 system 别名）
        if role == "system" || role == "developer" {
            flush_tool_results(&mut pending_tool_results, &mut messages);
            if let Some(text) = extract_text(&msg.content) {
                if !text.is_empty() {
                    system_parts.push(SystemMessage { text });
                }
            }
            continue;
        }

        if role == "tool" {
            // tool_result 是 user 消息里的 block，先缓冲
            pending_tool_results.push(build_tool_result(msg));
            continue;
        }

        // 非 tool 消息：先把缓冲的 tool_results 落成一条 user 消息
        flush_tool_results(&mut pending_tool_results, &mut messages);

        match role {
            "user" => {
                has_user = true;
                let blocks = user_content_blocks(&msg.content);
                messages.push(Message {
                    role: "user".to_string(),
                    content: Value::Array(blocks),
                });
            }
            "assistant" => {
                let blocks = assistant_content_blocks(msg);
                // 全空的 assistant 消息跳过，避免上游拒绝空内容块
                if !blocks.is_empty() {
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: Value::Array(blocks),
                    });
                }
            }
            other => {
                tracing::warn!(role = %other, "未知消息角色，已跳过");
            }
        }
    }

    // 收尾：最后一批 tool_results 构成当前轮 user 消息
    flush_tool_results(&mut pending_tool_results, &mut messages);

    if messages.is_empty() {
        return Err(OpenAiError::InvalidRequest(
            "messages must contain at least one non-system message".to_string(),
        ));
    }
    // tool_result 也算 user 上下文（对齐 Kiro-Go validateOpenAIRequestShape）
    let has_user_context = has_user
        || messages
            .iter()
            .any(|m| m.role == "user");
    if !has_user_context {
        return Err(OpenAiError::InvalidRequest(
            "messages must contain at least one user message".to_string(),
        ));
    }

    Ok(MessagesRequest {
        // model 原样传入：thinking 后缀由 handler 层的
        // override_thinking_from_model_name 处理（D8），不在此剥离
        model: req.model.clone(),
        max_tokens: req.resolved_max_tokens(),
        messages,
        stream: req.stream,
        system: if system_parts.is_empty() {
            None
        } else {
            Some(system_parts)
        },
        tools: convert_tools(req.tools.as_deref()),
        // temperature / top_p / tool_choice 接受但不透传：Kiro 上游无对应字段
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    })
}

/// 把缓冲的 tool_results 落成一条 user 消息
fn flush_tool_results(pending: &mut Vec<Value>, messages: &mut Vec<Message>) {
    if pending.is_empty() {
        return;
    }
    let blocks: Vec<Value> = std::mem::take(pending);
    messages.push(Message {
        role: "user".to_string(),
        content: Value::Array(blocks),
    });
}

/// tool role -> tool_result block
fn build_tool_result(msg: &ChatMessage) -> Value {
    let content = extract_text(&msg.content).unwrap_or_default();
    json!({
        "type": "tool_result",
        "tool_use_id": msg.tool_call_id.clone().unwrap_or_default(),
        "content": content,
    })
}

/// user content -> Anthropic content blocks
fn user_content_blocks(content: &Value) -> Vec<Value> {
    match content {
        Value::String(s) => vec![json!({"type": "text", "text": s})],
        Value::Array(parts) => {
            let mut blocks = Vec::with_capacity(parts.len());
            for part in parts {
                if let Some(block) = convert_part(part) {
                    blocks.push(block);
                }
            }
            if blocks.is_empty() {
                blocks.push(json!({"type": "text", "text": ""}));
            }
            blocks
        }
        _ => vec![json!({"type": "text", "text": ""})],
    }
}

/// 单个 content part -> Anthropic block
fn convert_part(part: &Value) -> Option<Value> {
    let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match part_type {
        "text" | "input_text" => {
            let text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
            Some(json!({"type": "text", "text": text}))
        }
        "image_url" | "input_image" => {
            let url = part
                .get("image_url")
                .and_then(|v| v.get("url"))
                .and_then(|v| v.as_str())
                .or_else(|| part.get("image_url").and_then(|v| v.as_str()))
                .unwrap_or("");
            parse_data_url(url).map(|src| {
                json!({
                    "type": "image",
                    "source": {
                        "type": src.source_type,
                        "media_type": src.media_type,
                        "data": src.data,
                    }
                })
            })
        }
        other => {
            tracing::warn!(part_type = %other, "未知 content part 类型，已跳过");
            None
        }
    }
}

/// 解析 base64 data URL；远程 http/https URL 返回 None（Kiro 上游不拉取远程图片）
fn parse_data_url(url: &str) -> Option<ImageSource> {
    if !url.starts_with("data:") {
        if !url.is_empty() {
            tracing::warn!("远程图片 URL 不受支持，已跳过（Kiro 上游不拉取远程资源）");
        }
        return None;
    }
    let rest = &url["data:".len()..];
    let (meta, data) = rest.split_once(",")?;
    if !meta.ends_with(";base64") {
        tracing::warn!("非 base64 data URL 不受支持，已跳过");
        return None;
    }
    let media_type = meta.trim_end_matches(";base64");
    if media_type.is_empty() || data.is_empty() {
        return None;
    }
    Some(ImageSource {
        source_type: "base64".to_string(),
        media_type: media_type.to_string(),
        data: data.to_string(),
    })
}

/// assistant content + tool_calls -> Anthropic blocks
fn assistant_content_blocks(msg: &ChatMessage) -> Vec<Value> {
    let mut blocks = Vec::new();

    if let Some(text) = extract_text(&msg.content) {
        if !text.is_empty() {
            blocks.push(json!({"type": "text", "text": text}));
        }
    }

    if let Some(calls) = &msg.tool_calls {
        for call in calls {
            // arguments 是 JSON 字符串；解析失败退化为空对象并 warn
            // （与 Kiro-Go translator.go:1207 同一策略）
            let input: Value = if call.function.arguments.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&call.function.arguments).unwrap_or_else(|e| {
                    tracing::warn!(
                        tool_call_id = %call.id,
                        "tool_calls.arguments JSON 解析失败: {}，退化为空对象",
                        e
                    );
                    json!({})
                })
            };
            blocks.push(json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.function.name,
                "input": input,
            }));
        }
    }

    blocks
}

/// 从 string / parts 数组中提取纯文本
fn extract_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let mut out = String::new();
            for part in parts {
                let is_text = part
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(|t| t == "text" || t == "input_text" || t == "output_text")
                    .unwrap_or(false);
                if is_text {
                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                        out.push_str(t);
                    }
                } else if let Some(s) = part.as_str() {
                    out.push_str(s);
                }
            }
            Some(out)
        }
        Value::Null => None,
        _ => None,
    }
}

/// OpenAI tools -> Anthropic Tool
fn convert_tools(tools: Option<&[OpenAiTool]>) -> Option<Vec<Tool>> {
    let tools = tools?;
    if tools.is_empty() {
        return None;
    }
    let converted: Vec<Tool> = tools
        .iter()
        .filter(|t| {
            if t.name.is_empty() {
                // 无名工具（web_search / tool_search 序列化后不带 name）在此丢弃。
                // 必须留痕：静默丢弃会让「模型说没有某能力」无从诊断。
                tracing::warn!(
                    tool_type = %t.tool_type,
                    "工具定义缺少 name，已丢弃（上游要求具名 toolSpecification）"
                );
                return false;
            }
            true
        })
        .map(|t| Tool {
            tool_type: None,
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: schema_to_map(&t.parameters),
            max_uses: None,
        })
        .collect();
    if converted.is_empty() {
        None
    } else {
        Some(converted)
    }
}

fn schema_to_map(schema: &Value) -> std::collections::HashMap<String, Value> {
    match schema {
        Value::Object(map) => map.clone().into_iter().collect(),
        _ => std::collections::HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(body: &str) -> ChatCompletionRequest {
        serde_json::from_str(body).expect("请求反序列化失败")
    }

    fn convert(body: &str) -> MessagesRequest {
        to_messages_request(&parse(body)).expect("转换失败")
    }

    #[test]
    fn test_empty_messages_rejected() {
        let err = to_messages_request(&parse(r#"{"model":"m","messages":[]}"#)).unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_no_user_message_rejected() {
        let err = to_messages_request(&parse(
            r#"{"model":"m","messages":[{"role":"system","content":"s"}]}"#,
        ))
        .unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(err.message().contains("user") || err.message().contains("non-system"));
    }

    #[test]
    fn test_system_messages_merged_in_order() {
        let req = convert(
            r#"{"model":"m","messages":[
                {"role":"system","content":"first"},
                {"role":"developer","content":"second"},
                {"role":"user","content":"hi"}]}"#,
        );
        let sys = req.system.expect("system 缺失");
        assert_eq!(sys.len(), 2);
        assert_eq!(sys[0].text, "first");
        assert_eq!(sys[1].text, "second");
    }

    #[test]
    fn test_model_passed_through_unchanged() {
        // thinking 后缀不在此处剥离（D8）
        let req = convert(
            r#"{"model":"claude-sonnet-4.5-thinking","messages":[{"role":"user","content":"hi"}]}"#,
        );
        assert_eq!(req.model, "claude-sonnet-4.5-thinking");
        assert!(req.thinking.is_none(), "converter 不应设置 thinking");
    }

    #[test]
    fn test_tool_calls_to_tool_use() {
        let req = convert(
            r#"{"model":"m","messages":[
                {"role":"user","content":"hi"},
                {"role":"assistant","content":null,"tool_calls":[
                    {"id":"call_1","type":"function",
                     "function":{"name":"get_weather","arguments":"{\"city\":\"SH\"}"}}]}]}"#,
        );
        let blocks = req.messages[1].content.as_array().unwrap();
        assert_eq!(blocks[0]["type"], "tool_use");
        assert_eq!(blocks[0]["id"], "call_1");
        assert_eq!(blocks[0]["name"], "get_weather");
        assert_eq!(blocks[0]["input"]["city"], "SH");
    }

    #[test]
    fn test_malformed_arguments_degrades_to_empty_object() {
        let req = convert(
            r#"{"model":"m","messages":[
                {"role":"user","content":"hi"},
                {"role":"assistant","content":null,"tool_calls":[
                    {"id":"c","function":{"name":"f","arguments":"not json"}}]}]}"#,
        );
        let blocks = req.messages[1].content.as_array().unwrap();
        assert_eq!(blocks[0]["input"], json!({}));
    }

    #[test]
    fn test_consecutive_tool_messages_collapse_into_one_user() {
        let req = convert(
            r#"{"model":"m","messages":[
                {"role":"user","content":"hi"},
                {"role":"assistant","content":null,"tool_calls":[
                    {"id":"c1","function":{"name":"f","arguments":"{}"}},
                    {"id":"c2","function":{"name":"g","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"c1","content":"r1"},
                {"role":"tool","tool_call_id":"c2","content":"r2"}]}"#,
        );
        // user / assistant / user(两个 tool_result)
        assert_eq!(req.messages.len(), 3);
        assert_eq!(req.messages[2].role, "user");
        let blocks = req.messages[2].content.as_array().unwrap();
        assert_eq!(blocks.len(), 2, "两条 tool 消息应归集到同一 user 消息");
        assert_eq!(blocks[0]["type"], "tool_result");
        assert_eq!(blocks[0]["tool_use_id"], "c1");
        assert_eq!(blocks[0]["content"], "r1");
        assert_eq!(blocks[1]["tool_use_id"], "c2");
    }

    #[test]
    fn test_tool_result_followed_by_user_splits() {
        let req = convert(
            r#"{"model":"m","messages":[
                {"role":"user","content":"hi"},
                {"role":"assistant","content":null,"tool_calls":[
                    {"id":"c1","function":{"name":"f","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"c1","content":"r1"},
                {"role":"user","content":"next"}]}"#,
        );
        assert_eq!(req.messages.len(), 4);
        let tr = req.messages[2].content.as_array().unwrap();
        assert_eq!(tr[0]["type"], "tool_result");
        let last = req.messages[3].content.as_array().unwrap();
        assert_eq!(last[0]["text"], "next");
    }

    #[test]
    fn test_image_data_url_converted() {
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":[
                {"type":"text","text":"看图"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,AAAB"}}]}]}"#,
        );
        let blocks = req.messages[0].content.as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["text"], "看图");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["type"], "base64");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
        assert_eq!(blocks[1]["source"]["data"], "AAAB");
    }

    #[test]
    fn test_remote_image_url_skipped() {
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":[
                {"type":"text","text":"看图"},
                {"type":"image_url","image_url":{"url":"https://example.com/a.png"}}]}]}"#,
        );
        let blocks = req.messages[0].content.as_array().unwrap();
        assert_eq!(blocks.len(), 1, "远程图片应被跳过而非导致失败");
        assert_eq!(blocks[0]["type"], "text");
    }

    #[test]
    fn test_tools_converted() {
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}],
                "tools":[{"type":"function","function":{"name":"f","description":"d",
                    "parameters":{"type":"object","properties":{}}}}]}"#,
        );
        let tools = req.tools.expect("tools 缺失");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "f");
        assert_eq!(tools[0].description, "d");
        assert_eq!(tools[0].input_schema.get("type").unwrap(), "object");
        assert!(tools[0].tool_type.is_none(), "普通 function tool 不应带 type");
    }

    #[test]
    fn test_temperature_and_top_p_not_forwarded() {
        // MessagesRequest 无 temperature/top_p 字段，能编译即证明未透传；
        // 这里断言请求不因携带它们而失败
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}],
                "temperature":0.7,"top_p":0.9}"#,
        );
        assert_eq!(req.messages.len(), 1);
    }

    #[test]
    fn test_tool_choice_not_forwarded() {
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}],
                "tool_choice":"auto"}"#,
        );
        assert!(req.tool_choice.is_none());
    }

    #[test]
    fn test_max_tokens_applied() {
        let req = convert(r#"{"model":"m","messages":[{"role":"user","content":"hi"}],"max_tokens":123}"#);
        assert_eq!(req.max_tokens, 123);

        let dflt = convert(r#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#);
        assert_eq!(dflt.max_tokens, super::super::types::DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn test_stream_flag_propagated() {
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
        );
        assert!(req.stream);
    }

    #[test]
    fn test_web_search_named_tool_stays_ordinary(){
        // D10：Chat 端点不劫持 web_search，它就是个普通 function tool
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}],
                "tools":[{"type":"function","function":{"name":"web_search",
                    "description":"search","parameters":{"type":"object"}}}]}"#,
        );
        let tools = req.tools.expect("tools 缺失");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "web_search");
        assert!(
            tools[0].tool_type.is_none(),
            "不得被当成 Anthropic server-side web_search 工具"
        );
    }

    #[test]
    fn test_unnamed_tool_dropped_others_kept() {
        // web_search / tool_search 序列化后不带 name，会被丢弃（带 warn）
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}],
                "tools":[{"type":"web_search"},
                         {"type":"function","name":"keep",
                          "parameters":{"type":"object"}}]}"#,
        );
        let tools = req.tools.expect("其余工具应保留");
        assert_eq!(tools.len(), 1, "无名工具被丢弃，具名工具不受影响");
        assert_eq!(tools[0].name, "keep");
    }

    #[test]
    fn test_all_tools_unnamed_yields_none() {
        let req = convert(
            r#"{"model":"m","messages":[{"role":"user","content":"hi"}],
                "tools":[{"type":"web_search"}]}"#,
        );
        assert!(req.tools.is_none(), "全部被丢弃时不应留空数组");
    }

    #[test]
    fn test_empty_assistant_message_skipped() {
        let req = convert(
            r#"{"model":"m","messages":[
                {"role":"user","content":"hi"},
                {"role":"assistant","content":null}]}"#,
        );
        assert_eq!(req.messages.len(), 1, "空 assistant 消息应被跳过");
    }

    #[test]
    fn test_unknown_role_skipped() {
        let req = convert(
            r#"{"model":"m","messages":[
                {"role":"user","content":"hi"},
                {"role":"function","content":"legacy"}]}"#,
        );
        assert_eq!(req.messages.len(), 1);
    }
}
