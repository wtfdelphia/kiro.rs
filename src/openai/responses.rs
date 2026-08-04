//! Responses 请求归一：`input` / `instructions` -> `ChatCompletionRequest`
//!
//! 对齐 Kiro-Go `proxy/responses_input.go`。归一后复用 Phase B 的
//! `to_messages_request`，不写第二套上游转换逻辑。

use serde_json::{Value, json};

use super::error::OpenAiError;
use super::responses_tools::{ToolRewriteMap, normalize_tools, rewrite_tool_choice};
use super::responses_types::ResponsesRequest;

/// 归一结果：等价的 Chat Completions 请求 JSON + 响应侧还原所需的映射
///
/// 用 JSON 中转而非直接构造 `ChatCompletionRequest`，因为后者的字段是私有
/// `Deserialize` 结构；走 serde 能保证与真实 Chat 请求走完全相同的解析路径。
///
/// `ToolRewriteMap` 不走 `prepare`（那是 Chat 端点共享函数，不注入 Responses
/// 专属概念），由调用方直接接住并交给响应侧，见 design D3。
pub fn to_chat_request_json(
    req: &ResponsesRequest,
) -> Result<(Value, ToolRewriteMap), OpenAiError> {
    // 首版无状态：不静默丢历史（D2）
    if req.wants_stateful() {
        return Err(OpenAiError::InvalidRequest(
            "previous_response_id is not supported: this service does not enable stateful \
             continuation. Send the full conversation in `input` instead."
                .to_string(),
        ));
    }

    if req.text.is_some() {
        tracing::warn!(
            "请求携带 text.format（结构化输出），但上游无对应能力，该字段被忽略：\
             Kiro 的 userInputMessageContext 只有 toolResults / tools"
        );
    }

    let (mut messages, extracted_tools) = parse_input(&req.input)?;

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

    // 工具来源合并：顶层 tools 在前，input 里的 additional_tools 在后
    //
    // 顶层 tools 经 OpenAiTool 反序列化，custom 的 format 与 namespace 的内层
    // tools[] 已丢失；additional_tools 走 Value 保真路径（design D4.1）。
    let mut tool_values: Vec<Value> = Vec::new();
    if let Some(tools) = &req.tools {
        // 重新序列化为 Chat 端点接受的形状（顶层 name/parameters 也被接受）
        tool_values.extend(tools.iter().map(|t| {
            json!({
                "type": if t.tool_type.is_empty() { "function" } else { &t.tool_type },
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        }));
    }
    tool_values.extend(extracted_tools);

    let mut rewrite = ToolRewriteMap::default();
    if !tool_values.is_empty() {
        let (normalized, map) = normalize_tools(tool_values)?;
        rewrite = map;
        if !normalized.is_empty() {
            chat["tools"] = Value::Array(normalized);
        }
    }

    // tool_choice 改写：客户端方言（custom / namespace）上游不认
    if let Some(choice) = &req.tool_choice {
        if let Some(rewritten) = rewrite_tool_choice(choice) {
            chat["tool_choice"] = rewritten;
        }
    }

    Ok((chat, rewrite))
}

/// `input` 三种顶层形状 -> (消息数组, 从 `additional_tools` 提取的工具定义)
fn parse_input(input: &Value) -> Result<(Vec<Value>, Vec<Value>), OpenAiError> {
    match input {
        Value::Null => Ok((Vec::new(), Vec::new())),
        Value::String(s) => {
            if s.trim().is_empty() {
                Ok((Vec::new(), Vec::new()))
            } else {
                Ok((vec![json!({"role": "user", "content": s})], Vec::new()))
            }
        }
        Value::Array(items) => Ok(convert_items(items)),
        Value::Object(_) => Ok(convert_items(std::slice::from_ref(input))),
        _ => Err(OpenAiError::InvalidRequest(
            "unsupported input shape: expected string, array or object".to_string(),
        )),
    }
}

/// 逐 item 转换 -> (消息数组, 提取到的工具定义)
///
/// 关键机制：不带 role 的裸 `input_text` / `input_image` item 要累积到
/// pending，在遇到下一个带 role 的 item 或结尾时 flush 成一条 user 消息。
/// 漏掉这个机制会让纯 parts 形式的 input 丢内容。
fn convert_items(items: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let mut messages: Vec<Value> = Vec::with_capacity(items.len());
    let mut pending: Vec<Value> = Vec::new();
    let mut tools: Vec<Value> = Vec::new();

    for item in items {
        let Some(obj) = item.as_object() else { continue };
        let item_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let role = obj.get("role").and_then(|v| v.as_str()).unwrap_or("");

        match item_type {
            // responses-lite：工具定义搬到了 input 里（无顶层 tools 字段）。
            // 该 item 是 tools 字段的载体，不是对话内容，不产生任何消息。
            "additional_tools" => {
                if let Some(list) = obj.get("tools").and_then(|v| v.as_array()) {
                    tools.extend(list.iter().cloned());
                } else {
                    tracing::warn!("additional_tools item 缺少 tools 数组");
                }
            }
            "message" => {
                flush_pending(&mut pending, &mut messages);
                if let Some(msg) = build_message(item, role) {
                    messages.push(msg);
                }
            }
            // `custom_tool_call_output` 归并到此：客户端方言的工具结果。
            // 漏掉它会让上一轮的调用与结果双双消失（Kiro 要求同轮配对），
            // 模型将以为自己从未调用过工具而重复执行。
            "function_call_output" | "tool_result" | "custom_tool_call_output" => {
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
            // `custom_tool_call` 归并到此：其载荷是裸文本 `input`，
            // 须包装成降级后 schema 约定的 `{"input": ...}`。
            "function_call" | "custom_tool_call" => {
                flush_pending(&mut pending, &mut messages);

                // namespace 限定的调用：拼成与请求侧 tools 一致的展平名。
                // 无条件拼接——历史与当前工具集未必一致，但同一工具在两处必须同名。
                let raw_name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let name = match obj.get("namespace").and_then(|v| v.as_str()) {
                    Some(ns) if !ns.is_empty() && !raw_name.is_empty() => {
                        super::responses_tools::flatten_namespace_name(ns, raw_name)
                    }
                    _ => raw_name.to_string(),
                };

                let arguments = if item_type == "custom_tool_call" {
                    let input = obj.get("input").map(stringify).unwrap_or_default();
                    serde_json::to_string(&json!({"input": input})).unwrap_or_default()
                } else {
                    obj.get("arguments").map(stringify).unwrap_or_default()
                };

                let call = json!({
                    "id": first_str(obj, &["call_id", "id"]),
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments,
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
                    match build_message(item, role) {
                        Some(msg) => messages.push(msg),
                        // 带 role 但无 content/text 的 item 会走到这里。
                        // `additional_tools` 曾因此被静默吞掉，故必须留痕。
                        None => tracing::warn!(
                            item_type = %item_type,
                            role = %role,
                            "input item 带 role 但无可用内容，已跳过"
                        ),
                    }
                } else {
                    tracing::warn!(item_type = %item_type, "未知 input item 类型，已跳过");
                }
            }
        }
    }

    flush_pending(&mut pending, &mut messages);
    (messages, tools)
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
        to_chat_request_json(&req(body)).expect("归一失败").0
    }

    fn tools_of(body: &str) -> Vec<Value> {
        chat(body)["tools"].as_array().cloned().unwrap_or_default()
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

    // === responses-lite：additional_tools 承载工具 ===

    /// 真实抓包形状：无顶层 tools，工具在 input[0] 的 additional_tools 里
    ///
    /// 该形状曾使全部工具静默丢失（模型因此声明自己没有终端能力）。
    fn lite_body() -> &'static str {
        r#"{
          "model": "gpt-5.6-sol",
          "input": [
            {"type":"additional_tools","role":"developer","tools":[
              {"type":"custom","name":"exec","description":"Run JavaScript code",
               "format":{"type":"grammar","syntax":"lark","definition":"start: SOURCE"}},
              {"type":"function","name":"wait","description":"Wait on a cell","strict":false,
               "parameters":{"type":"object","properties":{"cell_id":{"type":"string"}}}},
              {"type":"function","name":"request_user_input","description":"Ask the user",
               "strict":false,"parameters":{"type":"object","properties":{}}},
              {"type":"namespace","name":"collaboration","description":"Sub-agents",
               "tools":[
                 {"type":"function","name":"spawn_agent","description":"Spawn","strict":false,
                  "parameters":{"type":"object","properties":{
                    "message":{"type":"string","encrypted":true}}}},
                 {"type":"function","name":"wait_agent","description":"Wait","strict":false,
                  "parameters":{"type":"object","properties":{"target":{"type":"string"}}}}
               ]}
            ]},
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"sys"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"run git status"}]}
          ]
        }"#
    }

    #[test]
    fn test_additional_tools_reach_upstream() {
        let tools = tools_of(lite_body());
        let names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap_or(""))
            .collect();

        // 3 个顶层 + 2 个展平自 namespace
        assert_eq!(tools.len(), 5, "实际工具: {:?}", names);
        assert!(names.contains(&"exec"));
        assert!(names.contains(&"wait"));
        assert!(names.contains(&"request_user_input"));
        assert!(names.contains(&"collaboration__spawn_agent"));
        assert!(names.contains(&"collaboration__wait_agent"));
    }

    #[test]
    fn test_additional_tools_schemas_not_empty() {
        for t in tools_of(lite_body()) {
            let params = &t["parameters"];
            assert_eq!(
                params["type"], "object",
                "工具 {} 的 schema 应为 object: {}",
                t["name"], params
            );
            assert!(
                params.get("properties").is_some(),
                "工具 {} 的 schema 缺少 properties: {}",
                t["name"],
                params
            );
        }
    }

    #[test]
    fn test_additional_tools_item_produces_no_message() {
        let m = msgs(lite_body());
        // 仅 developer 与 user 两条，additional_tools 不产生消息
        // （developer -> system 的归并在下游 converter.rs 完成，归一层保留原 role）
        assert_eq!(m.len(), 2, "实际消息: {:?}", m);
        assert_eq!(m[0]["role"], "developer");
        assert_eq!(m[1]["role"], "user");
        let joined = serde_json::to_string(&m).unwrap();
        assert!(
            !joined.contains("additional_tools"),
            "工具承载 item 不应出现在消息里: {}",
            joined
        );
    }

    #[test]
    fn test_top_level_tools_merged_before_additional() {
        let c = chat(
            r#"{"input":[
                 {"type":"additional_tools","role":"developer","tools":[
                   {"type":"function","name":"from_input","parameters":{"type":"object"}}]},
                 {"type":"message","role":"user","content":"hi"}],
               "tools":[{"type":"function","name":"from_top","parameters":{"type":"object"}}]}"#,
        );
        let tools = c["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "from_top", "顶层 tools 在前");
        assert_eq!(tools[1]["name"], "from_input");
    }

    #[test]
    fn test_no_tools_field_when_none_present() {
        let c = chat(r#"{"input":"hi"}"#);
        assert!(c.get("tools").is_none(), "无工具时不应写 tools 字段");
    }

    #[test]
    fn test_text_format_does_not_reject_request() {
        // 上游无结构化输出能力，但该字段不应导致请求失败（只打 warn）
        let c = chat(
            r#"{"input":"hi","text":{"format":{"type":"json_schema","strict":true,
               "schema":{"type":"object"}}}}"#,
        );
        assert_eq!(c["messages"][0]["content"], "hi");
    }

    // === 历史 item 改写 ===

    #[test]
    fn test_custom_tool_call_becomes_function_call_with_input_wrapper() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"hi"},
                 {"type":"custom_tool_call","call_id":"c1","name":"exec",
                  "input":"const x = 1;"}]}"#,
        );
        let call = &m[1]["tool_calls"][0];
        assert_eq!(m[1]["role"], "assistant");
        assert_eq!(call["id"], "c1");
        assert_eq!(call["function"]["name"], "exec");

        let args: Value = serde_json::from_str(call["function"]["arguments"].as_str().unwrap())
            .expect("arguments 应为合法 JSON");
        assert_eq!(args["input"], "const x = 1;");
    }

    #[test]
    fn test_custom_tool_call_input_preserves_newlines_and_quotes() {
        let src = "const s = \"a\\nb\";\ntext(\"ok\");";
        let body = serde_json::to_string(&json!({
            "input": [
                {"type":"message","role":"user","content":"hi"},
                {"type":"custom_tool_call","call_id":"c1","name":"exec","input":src}
            ]
        }))
        .unwrap();
        let m = msgs(&body);
        let args: Value =
            serde_json::from_str(m[1]["tool_calls"][0]["function"]["arguments"].as_str().unwrap())
                .unwrap();
        assert_eq!(args["input"], src, "换行与引号须无损");
    }

    #[test]
    fn test_custom_tool_call_output_becomes_tool_message() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"hi"},
                 {"type":"custom_tool_call_output","call_id":"c1","output":"done"}]}"#,
        );
        assert_eq!(m[1]["role"], "tool");
        assert_eq!(m[1]["tool_call_id"], "c1");
        assert_eq!(m[1]["content"], "done");
    }

    #[test]
    fn test_custom_tool_call_output_stringifies_non_string() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"hi"},
                 {"type":"custom_tool_call_output","call_id":"c1",
                  "output":{"exit_code":0,"stdout":"ok"}}]}"#,
        );
        let content = m[1]["content"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(content).expect("对象 output 应被 JSON 字符串化");
        assert_eq!(parsed["exit_code"], 0);
    }

    #[test]
    fn test_custom_tool_call_output_null_becomes_empty() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"hi"},
                 {"type":"custom_tool_call_output","call_id":"c1","output":null}]}"#,
        );
        assert_eq!(m[1]["content"], "");
    }

    #[test]
    fn test_function_call_with_namespace_uses_flattened_name() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"hi"},
                 {"type":"function_call","call_id":"c1","namespace":"collaboration",
                  "name":"spawn_agent","arguments":"{}"}]}"#,
        );
        assert_eq!(
            m[1]["tool_calls"][0]["function"]["name"],
            "collaboration__spawn_agent"
        );
    }

    #[test]
    fn test_function_call_without_namespace_keeps_bare_name() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"hi"},
                 {"type":"function_call","call_id":"c1","name":"wait","arguments":"{}"}]}"#,
        );
        assert_eq!(m[1]["tool_calls"][0]["function"]["name"], "wait");
    }

    #[test]
    fn test_custom_tool_call_with_namespace_flattened() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"hi"},
                 {"type":"custom_tool_call","call_id":"c1","namespace":"ns",
                  "name":"exec","input":"x"}]}"#,
        );
        assert_eq!(m[1]["tool_calls"][0]["function"]["name"], "ns__exec");
    }

    /// 调用与结果必须成对到达，否则模型会重复执行同一操作
    #[test]
    fn test_custom_tool_call_and_output_pair_survive() {
        let m = msgs(
            r#"{"input":[
                 {"type":"message","role":"user","content":"run it"},
                 {"type":"custom_tool_call","call_id":"c1","name":"exec","input":"x"},
                 {"type":"custom_tool_call_output","call_id":"c1","output":"ok"},
                 {"type":"message","role":"user","content":"again"}]}"#,
        );
        assert_eq!(m.len(), 4, "调用与结果都须保留: {:?}", m);
        assert_eq!(m[1]["tool_calls"][0]["id"], "c1");
        assert_eq!(m[2]["tool_call_id"], "c1");
    }

    // === tool_choice 改写 ===

    #[test]
    fn test_tool_choice_custom_rewritten() {
        let c = chat(
            r#"{"input":"hi","tool_choice":{"type":"custom","name":"exec"},
                "tools":[{"type":"function","name":"exec","parameters":{"type":"object"}}]}"#,
        );
        assert_eq!(c["tool_choice"], json!({"type":"function","name":"exec"}));
    }

    #[test]
    fn test_tool_choice_namespace_becomes_auto() {
        let c = chat(r#"{"input":"hi","tool_choice":{"type":"namespace","name":"ns"}}"#);
        assert_eq!(c["tool_choice"], json!("auto"));
    }

    #[test]
    fn test_tool_choice_function_not_rewritten() {
        let c = chat(r#"{"input":"hi","tool_choice":{"type":"function","name":"f"}}"#);
        assert!(
            c.get("tool_choice").is_none(),
            "非方言形状不改写也不透传（既有行为）"
        );
    }
}
