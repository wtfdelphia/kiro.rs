//! Event -> Responses 语义事件状态机
//!
//! 逐事件对齐 Kiro-Go `responses_handler.go:292-580`。
//! 与 Chat Completions 的差异：SSE 带 `event:` 行；输出按 output item 组织，
//! message item 与每个 function_call item 各占一个 output_index。

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::anthropic::get_context_window_size;
use crate::kiro::model::events::{Event, EventStreamDiagnostics};

use super::responses_types::{
    ResponseOutputItem, ResponsesError, ResponsesObject, ResponsesUsage, output_item_id,
};

/// 一个待发送的 SSE 事件
#[derive(Debug, Clone, PartialEq)]
pub enum ResponsesSseEvent {
    /// `event: <name>\ndata: <json>\n\n`
    Named { event: String, data: Value },
    /// `data: [DONE]`
    Done,
    /// SSE 注释行保活
    Keepalive,
}

impl ResponsesSseEvent {
    pub fn named(event: impl Into<String>, data: Value) -> Self {
        Self::Named {
            event: event.into(),
            data,
        }
    }

    pub fn to_sse_string(&self) -> String {
        match self {
            Self::Named { event, data } => format!("event: {}\ndata: {}\n\n", event, data),
            Self::Done => "data: [DONE]\n\n".to_string(),
            Self::Keepalive => ": keepalive\n\n".to_string(),
        }
    }

    /// 事件名（Done / Keepalive 返回 None）
    #[cfg(test)]
    pub fn event_name(&self) -> Option<&str> {
        match self {
            Self::Named { event, .. } => Some(event),
            _ => None,
        }
    }
}

pub struct ResponsesStreamContext {
    id: String,
    /// 回显给客户端的模型名：原始请求值（D9）
    model: String,
    created_at: i64,
    instructions: Option<String>,
    metadata: Option<HashMap<String, String>>,
    thinking_enabled: bool,
    /// 短名 -> 原始工具名（D8 第二项）
    tool_name_map: HashMap<String, String>,
    /// 客户端方言工具的还原映射（design D3）
    tool_rewrite: super::responses_tools::ToolRewriteMap,

    estimated_input_tokens: i32,
    context_input_tokens: Option<i32>,

    /// 当前 output item 索引
    output_index: i32,
    /// message item 是否已开启
    message_item_id: Option<String>,
    /// 已累积的正文
    text: String,
    /// 已完成的 output items（用于最终 response 对象）
    finished_items: Vec<ResponseOutputItem>,
    /// 工具调用的参数累积：tool_use_id -> (item_id, name, args)
    tool_buffers: HashMap<String, (String, String, String)>,
    /// 被判定为不完整的 tool_use_id，后续同 id 分片全部抑制公开输出
    suppressed_tool_use_ids: HashSet<String>,
    /// 工具调用出现顺序
    tool_order: Vec<String>,

    /// thinking 标签跨 chunk 检测
    thinking_buffer: String,
    in_thinking_block: bool,
    thinking_done: bool,

    /// 是否已发出任何内容（决定上游失败时走 failed 事件还是错误响应）
    pub started: bool,
    failed: bool,
    /// Kiro EventStream 脱敏诊断摘要
    diagnostics: EventStreamDiagnostics,
}

