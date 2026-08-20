//! Event -> `chat.completion.chunk` 状态机
//!
//! 不复用 Anthropic 的 `StreamContext`（它产出 Anthropic 事件），平行实现。
//! thinking 内容走 `reasoning_content`，绝不混进 `content`（D12 相邻约束）。

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use uuid::Uuid;

use crate::anthropic::get_context_window_size;
use crate::kiro::model::events::{Event, EventStreamDiagnostics};

use super::types::Usage;

/// 思考标签
const THINKING_OPEN: &str = "<thinking>";
const THINKING_CLOSE: &str = "</thinking>";

/// 一个待发送的 SSE 数据块
#[derive(Debug, Clone, PartialEq)]
pub enum OpenAiSseChunk {
    /// `data: {json}`
    Data(Value),
    /// `data: [DONE]`
    Done,
    /// SSE 注释行保活（不会被 OpenAI 客户端解析为 chunk）
    Keepalive,
}

impl OpenAiSseChunk {
    pub fn to_sse_string(&self) -> String {
        match self {
            Self::Data(v) => format!("data: {}\n\n", v),
            Self::Done => "data: [DONE]\n\n".to_string(),
            Self::Keepalive => ": keepalive\n\n".to_string(),
        }
    }
}

/// 工具调用的流式累积状态
struct ToolCallState {
    index: i32,
    /// 是否已发出带 id + name 的首块
    announced: bool,
}

pub struct OpenAiStreamContext {
    id: String,
    /// 回显给客户端的模型名：原始请求值（D9）
    model: String,
    created: i64,
    /// 客户端是否要求随流返回 usage
    include_usage: bool,
    thinking_enabled: bool,
    /// 短名 -> 原始工具名（D8 第二项）
    tool_name_map: HashMap<String, String>,

    estimated_input_tokens: i32,
    /// contextUsageEvent 反算的准确输入 tokens
    context_input_tokens: Option<i32>,

    role_sent: bool,
    finish_reason: Option<String>,
    has_tool_use: bool,
    /// tool_use_id -> 状态
    tool_calls: HashMap<String, ToolCallState>,
    /// 被判定为不完整的 tool_use_id，后续同 id 分片全部抑制公开输出
    suppressed_tool_use_ids: HashSet<String>,
    next_tool_index: i32,

    /// thinking 标签跨 chunk 检测缓冲
    thinking_buffer: String,
    in_thinking_block: bool,
    thinking_done: bool,
    /// 输出文本累计（用于估算 completion_tokens）
    output_text_len: usize,
    /// 上游流内硬错误已渲染：抑制后续输出与正常收尾序列
    stream_failed: bool,
    /// Kiro EventStream 脱敏诊断摘要
    diagnostics: EventStreamDiagnostics,
}

impl OpenAiStreamContext {
    pub fn new(
        model: impl Into<String>,
        estimated_input_tokens: i32,
        thinking_enabled: bool,
        include_usage: bool,
        tool_name_map: HashMap<String, String>,
    ) -> Self {
        Self {
            id: format!("chatcmpl-{}", Uuid::new_v4().to_string().replace('-', "")),
            model: model.into(),
            created: chrono::Utc::now().timestamp(),
            include_usage,
            thinking_enabled,
            tool_name_map,
            estimated_input_tokens,
            context_input_tokens: None,
            role_sent: false,
            finish_reason: None,
            has_tool_use: false,
            tool_calls: HashMap::new(),
            suppressed_tool_use_ids: HashSet::new(),
            next_tool_index: 0,
            thinking_buffer: String::new(),
            in_thinking_block: false,
            thinking_done: false,
            output_text_len: 0,
            stream_failed: false,
            diagnostics: EventStreamDiagnostics::default(),
        }
    }

