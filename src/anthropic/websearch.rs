//! WebSearch 工具处理模块
//!
//! 实现 Anthropic WebSearch 请求到 Kiro MCP 的转换和响应生成

use std::convert::Infallible;

use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use futures::{Stream, stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::stream::SseEvent;
use super::types::{ErrorResponse, MessagesRequest};

/// MCP 请求
#[derive(Debug, Serialize)]
pub struct McpRequest {
    pub id: String,
    pub jsonrpc: String,
    pub method: String,
    pub params: McpParams,
}

/// MCP 请求参数
#[derive(Debug, Serialize)]
pub struct McpParams {
    pub name: String,
    pub arguments: McpArguments,
}

/// MCP 参数
#[derive(Debug, Serialize)]
pub struct McpArguments {
    pub query: String,
}

/// MCP 响应
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct McpResponse {
    pub error: Option<McpError>,
    pub id: String,
    pub jsonrpc: String,
    pub result: Option<McpResult>,
}

/// MCP 错误
#[derive(Debug, Deserialize)]
pub struct McpError {
    pub code: Option<i32>,
    pub message: Option<String>,
}

/// MCP 结果
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct McpResult {
    pub content: Vec<McpContent>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

/// MCP 内容
#[derive(Debug, Deserialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// WebSearch 搜索结果
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct WebSearchResults {
    pub results: Vec<WebSearchResult>,
    #[serde(rename = "totalResults")]
    pub total_results: Option<i32>,
    pub query: Option<String>,
    pub error: Option<String>,
}

/// 单个搜索结果
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    #[serde(rename = "publishedDate")]
    pub published_date: Option<i64>,
    pub id: Option<String>,
    pub domain: Option<String>,
    #[serde(rename = "maxVerbatimWordLimit")]
    pub max_verbatim_word_limit: Option<i32>,
    #[serde(rename = "publicDomain")]
    pub public_domain: Option<bool>,
}

/// 检查请求是否为纯 WebSearch 请求
///
/// 条件：tools 有且只有一个，且 name 为 web_search
pub fn has_web_search_tool(req: &MessagesRequest) -> bool {
    req.tools.as_ref().is_some_and(|tools| {
        tools.len() == 1 && tools.first().is_some_and(|t| t.name == "web_search")
    })
}

/// 从消息中提取搜索查询
///
/// 读取 messages 的第一条消息的第一个内容块
/// 并去除 "Perform a web search for the query: " 前缀
pub fn extract_search_query(req: &MessagesRequest) -> Option<String> {
    // 获取第一条消息
    let first_msg = req.messages.first()?;

    // 提取文本内容
    let text = match &first_msg.content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            // 获取第一个内容块
            let first_block = arr.first()?;
            if first_block.get("type")?.as_str()? == "text" {
                first_block.get("text")?.as_str()?.to_string()
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // 去除前缀 "Perform a web search for the query: "
    const PREFIX: &str = "Perform a web search for the query: ";
    let query = if text.starts_with(PREFIX) {
        text[PREFIX.len()..].to_string()
    } else {
        text
    };

    if query.is_empty() { None } else { Some(query) }
}

/// 生成22位大小写字母和数字的随机字符串
fn generate_random_id_22() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    (0..22)
        .map(|_| {
            let idx = fastrand::usize(..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// 生成8位小写字母和数字的随机字符串
fn generate_random_id_8() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..8)
        .map(|_| {
            let idx = fastrand::usize(..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// 创建 MCP 请求
///
/// ID 格式: web_search_tooluse_{22位随机}_{毫秒时间戳}_{8位随机}
pub(crate) fn create_mcp_request(query: &str) -> (String, McpRequest) {
    let random_22 = generate_random_id_22();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let random_8 = generate_random_id_8();

    let request_id = format!(
        "web_search_tooluse_{}_{}_{}",
        random_22, timestamp, random_8
    );

    // tool_use_id 使用相同格式
    let tool_use_id = format!(
        "srvtoolu_{}",
        Uuid::new_v4().to_string().replace('-', "")[..32].to_string()
    );

    let request = McpRequest {
        id: request_id,
        jsonrpc: "2.0".to_string(),
        method: "tools/call".to_string(),
        params: McpParams {
            name: "web_search".to_string(),
            arguments: McpArguments {
                query: query.to_string(),
            },
        },
    };

    (tool_use_id, request)
}

/// 解析 MCP 响应中的搜索结果
pub(crate) fn parse_search_results(mcp_response: &McpResponse) -> Option<WebSearchResults> {
    let result = mcp_response.result.as_ref()?;
    let content = result.content.first()?;

    if content.content_type != "text" {
        return None;
    }

    serde_json::from_str(&content.text).ok()
}

/// 生成 WebSearch SSE 响应流
pub fn create_websearch_sse_stream(
    model: String,
    query: String,
    tool_use_id: String,
    search_results: Option<WebSearchResults>,
    input_tokens: i32,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let events =
        generate_websearch_events(&model, &query, &tool_use_id, search_results, input_tokens);

    stream::iter(
        events
            .into_iter()
            .map(|e| Ok(Bytes::from(e.to_sse_string()))),
    )
}

/// 构造 web_search 响应的四个内容块（流式与非流式共用）
///
/// 返回 `(blocks, summary)`。blocks 顺序固定：
/// 0 text（搜索决策说明）、1 server_tool_use、2 web_search_tool_result、3 text（摘要）。
///
/// 同一次搜索在两种模式下必须产出相同内容，所以这里是唯一的构造入口。
fn build_websearch_blocks(
    query: &str,
    tool_use_id: &str,
    search_results: &Option<WebSearchResults>,
) -> (Vec<serde_json::Value>, String) {
    let decision_text = format!("I'll search for \"{}\".", query);

    // 官方 API 的 web_search_tool_result 没有 tool_use_id 字段
    let search_content = if let Some(results) = search_results {
        results
            .results
            .iter()
            .map(|r| {
                let page_age = r.published_date.and_then(|ms| {
                    chrono::DateTime::from_timestamp_millis(ms)
                        .map(|dt| dt.format("%B %-d, %Y").to_string())
                });
                json!({
                    "type": "web_search_result",
                    "title": r.title,
                    "url": r.url,
                    "encrypted_content": r.snippet.clone().unwrap_or_default(),
                    "page_age": page_age
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    let summary = generate_search_summary(query, search_results);

    let blocks = vec![
        json!({"type": "text", "text": decision_text}),
        json!({
            "id": tool_use_id,
            "type": "server_tool_use",
            "name": "web_search",
            "input": {"query": query}
        }),
        json!({
            "type": "web_search_tool_result",
            "content": search_content
        }),
        json!({"type": "text", "text": summary}),
    ];

    (blocks, summary)
}

/// 生成 WebSearch SSE 事件序列
fn generate_websearch_events(
    model: &str,
    query: &str,
    tool_use_id: &str,
    search_results: Option<WebSearchResults>,
    input_tokens: i32,
) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let message_id = format!(
        "msg_{}",
        Uuid::new_v4().to_string().replace('-', "")[..24].to_string()
    );

    // 内容块与非流式路径共用同一构造，保证两种模式产出一致
    let (blocks, summary) = build_websearch_blocks(query, tool_use_id, &search_results);

    // 1. message_start
    events.push(SseEvent::new(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": null,
                "usage": {
                    "input_tokens": input_tokens,
                    "output_tokens": 0,
                    "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": 0
                }
            }
        }),
    ));

    // 2-3. text（搜索决策说明，index 0）：start -> delta -> stop
    let decision_text = blocks[0]["text"].as_str().unwrap_or_default().to_string();
    events.push(SseEvent::new(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "text", "text": ""}
        }),
    ));
    events.push(SseEvent::new(
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": decision_text}
        }),
    ));
    events.push(SseEvent::new(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": 0}),
    ));

    // 4-5. server_tool_use（index 1）
    // server_tool_use 是服务端工具，input 在 content_block_start 中一次性完整发送，
    // 不像客户端 tool_use 需要通过 input_json_delta 增量传输。
    events.push(SseEvent::new(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 1,
            "content_block": blocks[1]
        }),
    ));
    events.push(SseEvent::new(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": 1}),
    ));

    // 6-7. web_search_tool_result（index 2）
    events.push(SseEvent::new(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 2,
            "content_block": blocks[2]
        }),
    ));
    events.push(SseEvent::new(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": 2}),
    ));

    // 8-9. text（结果摘要，index 3）：start -> delta* -> stop
    events.push(SseEvent::new(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 3,
            "content_block": {"type": "text", "text": ""}
        }),
    ));
    let chunk_size = 100;
    for chunk in summary.chars().collect::<Vec<_>>().chunks(chunk_size) {
        let text: String = chunk.iter().collect();
        events.push(SseEvent::new(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 3,
                "delta": {"type": "text_delta", "text": text}
            }),
        ));
    }
    events.push(SseEvent::new(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": 3}),
    ));

    // 10. message_delta
    // 官方 API 的 message_delta.delta 中没有 stop_sequence 字段
    events.push(SseEvent::new(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {
                "output_tokens": estimate_websearch_output_tokens(&summary),
                "server_tool_use": {"web_search_requests": 1}
            }
        }),
    ));

    // 11. message_stop
    events.push(SseEvent::new(
        "message_stop",
        json!({"type": "message_stop"}),
    ));

    events
}

