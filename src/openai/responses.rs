//! Responses 请求归一：`input` / `instructions` -> `ChatCompletionRequest`
//!
//! 对齐 Kiro-Go `proxy/responses_input.go`。归一后复用 Phase B 的
//! `to_messages_request`，不写第二套上游转换逻辑。

use serde_json::{Value, json};

use super::error::OpenAiError;
use super::responses_types::ResponsesRequest;

/// 归一结果：等价的 Chat Completions 请求 JSON
///
/// 用 JSON 中转而非直接构造 `ChatCompletionRequest`，因为后者的字段是私有
/// `Deserialize` 结构；走 serde 能保证与真实 Chat 请求走完全相同的解析路径。
pub fn to_chat_request_json(req: &ResponsesRequest) -> Result<Value, OpenAiError> {
    // 首版无状态：不静默丢历史（D2）
    if req.wants_stateful() {
        return Err(OpenAiError::InvalidRequest(
            "previous_response_id is not supported: this service does not enable stateful \
             continuation. Send the full conversation in `input` instead."
                .to_string(),
        ));
    }

    let mut messages = parse_input(&req.input)?;

    // instructions 作为当前轮 system 指令，置于归一消息之前
    if let Some(instr) = req.instructions.as_deref() {
        if !instr.trim().is_empty() {
            messages.insert(0, json!({"role": "system", "content": instr}));
        }
    }

    if messages.is_empty() {
        return Err(OpenAiError::InvalidRequest(
            "input must contain at least one message".to_string(),
        ));
    }

    let has_user = messages
        .iter()
        .any(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"));
    let has_tool = messages
        .iter()
        .any(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool"));
    if !has_user && !has_tool {
        return Err(OpenAiError::InvalidRequest(
            "input must contain at least one user message".to_string(),
        ));
    }

    let mut chat = json!({
        "model": req.resolved_model(),
        "messages": messages,
        "stream": req.stream,
        "max_tokens": req.resolved_max_tokens(),
    });

    if let Some(tools) = &req.tools {
        // 重新序列化为 Chat 端点接受的形状（顶层 name/parameters 也被接受）
        let tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": if t.tool_type.is_empty() { "function" } else { &t.tool_type },
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
        chat["tools"] = Value::Array(tools_json);
    }

    Ok(chat)
}

/// `input` 三种顶层形状 -> 消息数组
fn parse_input(input: &Value) -> Result<Vec<Value>, OpenAiError> {
    match input {
        Value::Null => Ok(Vec::new()),
        Value::String(s) => {
            if s.trim().is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![json!({"role": "user", "content": s})])
            }
        }
        Value::Array(items) => Ok(convert_items(items)),
        Value::Object(_) => Ok(convert_items(std::slice::from_ref(input))),
        _ => Err(OpenAiError::InvalidRequest(
            "unsupported input shape: expected string, array or object".to_string(),
        )),
    }
}

/// 逐 item 转换
///
/// 关键机制：不带 role 的裸 `input_text` / `input_image` item 要累积到
/// pending，在遇到下一个带 role 的 item 或结尾时 flush 成一条 user 消息。
/// 漏掉这个机制会让纯 parts 形式的 input 丢内容。
fn convert_items(items: &[Value]) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::with_capacity(items.len());
    let mut pending: Vec<Value> = Vec::new();

    for item in items {
        let Some(obj) = item.as_object() else { continue };
        let item_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let role = obj.get("role").and_then(|v| v.as_str()).unwrap_or("");

        match item_type {
            "message" => {
                flush_pending(&mut pending, &mut messages);
                if let Some(msg) = build_message(item, role) {
                    messages.push(msg);
                }
            }
            "function_call_output" | "tool_result" => {
                flush_pending(&mut pending, &mut messages);
                let call_id = first_str(obj, &["call_id", "tool_call_id"]);
                let output = obj
                    .get("output")
                    .or_else(|| obj.get("content"))
                    .map(stringify)
                    .unwrap_or_default();
                messages.push(json!({
                    "role": "tool",
                    "content": output,
                    "tool_call_id": call_id,
                }));
            }
            "function_call" => {
                flush_pending(&mut pending, &mut messages);
                let call = json!({
                    "id": first_str(obj, &["call_id", "id"]),
                    "type": "function",
                    "function": {
                        "name": obj.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "arguments": obj.get("arguments").map(stringify).unwrap_or_default(),
                    }
                });
                // 连续的 function_call 合并进同一条 assistant 消息：Responses API
                // 把并行工具调用拆成多个 item，但 Kiro 要求 tool_use / tool_result
                // 在同一轮配对，拆成多条 assistant 消息会破坏配对。
                if let Some(last) = messages.last_mut() {
                    let is_toolonly_assistant = last.get("role").and_then(|r| r.as_str())
                        == Some("assistant")
                        && last
                            .get("tool_calls")
                            .and_then(|t| t.as_array())
                            .map(|a| !a.is_empty())
                            .unwrap_or(false)
                        && last
                            .get("content")
                            .and_then(|c| c.as_str())
                            .map(|s| s.trim().is_empty())
                            .unwrap_or(true);
                    if is_toolonly_assistant {
                        if let Some(arr) = last.get_mut("tool_calls").and_then(|t| t.as_array_mut())
                        {
                            arr.push(call);
                            continue;
                        }
                    }
                }
                messages.push(json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [call],
                }));
            }
            "input_text" | "text" => {
                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        pending.push(json!({"type": "text", "text": text}));
                    }
                }
            }
            "input_image" | "image" | "image_url" => {
                pending.push(normalize_image_part(item));
            }
            "output_text" => {
                flush_pending(&mut pending, &mut messages);
                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        messages.push(json!({"role": "assistant", "content": text}));
                    }
                }
            }
            _ => {
                // 无 type 但有 role：按 role 建消息
                if !role.is_empty() {
                    flush_pending(&mut pending, &mut messages);
                    if let Some(msg) = build_message(item, role) {
                        messages.push(msg);
                    }
                } else {
                    tracing::warn!(item_type = %item_type, "未知 input item 类型，已跳过");
                }
            }
        }
    }

    flush_pending(&mut pending, &mut messages);
    messages
}

