//! Responses 端点的 server-side web_search
//!
//! 判定口径对齐 sub2api `gateway_websearch_emulation.go:96-105`（宽口径，见 D11）；
//! 输出映射为 Responses 的 `web_search_call` + `message` output items。
//!
//! **不改 Anthropic 侧的 `has_web_search_tool`**（D11）：放宽那侧判定会把
//! 现在转发上游的 `web_search_20250305` 请求变成代执行，属行为反转。
//!
//! Chat Completions 端点不提供该能力（D10）：该协议没有字段能诚实表达
//! 「服务端已代你执行了搜索」。

use serde_json::{Value, json};

use crate::anthropic::{
    WebSearchResults, call_mcp_api, create_mcp_request, generate_search_summary,
    parse_search_results,
};
use crate::kiro::provider::KiroProvider;

use super::error::OpenAiError;
use super::responses_stream::ResponsesSseEvent;
use super::responses_types::{ResponseOutputItem, output_item_id};
use super::types::OpenAiTool;

/// 判定单个工具是否为 web_search（宽口径）
///
/// 对齐 sub2api `isWebSearchToolJSON`：type 前缀匹配 `web_search`
/// 或等于 `google_search`，**或** name 命中三个已知名称之一。
pub fn is_web_search_tool(tool: &OpenAiTool) -> bool {
    let tool_type = tool.tool_type.trim().to_lowercase();
    if tool_type.starts_with("web_search") || tool_type == "google_search" {
        return true;
    }
    let name = tool.name.trim().to_lowercase();
    matches!(
        name.as_str(),
        "web_search" | "google_search" | "web_search_20250305"
    )
}

/// 是否应代执行搜索
///
/// 约束：**恰好一个** tool 且命中判定（与 Anthropic 侧同一约束，
/// 避免劫持混合工具场景）。`enabled` 为运行时开关（D11）。
pub fn should_emulate(tools: Option<&[OpenAiTool]>, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    match tools {
        Some(list) if list.len() == 1 => is_web_search_tool(&list[0]),
        _ => false,
    }
}

/// 从归一后的消息中提取搜索查询：取最后一条 user 消息的文本
///
/// 不复用 Anthropic 侧的 `extract_search_query` —— 那个版本会剥离
/// `"Perform a web search for the query: "` 前缀，那是 Claude Code 客户端约定，
/// OpenAI 客户端不发这个前缀。
pub fn extract_query(messages: &[Value]) -> Option<String> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        let text = match msg.get("content") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(parts)) => {
                let mut out = String::new();
                for p in parts {
                    let is_text = p
                        .get("type")
                        .and_then(|v| v.as_str())
                        .map(|t| t == "text" || t == "input_text")
                        .unwrap_or(false);
                    if is_text {
                        if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                            out.push_str(t);
                        }
                    }
                }
                out
            }
            _ => continue,
        };
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// 执行搜索并返回 (查询, 结果)
pub async fn run_search(
    provider: &KiroProvider,
    query: &str,
) -> (String, Option<WebSearchResults>) {
    let (_tool_use_id, mcp_request) = create_mcp_request(query);
    let results = match call_mcp_api(provider, &mcp_request).await {
        Ok(resp) => parse_search_results(&resp),
        Err(e) => {
            tracing::warn!("MCP 搜索调用失败: {}", e);
            None
        }
    };
    (query.to_string(), results)
}

/// 构造 Responses output items：web_search_call + message(摘要)
pub fn build_output_items(
    query: &str,
    results: &Option<WebSearchResults>,
) -> (Vec<ResponseOutputItem>, String) {
    let summary = generate_search_summary(query, results);
    let items = vec![
        ResponseOutputItem::web_search_call(output_item_id("ws"), query),
        ResponseOutputItem::message(output_item_id("msg"), summary.clone()),
    ];
    (items, summary)
}