/// 摘要的输出 token 估算（流式与非流式共用同一口径）
fn estimate_websearch_output_tokens(summary: &str) -> i32 {
    (summary.len() as i32 + 3) / 4
}

/// 生成搜索结果摘要
pub(crate) fn generate_search_summary(query: &str, results: &Option<WebSearchResults>) -> String {
    let mut summary = format!("Here are the search results for \"{}\":\n\n", query);

    if let Some(results) = results {
        for (i, result) in results.results.iter().enumerate() {
            summary.push_str(&format!("{}. **{}**\n", i + 1, result.title));
            if let Some(ref snippet) = result.snippet {
                // 截断过长的摘要（安全处理 UTF-8 多字节字符）
                let truncated = match snippet.char_indices().nth(200) {
                    Some((idx, _)) => format!("{}...", &snippet[..idx]),
                    None => snippet.clone(),
                };
                summary.push_str(&format!("   {}\n", truncated));
            }
            summary.push_str(&format!("   Source: {}\n\n", result.url));
        }
    } else {
        summary.push_str("No results found.\n");
    }

    summary.push_str("\nPlease note that these are web search results and may not be fully accurate or up-to-date.");

    summary
}

/// 处理 WebSearch 请求
pub async fn handle_websearch_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    payload: &MessagesRequest,
    input_tokens: i32,
) -> Response {
    // 1. 提取搜索查询
    let query = match extract_search_query(payload) {
        Some(q) => q,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "invalid_request_error",
                    "无法从消息中提取搜索查询",
                )),
            )
                .into_response();
        }
    };

    tracing::info!(query = %query, "处理 WebSearch 请求");

    // 2. 创建 MCP 请求
    let (tool_use_id, mcp_request) = create_mcp_request(&query);

    // 3. 调用 Kiro MCP API
    let search_results = match call_mcp_api(&provider, &mcp_request).await {
        Ok(response) => parse_search_results(&response),
        Err(e) => {
            tracing::warn!("MCP API 调用失败: {}", e);
            None
        }
    };

    // 4. 按客户端的 stream 选择响应形态
    let model = payload.model.clone();

    if !wants_stream(payload) {
        return build_websearch_non_stream_response(
            &model,
            &query,
            &tool_use_id,
            &search_results,
            input_tokens,
        );
    }

    let stream =
        create_websearch_sse_stream(model, query, tool_use_id, search_results, input_tokens);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// 响应形态是否为流式：由客户端的 `stream` 字段决定
