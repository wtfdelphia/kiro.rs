## Why

`handle_websearch_request`（`src/anthropic/websearch.rs:474-520`）**从不读取 `payload.stream`**，无条件返回 `Content-Type: text/event-stream`。

后果：客户端向 `/v1/messages` 或 `/cc/v1/messages` 发送 `stream: false` 且携带 `web_search` 工具的请求时，收到的是 SSE 流而非 JSON 对象。Anthropic SDK 会因 content-type 与响应体结构不符而解析失败——客户端拿不到可用响应，且错误信息指向解析层，不易定位到代理侧。

这是既有缺陷，与 Phase A/B/C 均无关（Phase C 的 design D11 记录了它，但明确不并入）。sub2api 的同类实现两条路径都齐（`gateway_websearch_emulation.go:211` 流式 / `:322` 非流式），可作为非流式响应结构的对照。

修复范围小且明确：读取 `payload.stream`，为非流式路径新增一个把现有四个内容块聚合成单个 Anthropic message 对象的分支。流式路径行为完全不变。

## What Changes

- **`handle_websearch_request` 读取 `payload.stream`**：`true` 走现有 SSE 路径（行为不变），`false` 走新增的 JSON 路径
- **新增非流式响应构造**：返回标准 Anthropic message 对象，`content` 依次含
  `text`（搜索决策说明）、`server_tool_use`、`web_search_tool_result`、`text`（结果摘要）四个块，
  与流式路径产出的块序列一致
- **usage 保持一致**：`input_tokens` 用传入的估算值，`output_tokens` 按摘要长度估算，
  并携带 `server_tool_use.web_search_requests`（与流式 `message_delta` 同源）
- **抽取共享构造逻辑**：把「决策文案 / server_tool_use 块 / 搜索结果块 / 摘要」的构造从
  `generate_websearch_events` 中提出，两条路径共用，避免同一份内容出现两处实现
- **不改流式路径的事件序列**：现有 11 段事件顺序、字段、index 分配一字不动
- **不改判定口径**：`has_web_search_tool` 保持 `tools.len() == 1 && name == "web_search"`

## Capabilities

### New Capabilities

- `anthropic-websearch`：Anthropic 端点 web_search 请求的流式与非流式响应契约、内容块结构、usage 语义

### Modified Capabilities

无。该能力此前未在 `openspec/specs/` 中定义，本 change 建立其首个契约。

## Impact

- **代码**：`src/anthropic/websearch.rs`（读取 `stream`、新增非流式分支、抽取共享块构造）。
  调用点 `handlers.rs:405`（`post_messages`）与 `handlers.rs:924`（`post_messages_cc`）无需改动——
  它们已传入完整 `payload`
- **API**：`/v1/messages` 与 `/cc/v1/messages` 在「携带 web_search 工具 + `stream: false`」这一
  组合下的响应形态由（错误的）SSE 变为 JSON。这是修复而非破坏：原行为无客户端能正确消费
- **配置**：无
- **风险类型**：协议（强制 OpenSpec）、转换逻辑
- **非目标**：
  - 不改流式路径的任何事件、字段或顺序
  - 不改 `has_web_search_tool` 的判定口径（放宽属独立的行为变更）
  - 不改 OpenAI 侧（`/v1/responses`）的 web_search 实现（Phase C 已含两条路径）
  - 不改 MCP 调用、搜索结果解析或摘要生成逻辑
  - 不为 `/cc/v1` 的 web_search 引入与 `/v1` 不同的缓冲行为（两者共用同一 handler）
