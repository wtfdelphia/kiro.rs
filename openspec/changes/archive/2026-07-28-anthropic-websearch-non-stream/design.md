# Design: Anthropic web_search 非流式响应

> 范围：修复 `handle_websearch_request` 无条件返回 SSE 的既有缺陷。
> 流式路径行为零改动。
> 对照：sub2api `gateway_websearch_emulation.go:322 writeWebSearchNonStreamResponse`

---

## 1. 缺陷定位

`src/anthropic/websearch.rs:474-520`：

```rust
pub async fn handle_websearch_request(
    provider: Arc<KiroProvider>,
    payload: &MessagesRequest,     // <- payload.stream 从未被读取
    input_tokens: i32,
) -> Response {
    // ... 提取查询 -> MCP 调用 ...
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")   // <- 无条件
        .body(Body::from_stream(stream))
}
```

调用点（两处，均已传入完整 `payload`，无需改动）：

- `handlers.rs:405` `post_messages`
- `handlers.rs:924` `post_messages_cc`

## 2. 流式路径产出的块序列（现状，作为非流式的对照基准）

`generate_websearch_events`（`websearch.rs:246-441`）产出 11 段事件，其中内容块为：

| index | 类型 | 内容 |
| --- | --- | --- |
| 0 | `text` | `I'll search for "<query>".` |
| 1 | `server_tool_use` | `{id, name:"web_search", input:{query}}` |
| 2 | `web_search_tool_result` | `{content:[{type:"web_search_result", title, url, encrypted_content, page_age}]}` |
| 3 | `text` | `generate_search_summary` 的输出 |

`message_delta` 携带：

```jsonc
{"delta":{"stop_reason":"end_turn"},
 "usage":{"output_tokens":N,"server_tool_use":{"web_search_requests":1}}}
```

## 3. 改动方案

### 3.1 抽取共享块构造

把上表四个块的构造从 `generate_websearch_events` 中提出为纯函数，两条路径共用：

```rust
/// 构造 web_search 响应的四个内容块（流式与非流式共用）
fn build_websearch_blocks(
    query: &str,
    tool_use_id: &str,
    results: &Option<WebSearchResults>,
) -> (Vec<serde_json::Value>, String)   // (blocks, summary)
```

`generate_websearch_events` 改为调用它，再把块拆成 SSE 事件——**事件序列与字段不变**，
只是块内容的来源改为共享函数。这是本 change 唯一触及流式路径的地方，由现有流式单测保护。

搜索结果块的构造（`page_age` 的时间戳格式化、`encrypted_content` 取 `snippet`）
原样保留，不改语义。

### 3.2 分派

```rust
pub async fn handle_websearch_request(
    provider: Arc<KiroProvider>,
    payload: &MessagesRequest,
    input_tokens: i32,
) -> Response {
    // 查询提取与 MCP 调用不变
    let (blocks, summary) = build_websearch_blocks(&query, &tool_use_id, &search_results);

    if payload.stream {
        // 现有 SSE 路径
    } else {
        // 新增 JSON 路径
    }
}
```

### 3.3 非流式响应结构

```jsonc
{
  "id": "msg_<24hex>",
  "type": "message",
  "role": "assistant",
  "model": "<payload.model 原值>",
  "content": [
    {"type":"text","text":"I'll search for \"<query>\"."},
    {"type":"server_tool_use","id":"srvtoolu_...","name":"web_search","input":{"query":"..."}},
    {"type":"web_search_tool_result","content":[...]},
    {"type":"text","text":"<summary>"}
  ],
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "usage": {
    "input_tokens": <传入的估算值>,
    "output_tokens": <按 summary 估算>,
    "cache_creation_input_tokens": 0,
    "cache_read_input_tokens": 0,
    "server_tool_use": {"web_search_requests": 1}
  }
}
```

对照 sub2api `:329-347`：它的非流式也是把三个块并列进一个 message
（少一个决策 text 块，因为它没有该文案）。kiro.rs 保留四个块以与自身流式路径一致——
同一次搜索在两种模式下应产出相同内容。

`message_id` 沿用流式路径的生成方式（`msg_` + uuid 前 24 hex）。
`stop_reason` 恒为 `end_turn`（搜索总是完成，不存在 tool_use 待续或超长）。

## 4. 测试

| 断言 | 说明 |
| --- | --- |
| **`stream: false` 返回 JSON** | content-type 为 `application/json`，非 `text/event-stream` |
| **`stream: true` 仍返回 SSE** | 回归保护 |
| 非流式四个块的类型与顺序 | text / server_tool_use / web_search_tool_result / text |
| 非流式与流式块内容一致 | 同一查询与结果下，两条路径的块内容逐字段相等 |
| `server_tool_use.input.query` | 等于提取出的查询 |
| usage 字段完备 | input/output tokens 与 `server_tool_use.web_search_requests` |
| `stop_reason` | 恒为 `end_turn` |
| model 回显 | 等于 `payload.model` 原值 |
| 无结果时 | 摘要明确说明未找到，`web_search_tool_result.content` 为空数组 |
| 查询无法提取 | 两种模式下均返回 400（现有行为，回归保护） |
| **流式事件序列未变** | 现有流式单测全绿 |

## 5. 风险

| 风险 | 缓解 |
| --- | --- |
| 抽取共享函数时改动流式块内容 | 新增「两路径块内容逐字段相等」断言；现有流式单测回归 |
| 客户端依赖旧的（错误的）SSE 行为 | 极不可能：旧行为下 `stream:false` 的客户端本就无法解析响应 |
| `/cc/v1` 与 `/v1` 行为分叉 | 两者共用同一 handler，非流式路径对二者一致（`/cc/v1` 的缓冲差异只作用于上游 generate 流，与 web_search 无关） |
