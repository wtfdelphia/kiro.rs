## Why

OpenAI Responses 是新式 Agent 客户端的入口协议（语义事件流、structured output items、server-side tools）。kiro.rs 目前不支持，Phase A 已把 `/v1/responses` 登记为 `planned`，请求返回 404。这是 `docs/multi-protocol-api-design.md` 里的 P2。

Phase B 已建成 `src/openai/` 的类型、转换核、错误方言与流式基础设施。Responses 不需要第二套上游调用逻辑：请求侧归一成 `ChatCompletionRequest` 后复用 Phase B 的 `to_messages_request`，响应侧新增语义事件状态机。

同时补齐 web_search：kiro.rs 有 Anthropic 专用的 MCP 搜索实现（`websearch.rs`），但 Chat Completions 协议无法诚实表达「服务端代执行搜索」（D10），Responses 协议有原生对应物（`web_search_call` output item），所以这个能力落在本 change。

设计输入：`docs/multi-protocol-api-design.md`（§7 Responses、D2/D10/D11/D12），经 Kiro-Go（`responses_handler.go`、`responses_input.go`、`responses_types.go`）与 sub2api（`gateway_websearch_emulation.go`）源码对照复核。

## What Changes

- **新增 `POST /v1/responses`**：流式（语义事件）+ 非流式（`ResponsesObject`）
- **input 三形状归一**：字符串 / 消息数组 / 单对象 → `Vec<ChatMessage>`，含 `function_call` / `function_call_output` / `input_text` / `input_image` / `output_text` 等 item 类型（对齐 `responses_input.go`）
- **`instructions` → system 消息**，置于展开历史之后以保证当前轮生效
- **首版无状态**（D2）：`previous_response_id` 非空返回 **400**（Kiro-Go 返回 404 因它有持久化；kiro.rs 是「不支持」，400 更准确）；`store` 被忽略
- **流式语义事件**：`response.created` → `in_progress` → `output_item.added` → `content_part.added` → `output_text.delta` → `content_part.done` → `output_item.done` → `completed`；工具走 `function_call` item + `function_call_arguments.delta`；失败走 `response.failed`
- **web_search 仅在本端点支持**（D10）：判定用 sub2api 宽口径（`type` 前缀 `web_search` / 等于 `google_search`，**或** `name` 命中 `web_search` / `google_search` / `web_search_20250305`，见 D11），命中后走 MCP 代执行，输出映射为 `web_search_call` output item
- **web_search 运行时开关**：新增 `GET/PUT /api/admin/settings/websearch`，默认开启；判定放宽后必须有关闭手段（对齐 sub2api 的三层可配设计）
- **catalog 中 `openai.responses` 改 Live**（`openai.responses.retrieve` 仍 planned）
- **不改 Anthropic 行为**：包括**不改** `has_web_search_tool`（D11）

## Capabilities

### New Capabilities

- `openai-responses`：`POST /v1/responses` 的请求归一、无状态语义、非流式对象契约、流式事件序列、usage 语义、错误方言
- `openai-responses-websearch`：Responses 端点的 server-side web_search 判定、输出映射、运行时开关

### Modified Capabilities

- `admin-runtime-settings`：新增 `GET/PUT /api/admin/settings/websearch`（web_search 代执行开关，默认启用、热更新 + 落盘）与配置字段 `webSearchEmulation`；「设置变更安全与校验」扩展到该新增设置组

不作为 Modified 的说明：`public-api-catalog` 与 `openai-chat-completions` 能力（Phase A/B）尚未归档进 `openspec/specs/`，因此 `openai.responses` 的 `planned → live` 状态变更以本 change 自有能力内的需求表达。

补充（2026-07-28 verify 复核）：原先设想「待前序 change 归档后由 `openspec-sync-specs` 收敛」**不成立**——三个 change 的 `specs/` 目录互不包含对方的能力文件，sync 没有可覆盖的目标。实际处置是让每个能力只断言自己持有的端点状态。本 change 的 spec 断言 `/v1/responses` = live、`/v1/responses/{id}` 仍 planned，两者均与代码一致，无需后续收敛。

## Impact

- **代码**：新增 `src/openai/{responses,responses_types,responses_stream,websearch}.rs`；`src/openai/mod.rs`（路由）；`src/anthropic/websearch.rs`（`call_mcp_api` / `create_mcp_request` / `parse_search_results` / `generate_search_summary` 提 `pub(crate)`，**不改实现**）；`src/public_api/catalog.rs`（status）；`src/admin/{router,handlers,service,types}.rs`（websearch 开关）；`src/model/config.rs`（`webSearchEmulation` 字段）；`admin-ui`（设置面板增加开关）
- **API**：新增对外端点 `POST /v1/responses`；新增 Admin `GET/PUT /api/admin/settings/websearch`
- **配置**：新增可选 `webSearchEmulation`（默认 true，缺省兼容现网）
- **风险类型**：协议 / SSE（强制 OpenSpec）、Admin API、配置 schema、认证（新路由必须自带 auth layer）
- **非目标**：
  - 不实现 `previous_response_id` 持久化与 `GET /v1/responses/{id}`（Phase D 独立 change）
  - 不改 Anthropic 侧 `has_web_search_tool` 的判定口径（D11；两端点行为差异写进 client_hints）
  - 不修复 Anthropic 侧 websearch 无条件返回 SSE 的既有问题（D11 相关，单列 change）
  - 不做路径别名 `/responses`（D5）
  - 不实现 `include` / `truncation` / `parallel_tool_calls` / `reasoning.effort` 等 Responses 高级字段
  - 不实现 `google_search` 的实际搜索后端（判定接受，但 MCP 侧仍走 `web_search`）