fn flush_pending(pending: &mut Vec<Value>, messages: &mut Vec<Value>) {
    if pending.is_empty() {
        return;
    }
    let parts: Vec<Value> = std::mem::take(pending);
    messages.push(json!({"role": "user", "content": parts}));
}

/// 由 item 构造消息（content 支持 string / parts / 嵌套对象）
fn build_message(item: &Value, role: &str) -> Option<Value> {
    let role = if role.is_empty() { "user" } else { role };
    let obj = item.as_object()?;

    if let Some(content) = obj.get("content") {
        match content {
            Value::String(s) => return Some(json!({"role": role, "content": s})),
            Value::Array(parts) => {
                let mut out: Vec<Value> = Vec::with_capacity(parts.len());
                let mut text_only = String::new();
                let mut has_non_text = false;

                for part in parts {
                    let Some(p) = part.as_object() else { continue };
                    let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match ptype {
                        "input_text" | "text" | "output_text" => {
                            if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                                text_only.push_str(t);
                                out.push(json!({"type": "text", "text": t}));
                            }
                        }
                        "input_image" | "image" | "image_url" => {
                            has_non_text = true;
                            out.push(normalize_image_part(part));
                        }
                        _ => {
                            if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                                if !t.is_empty() {
                                    text_only.push_str(t);
                                    out.push(json!({"type": "text", "text": t}));
                                }
                            }
                        }
                    }
                }

                // 纯文本时降级为字符串 content，减少下游结构层级
                if !has_non_text {
                    return Some(json!({"role": role, "content": text_only}));
                }
                return Some(json!({"role": role, "content": out}));
            }
            Value::Object(_) => return build_message(content, role),
            _ => {}
        }
    }

    if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            return Some(json!({"role": role, "content": text}));
        }
    }

    None
}