    fn envelope(&self, choices: Value) -> Value {
        json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "created": self.created,
            "model": self.model,
            "choices": choices,
        })
    }

    fn delta_chunk(&self, delta: Value) -> Value {
        self.envelope(json!([{
            "index": 0,
            "delta": delta,
            "finish_reason": Value::Null,
        }]))
    }

    /// 首块：只带 role，不带 content
    fn ensure_role(&mut self, out: &mut Vec<OpenAiSseChunk>) {
        if self.role_sent {
            return;
        }
        self.role_sent = true;
        out.push(OpenAiSseChunk::Data(
            self.delta_chunk(json!({"role": "assistant"})),
        ));
    }

    /// 处理一个 Kiro 事件，返回要发出的 chunk
    pub fn process_kiro_event(&mut self, event: &Event) -> Vec<OpenAiSseChunk> {
        let mut out = Vec::new();
        self.diagnostics.observe(event);

        // 硬错误已渲染：后续事件不再产出客户端可见内容
        if self.stream_failed {
            return out;
        }

        match event {
            Event::AssistantResponse(resp) => {
                self.ensure_role(&mut out);
                self.process_text(&resp.content, &mut out);
            }
            Event::ToolUse(tool_use) => {
                self.process_tool_use(tool_use, &mut out);
            }
            Event::ContextUsage(usage) => {
                let window = get_context_window_size(&self.model);
                self.context_input_tokens =
                    Some((usage.context_usage_percentage * (window as f64) / 100.0) as i32);
                if usage.context_usage_percentage >= 100.0 {
                    self.finish_reason = Some("length".to_string());
                }
            }
            Event::Exception { exception_type, .. }
                if exception_type == crate::kiro::stream_fault::CONTENT_LENGTH_EXCEEDED =>
            {
                // 保留既有语义：内容超长 → length，不作为硬错误
                self.finish_reason = Some("length".to_string());
            }
            _ => {
                if let Some(fault) = crate::kiro::stream_fault::classify_stream_fault(event) {
                    self.stream_failed = true;
                    tracing::error!(
                        code = %fault.code,
                        "收到上游流内硬错误，产出协议错误 chunk: {}",
                        fault.message
                    );
                    out.push(OpenAiSseChunk::Data(self.error_chunk(&fault)));
                    out.push(OpenAiSseChunk::Done);
                }
            }
        }
        out
    }

    /// 硬错误 chunk：空 choices + error 对象（OpenAI 流式错误惯例）
    fn error_chunk(&self, fault: &crate::kiro::stream_fault::StreamFault) -> Value {
        let mut chunk = self.envelope(json!([]));
        chunk["error"] = json!({
            "message": fault.client_message(),
            "type": "server_error",
            "code": fault.code,
        });
        chunk
    }

    /// 文本增量：分离 thinking 与正文
    fn process_text(&mut self, text: &str, out: &mut Vec<OpenAiSseChunk>) {
        if !self.thinking_enabled || self.thinking_done {
            self.emit_content(text, out);
            return;
        }

        self.thinking_buffer.push_str(text);

        loop {
            if self.in_thinking_block {
                if let Some(pos) = self.thinking_buffer.find(THINKING_CLOSE) {
                    let inner: String = self.thinking_buffer[..pos].to_string();
                    if !inner.is_empty() {
                        out.push(OpenAiSseChunk::Data(
                            self.delta_chunk(json!({"reasoning_content": inner})),
                        ));
                    }
                    self.thinking_buffer =
                        self.thinking_buffer[pos + THINKING_CLOSE.len()..].to_string();
                    self.in_thinking_block = false;
                    self.thinking_done = true;
                    // 剥离结束标签后紧跟的空白
                    self.thinking_buffer = self.thinking_buffer.trim_start().to_string();
                    continue;
                }
                // 未见结束标签：保留可能被截断的尾部，其余作为 reasoning 发出
                let safe = safe_flush_len(&self.thinking_buffer, THINKING_CLOSE);
                if safe > 0 {
                    let chunk: String = self.thinking_buffer[..safe].to_string();
                    self.thinking_buffer = self.thinking_buffer[safe..].to_string();
                    out.push(OpenAiSseChunk::Data(
                        self.delta_chunk(json!({"reasoning_content": chunk})),
                    ));
                }
                return;
            }

            match self.thinking_buffer.find(THINKING_OPEN) {
                Some(pos) => {
                    if pos > 0 {
                        let before: String = self.thinking_buffer[..pos].to_string();
                        self.emit_content(&before, out);
                    }
                    self.thinking_buffer =
                        self.thinking_buffer[pos + THINKING_OPEN.len()..].to_string();
                    // 剥离开始标签后紧跟的换行
                    self.thinking_buffer =
                        self.thinking_buffer.trim_start_matches('\n').to_string();
                    self.in_thinking_block = true;
                    continue;
                }
                None => {
                    // 可能是被截断的开始标签，保留尾部
                    let safe = safe_flush_len(&self.thinking_buffer, THINKING_OPEN);
                    if safe > 0 {
                        let chunk: String = self.thinking_buffer[..safe].to_string();
                        self.thinking_buffer = self.thinking_buffer[safe..].to_string();
                        self.emit_content(&chunk, out);
                    }
                    return;
                }
            }
        }
    }

    fn emit_content(&mut self, text: &str, out: &mut Vec<OpenAiSseChunk>) {
        if text.is_empty() {
            return;
        }
        self.output_text_len += text.len();
        out.push(OpenAiSseChunk::Data(
            self.delta_chunk(json!({"content": text})),
        ));
    }

    /// 工具调用增量：首块带 id+name+index，后续只带 arguments 片段
    fn process_tool_use(
        &mut self,
        tool_use: &crate::kiro::model::events::ToolUseEvent,
        out: &mut Vec<OpenAiSseChunk>,
    ) {
        let id = tool_use.tool_use_id.clone();
        if id.is_empty() {
            tracing::warn!("跳过缺少 tool_use_id 的 toolUseEvent public 输出");
            return;
        }

        if self.suppressed_tool_use_ids.contains(&id) {
            return;
        }

        let is_new = !self.tool_calls.contains_key(&id);
        if is_new && tool_use.name.is_empty() {
            self.suppressed_tool_use_ids.insert(id.clone());
            tracing::warn!(
                tool_use_id_hash = %EventStreamDiagnostics::hash_public_id(&id),
                "跳过缺少工具名的 toolUseEvent public 输出"
            );
            return;
        }
        self.ensure_role(out);
        self.has_tool_use = true;

        if is_new {
            let index = self.next_tool_index;
            self.next_tool_index += 1;
            self.tool_calls.insert(
                id.clone(),
                ToolCallState {
                    index,
                    announced: false,
                },
            );
        }

        let (index, announced) = {
            let state = self.tool_calls.get(&id).expect("刚插入必存在");
            (state.index, state.announced)
        };

        if !announced {
            // 工具名还原（D8 第二项）
            let name = self
                .tool_name_map
                .get(&tool_use.name)
                .cloned()
                .unwrap_or_else(|| tool_use.name.clone());
            out.push(OpenAiSseChunk::Data(self.delta_chunk(json!({
                "tool_calls": [{
                    "index": index,
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": "" }
                }]
            }))));
            if let Some(state) = self.tool_calls.get_mut(&id) {
                state.announced = true;
            }
        }

        if !tool_use.input.is_empty() {
            self.output_text_len += tool_use.input.len();
            out.push(OpenAiSseChunk::Data(self.delta_chunk(json!({
                "tool_calls": [{
                    "index": index,
                    "function": { "arguments": tool_use.input }
                }]
            }))));
        }
    }

    /// 收尾：finish_reason chunk -> 可选 usage chunk -> [DONE]
    pub fn finish(&mut self) -> Vec<OpenAiSseChunk> {
        // 硬错误已渲染：错误 chunk 与 [DONE] 已发出，不再产出正常收尾序列
        if self.stream_failed {
            self.diagnostics.log_summary("openai-chat");
            return Vec::new();
        }
        let mut out = Vec::new();
        self.ensure_role(&mut out);

        // thinking 缓冲残留
        let leftover = std::mem::take(&mut self.thinking_buffer);
        if !leftover.is_empty() {
            if self.in_thinking_block {
                out.push(OpenAiSseChunk::Data(
                    self.delta_chunk(json!({"reasoning_content": leftover})),
                ));
            } else {
                self.emit_content(&leftover, &mut out);
            }
        }

        let finish_reason = self.finish_reason.clone().unwrap_or_else(|| {
            if self.has_tool_use {
                "tool_calls".to_string()
            } else {
                "stop".to_string()
            }
        });

        out.push(OpenAiSseChunk::Data(self.envelope(json!([{
            "index": 0,
            "delta": {},
            "finish_reason": finish_reason,
        }]))));

        // usage 仅在客户端明确要求时发送（D12）
        if self.include_usage {
            let usage = self.usage();
            let mut chunk = self.envelope(json!([]));
            chunk["usage"] = serde_json::to_value(usage).expect("usage 序列化失败");
            out.push(OpenAiSseChunk::Data(chunk));
        }

        out.push(OpenAiSseChunk::Done);
        self.diagnostics.log_summary("openai-chat");
        out
    }

    /// prompt_tokens 优先用上游反算值，否则回落估算（D12）
    pub fn usage(&self) -> Usage {
        let prompt = self
            .context_input_tokens
            .unwrap_or(self.estimated_input_tokens);
        let completion = ((self.output_text_len + 3) / 4) as i32;
        Usage::new(prompt, completion.max(1))
    }
}