///
/// 抽成独立函数以便单测覆盖分派本身——直接测两个构造函数会绕过这个判断。
fn wants_stream(payload: &MessagesRequest) -> bool {
    payload.stream
}

/// 构造非流式 WebSearch 响应（标准 Anthropic message 对象）
///
/// 内容块与流式路径共用 `build_websearch_blocks`，两种模式产出一致。
fn build_websearch_non_stream_response(
    model: &str,
    query: &str,
    tool_use_id: &str,
    search_results: &Option<WebSearchResults>,
    input_tokens: i32,
) -> Response {
    let (blocks, summary) = build_websearch_blocks(query, tool_use_id, search_results);

    let body = json!({
        "id": format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")[..24].to_string()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": blocks,
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": estimate_websearch_output_tokens(&summary),
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
            "server_tool_use": {"web_search_requests": 1}
        }
    });

    (StatusCode::OK, Json(body)).into_response()
}

/// 调用 Kiro MCP API
pub(crate) async fn call_mcp_api(
    provider: &crate::kiro::provider::KiroProvider,
    request: &McpRequest,
) -> anyhow::Result<McpResponse> {
    let request_body = serde_json::to_string(request)?;

    tracing::debug!("MCP request: {}", request_body);

    let response = provider.call_mcp(&request_body).await?;

    let body = response.text().await?;
    tracing::debug!("MCP response: {}", body);

    let mcp_response: McpResponse = serde_json::from_str(&body)?;

    if let Some(ref error) = mcp_response.error {
        anyhow::bail!(
            "MCP error: {} - {}",
            error.code.unwrap_or(-1),
            error.message.as_deref().unwrap_or("Unknown error")
        );
    }

    Ok(mcp_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_web_search_tool_only_one() {
        use crate::anthropic::types::{Message, Tool};

        let req = MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: serde_json::json!("test"),
            }],
            stream: true,
            system: None,
            tools: Some(vec![Tool {
                tool_type: Some("web_search_20250305".to_string()),
                name: "web_search".to_string(),
                description: String::new(),
                input_schema: Default::default(),
                max_uses: Some(8),
            }]),
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        assert!(has_web_search_tool(&req));
    }

    #[test]
    fn test_has_web_search_tool_multiple_tools() {
        use crate::anthropic::types::{Message, Tool};

        let req = MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: serde_json::json!("test"),
            }],
            stream: true,
            system: None,
            tools: Some(vec![
                Tool {
                    tool_type: Some("web_search_20250305".to_string()),
                    name: "web_search".to_string(),
                    description: String::new(),
                    input_schema: Default::default(),
                    max_uses: Some(8),
                },
                Tool {
                    tool_type: None,
                    name: "other_tool".to_string(),
                    description: "Other tool".to_string(),
                    input_schema: Default::default(),
                    max_uses: None,
                },
            ]),
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        // 多个工具时不应该被识别为纯 websearch 请求
        assert!(!has_web_search_tool(&req));
    }

    #[test]
    fn test_extract_search_query_with_prefix() {
        use crate::anthropic::types::Message;

        let req = MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: serde_json::json!([{
                    "type": "text",
                    "text": "Perform a web search for the query: rust latest version 2026"
                }]),
            }],
            stream: true,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let query = extract_search_query(&req);
        // 前缀应该被去除
        assert_eq!(query, Some("rust latest version 2026".to_string()));
    }

    #[test]
    fn test_extract_search_query_plain_text() {
        use crate::anthropic::types::Message;

        let req = MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: serde_json::json!("What is the weather today?"),
            }],
            stream: true,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let query = extract_search_query(&req);
        assert_eq!(query, Some("What is the weather today?".to_string()));
    }

    #[test]
    fn test_create_mcp_request() {
        let (tool_use_id, request) = create_mcp_request("test query");

        assert!(tool_use_id.starts_with("srvtoolu_"));
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "tools/call");
        assert_eq!(request.params.name, "web_search");
        assert_eq!(request.params.arguments.query, "test query");

        // 验证 ID 格式: web_search_tooluse_{22位}_{时间戳}_{8位}
        assert!(request.id.starts_with("web_search_tooluse_"));
    }

    #[test]
    fn test_mcp_request_id_format() {
        let (_, request) = create_mcp_request("test");

        // 格式: web_search_tooluse_{22位}_{毫秒时间戳}_{8位}
        let id = &request.id;
        assert!(id.starts_with("web_search_tooluse_"));

        let suffix = &id["web_search_tooluse_".len()..];
        let parts: Vec<&str> = suffix.split('_').collect();
        assert_eq!(parts.len(), 3, "应该有3个部分: 22位随机_时间戳_8位随机");

        // 第一部分: 22位大小写字母和数字
        assert_eq!(parts[0].len(), 22);
        assert!(parts[0].chars().all(|c| c.is_ascii_alphanumeric()));

        // 第二部分: 毫秒时间戳
        assert!(parts[1].parse::<i64>().is_ok());

        // 第三部分: 8位小写字母和数字
        assert_eq!(parts[2].len(), 8);
        assert!(
            parts[2]
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
    }

    #[test]
    fn test_parse_search_results() {
        let response = McpResponse {
            error: None,
            id: "test_id".to_string(),
            jsonrpc: "2.0".to_string(),
            result: Some(McpResult {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: r#"{"results":[{"title":"Test","url":"https://example.com","snippet":"Test snippet"}],"totalResults":1}"#.to_string(),
                }],
                is_error: false,
            }),
        };

        let results = parse_search_results(&response);
        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].title, "Test");
    }

    #[test]
    fn test_generate_search_summary() {
        let results = WebSearchResults {
            results: vec![WebSearchResult {
                title: "Test Result".to_string(),
                url: "https://example.com".to_string(),
                snippet: Some("This is a test snippet".to_string()),
                published_date: None,
                id: None,
                domain: None,
                max_verbatim_word_limit: None,
                public_domain: None,
            }],
            total_results: Some(1),
            query: Some("test".to_string()),
            error: None,
        };

        let summary = generate_search_summary("test", &Some(results));

        assert!(summary.contains("Test Result"));
        assert!(summary.contains("https://example.com"));
        assert!(summary.contains("This is a test snippet"));
    }

    // === 非流式响应（本 change 新增） ===

    fn sample_results() -> WebSearchResults {
        WebSearchResults {
            results: vec![WebSearchResult {
                title: "Rust 1.90".to_string(),
                url: "https://blog.rust-lang.org/".to_string(),
                snippet: Some("release notes".to_string()),
                published_date: Some(1_700_000_000_000),
                id: None,
                domain: None,
                max_verbatim_word_limit: None,
                public_domain: None,
            }],
            total_results: Some(1),
            query: Some("rust".to_string()),
            error: None,
        }
    }

    /// 从非流式 Response 中取出 JSON body
    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .expect("读取响应体失败");
        serde_json::from_slice(&bytes).expect("响应体非 JSON")
    }

    #[tokio::test]
    async fn test_non_stream_returns_json_not_sse() {
        let results = Some(sample_results());
        let resp = build_websearch_non_stream_response(
            "claude-sonnet-4.5",
            "rust",
            "srvtoolu_x",
            &results,
            42,
        );
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert!(
            ct.contains("application/json"),
            "非流式必须返回 JSON content-type，实际: {}",
            ct
        );
        assert!(
            !ct.contains("text/event-stream"),
            "非流式不得返回事件流 content-type"
        );
    }

    #[tokio::test]
    async fn test_non_stream_block_order_and_types() {
        let results = Some(sample_results());
        let json = body_json(build_websearch_non_stream_response(
            "m", "rust", "srvtoolu_x", &results, 10,
        ))
        .await;

        let content = json["content"].as_array().expect("content 应为数组");
        assert_eq!(content.len(), 4, "应有四个内容块");
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "server_tool_use");
        assert_eq!(content[2]["type"], "web_search_tool_result");
        assert_eq!(content[3]["type"], "text");
    }

    #[tokio::test]
    async fn test_non_stream_message_envelope() {
        let json = body_json(build_websearch_non_stream_response(
            "claude-sonnet-4.5",
            "rust",
            "srvtoolu_x",
            &Some(sample_results()),
            77,
        ))
        .await;

        assert_eq!(json["type"], "message");
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["model"], "claude-sonnet-4.5", "model 应回显原值");
        assert_eq!(json["stop_reason"], "end_turn");
        assert!(json["stop_sequence"].is_null());
        assert!(
            json["id"].as_str().unwrap().starts_with("msg_"),
            "id 前缀应为 msg_"
        );
    }

    #[tokio::test]
    async fn test_non_stream_usage_fields() {
        let json = body_json(build_websearch_non_stream_response(
            "m", "rust", "srvtoolu_x", &Some(sample_results()), 77,
        ))
        .await;

        let usage = &json["usage"];
        assert_eq!(usage["input_tokens"], 77);
        assert!(usage["output_tokens"].as_i64().unwrap() > 0);
        assert_eq!(
            usage["server_tool_use"]["web_search_requests"], 1,
            "必须记录服务端搜索次数"
        );
    }

    #[tokio::test]
    async fn test_non_stream_server_tool_use_carries_query() {
        let json = body_json(build_websearch_non_stream_response(
            "m", "rust news", "srvtoolu_abc", &Some(sample_results()), 10,
        ))
        .await;

        let block = &json["content"][1];
        assert_eq!(block["name"], "web_search");
        assert_eq!(block["id"], "srvtoolu_abc");
        assert_eq!(block["input"]["query"], "rust news");
    }

    #[tokio::test]
    async fn test_non_stream_no_results_is_explicit() {
        let json = body_json(build_websearch_non_stream_response(
            "m", "nothing", "srvtoolu_x", &None, 10,
        ))
        .await;

        // 结果列表为空
        assert!(
            json["content"][2]["content"]
                .as_array()
                .unwrap()
                .is_empty(),
            "无结果时 web_search_tool_result.content 应为空数组"
        );
        // 摘要明确说明
        let summary = json["content"][3]["text"].as_str().unwrap();
        assert!(
            summary.contains("No results"),
            "无结果时摘要必须明确说明，实际: {}",
            summary
        );
        assert!(!summary.trim().is_empty());
    }

    #[test]
    fn test_blocks_identical_across_modes() {
        // 两种模式必须产出逐字段相等的内容块
        let results = Some(sample_results());
        let (blocks, summary) = build_websearch_blocks("rust", "srvtoolu_x", &results);

        let events = generate_websearch_events(
            "m",
            "rust",
            "srvtoolu_x",
            Some(sample_results()),
            10,
        );

        // 流式的 server_tool_use 与 web_search_tool_result 块整块发送，可直接比对
        let stream_tool_use = events
            .iter()
            .find_map(|e| {
                let cb = e.data.get("content_block")?;
                (cb.get("type")? == "server_tool_use").then(|| cb.clone())
            })
            .expect("流式缺少 server_tool_use 块");
        assert_eq!(stream_tool_use, blocks[1], "server_tool_use 块跨模式应一致");

        let stream_result = events
            .iter()
            .find_map(|e| {
                let cb = e.data.get("content_block")?;
                (cb.get("type")? == "web_search_tool_result").then(|| cb.clone())
            })
            .expect("流式缺少 web_search_tool_result 块");
        assert_eq!(
            stream_result, blocks[2],
            "web_search_tool_result 块跨模式应一致"
        );

        // 流式的两个 text 块以 delta 分片发送，拼回后比对
        let decision: String = events
            .iter()
            .filter(|e| e.event == "content_block_delta" && e.data["index"] == 0)
            .filter_map(|e| e.data["delta"]["text"].as_str())
            .collect();
        assert_eq!(decision, blocks[0]["text"].as_str().unwrap());

        let stream_summary: String = events
            .iter()
            .filter(|e| e.event == "content_block_delta" && e.data["index"] == 3)
            .filter_map(|e| e.data["delta"]["text"].as_str())
            .collect();
        assert_eq!(stream_summary, summary, "摘要跨模式应一致");
    }

    #[test]
    fn test_stream_usage_matches_non_stream() {
        let events =
            generate_websearch_events("m", "rust", "srvtoolu_x", Some(sample_results()), 10);
        let (_, summary) = build_websearch_blocks("rust", "srvtoolu_x", &Some(sample_results()));

        let delta = events
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("缺少 message_delta");
        assert_eq!(
            delta.data["usage"]["output_tokens"],
            estimate_websearch_output_tokens(&summary),
            "两种模式的 output_tokens 口径应一致"
        );
        assert_eq!(delta.data["usage"]["server_tool_use"]["web_search_requests"], 1);
    }

    fn req_with_stream(stream: bool) -> MessagesRequest {
        use crate::anthropic::types::{Message, Tool};
        MessagesRequest {
            model: "claude-sonnet-4.5".to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: serde_json::json!("rust"),
            }],
            stream,
            system: None,
            tools: Some(vec![Tool {
                tool_type: Some("web_search_20250305".to_string()),
                name: "web_search".to_string(),
                description: String::new(),
                input_schema: Default::default(),
                max_uses: Some(8),
            }]),
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        }
    }

    #[test]
    fn test_detection_independent_of_websearch_emulation_switch() {
        // Responses 端点的 web_search 开关（config.web_search_emulation）
        // MUST NOT 影响 Anthropic 侧判定。这里由签名保证：判定只依赖请求本身，
        // 不接受任何配置入参——若将来有人把开关接进来，本测试会因签名变化而编译失败。
        let req = req_with_stream(false);
        let before = has_web_search_tool(&req);
        assert!(before, "Anthropic 侧应按自身口径命中");

        // 判定为纯函数，同一请求多次调用结果恒定，不读取任何运行时配置
        assert_eq!(has_web_search_tool(&req), before);
    }

    #[test]
    fn test_dispatch_follows_client_stream_field() {
        // 本 change 的核心：响应形态必须跟随客户端的 stream 选择
        assert!(
            !wants_stream(&req_with_stream(false)),
            "stream:false 必须走非流式路径"
        );
        assert!(
            wants_stream(&req_with_stream(true)),
            "stream:true 必须走流式路径"
        );
    }

    #[test]
    fn test_stream_field_defaults_to_false() {
        // MessagesRequest 的 stream 缺省为 false，缺省请求应走非流式
        let req: MessagesRequest = serde_json::from_str(
            r#"{"model":"claude-sonnet-4.5","max_tokens":10,
                "messages":[{"role":"user","content":"hi"}]}"#,
        )
        .expect("反序列化失败");
        assert!(!wants_stream(&req), "缺省 stream 应视为非流式");
    }

    #[test]
    fn test_stream_event_sequence_unchanged() {
        // 回归保护：抽取共享构造不得改变事件序列
        let events =
            generate_websearch_events("m", "rust", "srvtoolu_x", Some(sample_results()), 10);
        let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();

        assert_eq!(names.first(), Some(&"message_start"));
        assert_eq!(names.last(), Some(&"message_stop"));
        assert_eq!(
            names[names.len() - 2],
            "message_delta",
            "message_stop 之前应为 message_delta"
        );

        // 四个块各有 start/stop，index 依次为 0..3
        for idx in 0..4 {
            assert!(
                events.iter().any(|e| e.event == "content_block_start"
                    && e.data["index"] == idx),
                "缺少 index {} 的 content_block_start",
                idx
            );
            assert!(
                events.iter().any(|e| e.event == "content_block_stop"
                    && e.data["index"] == idx),
                "缺少 index {} 的 content_block_stop",
                idx
            );
        }
    }
}