/// 构造流式事件序列
///
/// web_search_call item 占 output_index 0，message item 占 1。
pub fn build_stream_events(
    query: &str,
    results: &Option<WebSearchResults>,
) -> (Vec<ResponsesSseEvent>, Vec<ResponseOutputItem>, String) {
    let (items, summary) = build_output_items(query, results);
    let ws_item = items[0].clone();
    let msg_item = items[1].clone();
    let msg_id = msg_item.id.clone();

    let mut out = Vec::new();

    // web_search_call：added(in_progress) -> done(completed)
    out.push(ResponsesSseEvent::named(
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "id": ws_item.id,
                "type": "web_search_call",
                "status": "in_progress",
                "action": {"type": "search", "query": query},
            },
        }),
    ));
    out.push(ResponsesSseEvent::named(
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": serde_json::to_value(&ws_item).expect("item 序列化失败"),
        }),
    ));

    // message：added -> content_part.added -> delta* -> content_part.done -> done
    out.push(ResponsesSseEvent::named(
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "output_index": 1,
            "item": {
                "id": msg_id,
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
            "item_id": msg_id,
            "output_index": 1,
            "content_index": 0,
            "part": {"type": "output_text", "text": ""},
        }),
    ));

    // 摘要按字符分片发送（对齐 Anthropic 侧的分块策略）
    for chunk in chunk_text(&summary, 100) {
        out.push(ResponsesSseEvent::named(
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": msg_id,
                "output_index": 1,
                "content_index": 0,
                "delta": chunk,
            }),
        ));
    }

    out.push(ResponsesSseEvent::named(
        "response.content_part.done",
        json!({
            "type": "response.content_part.done",
            "item_id": msg_id,
            "output_index": 1,
            "content_index": 0,
            "part": {"type": "output_text", "text": summary},
        }),
    ));
    out.push(ResponsesSseEvent::named(
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "output_index": 1,
            "item": serde_json::to_value(&msg_item).expect("item 序列化失败"),
        }),
    ));

    (out, items, summary)
}