impl ResponsesStreamContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        model: String,
        instructions: Option<String>,
        metadata: Option<HashMap<String, String>>,
        estimated_input_tokens: i32,
        thinking_enabled: bool,
        tool_name_map: HashMap<String, String>,
        tool_rewrite: super::responses_tools::ToolRewriteMap,
    ) -> Self {
        Self {
            id,
            model,
            created_at: chrono::Utc::now().timestamp(),
            instructions,
            metadata,
            thinking_enabled,
            tool_name_map,
            tool_rewrite,
            estimated_input_tokens,
            context_input_tokens: None,
            output_index: 0,
            message_item_id: None,
            text: String::new(),
            finished_items: Vec::new(),
            tool_buffers: HashMap::new(),
            suppressed_tool_use_ids: HashSet::new(),
            tool_order: Vec::new(),
            thinking_buffer: String::new(),
            in_thinking_block: false,
            thinking_done: false,
            started: false,
            failed: false,
            diagnostics: EventStreamDiagnostics::default(),
        }
    }

    /// 快照当前 response 对象（用于 created / in_progress / completed 事件）
    fn snapshot(&self, status: &'static str, output: Vec<ResponseOutputItem>) -> ResponsesObject {
        ResponsesObject {
            id: self.id.clone(),
            object: "response",
            created_at: self.created_at,
            status,
            model: self.model.clone(),
            output,
            usage: self.usage(),
            instructions: self.instructions.clone(),
            metadata: self.metadata.clone(),
            error: None,
        }
    }

    /// 开场：created + in_progress
    pub fn initial_events(&self) -> Vec<ResponsesSseEvent> {
        let snap = self.snapshot("in_progress", Vec::new());
        let payload = |t: &str| {
            json!({
                "type": t,
                "response": serde_json::to_value(&snap).expect("response 序列化失败"),
            })
        };
        vec![
            ResponsesSseEvent::named("response.created", payload("response.created")),
            ResponsesSseEvent::named("response.in_progress", payload("response.in_progress")),
        ]
    }

    pub fn process_kiro_event(&mut self, event: &Event) -> Vec<ResponsesSseEvent> {
        let mut out = Vec::new();
        self.diagnostics.observe(event);

        // 硬错误已渲染：后续事件不再产出客户端可见内容
        if self.failed {
            return out;
        }

        match event {
            Event::AssistantResponse(resp) => self.process_text(&resp.content, &mut out),
            Event::ToolUse(tool_use) => self.process_tool_use(tool_use, &mut out),
            Event::ContextUsage(usage) => {
                let window = get_context_window_size(&self.model);
                self.context_input_tokens =
                    Some((usage.context_usage_percentage * (window as f64) / 100.0) as i32);
            }
            _ => {
                if let Some(fault) = crate::kiro::stream_fault::classify_stream_fault(event) {
                    tracing::error!(
                        code = %fault.code,
                        "收到上游流内硬错误，产出 response.failed: {}",
                        fault.message
                    );
                    out.extend(self.fail(fault.client_message()));
                }
            }
        }
        out
    }

    // === 文本 ===

    fn process_text(&mut self, text: &str, out: &mut Vec<ResponsesSseEvent>) {
        if !self.thinking_enabled || self.thinking_done {
            self.emit_text(text, out);
            return;
        }

        self.thinking_buffer.push_str(text);
        loop {
            if self.in_thinking_block {
                if let Some(pos) = self.thinking_buffer.find("</thinking>") {
                    // thinking 内容首版不进 output（Responses 无稳定的 reasoning part 契约）
                    self.thinking_buffer =
                        self.thinking_buffer[pos + "</thinking>".len()..].to_string();
                    self.in_thinking_block = false;
                    self.thinking_done = true;
                    self.thinking_buffer = self.thinking_buffer.trim_start().to_string();
                    continue;
                }
                // 全部是 thinking，丢弃但保留可能被截断的尾部
                let safe = safe_flush_len(&self.thinking_buffer, "</thinking>");
                self.thinking_buffer = self.thinking_buffer[safe..].to_string();
                return;
            }

            match self.thinking_buffer.find("<thinking>") {
                Some(pos) => {
                    if pos > 0 {
                        let before = self.thinking_buffer[..pos].to_string();
                        self.emit_text(&before, out);
                    }
                    self.thinking_buffer =
                        self.thinking_buffer[pos + "<thinking>".len()..].to_string();
                    self.in_thinking_block = true;
                    continue;
                }
                None => {
                    let safe = safe_flush_len(&self.thinking_buffer, "<thinking>");
                    if safe > 0 {
                        let chunk = self.thinking_buffer[..safe].to_string();
                        self.thinking_buffer = self.thinking_buffer[safe..].to_string();
                        self.emit_text(&chunk, out);
                    }
                    return;
                }
            }
        }
    }

    fn emit_text(&mut self, text: &str, out: &mut Vec<ResponsesSseEvent>) {
        if text.is_empty() {
            return;
        }
        self.ensure_message_item(out);
        self.text.push_str(text);
        self.started = true;

        let item_id = self.message_item_id.clone().expect("message item 必已开启");
        out.push(ResponsesSseEvent::named(
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": item_id,
                "output_index": self.output_index,
                "content_index": 0,
                "delta": text,
            }),
        ));
    }

    /// 开启 message item：output_item.added + content_part.added
    fn ensure_message_item(&mut self, out: &mut Vec<ResponsesSseEvent>) {
        if self.message_item_id.is_some() {
            return;
        }
        let item_id = output_item_id("msg");
        self.message_item_id = Some(item_id.clone());

        out.push(ResponsesSseEvent::named(
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "output_index": self.output_index,
                "item": {
                    "id": item_id,
                    "type": "message",
                    "role": "assistant",
                    "status": "in_progress",
                    "content": [],
                },
            }),
        ));
        out.push(ResponsesSseEvent::named(
            "response.content_part.added",
            json!({
                "type": "response.content_part.added",
                "item_id": item_id,
                "output_index": self.output_index,
                "content_index": 0,
                "part": {"type": "output_text", "text": ""},
            }),
        ));
    }

    /// 关闭 message item：content_part.done + output_item.done，并推进 output_index
    fn close_message_item(&mut self, out: &mut Vec<ResponsesSseEvent>) {
        let Some(item_id) = self.message_item_id.take() else {
            return;
        };
        let text = std::mem::take(&mut self.text);

        out.push(ResponsesSseEvent::named(
            "response.content_part.done",
            json!({
                "type": "response.content_part.done",
                "item_id": item_id,
                "output_index": self.output_index,
                "content_index": 0,
                "part": {"type": "output_text", "text": text},
            }),
        ));
        let item = ResponseOutputItem::message(item_id, text);
        out.push(ResponsesSseEvent::named(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": self.output_index,
                "item": serde_json::to_value(&item).expect("item 序列化失败"),
            }),
        ));
        self.finished_items.push(item);
        self.output_index += 1;
    }

    // === 工具调用 ===

    fn process_tool_use(
        &mut self,
        tool_use: &crate::kiro::model::events::ToolUseEvent,
        out: &mut Vec<ResponsesSseEvent>,
    ) {
        let id = tool_use.tool_use_id.clone();
        if id.is_empty() {
            tracing::warn!("跳过缺少 tool_use_id 的 toolUseEvent public 输出");
            return;
        }

        if self.suppressed_tool_use_ids.contains(&id) {
            return;
        }

        let is_new = !self.tool_buffers.contains_key(&id);
        if is_new && tool_use.name.is_empty() {
            self.suppressed_tool_use_ids.insert(id.clone());
            tracing::warn!(
                tool_use_id_hash = %EventStreamDiagnostics::hash_public_id(&id),
                "跳过缺少工具名的 toolUseEvent public 输出"
            );
            return;
        }

        // 出现合法工具调用前必须先关闭已开启的 message item，否则 output_index 会串
        self.close_message_item(out);
        self.started = true;

        if is_new {
            // 工具名还原（D8 第二项）。还原后的名字即 ToolRewriteMap 的 key（design D3.1）
            let name = self
                .tool_name_map
                .get(&tool_use.name)
                .cloned()
                .unwrap_or_else(|| tool_use.name.clone());
            let is_freeform = self.tool_rewrite.freeform.contains(&name);
            tracing::info!(
                upstream_name = %name,
                item_type = if is_freeform { "custom_tool_call" } else { "function_call" },
                namespace = self
                    .tool_rewrite
                    .namespaces
                    .get(&name)
                    .map(|(ns, _)| ns.as_str())
                    .unwrap_or("-"),
                "工具调用已分派（流式）"
            );
            let item_id = output_item_id(if is_freeform { "ctc" } else { "fc" });
            self.tool_buffers
                .insert(id.clone(), (item_id.clone(), name.clone(), String::new()));
            self.tool_order.push(id.clone());

            // freeform 工具的 item 形状：客户端只读 `input`，不读 `arguments`
            let item = if is_freeform {
                let (display_name, namespace) = self.restore_name(&name);
                let mut item = json!({
                    "id": item_id,
                    "type": "custom_tool_call",
                    "status": "in_progress",
                    "call_id": id,
                    "name": display_name,
                    "input": "",
                });
                if let Some(ns) = namespace {
                    item["namespace"] = json!(ns);
                }
                item
            } else {
                let (display_name, namespace) = self.restore_name(&name);
                let mut item = json!({
                    "id": item_id,
                    "type": "function_call",
                    "status": "in_progress",
                    "call_id": id,
                    "name": display_name,
                    "arguments": "",
                });
                if let Some(ns) = namespace {
                    item["namespace"] = json!(ns);
                }
                item
            };

            out.push(ResponsesSseEvent::named(
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": self.output_index,
                    "item": item,
                }),
            ));
        }

        if !tool_use.input.is_empty() {
            let (item_id, name, args) = self.tool_buffers.get_mut(&id).expect("刚插入必存在");
            args.push_str(&tool_use.input);
            let item_id = item_id.clone();
            let is_freeform = self.tool_rewrite.freeform.contains(name);

            // freeform 工具：**吞掉增量不转发**。
            // `custom_tool_call_input.delta` 的载荷是已提取的 `input`，
            // 而提取要求参数 JSON 完整，因此只能缓冲到 stop 再一次性发出。
            if !is_freeform {
                out.push(ResponsesSseEvent::named(
                    "response.function_call_arguments.delta",
                    json!({
                        "type": "response.function_call_arguments.delta",
                        "item_id": item_id,
                        "output_index": self.output_index,
                        "delta": tool_use.input,
                    }),
                ));
            }
        }

        if tool_use.stop {
            self.close_tool_item(&id, out);
        }
    }

    /// 展平名 -> (回给客户端的名字, namespace)
    ///
    /// 客户端按 `(namespace, name)` 查注册表，只回展平名会匹配失败。
    fn restore_name(&self, name: &str) -> (String, Option<String>) {
        match self.tool_rewrite.namespaces.get(name) {
            Some((ns, original)) => (original.clone(), Some(ns.clone())),
            None => (name.to_string(), None),
        }
    }

    fn close_tool_item(&mut self, id: &str, out: &mut Vec<ResponsesSseEvent>) {
        let Some((item_id, name, args)) = self.tool_buffers.remove(id) else {
            return;
        };

        if self.tool_rewrite.freeform.contains(&name) {
            self.close_freeform_tool_item(&item_id, id, &name, &args, out);
            return;
        }

        let arguments = if args.is_empty() {
            "{}".to_string()
        } else {
            args
        };
        let (display_name, namespace) = self.restore_name(&name);
        let mut item = ResponseOutputItem::function_call(item_id, id, display_name, arguments);
        item.namespace = namespace;
        out.push(ResponsesSseEvent::named(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": self.output_index,
                "item": serde_json::to_value(&item).expect("item 序列化失败"),
            }),
        ));
        self.finished_items.push(item);
        self.output_index += 1;
    }

    /// freeform 工具收尾：先补发被吞掉的输入事件，再发 item.done
    ///
    /// 一个上游 stop 事件在此对应 3 个下游事件（input.delta + input.done + item.done），
    /// 而先前的每个参数增量对应 0 个。
    fn close_freeform_tool_item(
        &mut self,
        item_id: &str,
        call_id: &str,
        name: &str,
        args: &str,
        out: &mut Vec<ResponsesSseEvent>,
    ) {
        let input = super::responses_tools::extract_custom_input(args);

        if !input.is_empty() {
            out.push(ResponsesSseEvent::named(
                "response.custom_tool_call_input.delta",
                json!({
                    "type": "response.custom_tool_call_input.delta",
                    "item_id": item_id,
                    "output_index": self.output_index,
                    "call_id": call_id,
                    "delta": input,
                }),
            ));
        }
        out.push(ResponsesSseEvent::named(
            "response.custom_tool_call_input.done",
            json!({
                "type": "response.custom_tool_call_input.done",
                "item_id": item_id,
                "output_index": self.output_index,
                "call_id": call_id,
                "input": input,
            }),
        ));

        let (display_name, namespace) = self.restore_name(name);
        let mut item = ResponseOutputItem::custom_tool_call(item_id, call_id, display_name, input);
        item.namespace = namespace;
        out.push(ResponsesSseEvent::named(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": self.output_index,
                "item": serde_json::to_value(&item).expect("item 序列化失败"),
            }),
        ));
        self.finished_items.push(item);
        self.output_index += 1;
    }

    // === 收尾 ===

    /// 正常收尾：关闭未完成的 item -> completed -> [DONE]
    pub fn finish(&mut self) -> Vec<ResponsesSseEvent> {
        // 硬错误已渲染：不再补发 completed，避免把失败包装成成功终态
        if self.failed {
            return Vec::new();
        }
        let mut out = Vec::new();

        // thinking 缓冲里的正文残留
        if !self.in_thinking_block {
            let leftover = std::mem::take(&mut self.thinking_buffer);
            if !leftover.is_empty() {
                self.emit_text(&leftover, &mut out);
            }
        }

        self.close_message_item(&mut out);

        // 未收到 stop 的工具调用也要收尾
        let pending: Vec<String> = self
            .tool_order
            .iter()
            .filter(|id| self.tool_buffers.contains_key(*id))
            .cloned()
            .collect();
        for id in pending {
            self.close_tool_item(&id, &mut out);
        }

        let items = self.finished_items.clone();
        let snap = self.snapshot("completed", items);
        out.push(ResponsesSseEvent::named(
            "response.completed",
            json!({
                "type": "response.completed",
                "response": serde_json::to_value(&snap).expect("response 序列化失败"),
            }),
        ));
        out.push(ResponsesSseEvent::Done);
        self.diagnostics.log_summary("openai-responses");
        out
    }

    /// 失败收尾（上游在已开始输出后失败）
    pub fn fail(&mut self, message: impl Into<String>) -> Vec<ResponsesSseEvent> {
        self.failed = true;
        self.diagnostics.log_summary("openai-responses");
        let msg = message.into();
        vec![ResponsesSseEvent::named(
            "response.failed",
            json!({
                "type": "response.failed",
                "response": {
                    "id": self.id,
                    "object": "response",
                    "created_at": self.created_at,
                    "status": "failed",
                    "model": self.model,
                    "error": serde_json::to_value(ResponsesError {
                        error_type: "server_error".to_string(),
                        message: msg,
                    }).expect("error 序列化失败"),
                },
            }),
        )]
    }

    /// 客户端取消收尾（WS `response.cancel`）：关闭已开启的 item，
    /// 发出 `response.cancelled` 终态事件；已输出的部分内容保留在 response 对象里
    pub fn cancel(&mut self) -> Vec<ResponsesSseEvent> {
        let mut out = Vec::new();
        if !self.in_thinking_block {
            let leftover = std::mem::take(&mut self.thinking_buffer);
            if !leftover.is_empty() {
                self.emit_text(&leftover, &mut out);
            }
        }
        self.close_message_item(&mut out);
        let items = self.finished_items.clone();
        let snap = self.snapshot("cancelled", items);
        out.push(ResponsesSseEvent::named(
            "response.cancelled",
            json!({
                "type": "response.cancelled",
                "response": serde_json::to_value(&snap).expect("response 序列化失败"),
            }),
        ));
        self.diagnostics.log_summary("openai-responses");
        out
    }

    pub fn usage(&self) -> ResponsesUsage {
        let input = self
            .context_input_tokens
            .unwrap_or(self.estimated_input_tokens);
        let mut output_chars = self.text.len();
        for item in &self.finished_items {
            if let Some(parts) = &item.content {
                for p in parts {
                    output_chars += p.text.len();
                }
            }
            if let Some(a) = &item.arguments {
                output_chars += a.len();
            }
        }
        ResponsesUsage::new(input, (((output_chars + 3) / 4) as i32).max(1))
    }
}