/// 计算可安全发出的前缀长度
///
/// 标签可能跨 chunk 分割，所以要保留 buf 尾部中「恰好是 tag 前缀」的那一段
/// （取最长者），其余可安全发出。切点保证落在 UTF-8 字符边界。
fn safe_flush_len(buf: &str, tag: &str) -> usize {
    // 从最长可能的部分标签开始试：tag.len()-1 .. 1
    let max_keep = (tag.len() - 1).min(buf.len());
    for keep in (1..=max_keep).rev() {
        let cut = buf.len() - keep;
        if !buf.is_char_boundary(cut) {
            continue;
        }
        if tag.as_bytes().starts_with(&buf.as_bytes()[cut..]) {
            return cut;
        }
    }
    buf.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiro::model::events::{
        ContextUsageEvent, MeteringEvent, ReasoningContentEvent, ToolUseEvent,
    };

    fn text_event(content: &str) -> Event {
        // AssistantResponseEvent 含私有 extra 字段，用 serde 构造
        Event::AssistantResponse(
            serde_json::from_value(json!({"content": content})).expect("构造事件失败"),
        )
    }

    fn tool_event(id: &str, name: &str, input: &str, stop: bool) -> Event {
        Event::ToolUse(ToolUseEvent {
            name: name.to_string(),
            tool_use_id: id.to_string(),
            input: input.to_string(),
            stop,
        })
    }

    fn ctx_usage(pct: f64) -> Event {
        Event::ContextUsage(ContextUsageEvent {
            context_usage_percentage: pct,
        })
    }

    fn new_ctx(thinking: bool, include_usage: bool) -> OpenAiStreamContext {
        OpenAiStreamContext::new("gpt-4o", 100, thinking, include_usage, HashMap::new())
    }

    /// 提取所有 data chunk 的 JSON（跳过 keepalive 与 DONE）
    fn datas(chunks: &[OpenAiSseChunk]) -> Vec<Value> {
        chunks
            .iter()
            .filter_map(|c| match c {
                OpenAiSseChunk::Data(v) => Some(v.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_first_chunk_carries_only_role() {
        let mut ctx = new_ctx(false, false);
        let out = ctx.process_kiro_event(&text_event("hello"));
        let ds = datas(&out);
        assert_eq!(ds[0]["choices"][0]["delta"]["role"], "assistant");
        assert!(
            ds[0]["choices"][0]["delta"].get("content").is_none(),
            "首块不得带 content"
        );
        assert_eq!(ds[1]["choices"][0]["delta"]["content"], "hello");
    }

    #[test]
    fn test_role_sent_only_once() {
        let mut ctx = new_ctx(false, false);
        let mut all = ctx.process_kiro_event(&text_event("a"));
        all.extend(ctx.process_kiro_event(&text_event("b")));
        let role_count = datas(&all)
            .iter()
            .filter(|d| d["choices"][0]["delta"].get("role").is_some())
            .count();
        assert_eq!(role_count, 1);
    }

    #[test]
    fn test_chunk_object_and_model_echo() {
        let mut ctx = new_ctx(false, false);
        let out = ctx.process_kiro_event(&text_event("x"));
        let d = &datas(&out)[0];
        assert_eq!(d["object"], "chat.completion.chunk");
        // D9：回显原始请求 model
        assert_eq!(d["model"], "gpt-4o");
        assert!(d["id"].as_str().unwrap().starts_with("chatcmpl-"));
    }

    #[test]
    fn test_stream_ends_with_done() {
        let mut ctx = new_ctx(false, false);
        let _ = ctx.process_kiro_event(&text_event("x"));
        let fin = ctx.finish();
        assert_eq!(*fin.last().unwrap(), OpenAiSseChunk::Done);
        assert_eq!(fin.last().unwrap().to_sse_string(), "data: [DONE]\n\n");
    }

    #[test]
    fn test_finish_reason_stop() {
        let mut ctx = new_ctx(false, false);
        let _ = ctx.process_kiro_event(&text_event("x"));
        let ds = datas(&ctx.finish());
        assert_eq!(ds.last().unwrap()["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_finish_reason_tool_calls() {
        let mut ctx = new_ctx(false, false);
        let _ = ctx.process_kiro_event(&tool_event("c1", "f", "{}", true));
        let ds = datas(&ctx.finish());
        assert_eq!(
            ds.last().unwrap()["choices"][0]["finish_reason"],
            "tool_calls"
        );
    }

    #[test]
    fn test_finish_reason_length_on_context_full() {
        let mut ctx = new_ctx(false, false);
        let _ = ctx.process_kiro_event(&text_event("x"));
        let _ = ctx.process_kiro_event(&ctx_usage(100.0));
        let ds = datas(&ctx.finish());
        assert_eq!(ds.last().unwrap()["choices"][0]["finish_reason"], "length");
    }

    #[test]
    fn test_finish_reason_length_on_content_length_exceeded() {
        let mut ctx = new_ctx(false, false);
        let _ = ctx.process_kiro_event(&Event::Exception {
            exception_type: "ContentLengthExceededException".to_string(),
            message: String::new(),
        });
        let ds = datas(&ctx.finish());
        assert_eq!(ds.last().unwrap()["choices"][0]["finish_reason"], "length");
    }

    #[test]
    fn test_tool_call_announce_then_arguments() {
        let mut ctx = new_ctx(false, false);
        let mut all = ctx.process_kiro_event(&tool_event("call_1", "get_weather", "", false));
        all.extend(ctx.process_kiro_event(&tool_event("call_1", "get_weather", "{\"ci", false)));
        all.extend(ctx.process_kiro_event(&tool_event("call_1", "get_weather", "ty\":1}", true)));

        let tool_deltas: Vec<Value> = datas(&all)
            .into_iter()
            .filter(|d| d["choices"][0]["delta"].get("tool_calls").is_some())
            .collect();

        // 首块带 id + name
        let first = &tool_deltas[0]["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(first["index"], 0);
        assert_eq!(first["id"], "call_1");
        assert_eq!(first["type"], "function");
        assert_eq!(first["function"]["name"], "get_weather");

        // 后续只带 arguments 片段
        let args: Vec<String> = tool_deltas[1..]
            .iter()
            .map(|d| {
                d["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert_eq!(args.concat(), r#"{"city":1}"#);
        for d in &tool_deltas[1..] {
            let tc = &d["choices"][0]["delta"]["tool_calls"][0];
            assert!(tc.get("id").is_none(), "后续 chunk 不应重复带 id");
            assert!(
                tc["function"].get("name").is_none(),
                "后续 chunk 不应重复带 name"
            );
        }
    }

    #[test]
    fn test_multiple_tools_have_stable_distinct_index() {
        let mut ctx = new_ctx(false, false);
        let mut all = ctx.process_kiro_event(&tool_event("a", "f", "1", false));
        all.extend(ctx.process_kiro_event(&tool_event("b", "g", "2", false)));
        all.extend(ctx.process_kiro_event(&tool_event("a", "f", "3", true)));
        all.extend(ctx.process_kiro_event(&tool_event("b", "g", "4", true)));

        let mut index_by_arg = HashMap::new();
        for d in datas(&all) {
            let tc = &d["choices"][0]["delta"]["tool_calls"][0];
            if let Some(args) = tc["function"].get("arguments").and_then(|v| v.as_str()) {
                if !args.is_empty() {
                    index_by_arg.insert(args.to_string(), tc["index"].as_i64().unwrap());
                }
            }
        }
        // 同一工具的 index 稳定，不同工具的 index 不同
        assert_eq!(index_by_arg["1"], index_by_arg["3"]);
        assert_eq!(index_by_arg["2"], index_by_arg["4"]);
        assert_ne!(index_by_arg["1"], index_by_arg["2"]);
    }

    #[test]
    fn test_tool_name_restored_from_map() {
        // D8 第二项：超长工具名缩短后必须回显原名
        let mut map = HashMap::new();
        map.insert(
            "short_abc".to_string(),
            "very_long_original_tool_name".to_string(),
        );
        let mut ctx = OpenAiStreamContext::new("m", 10, false, false, map);
        let out = ctx.process_kiro_event(&tool_event("c", "short_abc", "", false));
        let d = datas(&out)
            .into_iter()
            .find(|d| d["choices"][0]["delta"].get("tool_calls").is_some())
            .expect("缺少 tool_calls chunk");
        assert_eq!(
            d["choices"][0]["delta"]["tool_calls"][0]["function"]["name"],
            "very_long_original_tool_name"
        );
    }

    #[test]
    fn test_tool_use_missing_name_suppresses_entire_lifecycle() {
        let mut ctx = new_ctx(false, false);

        let first = ctx.process_kiro_event(&tool_event("tool_1", "", "part1", false));
        let second = ctx.process_kiro_event(&tool_event("tool_1", "test_tool", "part2", true));

        assert!(first.is_empty());
        assert!(second.is_empty());
        assert!(ctx.tool_calls.is_empty());
        assert!(ctx.suppressed_tool_use_ids.contains("tool_1"));
        assert!(!ctx.has_tool_use);
    }

    #[test]
    fn test_reasoning_and_metering_events_are_not_public_chat_chunks() {
        let mut ctx = new_ctx(true, false);
        let mut all = ctx.process_kiro_event(&Event::Reasoning(ReasoningContentEvent {
            text: "abc".to_string(),
            signature: "fixture_signature_value".to_string(),
            ..Default::default()
        }));
        all.extend(ctx.process_kiro_event(&Event::Metering(MeteringEvent {
            unit: "request".to_string(),
            unit_plural: "requests".to_string(),
            usage: 1.0,
            ..Default::default()
        })));
        all.extend(ctx.process_kiro_event(&Event::Unknown {
            event_type: "futureEvent".to_string(),
            payload: b"fixture hidden payload".to_vec(),
        }));
        all.extend(ctx.finish());

        let serialized = serde_json::to_string(&datas(&all)).expect("序列化 OpenAI chunk 失败");
        assert!(!serialized.contains("diagnostic"));
        assert!(!serialized.contains("metering"));
        assert!(!serialized.contains("fixture_signature_value"));
        assert!(!serialized.contains("fixture hidden payload"));
        assert!(
            datas(&all)
                .iter()
                .all(|d| d["choices"][0]["delta"].get("reasoning_content").is_none()),
            "reasoningContentEvent 不应直接变成 OpenAI reasoning_content"
        );
    }

    #[test]
    fn test_include_usage_emits_usage_chunk_before_done() {
        let mut ctx = new_ctx(false, true);
        let _ = ctx.process_kiro_event(&text_event("hello"));
        let fin = ctx.finish();

        assert_eq!(*fin.last().unwrap(), OpenAiSseChunk::Done);
        let ds = datas(&fin);
        let usage_chunk = ds.last().expect("缺少 chunk");
        assert!(
            usage_chunk["choices"].as_array().unwrap().is_empty(),
            "usage chunk 的 choices 必须为空数组"
        );
        assert!(usage_chunk["usage"]["prompt_tokens"].is_number());
        assert!(usage_chunk["usage"]["total_tokens"].is_number());
        // finish_reason chunk 在 usage chunk 之前
        assert_eq!(ds[ds.len() - 2]["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_no_usage_chunk_when_not_requested() {
        let mut ctx = new_ctx(false, false);
        let _ = ctx.process_kiro_event(&text_event("hello"));
        let ds = datas(&ctx.finish());
        assert!(
            ds.iter().all(|d| d.get("usage").is_none()),
            "未请求 include_usage 时不得发送 usage chunk"
        );
    }

    #[test]
    fn test_usage_prefers_context_signal() {
        let mut ctx = new_ctx(false, true);
        let _ = ctx.process_kiro_event(&text_event("x"));
        // 反算值应覆盖估算值 100
        let _ = ctx.process_kiro_event(&ctx_usage(50.0));
        let usage = ctx.usage();
        assert_ne!(usage.prompt_tokens, 100, "应使用上游反算值而非估算值");
        assert!(usage.prompt_tokens > 0);
    }

    #[test]
    fn test_usage_falls_back_to_estimate() {
        let mut ctx = new_ctx(false, true);
        let _ = ctx.process_kiro_event(&text_event("x"));
        assert_eq!(ctx.usage().prompt_tokens, 100);
    }

    #[test]
    fn test_thinking_routed_to_reasoning_content() {
        let mut ctx = new_ctx(true, false);
        let out = ctx.process_kiro_event(&text_event("<thinking>思考中</thinking>正文"));
        let ds = datas(&out);

        let reasoning: String = ds
            .iter()
            .filter_map(|d| d["choices"][0]["delta"]["reasoning_content"].as_str())
            .collect();
        let content: String = ds
            .iter()
            .filter_map(|d| d["choices"][0]["delta"]["content"].as_str())
            .collect();

        assert_eq!(reasoning, "思考中");
        assert_eq!(content, "正文");
        assert!(!content.contains("思考中"), "思考内容不得混进 content");
        assert!(!content.contains("<thinking>"), "标签不得出现在 content");
    }

    #[test]
    fn test_thinking_tag_split_across_chunks() {
        let mut ctx = new_ctx(true, false);
        // 标签被拆成三段
        let mut all = ctx.process_kiro_event(&text_event("<thin"));
        all.extend(ctx.process_kiro_event(&text_event("king>思考")));
        all.extend(ctx.process_kiro_event(&text_event("</think")));
        all.extend(ctx.process_kiro_event(&text_event("ing>答案")));
        all.extend(ctx.finish());

        let ds = datas(&all);
        let reasoning: String = ds
            .iter()
            .filter_map(|d| d["choices"][0]["delta"]["reasoning_content"].as_str())
            .collect();
        let content: String = ds
            .iter()
            .filter_map(|d| d["choices"][0]["delta"]["content"].as_str())
            .collect();

        assert_eq!(reasoning, "思考");
        assert_eq!(content, "答案");
        assert!(
            !content.contains("thinking"),
            "被拆分的标签不得泄漏到 content"
        );
    }

    #[test]
    fn test_thinking_disabled_keeps_text_intact() {
        let mut ctx = new_ctx(false, false);
        let out = ctx.process_kiro_event(&text_event("<thinking>x</thinking>y"));
        let content: String = datas(&out)
            .iter()
            .filter_map(|d| d["choices"][0]["delta"]["content"].as_str())
            .collect();
        // thinking 未启用时不解析标签，原文透传
        assert_eq!(content, "<thinking>x</thinking>y");
    }

    #[test]
    fn test_keepalive_is_sse_comment_not_chunk() {
        let s = OpenAiSseChunk::Keepalive.to_sse_string();
        assert!(s.starts_with(':'), "保活必须是 SSE 注释行");
        assert!(!s.contains("data:"), "保活不得是可被解析的 chunk");
    }

    #[test]
    fn test_finish_emits_role_even_without_events() {
        // 上游无任何输出时也要给出合法的最小响应
        let mut ctx = new_ctx(false, false);
        let ds = datas(&ctx.finish());
        assert_eq!(ds[0]["choices"][0]["delta"]["role"], "assistant");
        assert_eq!(ds.last().unwrap()["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_safe_flush_len_keeps_partial_tag() {
        // 尾部是 "<thin"，可能是 <thinking> 的前缀，须保留
        assert_eq!(safe_flush_len("abc<thin", THINKING_OPEN), 3);
        // 尾部无标签前缀，可全发
        assert_eq!(safe_flush_len("abcdef", THINKING_OPEN), 6);
        // 整个 buf 都是前缀
        assert_eq!(safe_flush_len("<thi", THINKING_OPEN), 0);
    }

    #[test]
    fn test_safe_flush_len_respects_utf8_boundary() {
        // 中文尾部不得被切在字符中间
        let buf = "答案是中文";
        let cut = safe_flush_len(buf, THINKING_OPEN);
        assert!(buf.is_char_boundary(cut));
    }

    // === 流内硬错误传播（add-stream-error-propagation） ===

    fn kiro_error_event(code: &str, message: &str) -> Event {
        Event::Error {
            error_code: code.to_string(),
            error_message: message.to_string(),
        }
    }

    #[test]
    fn test_fault_emits_error_chunk_then_done() {
        let mut ctx = new_ctx(false, false);
        let mut all = ctx.process_kiro_event(&text_event("partial"));
        all.extend(ctx.process_kiro_event(&kiro_error_event(
            "InternalServerException",
            "upstream exploded",
        )));

        // 错误 chunk：空 choices + error 对象
        let ds = datas(&all);
        let err = ds
            .iter()
            .find(|d| d.get("error").is_some())
            .expect("应产出错误 chunk");
        assert_eq!(err["choices"], json!([]));
        assert_eq!(err["error"]["type"], "server_error");
        assert_eq!(err["error"]["code"], "InternalServerException");
        assert_eq!(
            err["error"]["message"],
            "Kiro upstream error (InternalServerException): upstream exploded"
        );
        // 错误 chunk 之后紧跟 [DONE]
        let done_pos = all.iter().position(|c| matches!(c, OpenAiSseChunk::Done));
        assert!(done_pos.is_some(), "错误 chunk 后应有 [DONE]");
    }

    #[test]
    fn test_fault_suppresses_normal_finish() {
        let mut ctx = new_ctx(false, false);
        let _ = ctx.process_kiro_event(&kiro_error_event("BadThing", "boom"));

        // finish 不再产出 finish_reason chunk（错误 chunk + DONE 已在 process 时发出）
        let fin = ctx.finish();
        assert!(
            fin.is_empty(),
            "硬错误后 finish() MUST NOT 产出正常 finish_reason chunk"
        );
    }

    #[test]
    fn test_fault_first_wins_and_suppresses_later_content() {
        let mut ctx = new_ctx(false, false);
        let first = ctx.process_kiro_event(&kiro_error_event("First", "one"));
        let err_count = datas(&first).iter().filter(|d| d.get("error").is_some()).count();
        assert_eq!(err_count, 1);

        // 错误后的内容与第二个错误都不再产出 chunk
        assert!(datas(&ctx.process_kiro_event(&text_event("late"))).is_empty());
        assert!(datas(&ctx.process_kiro_event(&kiro_error_event("Second", "two"))).is_empty());
    }

    #[test]
    fn test_content_length_exception_keeps_length_semantics() {
        let mut ctx = new_ctx(false, false);
        let mut all = ctx.process_kiro_event(&text_event("x"));
        all.extend(ctx.process_kiro_event(&Event::Exception {
            exception_type: "ContentLengthExceededException".to_string(),
            message: "too long".to_string(),
        }));

        assert!(
            datas(&all).iter().all(|d| d.get("error").is_none()),
            "ContentLengthExceededException 保留 length 语义，不产出错误 chunk"
        );
        let fin = ctx.finish();
        let ds = datas(&fin);
        let last_data = ds
            .iter()
            .find(|d| d["choices"][0].get("finish_reason").is_some())
            .expect("正常收尾应保留");
        assert_eq!(last_data["choices"][0]["finish_reason"], "length");
    }

    /// 任务 5.2：错误渲染只含上游 code+message 与固定文案，
    /// error 对象不附带任何额外字段（凭据/ARN/请求细节无从注入）
    #[test]
    fn test_fault_rendering_exposes_only_code_and_message() {
        let sensitive = "profile arn:aws:security-profile:::SECRET token=leak";
        let mut ctx = new_ctx(false, false);
        let all = ctx.process_kiro_event(&kiro_error_event("AccessDeniedException", sensitive));

        let ds = datas(&all);
        let err = ds
            .iter()
            .find(|d| d.get("error").is_some())
            .expect("应产出错误 chunk");
        let error_obj = err["error"].as_object().expect("error 应为对象");
        let keys: Vec<&String> = error_obj.keys().collect();
        assert_eq!(
            keys.len(),
            3,
            "error 对象只允许 message/type/code 三个字段，实际: {:?}",
            keys
        );
        assert_eq!(error_obj["type"], "server_error");
        assert_eq!(error_obj["code"], "AccessDeniedException");
        // 消息为固定前缀 + 上游 code/message 的精确拼接，无其他拼接物
        assert_eq!(
            error_obj["message"],
            format!("Kiro upstream error (AccessDeniedException): {}", sensitive)
        );
    }
}