/// Responses 的图片 part 统一成 Chat 的 image_url 形状，交给 Phase B 的转换器处理
fn normalize_image_part(part: &Value) -> Value {
    let obj = part.as_object();
    let url = obj
        .and_then(|o| o.get("image_url"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            obj.and_then(|o| o.get("image_url"))
                .and_then(|v| v.get("url"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| obj.and_then(|o| o.get("url")).and_then(|v| v.as_str()))
        .unwrap_or("");
    json!({"type": "image_url", "image_url": {"url": url}})
}

fn first_str(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> String {
    for k in keys {
        if let Some(s) = obj.get(*k).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

/// 任意值转字符串：字符串原样，其它序列化为 JSON
fn stringify(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(body: &str) -> ResponsesRequest {
        serde_json::from_str(body).expect("请求反序列化失败")
    }

    fn chat(body: &str) -> Value {
        to_chat_request_json(&req(body)).expect("归一失败")
    }

    fn msgs(body: &str) -> Vec<Value> {
        chat(body)["messages"].as_array().unwrap().clone()
    }

    #[test]
    fn test_string_input() {
        let m = msgs(r#"{"input":"hello"}"#);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0]["role"], "user");
        assert_eq!(m[0]["content"], "hello");
    }

    #[test]
    fn test_single_object_equals_one_element_array() {
        let item = r#"{"type":"message","role":"user","content":"hi"}"#;
        let obj = msgs(&format!(r#"{{"input":{}}}"#, item));
        let arr = msgs(&format!(r#"{{"input":[{}]}}"#, item));
        assert_eq!(obj, arr);
    }

    #[test]
    fn test_empty_input_rejected() {
        for body in [
            r#"{"input":""}"#,
            r#"{"input":[]}"#,
            r#"{"input":null}"#,
            r#"{}"#,
        ] {
            let err = to_chat_request_json(&req(body)).unwrap_err();
            assert_eq!(
                err.status(),
                axum::http::StatusCode::BAD_REQUEST,
                "body={}",
                body
            );
        }
    }

    #[test]
    fn test_message_item() {
        let m = msgs(r#"{"input":[{"type":"message","role":"user","content":"hi"}]}"#);
        assert_eq!(m[0]["role"], "user");
        assert_eq!(m[0]["content"], "hi");
    }

    #[test]
    fn test_message_item_without_type_uses_role() {
        let m = msgs(r#"{"input":[{"role":"user","content":"hi"}]}"#);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0]["role"], "user");
    }

    #[test]
    fn test_function_call_output_pairing() {
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"function_call","call_id":"c1","name":"f","arguments":"{}"},
                {"type":"function_call_output","call_id":"c1","output":"result"}]}"#,
        );
        let tool = m.iter().find(|x| x["role"] == "tool").expect("缺少 tool 消息");
        assert_eq!(tool["tool_call_id"], "c1");
        assert_eq!(tool["content"], "result");
    }

    #[test]
    fn test_function_call_output_accepts_tool_call_id() {
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"function_call_output","tool_call_id":"c9","output":"r"}]}"#,
        );
        let tool = m.iter().find(|x| x["role"] == "tool").unwrap();
        assert_eq!(tool["tool_call_id"], "c9");
    }

    #[test]
    fn test_function_call_output_falls_back_to_content() {
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"function_call_output","call_id":"c1","content":"from-content"}]}"#,
        );
        let tool = m.iter().find(|x| x["role"] == "tool").unwrap();
        assert_eq!(tool["content"], "from-content");
    }

    #[test]
    fn test_function_call_output_stringifies_non_string() {
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"function_call_output","call_id":"c1","output":{"k":1}}]}"#,
        );
        let tool = m.iter().find(|x| x["role"] == "tool").unwrap();
        assert_eq!(tool["content"], r#"{"k":1}"#);
    }

    #[test]
    fn test_consecutive_function_calls_merge_into_one_assistant() {
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"function_call","call_id":"c1","name":"f","arguments":"{\"a\":1}"},
                {"type":"function_call","call_id":"c2","name":"g","arguments":"{\"b\":2}"}]}"#,
        );
        let assistants: Vec<&Value> = m.iter().filter(|x| x["role"] == "assistant").collect();
        assert_eq!(
            assistants.len(),
            1,
            "并行工具调用必须留在同一条 assistant 消息，实际: {:?}",
            m
        );
        let calls = assistants[0]["tool_calls"].as_array().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["id"], "c1");
        assert_eq!(calls[1]["id"], "c2");
        assert_eq!(calls[1]["function"]["name"], "g");
    }

    #[test]
    fn test_function_call_after_text_assistant_not_merged() {
        // 带文本的 assistant 消息之后的 function_call 应另起一条
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"output_text","text":"thinking..."},
                {"type":"function_call","call_id":"c1","name":"f","arguments":"{}"}]}"#,
        );
        let assistants: Vec<&Value> = m.iter().filter(|x| x["role"] == "assistant").collect();
        assert_eq!(assistants.len(), 2);
    }

    #[test]
    fn test_function_call_arguments_stringified() {
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"function_call","call_id":"c1","name":"f","arguments":{"a":1}}]}"#,
        );
        let a = m.iter().find(|x| x["role"] == "assistant").unwrap();
        assert_eq!(a["tool_calls"][0]["function"]["arguments"], r#"{"a":1}"#);
    }

    #[test]
    fn test_bare_text_and_image_items_collapse_into_user() {
        let m = msgs(
            r#"{"input":[
                {"type":"input_text","text":"看图"},
                {"type":"input_image","image_url":"data:image/png;base64,AAAB"}]}"#,
        );
        assert_eq!(m.len(), 1, "裸 parts 应归集为一条 user 消息");
        assert_eq!(m[0]["role"], "user");
        let parts = m[0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "看图");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,AAAB");
    }

    #[test]
    fn test_pending_flushed_before_roled_item() {
        let m = msgs(
            r#"{"input":[
                {"type":"input_text","text":"first"},
                {"role":"user","content":"second"}]}"#,
        );
        assert_eq!(m.len(), 2, "顺序必须保持");
        assert_eq!(m[0]["content"][0]["text"], "first");
        assert_eq!(m[1]["content"], "second");
    }

    #[test]
    fn test_output_text_becomes_assistant() {
        let m = msgs(
            r#"{"input":[
                {"role":"user","content":"hi"},
                {"type":"output_text","text":"prev answer"}]}"#,
        );
        assert_eq!(m[1]["role"], "assistant");
        assert_eq!(m[1]["content"], "prev answer");
    }

    #[test]
    fn test_message_content_parts_text_only_collapses_to_string() {
        let m = msgs(
            r#"{"input":[{"type":"message","role":"user","content":[
                {"type":"input_text","text":"a"},{"type":"input_text","text":"b"}]}]}"#,
        );
        assert_eq!(m[0]["content"], "ab");
    }

    #[test]
    fn test_message_content_parts_with_image_stays_array() {
        let m = msgs(
            r#"{"input":[{"type":"message","role":"user","content":[
                {"type":"input_text","text":"a"},
                {"type":"input_image","image_url":{"url":"data:image/png;base64,X"}}]}]}"#,
        );
        let parts = m[0]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,X");
    }

    #[test]
    fn test_instructions_becomes_leading_system() {
        let m = msgs(r#"{"input":"hi","instructions":"你是助手"}"#);
        assert_eq!(m[0]["role"], "system");
        assert_eq!(m[0]["content"], "你是助手");
        assert_eq!(m[1]["role"], "user");
    }

    #[test]
    fn test_blank_instructions_ignored() {
        let m = msgs(r#"{"input":"hi","instructions":"   "}"#);
        assert!(m.iter().all(|x| x["role"] != "system"));
    }

    #[test]
    fn test_previous_response_id_rejected() {
        let err =
            to_chat_request_json(&req(r#"{"input":"hi","previous_response_id":"resp_1"}"#))
                .unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(
            err.message().contains("previous_response_id"),
            "错误信息应点明该字段: {}",
            err.message()
        );
        assert!(
            err.message().contains("stateful") || err.message().contains("input"),
            "应说明不支持有状态续接并给出替代做法: {}",
            err.message()
        );
    }

    #[test]
    fn test_blank_previous_response_id_allowed() {
        assert!(to_chat_request_json(&req(r#"{"input":"hi","previous_response_id":""}"#)).is_ok());
    }

    #[test]
    fn test_store_does_not_fail_request() {
        assert!(to_chat_request_json(&req(r#"{"input":"hi","store":true}"#)).is_ok());
    }

    #[test]
    fn test_only_system_rejected() {
        let err = to_chat_request_json(&req(
            r#"{"input":[{"role":"system","content":"s"}]}"#,
        ))
        .unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_tool_only_input_accepted() {
        // 仅含 tool 结果的续轮请求应被接受
        assert!(
            to_chat_request_json(&req(
                r#"{"input":[{"type":"function_call_output","call_id":"c","output":"r"}]}"#
            ))
            .is_ok()
        );
    }

    #[test]
    fn test_chat_json_carries_model_stream_and_tokens() {
        let c = chat(r#"{"model":"gpt-4o","input":"hi","stream":true,"max_output_tokens":42}"#);
        assert_eq!(c["model"], "gpt-4o");
        assert_eq!(c["stream"], true);
        assert_eq!(c["max_tokens"], 42);
    }

    #[test]
    fn test_tools_forwarded_in_chat_json() {
        let c = chat(
            r#"{"input":"hi","tools":[{"type":"function","name":"f","description":"d",
                "parameters":{"type":"object"}}]}"#,
        );
        let tools = c["tools"].as_array().unwrap();
        assert_eq!(tools[0]["name"], "f");
        assert_eq!(tools[0]["parameters"]["type"], "object");
    }

    #[test]
    fn test_normalized_json_parses_as_chat_request() {
        // 归一结果必须能被 Phase B 的类型直接消费
        let c = chat(r#"{"input":"hi","instructions":"sys"}"#);
        let parsed: Result<super::super::types::ChatCompletionRequest, _> =
            serde_json::from_value(c);
        assert!(parsed.is_ok(), "归一结果应可被 ChatCompletionRequest 解析");
    }

    #[test]
    fn test_unknown_item_type_without_role_skipped() {
        let m = msgs(r#"{"input":[{"role":"user","content":"hi"},{"type":"mystery"}]}"#);
        assert_eq!(m.len(), 1);
    }
}