/// 保留 buf 尾部可能是 tag 前缀的那一段，其余可安全发出
fn safe_flush_len(buf: &str, tag: &str) -> usize {
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

    fn new_ctx(thinking: bool) -> ResponsesStreamContext {
        ResponsesStreamContext::new(
            "resp_test".to_string(),
            "gpt-4o".to_string(),
            None,
            None,
            100,
            thinking,
            HashMap::new(),
            Default::default(),
        )
    }

    fn names(events: &[ResponsesSseEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| e.event_name().map(|s| s.to_string()))
            .collect()
    }

    fn payloads(events: &[ResponsesSseEvent]) -> Vec<Value> {
        events
            .iter()
            .filter_map(|e| match e {
                ResponsesSseEvent::Named { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_sse_format_has_event_line() {
        // 与 Chat Completions 的纯 data: 行不同
        let e = ResponsesSseEvent::named("response.created", json!({"a": 1}));
        let s = e.to_sse_string();
        assert!(s.starts_with("event: response.created\n"), "实际: {}", s);
        assert!(s.contains("\ndata: "), "实际: {}", s);
        assert!(s.ends_with("\n\n"));
    }

    #[test]
    fn test_done_and_keepalive_format() {
        assert_eq!(ResponsesSseEvent::Done.to_sse_string(), "data: [DONE]\n\n");
        let ka = ResponsesSseEvent::Keepalive.to_sse_string();
        assert!(ka.starts_with(':'), "保活必须是 SSE 注释行");
        assert!(!ka.contains("data:"));
        assert!(!ka.contains("event:"));
    }

    #[test]
    fn test_initial_events() {
        let ctx = new_ctx(false);
        let ev = ctx.initial_events();
        assert_eq!(names(&ev), vec!["response.created", "response.in_progress"]);
        let p = payloads(&ev);
        assert_eq!(p[0]["response"]["status"], "in_progress");
        assert_eq!(p[0]["response"]["object"], "response");
        assert_eq!(p[0]["response"]["id"], "resp_test");
        // D9：回显原始 model
        assert_eq!(p[0]["response"]["model"], "gpt-4o");
    }

    #[test]
    fn test_text_only_event_sequence() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.initial_events();
        all.extend(ctx.process_kiro_event(&text_event("Hello")));
        all.extend(ctx.process_kiro_event(&text_event(" world")));
        all.extend(ctx.finish());

        assert_eq!(
            names(&all),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.delta",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        assert_eq!(*all.last().unwrap(), ResponsesSseEvent::Done);
    }

    #[test]
    fn test_text_deltas_and_final_text() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&text_event("ab"));
        all.extend(ctx.process_kiro_event(&text_event("cd")));
        all.extend(ctx.finish());
        let p = payloads(&all);

        let deltas: String = p
            .iter()
            .filter(|d| d["type"] == "response.output_text.delta")
            .filter_map(|d| d["delta"].as_str())
            .collect();
        assert_eq!(deltas, "abcd");

        let part_done = p
            .iter()
            .find(|d| d["type"] == "response.content_part.done")
            .unwrap();
        assert_eq!(part_done["part"]["text"], "abcd");

        let completed = p.last().unwrap();
        assert_eq!(completed["response"]["status"], "completed");
        assert_eq!(
            completed["response"]["output"][0]["content"][0]["text"],
            "abcd"
        );
    }

    #[test]
    fn test_item_added_status_in_progress_done_completed() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&text_event("x"));
        all.extend(ctx.finish());
        let p = payloads(&all);

        let added = p
            .iter()
            .find(|d| d["type"] == "response.output_item.added")
            .unwrap();
        assert_eq!(added["item"]["status"], "in_progress");
        assert_eq!(added["item"]["type"], "message");
        assert_eq!(added["item"]["role"], "assistant");

        let done = p
            .iter()
            .find(|d| d["type"] == "response.output_item.done")
            .unwrap();
        assert_eq!(done["item"]["status"], "completed");
    }

    #[test]
    fn test_function_call_event_sequence() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&tool_event("c1", "get_weather", "", false));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "get_weather", "{\"a\":1}", true)));
        all.extend(ctx.finish());

        assert_eq!(
            names(&all),
            vec![
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let p = payloads(&all);
        assert_eq!(p[0]["item"]["type"], "function_call");
        assert_eq!(p[0]["item"]["status"], "in_progress");
        assert_eq!(p[0]["item"]["call_id"], "c1");
        assert_eq!(p[0]["item"]["name"], "get_weather");
        assert_eq!(p[1]["delta"], r#"{"a":1}"#);
        assert_eq!(p[2]["item"]["status"], "completed");
        assert_eq!(p[2]["item"]["arguments"], r#"{"a":1}"#);
    }

    #[test]
    fn test_text_then_tool_closes_message_item_first() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&text_event("thinking..."));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "f", "{}", true)));
        all.extend(ctx.finish());

        let n = names(&all);
        let msg_done = n
            .iter()
            .position(|x| x == "response.output_item.done")
            .unwrap();
        let fc_added = n
            .iter()
            .position(|x| x == "response.output_item.added" && { true })
            .unwrap();
        // 第一个 added 是 message，msg 的 content_part.done 必须在 fc 的 added 之前
        let part_done = n
            .iter()
            .position(|x| x == "response.content_part.done")
            .unwrap();
        let second_added = n
            .iter()
            .enumerate()
            .filter(|(_, x)| *x == "response.output_item.added")
            .nth(1)
            .map(|(i, _)| i)
            .expect("应有两个 output_item.added");

        assert!(part_done < second_added, "message item 必须先关闭: {:?}", n);
        assert!(msg_done < second_added);
        assert_eq!(fc_added, 0);
    }

    #[test]
    fn test_output_index_increments_per_item() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&text_event("text"));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "f", "{}", true)));
        all.extend(ctx.process_kiro_event(&tool_event("c2", "g", "{}", true)));
        all.extend(ctx.finish());

        let p = payloads(&all);
        // message item = 0，两个 function_call = 1, 2
        let indices: Vec<i64> = p
            .iter()
            .filter(|d| d["type"] == "response.output_item.done")
            .map(|d| d["output_index"].as_i64().unwrap())
            .collect();
        assert_eq!(indices, vec![0, 1, 2], "每个 item 应有独立且递增的索引");

        let completed = p.last().unwrap();
        let output = completed["response"]["output"].as_array().unwrap();
        assert_eq!(output.len(), 3);
        assert_eq!(output[0]["type"], "message");
        assert_eq!(output[1]["type"], "function_call");
        assert_eq!(output[2]["type"], "function_call");
    }

    #[test]
    fn test_tool_name_restored() {
        let mut map = HashMap::new();
        map.insert("short_x".to_string(), "original_very_long_name".to_string());
        let mut ctx = ResponsesStreamContext::new(
            "resp_1".into(),
            "m".into(),
            None,
            None,
            10,
            false,
            map,
            Default::default(),
        );
        let all = ctx.process_kiro_event(&tool_event("c1", "short_x", "", false));
        let p = payloads(&all);
        assert_eq!(p[0]["item"]["name"], "original_very_long_name");
    }

    #[test]
    fn test_tool_use_missing_name_suppresses_entire_lifecycle() {
        let mut ctx = new_ctx(false);

        let first = ctx.process_kiro_event(&tool_event("tool_1", "", "part1", false));
        let second = ctx.process_kiro_event(&tool_event("tool_1", "test_tool", "part2", true));

        assert!(first.is_empty());
        assert!(second.is_empty());
        assert!(ctx.tool_buffers.is_empty());
        assert!(ctx.suppressed_tool_use_ids.contains("tool_1"));
        assert!(!ctx.started);
    }

    #[test]
    fn test_reasoning_and_metering_events_are_not_public_responses_events() {
        let mut ctx = new_ctx(true);
        let mut all = ctx.initial_events();
        all.extend(
            ctx.process_kiro_event(&Event::Reasoning(ReasoningContentEvent {
                text: "abc".to_string(),
                signature: "fixture_signature_value".to_string(),
                ..Default::default()
            })),
        );
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

        let event_names = names(&all);
        let serialized =
            serde_json::to_string(&payloads(&all)).expect("序列化 Responses payload 失败");

        assert!(!event_names.iter().any(|name| name.contains("diagnostic")));
        assert!(!event_names.iter().any(|name| name.contains("metering")));
        assert!(!event_names.iter().any(|name| name.contains("reasoning")));
        assert!(!serialized.contains("diagnostic"));
        assert!(!serialized.contains("metering"));
        assert!(!serialized.contains("fixture_signature_value"));
        assert!(!serialized.contains("fixture hidden payload"));
    }

    #[test]
    fn test_unstopped_tool_closed_on_finish() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&tool_event("c1", "f", "{\"a\":1}", false));
        all.extend(ctx.finish());
        let p = payloads(&all);
        let done = p
            .iter()
            .find(|d| d["type"] == "response.output_item.done")
            .expect("未 stop 的工具调用也应收尾");
        assert_eq!(done["item"]["arguments"], r#"{"a":1}"#);
    }

    #[test]
    fn test_empty_arguments_becomes_empty_object() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&tool_event("c1", "f", "", true));
        all.extend(ctx.finish());
        let p = payloads(&all);
        let done = p
            .iter()
            .find(|d| d["type"] == "response.output_item.done")
            .unwrap();
        assert_eq!(done["item"]["arguments"], "{}");
    }

    #[test]
    fn test_failed_event() {
        let mut ctx = new_ctx(false);
        let _ = ctx.process_kiro_event(&text_event("partial"));
        assert!(ctx.started);
        let ev = ctx.fail("upstream exploded");
        assert_eq!(names(&ev), vec!["response.failed"]);
        let p = payloads(&ev);
        assert_eq!(p[0]["response"]["status"], "failed");
        assert_eq!(p[0]["response"]["error"]["type"], "server_error");
        assert_eq!(p[0]["response"]["error"]["message"], "upstream exploded");
    }

    // === 流内硬错误传播（add-stream-error-propagation） ===

    fn kiro_error_event(code: &str, message: &str) -> Event {
        Event::Error {
            error_code: code.to_string(),
            error_message: message.to_string(),
        }
    }

    fn kiro_exception_event(exception_type: &str, message: &str) -> Event {
        Event::Exception {
            exception_type: exception_type.to_string(),
            message: message.to_string(),
        }
    }

    #[test]
    fn test_hard_error_after_content_emits_failed_without_completed() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&text_event("partial"));
        all.extend(ctx.process_kiro_event(&kiro_error_event(
            "InternalServerException",
            "upstream exploded",
        )));
        let n = names(&all);
        assert_eq!(
            n.last().map(String::as_str),
            Some("response.failed"),
            "硬错误必须是最后一个事件"
        );
        assert!(
            n.iter().any(|x| x == "response.output_text.delta"),
            "错误前已产出的内容不被回收"
        );
        let failed = payloads(&all)
            .into_iter()
            .find(|d| d["type"] == "response.failed")
            .expect("应含 response.failed");
        assert_eq!(failed["response"]["status"], "failed");
        assert_eq!(
            failed["response"]["error"]["message"],
            "Kiro upstream error (InternalServerException): upstream exploded"
        );

        // finish 不再补发 completed，避免把失败包装成成功终态
        let fin = ctx.finish();
        assert!(
            names(&fin).is_empty(),
            "failed 后 finish 不应产出任何事件，实际: {:?}",
            names(&fin)
        );
    }

    #[test]
    fn test_hard_error_before_content_emits_failed() {
        let mut ctx = new_ctx(false);
        let ev = ctx.process_kiro_event(&kiro_error_event("ThrottlingException", "slow down"));
        assert_eq!(names(&ev), vec!["response.failed"]);
        let fin = ctx.finish();
        assert!(
            !names(&fin).iter().any(|n| n == "response.completed"),
            "硬错误后不得出现 response.completed"
        );
    }

    #[test]
    fn test_unmapped_exception_is_hard_error() {
        let mut ctx = new_ctx(false);
        let ev =
            ctx.process_kiro_event(&kiro_exception_event("ValidationException", "bad request"));
        assert_eq!(names(&ev), vec!["response.failed"]);
        let p = payloads(&ev);
        assert_eq!(
            p[0]["response"]["error"]["message"],
            "Kiro upstream error (ValidationException): bad request"
        );
    }

    #[test]
    fn test_content_length_exception_keeps_completed_semantics() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&text_event("partial"));
        all.extend(ctx.process_kiro_event(&kiro_exception_event(
            "ContentLengthExceededException",
            "too long",
        )));
        assert!(
            names(&all).iter().all(|n| n != "response.failed"),
            "ContentLengthExceededException 保留 length 语义，不是硬错误"
        );
        all.extend(ctx.finish());
        assert!(
            names(&all).iter().any(|n| n == "response.completed"),
            "无硬错误时正常收尾不变"
        );
    }

    #[test]
    fn test_first_fault_wins_subsequent_faults_suppressed() {
        let mut ctx = new_ctx(false);
        let first = ctx.process_kiro_event(&kiro_error_event("FirstException", "one"));
        let second = ctx.process_kiro_event(&kiro_error_event("SecondException", "two"));
        let late_text = ctx.process_kiro_event(&text_event("late"));
        assert_eq!(names(&first), vec!["response.failed"]);
        assert!(second.is_empty(), "后续硬错误仅记录，不再产出事件");
        assert!(late_text.is_empty(), "失败后内容事件不再产出");
        let p = payloads(&first);
        assert!(p[0]["response"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("FirstException"));
    }

    /// 任务 5.2：failed 事件的 error 对象只含 type/message 两字段，
    /// message 为固定前缀 + 上游 code/message 的精确拼接，无额外上下文
    #[test]
    fn test_fault_rendering_exposes_only_code_and_message() {
        let sensitive = "profile arn:aws:security-profile:::SECRET cookie=session=leak";
        let mut ctx = new_ctx(false);
        let ev =
            ctx.process_kiro_event(&kiro_error_event("AccessDeniedException", sensitive));

        let failed = payloads(&ev)
            .into_iter()
            .find(|d| d["type"] == "response.failed")
            .expect("应含 response.failed");
        let error_obj = failed["response"]["error"]
            .as_object()
            .expect("error 应为对象");
        let keys: Vec<&String> = error_obj.keys().collect();
        assert_eq!(
            keys.len(),
            2,
            "error 对象只允许 type/message 两个字段，实际: {:?}",
            keys
        );
        assert_eq!(error_obj["type"], "server_error");
        assert_eq!(
            error_obj["message"],
            format!("Kiro upstream error (AccessDeniedException): {}", sensitive)
        );
    }

    #[test]
    fn test_started_false_before_output() {
        let ctx = new_ctx(false);
        assert!(
            !ctx.started,
            "未输出前 started 应为 false（决定走错误响应）"
        );
    }

    #[test]
    fn test_usage_prefers_context_signal() {
        let mut ctx = new_ctx(false);
        let _ = ctx.process_kiro_event(&text_event("x"));
        let _ = ctx.process_kiro_event(&Event::ContextUsage(ContextUsageEvent {
            context_usage_percentage: 50.0,
        }));
        assert_ne!(ctx.usage().input_tokens, 100, "应使用上游反算值");
    }

    #[test]
    fn test_usage_falls_back_to_estimate() {
        let mut ctx = new_ctx(false);
        let _ = ctx.process_kiro_event(&text_event("x"));
        assert_eq!(ctx.usage().input_tokens, 100);
    }

    #[test]
    fn test_thinking_content_excluded_from_output() {
        let mut ctx = new_ctx(true);
        let mut all = ctx.process_kiro_event(&text_event("<thinking>内部思考</thinking>正式答案"));
        all.extend(ctx.finish());
        let p = payloads(&all);

        let deltas: String = p
            .iter()
            .filter(|d| d["type"] == "response.output_text.delta")
            .filter_map(|d| d["delta"].as_str())
            .collect();
        assert_eq!(deltas, "正式答案");
        assert!(!deltas.contains("内部思考"), "思考内容不得进 output");

        let completed = p.last().unwrap();
        let text = completed["response"]["output"][0]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert!(!text.contains("内部思考"));
        assert!(!text.contains("<thinking>"));
    }

    #[test]
    fn test_thinking_tag_split_across_events() {
        let mut ctx = new_ctx(true);
        let mut all = ctx.process_kiro_event(&text_event("<thin"));
        all.extend(ctx.process_kiro_event(&text_event("king>思考</think")));
        all.extend(ctx.process_kiro_event(&text_event("ing>答案")));
        all.extend(ctx.finish());
        let p = payloads(&all);
        let deltas: String = p
            .iter()
            .filter(|d| d["type"] == "response.output_text.delta")
            .filter_map(|d| d["delta"].as_str())
            .collect();
        assert_eq!(deltas, "答案");
        assert!(!deltas.contains("thinking"), "拆分标签不得泄漏");
    }

    #[test]
    fn test_metadata_and_instructions_echoed() {
        let mut meta = HashMap::new();
        meta.insert("k".to_string(), "v".to_string());
        let mut ctx = ResponsesStreamContext::new(
            "resp_1".into(),
            "m".into(),
            Some("sys".into()),
            Some(meta),
            10,
            false,
            HashMap::new(),
            Default::default(),
        );
        let mut all = ctx.process_kiro_event(&text_event("x"));
        all.extend(ctx.finish());
        let completed = payloads(&all).last().unwrap().clone();
        assert_eq!(completed["response"]["metadata"]["k"], "v");
        assert_eq!(completed["response"]["instructions"], "sys");
    }

    #[test]
    fn test_no_output_still_completes() {
        let mut ctx = new_ctx(false);
        let mut all = ctx.initial_events();
        all.extend(ctx.finish());
        let n = names(&all);
        assert_eq!(n.last().unwrap(), "response.completed");
        let completed = payloads(&all).last().unwrap().clone();
        assert_eq!(completed["response"]["status"], "completed");
        assert!(
            completed["response"]["output"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    // === freeform 工具的流式还原 ===

    fn freeform_ctx(name: &str) -> ResponsesStreamContext {
        let mut rewrite = super::super::responses_tools::ToolRewriteMap::default();
        rewrite.freeform.insert(name.to_string());
        ResponsesStreamContext::new(
            "resp_test".to_string(),
            "gpt-4o".to_string(),
            None,
            None,
            100,
            false,
            HashMap::new(),
            rewrite,
        )
    }

    /// 锁定：上游参数增量不得透传
    ///
    /// `custom_tool_call_input.delta` 的载荷是已提取的 `input`，
    /// 逐条改名转发会让客户端收到语义错误的增量。
    #[test]
    fn test_freeform_upstream_arguments_delta_not_forwarded() {
        let mut ctx = freeform_ctx("exec");
        let mut all = ctx.process_kiro_event(&tool_event("c1", "exec", r#"{"input":"#, false));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "exec", r#""src"}"#, false)));

        let n = names(&all);
        assert!(
            !n.iter()
                .any(|e| e == "response.function_call_arguments.delta"),
            "freeform 工具不得透传上游参数增量: {:?}",
            n
        );
        assert!(
            !n.iter().any(|e| e.contains("custom_tool_call_input")),
            "参数未完成时不得发出 input 事件: {:?}",
            n
        );
    }

    #[test]
    fn test_freeform_input_events_emitted_once_after_stop() {
        let mut ctx = freeform_ctx("exec");
        let mut all = ctx.process_kiro_event(&tool_event("c1", "exec", r#"{"input":"#, false));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "exec", r#""src"}"#, false)));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "exec", "", true)));

        let n = names(&all);
        let delta_count = n
            .iter()
            .filter(|e| *e == "response.custom_tool_call_input.delta")
            .count();
        assert_eq!(delta_count, 1, "input.delta 须只出现一次: {:?}", n);

        // 序列：added -> input.delta -> input.done -> item.done
        let tool_events: Vec<&String> = n
            .iter()
            .filter(|e| e.starts_with("response.output_item") || e.contains("custom_tool_call"))
            .collect();
        assert_eq!(
            tool_events,
            vec![
                "response.output_item.added",
                "response.custom_tool_call_input.delta",
                "response.custom_tool_call_input.done",
                "response.output_item.done",
            ],
            "事件序列不符: {:?}",
            n
        );
    }

    #[test]
    fn test_freeform_item_shape_added_and_done() {
        let mut ctx = freeform_ctx("exec");
        let mut all = ctx.process_kiro_event(&tool_event("c1", "exec", r#"{"input":"x"}"#, false));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "exec", "", true)));
        let p = payloads(&all);

        let added = p
            .iter()
            .find(|e| e["type"] == "response.output_item.added")
            .expect("须有 added");
        assert_eq!(added["item"]["type"], "custom_tool_call");
        assert_eq!(added["item"]["input"], "", "added 时 input 为空串");
        assert!(
            added["item"].get("arguments").is_none(),
            "custom_tool_call 不应带 arguments"
        );

        let done = p
            .iter()
            .find(|e| e["type"] == "response.output_item.done")
            .expect("须有 done");
        assert_eq!(done["item"]["type"], "custom_tool_call");
        assert_eq!(done["item"]["input"], "x", "done 时填完整提取结果");
    }

    #[test]
    fn test_freeform_raw_source_input_extracted() {
        // 模型直接回裸源码（非 JSON）
        let raw = "await tools.exec_command({cmd: \"ls\"});";
        let mut ctx = freeform_ctx("exec");
        let mut all = ctx.process_kiro_event(&tool_event("c1", "exec", raw, false));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "exec", "", true)));

        let p = payloads(&all);
        let done = p
            .iter()
            .find(|e| e["type"] == "response.output_item.done")
            .unwrap();
        assert_eq!(done["item"]["input"], raw);
    }

    #[test]
    fn test_plain_tool_still_forwards_arguments_delta() {
        // 非 freeform 工具的既有行为不变
        let mut ctx = new_ctx(false);
        let mut all = ctx.process_kiro_event(&tool_event("c1", "wait", r#"{"a":1}"#, false));
        all.extend(ctx.process_kiro_event(&tool_event("c1", "wait", "", true)));

        let n = names(&all);
        assert!(
            n.iter()
                .any(|e| e == "response.function_call_arguments.delta"),
            "普通工具须继续透传参数增量: {:?}",
            n
        );
        let p = payloads(&all);
        let done = p
            .iter()
            .find(|e| e["type"] == "response.output_item.done")
            .unwrap();
        assert_eq!(done["item"]["type"], "function_call");
    }

    #[test]
    fn test_stream_namespace_restored_in_items() {
        let mut rewrite = super::super::responses_tools::ToolRewriteMap::default();
        rewrite.namespaces.insert(
            "collaboration__spawn_agent".to_string(),
            ("collaboration".to_string(), "spawn_agent".to_string()),
        );
        let mut ctx = ResponsesStreamContext::new(
            "resp_test".to_string(),
            "gpt-4o".to_string(),
            None,
            None,
            100,
            false,
            HashMap::new(),
            rewrite,
        );

        let mut all =
            ctx.process_kiro_event(&tool_event("c1", "collaboration__spawn_agent", "{}", false));
        all.extend(ctx.process_kiro_event(&tool_event(
            "c1",
            "collaboration__spawn_agent",
            "",
            true,
        )));

        let p = payloads(&all);
        for kind in ["response.output_item.added", "response.output_item.done"] {
            let e = p.iter().find(|e| e["type"] == kind).expect(kind);
            assert_eq!(e["item"]["name"], "spawn_agent", "{} 须还原原名", kind);
            assert_eq!(
                e["item"]["namespace"], "collaboration",
                "{} 须带 namespace",
                kind
            );
        }
    }

    /// 两级映射组合：namespace 内层 custom 的展平名必须回 custom_tool_call，
    /// 还原原名与 namespace，且 freeform 参数缓冲语义不变（增量不透传、一次性发出）。
    #[test]
    fn test_stream_freeform_and_namespace_combined() {
        let mut rewrite = super::super::responses_tools::ToolRewriteMap::default();
        rewrite.freeform.insert("functions__apply_patch".to_string());
        rewrite.namespaces.insert(
            "functions__apply_patch".to_string(),
            ("functions".to_string(), "apply_patch".to_string()),
        );
        let mut ctx = ResponsesStreamContext::new(
            "resp_test".to_string(),
            "gpt-4o".to_string(),
            None,
            None,
            100,
            false,
            HashMap::new(),
            rewrite,
        );

        let mut all = ctx.process_kiro_event(&tool_event(
            "c1",
            "functions__apply_patch",
            r#"{"input":"#,
            false,
        ));
        all.extend(ctx.process_kiro_event(&tool_event(
            "c1",
            "functions__apply_patch",
            r#""*** Begin Patch"}"#,
            false,
        )));
        all.extend(ctx.process_kiro_event(&tool_event(
            "c1",
            "functions__apply_patch",
            "",
            true,
        )));

        let n = names(&all);
        assert!(
            !n.iter().any(|e| e == "response.function_call_arguments.delta"),
            "freeform 展平名不得透传参数增量: {:?}",
            n
        );
        let delta_count = n
            .iter()
            .filter(|e| *e == "response.custom_tool_call_input.delta")
            .count();
        assert_eq!(delta_count, 1, "input.delta 须只出现一次: {:?}", n);

        let p = payloads(&all);
        for kind in ["response.output_item.added", "response.output_item.done"] {
            let e = p.iter().find(|e| e["type"] == kind).expect(kind);
            assert_eq!(e["item"]["type"], "custom_tool_call", "{} 类型不符", kind);
            assert_eq!(e["item"]["name"], "apply_patch", "{} 须还原原名", kind);
            assert_eq!(
                e["item"]["namespace"], "functions",
                "{} 须带 namespace",
                kind
            );
        }
        let done = p
            .iter()
            .find(|e| e["type"] == "response.output_item.done")
            .expect("须有 done");
        assert_eq!(done["item"]["input"], "*** Begin Patch", "done 时填完整提取结果");
    }

    #[test]
    fn test_freeform_unfinished_tool_closed_on_finish() {
        // 未收到 stop 也要收尾，且走 freeform 分支
        let mut ctx = freeform_ctx("exec");
        let mut all = ctx.process_kiro_event(&tool_event("c1", "exec", r#"{"input":"y"}"#, false));
        all.extend(ctx.finish());

        let p = payloads(&all);
        let done = p
            .iter()
            .find(|e| e["type"] == "response.output_item.done")
            .expect("finish 须收尾未完成的工具");
        assert_eq!(done["item"]["type"], "custom_tool_call");
        assert_eq!(done["item"]["input"], "y");
    }
}