/// 按字符数分片（UTF-8 安全）
fn chunk_text(text: &str, size: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(size)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

/// 查询无法提取时的错误
pub fn missing_query_error() -> OpenAiError {
    OpenAiError::InvalidRequest(
        "cannot extract a search query from input: expected user text content".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(json_str: &str) -> OpenAiTool {
        serde_json::from_str(json_str).expect("工具反序列化失败")
    }

    #[test]
    fn test_detect_by_type_prefix() {
        for t in [
            r#"{"type":"web_search"}"#,
            r#"{"type":"web_search_preview"}"#,
            r#"{"type":"web_search_2025_03_05"}"#,
            r#"{"type":"WEB_SEARCH"}"#,
        ] {
            assert!(is_web_search_tool(&tool(t)), "应命中: {}", t);
        }
    }

    #[test]
    fn test_detect_google_search_type() {
        assert!(is_web_search_tool(&tool(r#"{"type":"google_search"}"#)));
    }

    #[test]
    fn test_detect_by_name() {
        for n in ["web_search", "google_search", "web_search_20250305"] {
            let t = format!(r#"{{"type":"function","name":"{}"}}"#, n);
            assert!(is_web_search_tool(&tool(&t)), "应命中 name: {}", n);
        }
    }

    #[test]
    fn test_dated_official_name_detected() {
        // Anthropic 带日期的官方名：kiro.rs Anthropic 侧接不住，本端点能接住（D11）
        assert!(is_web_search_tool(&tool(
            r#"{"type":"web_search_20250305","name":"web_search"}"#
        )));
    }

    #[test]
    fn test_ordinary_function_tool_not_detected() {
        for t in [
            r#"{"type":"function","name":"get_weather"}"#,
            r#"{"type":"function","name":"search_database"}"#,
            r#"{"type":"function","name":"websearch"}"#,
        ] {
            assert!(!is_web_search_tool(&tool(t)), "不应命中: {}", t);
        }
    }

    #[test]
    fn test_should_emulate_single_tool() {
        let tools = vec![tool(r#"{"type":"web_search"}"#)];
        assert!(should_emulate(Some(&tools), true));
    }

    #[test]
    fn test_should_not_emulate_when_disabled() {
        let tools = vec![tool(r#"{"type":"web_search"}"#)];
        assert!(
            !should_emulate(Some(&tools), false),
            "开关关闭时不得拦截（D11）"
        );
    }

    #[test]
    fn test_should_not_emulate_mixed_tools() {
        let tools = vec![
            tool(r#"{"type":"web_search"}"#),
            tool(r#"{"type":"function","name":"f"}"#),
        ];
        assert!(
            !should_emulate(Some(&tools), true),
            "混合工具场景不得劫持"
        );
    }

    #[test]
    fn test_should_not_emulate_without_tools() {
        assert!(!should_emulate(None, true));
        assert!(!should_emulate(Some(&[]), true));
    }

    #[test]
    fn test_extract_query_last_user_message() {
        let msgs = vec![
            json!({"role": "user", "content": "first"}),
            json!({"role": "assistant", "content": "reply"}),
            json!({"role": "user", "content": "second question"}),
        ];
        assert_eq!(extract_query(&msgs).unwrap(), "second question");
    }

    #[test]
    fn test_extract_query_from_parts() {
        let msgs = vec![json!({"role": "user", "content": [
            {"type": "text", "text": "rust "},
            {"type": "text", "text": "news"}
        ]})];
        assert_eq!(extract_query(&msgs).unwrap(), "rust news");
    }

    #[test]
    fn test_extract_query_skips_non_user() {
        let msgs = vec![
            json!({"role": "user", "content": "the question"}),
            json!({"role": "assistant", "content": "not this"}),
        ];
        assert_eq!(extract_query(&msgs).unwrap(), "the question");
    }

    #[test]
    fn test_extract_query_none_when_no_user_text() {
        let msgs = vec![json!({"role": "system", "content": "s"})];
        assert!(extract_query(&msgs).is_none());

        let blank = vec![json!({"role": "user", "content": "   "})];
        assert!(extract_query(&blank).is_none());
    }

    #[test]
    fn test_anthropic_prefix_not_stripped() {
        // 本端点不做 Claude Code 前缀处理
        let msgs = vec![json!({
            "role": "user",
            "content": "Perform a web search for the query: rust"
        })];
        assert_eq!(
            extract_query(&msgs).unwrap(),
            "Perform a web search for the query: rust",
            "OpenAI 端点不应剥离 Anthropic 专用前缀"
        );
    }

    #[test]
    fn test_output_items_structure() {
        let (items, summary) = build_output_items("rust news", &None);
        assert_eq!(items.len(), 2);

        let ws = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(ws["type"], "web_search_call");
        assert_eq!(ws["status"], "completed");
        assert_eq!(ws["action"]["query"], "rust news");

        let msg = serde_json::to_value(&items[1]).unwrap();
        assert_eq!(msg["type"], "message");
        assert_eq!(msg["role"], "assistant");
        assert_eq!(msg["content"][0]["type"], "output_text");
        assert_eq!(msg["content"][0]["text"], summary);
    }

    #[test]
    fn test_no_results_summary_is_explicit() {
        let (_, summary) = build_output_items("nothing", &None);
        assert!(
            summary.to_lowercase().contains("no results")
                || summary.contains("未")
                || summary.contains("No results"),
            "无结果时摘要必须明确说明，实际: {}",
            summary
        );
        assert!(!summary.trim().is_empty(), "摘要不得为空");
    }

    #[test]
    fn test_stream_event_sequence() {
        let (events, items, _) = build_stream_events("q", &None);
        let names: Vec<&str> = events.iter().filter_map(|e| e.event_name()).collect();

        assert_eq!(names[0], "response.output_item.added");
        assert_eq!(names[1], "response.output_item.done");
        assert_eq!(names[2], "response.output_item.added");
        assert_eq!(names[3], "response.content_part.added");
        assert!(
            names[4..names.len() - 2]
                .iter()
                .all(|n| *n == "response.output_text.delta"),
            "中段应全是文本增量: {:?}",
            names
        );
        assert_eq!(names[names.len() - 2], "response.content_part.done");
        assert_eq!(names[names.len() - 1], "response.output_item.done");
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_stream_output_indices() {
        let (events, _, _) = build_stream_events("q", &None);
        for e in &events {
            if let ResponsesSseEvent::Named { data, .. } = e {
                let idx = data["output_index"].as_i64().unwrap();
                let is_ws = data["item"]["type"] == "web_search_call";
                if is_ws {
                    assert_eq!(idx, 0, "web_search_call 应占 index 0");
                }
            }
        }
        // message 相关事件都在 index 1
        let msg_events: Vec<&ResponsesSseEvent> = events
            .iter()
            .filter(|e| match e {
                ResponsesSseEvent::Named { data, .. } => {
                    data["item"]["type"] == "message" || data["type"] == "response.output_text.delta"
                }
                _ => false,
            })
            .collect();
        assert!(!msg_events.is_empty());
        for e in msg_events {
            if let ResponsesSseEvent::Named { data, .. } = e {
                assert_eq!(data["output_index"], 1);
            }
        }
    }

    #[test]
    fn test_stream_deltas_reassemble_to_summary() {
        let (events, _, summary) = build_stream_events("q", &None);
        let deltas: String = events
            .iter()
            .filter_map(|e| match e {
                ResponsesSseEvent::Named { data, .. }
                    if data["type"] == "response.output_text.delta" =>
                {
                    data["delta"].as_str().map(|s| s.to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(deltas, summary, "分片拼回必须等于完整摘要");
    }

    #[test]
    fn test_chunk_text_utf8_safe() {
        let text = "中文测试文本重复内容".repeat(30);
        let chunks = chunk_text(&text, 100);
        assert_eq!(chunks.concat(), text, "分片必须无损");
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_missing_query_error_is_bad_request() {
        let err = missing_query_error();
        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.error_type(), "invalid_request_error");
    }
}
