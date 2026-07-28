# 多协议 API 设计定稿（Public API Catalog + OpenAI 兼容层）

> 状态：**定稿**。Phase A / B / C 已实现，Phase D 待评估
> 日期：2026-07-27
> 本文 supersedes 并替换以下四份并行稿（已删除）：
> - `multi-protocol-api-endpoints-admin-display-design.md`（Catalog / 产品视角）
> - `multi-protocol-endpoints-and-admin-view-optimization-design.md`（协议实现规格）
> - `multi-protocol-api-unified-design.md`（第一次合并尝试）
> - `multi-protocol-public-api-and-admin-catalog-design.md`（第二次合并尝试）
>
> 分析手段：kiro.rs 源码精读（含 `file:line` 复核）+ Kiro-Go 源码精读（`proxy/handler.go`、
> `translator.go`、`responses_*.go`）+ sub2api CodeGraph 与源码精读
> （`backend/internal/service/gateway_websearch_emulation.go`、`openai_alpha_search.go`）
>
> Phase A 的实现契约见 `openspec/changes/public-api-catalog-admin-display/`
> （proposal / design / specs / tasks / evidence）。本文是跨 Phase 的长期设计参考。

---

## 1. Context and motivation

### 1.1 现状

kiro.rs 是把 **Anthropic Messages API** 代理到上游 **Kiro / AWS CodeWhisperer**
（`generateAssistantResponse`）的 Rust/Axum 服务。核心链路：

```
Anthropic 请求
  -> converter (convert_request_with_policy)
  -> Kiro ConversationState (KiroRequest)
  -> KiroProvider 多凭据调用上游
  -> AWS event-stream 解码 (EventStreamDecoder + Event)
  -> stream/handlers 转回 Anthropic SSE / JSON
```

对外暴露（`src/anthropic/router.rs:49-74`）：`/v1/models`、`/v1/messages`、
`/v1/messages/count_tokens`、`/cc/v1/messages`、`/cc/v1/messages/count_tokens`。

对 OpenAI 协议的支持仅停留在「把 `gpt-4o` 等模型名当别名映射到 Claude」
（`converter.rs` `builtin_compat_aliases`），**没有 OpenAI 协议的路由、类型或格式转换**。

### 1.2 问题陈述

| # | 现象 | 根因 | 影响 | 状态 |
| --- | --- | --- | --- | --- |
| P1 | 无 `/v1/chat/completions` | 未实现 OpenAI 协议层 | OpenAI SDK / Cursor / OpenWebUI 无法直连 | **已解决（B）** |
| P2 | 无 `/v1/responses` | 未实现 Responses 协议层 | 新式 OpenAI Agent 客户端无法接入 | **已解决（C）** |
| P3 | Admin 不展示对外端点 | 无公开端点目录 | Base URL / path / 鉴权头靠猜 | **已解决（A）** |
| P4 | 端点事实三处漂移 | router、启动日志、文档各写一份 | 改一处漏两处 | **已解决（A）** |
| P5 | `/settings/endpoint` 语义易误解 | 它指「上游 Kiro 端点(ide)」 | 与「对外 API 端点」概念混淆 | **已解决（A）** |

P4 的实例：Phase A 实现时发现原手写启动日志只有 3 条，漏了 `/cc/v1` 两条端点。

### 1.3 概念澄清（贯穿全文的命名纪律）

```
A. Public Client API   客户端 -> 本代理（/v1/messages、/v1/chat/completions …）
B. Upstream Endpoint   本代理 -> 上游 Kiro（ide；Go 侧还有 kiro/cw/amazonq preferred+fallback）
```

- 既有 `/api/admin/settings/endpoint` = **B**，不可复用于对外端点
- 一切对外端点相关标识符用 `publicApi` / `public-api` / `publicEndpoints`
- **禁止裸用 `endpoint`**。反例：`GET /api/admin/endpoints` 会直接踩 P5 的歧义

### 1.4 Goals

- G1：建立 **Public API Catalog** 作为路径 / 鉴权 / 状态的单一事实源（消除 P4）
- G2：Admin 只读面板展示 Base URL、端点、鉴权示例、curl / SDK 片段、一键复制
- G3：新增 OpenAI Chat Completions（流式 + 非流式，含 function tools）
- G4：新增 OpenAI Responses（流式 + 非流式，首版无状态）
- G5：全部复用 `resolve_model` + `KiroProvider` + `EventStreamDecoder`，不另起上游栈
- G6：既有 Anthropic 行为零回归；每阶段可独立验证

### 1.5 Non-goals

- 不改上游调用协议（仍是 `generateAssistantResponse`）
- 不做路径别名（`/messages`、`/chat/completions` 等）——见 D5
- 不做 Responses 服务端持久化 `previous_response_id`——见 D2
- 不做 Assistants / Realtime / Embeddings / Images
- 不复刻 Go 的多 API Key 配额、审计日志、request-logs
- 不实现 OpenAI 的 `logprobs`、`n>1`、`seed`
- 不改 `/cc/v1` 既有行为，不改双 key 鉴权模型（client apiKey / adminApiKey 分离）
- 不改负载均衡、Docker/CI、密钥存储策略

---

## 2. 三项目对照

### 2.1 CodeGraph 索引规模（实测）

| 项目 | Files | Nodes | Edges | 主语言 |
| --- | --- | --- | --- | --- |
| kiro.rs | 103 | 1,813 | 4,743 | rust / tsx |
| Kiro-Go | 111 | 1,539 | 4,026 | go / jsx |
| sub2api | 2,840 | 87,114 | 191,426 | go / vue |

### 2.2 Kiro-Go 关键事实（源码核对）

- 路由是 `proxy/handler.go:341 ServeHTTP` 里的一个大 `switch`：
  - `/v1/messages`（+ `/messages`、`/anthropic/v1/messages`）→ `handleClaudeMessages`
  - `/v1/chat/completions`（+ `/chat/completions`）→ `handleOpenAIChat`（`handler.go:1568`）
  - `/v1/responses`（+ `/responses`）→ `handleOpenAIResponses`（`responses_handler.go:15`）
  - `/v1/models`（+ `/models`）→ `handleModels`（`handler.go:447`，返回 **Anthropic 风格**列表
    + `auto`/`gpt-4o`/`gpt-4` 别名，**不要求 api key**）
- thinking 是**纯函数 + bool 参数**：`ParseModelAndThinking(model, suffix) (string, bool)`
  （`translator.go:74`），三个入口各调一次（`handler.go:1592`、`handler.go:3426`、
  `responses_handler.go:107`），bool 一路传参到 `OpenAIToKiro(&req, thinking)`，
  由后者把 `ThinkingModePrompt` 常量拼到 system prompt 前（`translator.go:1175`）。
  **漏调编译不过**
- Responses 复用 Chat 执行核：`parseResponsesInput` 把 `input`/`instructions`/
  `previous_response_id` 归一为 `[]OpenAIMessage`，再拼 `OpenAIRequest` 交给 `OpenAIToKiro`
- `OpenAITool.UnmarshalJSON`（`translator.go:1096`）**同时兼容** Chat 的
  `{type,function:{name,parameters}}` 与 Responses 的顶层 `{type,name,parameters}`
- OpenAI 流式（`handler.go:1606`）：逐块 `chat.completion.chunk`，thinking 支持三种承载
  （`thinking` 标签 / `think` 标签 / `reasoning_content` 字段），末尾 `data: [DONE]`
- `responses_store.go` / `responses_history.go` 负责 `previous_response_id` 落盘与历史展开，
  找不到时返回 **404**
- **完全没有 web_search**（`grep -rn "web_search" proxy/*.go` 无匹配）

### 2.3 sub2api 关键事实（源码核对）

- web_search 有两套**互不相通**的机制：
  - `gateway_websearch_emulation.go`（394 行）：代理自己调第三方搜索 API（brave / tavily），
    输出 Anthropic 形状。拦截点在 `gateway_forward.go:90`，是 `Forward` 的第一个分支，
    早于 passthrough 与任何上游调用
  - `openai_alpha_search.go`（670 行）：把请求转成 `tools:[{"type":"web_search"}]` 的
    Responses 请求，交给**上游 hosted web_search** 执行，再把 SSE 转回 alpha/search 形状
  - `grep -rln "shouldEmulateWebSearch|handleWebSearchEmulation" backend/internal/` 只有
    定义处与 `gateway_forward.go` 两个文件 —— **emulation 没有接进任何 OpenAI 端点**，
    尽管 sub2api 有 200+ 个 `openai_*.go`
- emulation 的判定比 kiro.rs 宽（`isWebSearchToolJSON`，`:96-105`）：`type` 前缀匹配
  `web_search` 或等于 `google_search`，**或** `name` 命中
  `web_search` / `google_search` / `web_search_20250305`
- emulation 流式与非流式**两条路径都齐**（`:211` / `:322`）。非流式就是把
  `server_tool_use` + `web_search_tool_result` + `text` 三个 block 并列在一个 message 里（`:329-347`）
- emulation 有**三层开关**：全局 setting → account mode（`enabled`/`disabled`/`default`）
  → channel config（`:53-79`），不是硬编码行为

### 2.4 kiro.rs 可复用资产

| 能力 | 位置 | OpenAI 端点如何复用 |
| --- | --- | --- |
| 模型解析（gpt 别名、auto、catalog 透传） | `converter.rs:256 resolve_model` | 直接复用 |
| 请求转换（Kiro 侧全部约束） | `converter.rs:471 convert_request_with_policy` | 直接复用（D1） |
| 上游调用 + 多凭据故障转移 | `kiro/provider.rs call_api / call_api_stream` | 直接复用 |
| AWS event-stream 解码 | `kiro/parser/decoder.rs` + `kiro/model/events/` | 直接复用 |
| token 估算 | `token::count_all_tokens` | 复用（按 OpenAI 消息适配入口） |
| 鉴权中间件 + 热更新 | `anthropic/middleware.rs AppState/auth_middleware` | 复用同一 state |
| 模型 catalog | `token_manager().global_model_catalog()` | 复用生成模型列表 |
| 内部 Kiro 请求结构 | `kiro/model/requests/` | 直接复用 |
| MCP 搜索调用 | `websearch.rs:522 call_mcp_api`、`:203 parse_search_results` | Phase C 提 `pub(crate)` 共用 |
| 错误映射 | `handlers.rs:39 map_provider_error` | 抽出通用部分 + OpenAI 方言包装 |

**结论**：底层 100% 可复用。新增工作集中在**协议类型 + 请求映射 + 响应/流转换 + handler
+ 路由 + catalog** 六处。

### 2.5 能力矩阵

| 能力 | Kiro-Go | sub2api | kiro.rs 现状 | 目标 |
| --- | --- | --- | --- | --- |
| Claude Messages | 有 | 有 | 有 | 保持 |
| count_tokens | 有 | 有 | 有 | 保持 |
| Models | 有（免鉴权） | 有 | 有（需鉴权） | 共享 catalog（D6） |
| gpt 别名 | 有 | 有 | `resolve_model` 已有 | 共用 |
| OpenAI Chat | 有 | 有 | **已实现** | 保持 |
| OpenAI Responses | 有（有状态） | 有 | **已实现（无状态）** | 保持 |
| web_search | 无 | emulation + 上游原生 | Anthropic 侧 MCP | **已实现（仅 Responses，D10）** |
| Admin 展示对外 API | 弱 | 有 | **已有（A）** | — |
| 路径别名 | 有 | 有 | 仅 `/v1` + `/cc/v1` | 不做（D5） |

---

## 3. 设计原则

1. **Surgical**：先 catalog + Admin，再协议增量；每改动一行都能追溯到具体 Goal
2. **单一事实源**：路径 / 方法 / 状态 / 鉴权只在 catalog 定义一次，router 挂载、启动日志、
   Admin DTO 全部引用它
3. **适配器薄，核心厚**：协议差异只体现在类型映射与输出状态机，执行核共用
4. **错误方言正确**：Anthropic 端点返回 Anthropic error shape，OpenAI 端点返回 OpenAI error shape
5. **状态不撒谎**：`status = live | beta | planned`，planned 不得展示为可用
6. **永不回传完整密钥**：Admin DTO 只给 mask；示例统一用占位符 `API_KEY`
7. **canonical 固定 `/v1/...`**：别名是可选增强，不是默认

方案取舍：

| 方案 | 结论 |
| --- | --- |
| 仅 UI 展示现有 Claude 端点 | 不够，解决不了 P1/P2 |
| 一次性移植 Kiro-Go 全协议 + 持久化 | 拒绝，回归面过大且含落盘风险 |
| **Catalog + 分阶段协议** | 采纳 |

---

## 4. 目标数据流

```
OpenAI Chat / Responses 请求
  -> openai::converter / responses 归一
  -> anthropic::types::MessagesRequest（内存，不经 HTTP / 不做 JSON 往返）
  -> convert_request_with_policy + resolve_model
  -> KiroProvider call_api / call_api_stream
  -> EventStreamDecoder + Event
  -> openai::stream 写出 Chat chunk 或 Responses 语义事件
     （Anthropic StreamContext 不复用产出，平行实现输出状态机）

Claude 路径      -> 现有 handlers 不变
Admin UI         -> GET /api/admin/public-api <- Public API Catalog
```

模块划分：

```
src/public_api/          # 已实现（Phase A）
├── mod.rs
├── catalog.rs           # canonical 清单（唯一事实源）
├── dto.rs               # Admin 响应 DTO + 示例生成
└── routes_test.rs       # live ⊆ routes / planned ∉ routes 双向断言

src/openai/              # 待实现（Phase B/C）
├── mod.rs               # create_openai_routes(app_state)
├── types.rs             # Chat / Responses 请求&响应类型（serde）
├── converter.rs         # OpenAI -> anthropic::types::MessagesRequest 映射
├── stream.rs            # Event -> chat.completion.chunk / response.* 状态机
├── error.rs             # OpenAI error shape 包装
├── handlers.rs          # post_chat_completions / post_responses
├── responses.rs         # Responses input/instructions 归一（Phase C）
└── websearch.rs         # Responses 端点的 web_search（Phase C，D10）
```

---

## 5. Public API Catalog（Phase A，已实现）

### 5.1 数据模型

```rust
pub enum EndpointStatus { Live, Beta, Planned }
pub enum AuthKind { ClientApiKey }

pub struct PublicEndpoint {
    pub id: &'static str,                  // "openai.chat.completions"
    pub family: &'static str,              // "claude" | "openai-chat" | "openai-responses" | "models"
    pub method: &'static str,
    pub path: &'static str,
    pub aliases: &'static [&'static str],  // 首版全为空（D5）
    pub auth: AuthKind,
    pub status: EndpointStatus,
    pub stream: bool,
    pub summary: &'static str,
    pub client_hints: &'static [&'static str],
}
```

全静态常量表，无运行时构造。

### 5.2 Canonical 清单

| id | family | method | path | stream | status |
| --- | --- | --- | --- | --- | --- |
| `models.list` | models | GET | `/v1/models` | - | Live |
| `claude.messages` | claude | POST | `/v1/messages` | 是 | Live |
| `claude.count_tokens` | claude | POST | `/v1/messages/count_tokens` | 否 | Live |
| `claude.cc.messages` | claude | POST | `/cc/v1/messages` | 是（缓冲流） | Live |
| `claude.cc.count_tokens` | claude | POST | `/cc/v1/messages/count_tokens` | 否 | Live |
| `openai.chat.completions` | openai-chat | POST | `/v1/chat/completions` | 是 | **Live** |
| `openai.responses` | openai-responses | POST | `/v1/responses` | 是 | **Live** |
| `openai.responses.retrieve` | openai-responses | GET | `/v1/responses/{id}` | 否 | Planned（D+） |

### 5.3 防漂移契约（本方案的核心机制）

双向断言：

- **`live ⊆ routes`**：每个 Live 的 `(method, path)` 打真实 Router 非 404（401 算命中，
  因为它证明路由存在）
- **`planned ∉ routes`**：每个 Planned 的 `(method, path)` 必须 404

这是 planned → live 切换的门禁。实现时验证过有效性：临时把
`openai.chat.completions` 改成 Live，5 个测试立即转红。

### 5.4 Admin 接口

```http
GET /api/admin/public-api      # admin_auth_middleware 保护
```

响应含 `server`（listenHost / port / requireApiKey / apiKeyMask / authHeaders /
suggestedBaseUrl）与按协议族分组的 `families[]`，每个端点带 method / path / aliases /
status / stream / summary / clientHints / examples.curl。

约束：只回 mask，示例用 `API_KEY` 占位符，`suggestedBaseUrl` 未配置时为 `null`
（前端回落 `window.location.origin`）。

### 5.5 启动日志

`main.rs` 改为遍历 catalog 中 `status == Live` 的条目打印 `method path`。
Admin 段落保持手写并加注释——Admin 路由不属于 Public Client API，catalog 只管 A 类。

---

## 6. OpenAI Chat Completions（Phase B）

### 6.1 请求类型

```rust
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)] pub stream: bool,
    pub stream_options: Option<StreamOptions>,   // include_usage（D12）
    pub max_tokens: Option<i32>,
    pub max_completion_tokens: Option<i32>,      // 新 SDK 字段，与 max_tokens 二者取一
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub tools: Option<Vec<OpenAiTool>>,
    pub tool_choice: Option<serde_json::Value>,
}

pub struct ChatMessage {
    pub role: String,                                  // system|user|assistant|tool
    #[serde(default)] pub content: serde_json::Value,  // string 或 parts[]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    pub tool_call_id: Option<String>,
}
```

**`OpenAiTool` 必须手写 `Deserialize`**，同时接受两种形状（对齐 `translator.go:1096`）：

```jsonc
{"type":"function","function":{"name":"f","parameters":{}}}  // Chat Completions
{"type":"function","name":"f","parameters":{}}               // Responses
```

漏掉任一形状会导致 name 为空、上游返回 400。这是 Phase C 复用 Phase B 转换器的前提。

### 6.2 请求映射

`fn to_messages_request(req: &ChatCompletionRequest) -> Result<MessagesRequest, OpenAiError>`：

1. `model` 原样传入（thinking 后缀处理见 D8，**不在此处剥离**）
2. `system` role 消息 → `MessagesRequest.system`（多条按顺序合并）
3. `user` / `assistant` → `messages`（content string 或 parts 数组 → Anthropic content blocks，
   含 image parts）
4. `assistant.tool_calls` → `tool_use` block；`tool` role → `tool_result` block
   （按 `tool_call_id` 配对）
5. `tools[].function` → Anthropic `Tool { name, description, input_schema }`
6. `max_tokens` / `max_completion_tokens` 取先出现的非空值，都缺省时填 64000
7. 结果交给 `convert_request_with_policy(&msg_req, &policy, catalog_set)`

映射要点对照 `OpenAIToKiro`（`translator.go:1155-1290`）：system 合并、最后一条 user 作为
current message、tool_results 归集、图片 parts 提取。

### 6.3 非流式响应

```jsonc
{
  "id": "chatcmpl-xxx", "object": "chat.completion", "created": 1719446400,
  "model": "<原始请求 model，见 D9>",
  "choices": [{ "index": 0,
    "message": { "role": "assistant", "content": "...", "tool_calls": [] },
    "finish_reason": "stop" }],
  "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 }
}
```

`finish_reason`：有 tool_use → `tool_calls`；上下文超限 → `length`；否则 `stop`。
`prompt_tokens` 优先 `context_input_tokens`，`None` 时回落 `count_all_tokens`（D12）。

### 6.4 流式响应

`Content-Type: text/event-stream`，逐个发 `chat.completion.chunk`：

- 首块 `delta: {"role":"assistant"}`
- 文本增量 `delta: {"content":"..."}`
- 工具调用增量 `delta: {"tool_calls":[{"index":0,"id":"...","function":{"name":"...",
  "arguments":"片段"}}]}`（arguments 分片累积）
- 末块带 `finish_reason`
- `include_usage` 为 true 时，追加一个 `choices: []` 且带 `usage` 的 chunk（D12）
- `data: [DONE]`
- thinking：默认走 `reasoning_content` 字段（对齐 Kiro-Go 默认），标签格式（`<think>`）
  作为后续可选配置，**不污染 `content`**

新增 `OpenAiStreamContext`（`openai/stream.rs`），职责对应 Anthropic `StreamContext`
但产出 OpenAI chunk。ping 保活复用 `handlers.rs` 的 25s interval 模式。

---

## 7. OpenAI Responses（Phase C）

### 7.1 请求类型与归一

`ResponsesRequest` 对齐 Go 的 `responses_types.go`：`model / input / instructions / stream /
tools / tool_choice / previous_response_id / store / temperature / max_output_tokens / metadata`。

`parse_responses_input`（参考 `responses_handler.go:33-124` + `responses_input.go`）：

1. `input` 支持三种形状：字符串、消息数组、单对象 → 统一转 `Vec<ChatMessage>`
2. `instructions` → 追加为 `system` 消息（置于展开历史之后，保证当前轮生效）
3. `previous_response_id` 非空 → 400 OpenAI error（D2）
4. 归一后拼成 `ChatCompletionRequest`，复用 §6.2 的 `to_messages_request`

分层：`ResponsesRequest -> Vec<ChatMessage> -> ChatCompletionRequest -> 共享执行核
-> ResponsesObject / SSE`。Responses 不写第二套上游调用逻辑。

### 7.2 非流式响应

```jsonc
{
  "id": "resp_xxx", "object": "response", "created_at": 1719446400,
  "status": "completed", "model": "<model>",
  "output": [{ "id": "msg_x", "type": "message", "role": "assistant",
               "content": [{ "type": "output_text", "text": "..." }] }],
  "usage": { "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 }
}
```

工具调用输出为 `{"type":"function_call","call_id":"...","name":"...","arguments":"..."}` item。

### 7.3 流式语义事件

按 `responses_handler.go:292+` 逐事件对齐：

```
event: response.created             data: {...}
event: response.in_progress         data: {...}
event: response.output_item.added   data: {... type:"message"}
event: response.content_part.added  data: {... type:"output_text"}
event: response.output_text.delta   data: {"delta":"..."}
...
event: response.content_part.done   data: {...}
event: response.output_item.done    data: {...}
event: response.completed           data: {"response":{...usage...}}
```

工具调用路径：`response.output_item.added(function_call)` →
`response.function_call_arguments.delta` → `response.output_item.done`。
失败路径：`response.failed`。

`usage` 在 `response.completed` 里，位置在末尾，`contextUsageEvent` 天然赶得上（D12）。

### 7.4 web_search（仅 Responses 端点）

见 D10 / D11。判定用 sub2api 宽口径，输出映射：

```
server_tool_use        → response.output_item.added { type: "web_search_call" }
web_search_tool_result → response.output_item.done  { status: "completed" }
text                   → response.content_part.added + response.output_text.delta
```

复用 `websearch.rs` 的 `call_mcp_api` / `parse_search_results` / `create_mcp_request` /
`generate_search_summary`（提 `pub(crate)`）。`extract_search_query` 必须新写——
Anthropic 版剥的是 `"Perform a web search for the query: "` 前缀（`websearch.rs:137`），
那是 Claude Code 客户端约定。

同时需加运行时开关（对齐 sub2api 的可配置设计），否则判定放宽后用户没法关闭。

---

## 8. 路由挂载（Phase B/C）

```rust
let openai_app = openai::create_openai_routes(app_state.clone());
let app = anthropic_app.merge(openai_app);
```

**`merge` 只合并路由表，不传播已应用的 layer。** `router.rs:69-74` 在 `anthropic_app`
上挂了三样，`openai_app` 必须各自补齐：

| 遗漏 | 后果 |
| --- | --- |
| `auth_middleware`（`router.rs:55-58` 模式） | **OpenAI 端点裸奔，任何人可调用（安全事故）** |
| `cors_layer()` | 浏览器端客户端全部被 CORS 拦截 |
| `DefaultBodyLimit::max(MAX_BODY_SIZE)`（50MB） | 退回 axum 默认 2MB，带图片请求 413 |

第三项症状最像上游问题，最容易漏。

其它：

- `/v1/chat/completions`、`/v1/responses` 与 `/v1/messages` 同前缀不同路径，axum 可共存
- `/v1/models` 已由 Anthropic 侧注册，OpenAI 端**不重复注册**。其响应
  （`ModelsResponse { object: "list", data: [...] }`，`types.rs:43-59`）是 OpenAI list shape
  的超集，SDK 可直接消费。若日后需要严格 OpenAI 字段子集，可新增
  `GET /openai/v1/models` 复用 `models_from_catalog` 但只输出 `{id,object,created,owned_by}`
  ——首版不做

---

## 9. Admin UI（Phase A，已实现）

顶栏「对外 API 端点」按钮（与运行时设置并列）→ Dialog。四段：

1. **服务概要**：Base URL（默认 `window.location.origin`，可本地覆盖且仅影响复制文本）、
   `requireApiKey`、apiKey mask、支持的鉴权头
2. **协议分组卡**：METHOD + path + status badge + stream 徽章 + 复制 URL / curl
3. **客户端配方**：

| 客户端 | Base URL | 主路径 | 鉴权 |
| --- | --- | --- | --- |
| Anthropic SDK | `http://host:port` | `/v1/messages` | `x-api-key` 或 Bearer |
| Claude Code | `http://host:port` | `/cc/v1/messages` | 同上 |
| OpenAI SDK (Chat) | `http://host:port/v1` | `/chat/completions` | Bearer |
| OpenAI SDK (Responses) | `http://host:port/v1` | `/responses` | Bearer |
| Models | `http://host:port/v1` | `/models` | 同 public API（D6） |

`OPENAI_BASE_URL` 要带 `/v1`、`ANTHROPIC_BASE_URL` 不带，这是接入时最高频的错误。

4. **接入须知**：区分「对外 Public API」与「Kiro 上游 endpoint」；planned 标注未启用；
   `/cc/v1` 与 `/v1` 的流式差异；Models 需鉴权；示例 key 为占位符

---

## 10. 关键决策记录

D1–D6 来自前序设计稿的冲突定稿；D7–D12 来自设计评审（grilling）中对源码复核后的修正。

### D1 转换路线：OpenAI → `MessagesRequest`（内存）→ 复用 `convert_request_with_policy`

Kiro-Go 为每种协议写**直达 Kiro** 的转换器（`ClaudeToKiro` / `OpenAIToKiro`）。kiro.rs 不照搬。

`convert_request_with_policy` 里已沉淀的 prefill 处理、工具名缩短、system 分块、
thinking 前缀、tool_use/tool_result 配对校验、孤儿清理，**都是 Kiro 侧约束而非 Anthropic
协议特性**。重写一遍等于把这批边界条件复制一份并各自演化。

注意这不是「二次经 Anthropic JSON」，是内存结构映射。响应侧无法复用 Anthropic
`StreamContext`（它产出 Anthropic 事件），必须平行实现。

若日后证明中间层损失语义，再单列「直达 Kiro」change，以契约测试驱动。

### D2 Responses 首版无状态，`previous_response_id` 明确报 400

`store` 忽略；`previous_response_id` 存在时返回 **400**（OpenAI error shape，message 说明
本服务未启用有状态续接）。Kiro-Go 返回 404（它有持久化，是「找不到」）；kiro.rs 是
「不支持」，400 更准确。

**不静默丢历史**——静默降级会让客户端拿到无上下文的答复且无法察觉。

落盘持久化（`responses/` 目录 + TTL + 深度上限防环）或进程内 LRU 作为**独立 change** 评估：
涉及敏感内容落盘、内存增长、多实例一致性三类风险，不与协议实现同批交付。

### D3 Admin 接口命名 `GET /api/admin/public-api`

DTO 字段用 `publicApi` / `publicEndpoints`。理由见 §1.3。

### D4 阶段顺序：先 Catalog + Admin，再 OpenAI 协议

Phase A 不触碰上游链路，回归风险接近零，且立刻交付可见价值。代价是需要 `status` 字段
区分 planned/live——这个字段本身就是防漂移机制的一部分。

### D5 不做路径别名

Kiro-Go 与 sub2api 都支持 `/messages`、`/chat/completions`、`/anthropic/v1/messages` 等别名。
收益是兼容硬编码客户端，成本是路由表膨胀 + 与 `/admin` nest 的潜在通配冲突。

前序两稿在此冲突（一稿主张首版 strict、一稿主张默认 compat）。**定稿取 strict**：
主张 compat 的那稿自己的风险表里也承认「别名与 Admin UI 通配冲突」，默认开启与该风险自相矛盾。

catalog 预留 `aliases` 字段（首版全为空数组），需要时再开 `aliasMode = strict|compat`。

### D6 `/v1/models` 鉴权口径：与其它 public API 一致

保持 kiro.rs 现状（受 `require_api_key` 约束，比 Kiro-Go 严）。**这是与 Kiro-Go 的第一处
已知行为差异**：某些客户端会在未配置 key 时先探测模型列表并因 401 失败。
Admin 面板必须在 Models 卡片上显式标注「需鉴权」。

### D7 planned 端点统一返回 404，不做 501 占位

前序两稿冲突（一稿定 404，一稿写「404/501 + body 说明未启用」）。

**定稿取 404**。501 需要真实挂一个占位 handler，这与 `planned ∉ routes` 断言直接矛盾——
一个能返回 501 的路径就是已挂载的路径。要么可命中要么不可命中，不能既标 planned
又占着路由表。

### D8 OpenAI 侧不抽共享前置层，每个 handler 各自捞状态

`post_messages`（`handlers.rs:401-500`）与 `post_messages_cc`（`handlers.rs:924-1023`）的
前置段目前是**逐字拷贝的两份**。加 Chat 变三份，加 Responses 变四份。

**定稿：不抽 `prepare_messages_request` 共享函数**，OpenAI handler 各自捞。理由是抽取会
改动 Anthropic 现有链路，违反「anthropic 零行为侵入」。

代价必须靠测试兜住——以下四项漏捞全部是**静默错误，编译器不报**：

| 项 | 来源 | 漏了的症状 |
| --- | --- | --- |
| `override_thinking_from_model_name(&mut payload)` | `handlers.rs:834` | `-thinking` 后缀失效，`reasoning_content` 永远为空 |
| `tool_name_map` | `ConversionResult` 第二字段（`converter.rs:372`） | 超长工具名回显哈希短名，工具调用链断 |
| `input_tokens` | `token::count_all_tokens(model, system, messages, tools)` | usage 全 0 |
| `thinking_enabled` | `payload.thinking.as_ref().map(is_enabled)` | 流式不解析 `<thinking>` 标签，思考内容混进 content |

**前序四稿都误述了 thinking 的生效点**（写成「由下游 `resolve_model` 处理」）。实际：
`resolve_model` 返回的 `thinking_requested`（`converter.rs:129,267`）**全项目无消费者**；
真正生效的是 handler 层的 `override_thinking_from_model_name`，它写 `payload.thinking` 与
`payload.output_config.effort`（adaptive 时，适用于 opus 4.6 / sonnet-5），
再由 `generate_thinking_prefix`（`converter.rs:875`）读取生成 `<thinking_mode>` 标签。

Kiro-Go 用纯函数 + bool 参数，漏调编译不过（§2.2）。kiro.rs 是副作用式改写请求字段，
漏调静默降级——这是 Go 那套结构不会有的失效模式。

Phase B 必须包含的两条锁定测试：

1. 带 `-thinking` 后缀的 OpenAI 请求 → 断言最终 `ConversationState` 的 system 含 `<thinking_mode>`
2. 超长工具名 → 断言输出 `tool_calls[].function.name` 是原名而非哈希短名

`count_all_tokens` 按值消费 `payload.system/messages/tools`（`handlers.rs:462-467`），
调用顺序被锁死在 `convert_request_with_policy` 之后。

### D9 model 回显原始公开名，不覆写为 resolve 后的 id

Kiro-Go 的写法是 `req.Model = actualModel`（`handler.go:1593`）后传给流，回显 resolve 后的 id。
kiro.rs 的 `StreamContext.model` 存的是**原始公开模型名**用于回显，resolve 后的 id 只存在于
`conversation_state`。

**定稿：OpenAI 侧保持 kiro.rs 现有行为，回显原值。** 这是与 Kiro-Go 的第二处已知行为差异。

副作用：客户端传 `gpt-4o` 时回显 `gpt-4o`，实际跑 Claude。OpenAI SDK 不校验该字段，
但按回显 model 归类计费的中间层（LiteLLM、OpenWebUI 用量统计）会归到 `gpt-4o`。
已写进 catalog 的 `client_hints`。

### D10 web_search：只挂 Responses，不挂 Chat Completions

kiro.rs 现有 websearch 是 Anthropic 专用的完整实现：`has_web_search_tool`
（`websearch.rs:108`，条件 `tools.len() == 1 && name == "web_search"`）拦截后走 MCP，
`generate_websearch_events`（`websearch.rs:246-441`）产出 11 段硬编码 Anthropic SSE，
含 `server_tool_use`、`web_search_tool_result` 两种 block 与
`usage.server_tool_use.web_search_requests`。

对照结论（§2.2、§2.3）：Kiro-Go 完全没有；sub2api 有两套但 emulation 从未接进 OpenAI 端点。
kiro.rs 上游是 `generateAssistantResponse`，没有 hosted web_search，所以 sub2api 的
OpenAI 路线（走上游原生）不可行，只能走 emulation 路线。

**定稿：web_search 在 `/v1/responses` 支持，`/v1/chat/completions` 不支持。**
Chat Completions 协议里没有任何字段能诚实表达「服务端已代你执行了搜索」；
往 `content` 塞 markdown 或伪造 `tool_calls` 都是编造语义。sub2api 有 200+ 个
`openai_*.go` 却唯独没把 emulation 接到 chat completions，这个缺位不像是遗漏。

### D11 OpenAI 侧 web_search 判定用 sub2api 宽口径；Anthropic 侧不改

OpenAI 侧判定照 sub2api `isWebSearchToolJSON`（`gateway_websearch_emulation.go:96-105`）：
`type` 前缀匹配 `web_search` 或等于 `google_search`，**或** `name` 命中
`web_search` / `google_search` / `web_search_20250305`。

kiro.rs 现有 Anthropic 侧的 `name == "web_search"` 精确匹配接不住 `web_search_20250305`
（Anthropic 带日期的官方 tool 名）。

**Anthropic 侧 `has_web_search_tool` 不改。** 放宽它会让现在走正常 tools 路径转发上游的
`web_search_20250305` 请求变成被代执行——行为反转，属于必须独立走 OpenSpec 的既有协议变更，
且会让 Phase C 回归面失控。

已知不一致（须写进 `client_hints` 与 Phase C 的 non-goal）：同一个 `web_search_20250305`
请求，打 `/v1/messages` 转发上游，打 `/v1/responses` 被代执行。

**相关既有问题（已修复）**：`handle_websearch_request` 曾无条件返回
`text/event-stream`，不看 `payload.stream`，导致 Anthropic 客户端发 `stream: false`
时拿不到可解析的响应。已由 `openspec/changes/anthropic-websearch-non-stream/` 修复：
现在两条路径共用同一份内容块构造，响应形态跟随客户端的 `stream` 字段。

Phase C 的 websearch 非流式必须新写，照 sub2api 的三 block 并列结构
（`gateway_websearch_emulation.go:329-347`）。

### D12 实现 `stream_options.include_usage`

Anthropic 与 OpenAI 在 usage 位置上恰好相反：

- Anthropic：`usage.input_tokens` 必须在**首个**事件（`message_start`）
- OpenAI 流式：`usage` 默认不发；传 `stream_options: {"include_usage": true}` 时在
  `[DONE]` 前的最后一个 chunk 发（该 chunk `choices` 为空数组）

而 `input_tokens` 的准确值来自 `contextUsageEvent`，它在流的中后段才到
（`stream.rs:638-644`：`context_usage_percentage × get_context_window_size(model) / 100`）。

这个时序冲突是 `/cc/v1` 缓冲流存在的**唯一原因**：`handle_stream_request_buffered`
（`handlers.rs:1036`）全程只发 ping，等流结束拿到真值回填 `message_start`
（`stream.rs:1206-1221`），代价是完全失去增量。`/v1/messages` 则用估算值先发，
真值到了只更新 `message_delta`。

**OpenAI 端点不需要缓冲流**——usage 在末尾，`contextUsageEvent` 天然赶得上。
三个协议里唯一没有这个矛盾的。

定稿：实现 `stream_options.include_usage`（约 30 行）。不实现等于白扔一个已算好的准确值，
而 Anthropic 侧为拿同一个值付出了整条缓冲流的代价。

非流式（Chat 与 Responses 同）：`prompt_tokens` 优先取 `context_input_tokens`，
`None` 时回落 `count_all_tokens`——与 `stream.rs:1206-1209` 同一套逻辑。

---

## 11. 与 Kiro-Go 的已知行为差异

实现 OpenAI 端点后，kiro.rs 与 Kiro-Go 在以下三点行为不同。均为有意选择，须写进
`client_hints`：

| # | 项 | Kiro-Go | kiro.rs | 依据 |
| --- | --- | --- | --- | --- |
| 1 | `/v1/models` 鉴权 | 免鉴权 | 受 `requireApiKey` 约束 | D6 |
| 2 | 响应 model 回显 | resolve 后的 id | 原始请求 model | D9 |
| 3 | `previous_response_id` | 404（有持久化，找不到） | 400（不支持有状态） | D2 |

kiro.rs 内部也有一处不一致（D11）：`web_search_20250305` 在 `/v1/messages` 转发上游，
在 `/v1/responses` 被代执行。

---

## 12. 测试与验证

### 12.1 单元测试

| 范围 | 断言 | 状态 |
| --- | --- | --- |
| catalog | id 唯一、path+method 组合唯一、Live 项字段完备、aliases 全空 | 已实现 |
| **防漂移** | `live ⊆ routes` 且 `planned ∉ routes` | 已实现 |
| Admin DTO | 正则断言不含完整 apiKey；示例含 `API_KEY` 占位符 | 已实现 |
| OpenAI converter | system 提取合并、tool_calls → tool_use、tool 角色 → tool_result 配对、image parts、两种 tool shape 反序列化 | Phase B |
| **thinking 锁定** | 带 `-thinking` 的 OpenAI 请求 → system 含 `<thinking_mode>`（D8） | Phase B |
| **tool_name_map 锁定** | 超长工具名 → 回显原名（D8） | Phase B |
| OpenAI stream | 给定 `Event` 序列断言 chunk 序列、`include_usage` 末块与 `[DONE]` | Phase B |
| Responses | 三种 input 形状归一、`instructions` 位置、SSE 事件顺序 | Phase C |
| websearch | 宽判定命中形状、Responses 事件映射、非流式三 block | Phase C |
| error shape | Anthropic 端点 Anthropic shape、OpenAI 端点 OpenAI shape | Phase B |

### 12.2 集成 / 手工

- auth 矩阵：`require_api_key` 开 / 关 × 有 key / 无 key × 各端点
- 本地 curl：Chat 非流式 + `stream:true`；Responses 非流式 + 流式
- Claude 回归：`/v1/messages`、`/cc/v1/messages`、`/v1/models` 行为不变
- Admin 面板复制配置后能直接跑通

### 12.3 错误与 UX 矩阵

| 场景 | 行为 |
| --- | --- |
| 请求 planned 端点 | 404（未挂载），Admin 面板已提前标注 |
| OpenAI 非法 JSON | 400 `invalid_request_error` |
| 模型无法 resolve | 400，OpenAI 方言包装（沿用现有错误文案，不为 OpenAI 放宽映射） |
| 无可用凭据 | 503 `server_error` |
| `previous_response_id` 非空 | 400，message 说明未启用有状态续接（D2） |
| Admin 未登录 | 现有 401 |

### 12.4 安全检查

- 无完整密钥出现在任何响应或日志
- `git status --short` 无 credentials / config 真实文件
- OpenAI 路由确认已挂 `auth_middleware`（§8）

---

## 13. 分阶段实施

### Phase A（P0）Catalog + Admin 展示 — **已完成**

见 `openspec/changes/public-api-catalog-admin-display/`。验证：`openspec validate --all`
12 passed、`cargo test` 306 passed、`pnpm build` 通过、Playwright 面板渲染验证通过。

### Phase B OpenAI Chat Completions — **已完成**

见 `openspec/changes/openai-chat-completions-compat/`。原计划步骤：

1. `openai/types.rs`（含 `OpenAiTool` 双形状 `Deserialize`、`stream_options`）→ verify：serde 往返单测
2. `openai/converter.rs to_messages_request` → verify：转换单测 + D8 两条锁定测试
3. `openai/stream.rs OpenAiStreamContext` → verify：Event 序列 → chunk 断言（含 usage 末块）
4. `openai/error.rs` + `handlers.rs`（流式 + 非流式）→ verify：本地 curl 两种模式
5. `main.rs` merge 挂载 + **三个 layer 补齐**（§8）→ verify：`cargo build` + auth 矩阵测试
6. catalog 中 `openai.chat.completions` 改 Live → verify：防漂移单测通过

### Phase C OpenAI Responses — **已完成**

见 `openspec/changes/openai-responses-api-compat/`。原计划步骤：

1. `openai/responses.rs` input / instructions 归一 → verify：三种形状单测
2. Responses 类型 + 非流式 `ResponsesObject` → verify：curl
3. 流式语义事件状态机 → verify：SSE 顺序断言 + curl
4. `openai/websearch.rs`（宽判定 + 流式/非流式 + 运行时开关）→ verify：判定单测 + curl
5. catalog 改 Live → verify：防漂移单测

### Phase D（可选，各自单列 change）

- `previous_response_id` 持久化（落盘或内存 LRU，含 TTL / 防环 / 多实例评估）
- `GET /v1/responses/{id}`
- Anthropic 侧 `has_web_search_tool` 判定放宽（D11，行为变更）
- 路径别名 + `aliasMode = strict|compat`
- thinking 输出格式开关（标签 vs `reasoning_content`）
- `GET /openai/v1/models`（严格 OpenAI 字段子集，仅在成为硬需求时）

---

## 14. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| **OpenAI 路由漏挂 auth（安全）** | §8 三 layer 清单 + auth 矩阵集成测试作为 Phase B 门禁 |
| **漏捞 handler 前置状态（静默）** | D8 四项表 + 两条锁定测试 |
| 流式状态机复杂度（最大工作量） | 以 Kiro-Go 实现为逐事件参照；先文本增量再补工具调用增量 |
| catalog 与实际路由漂移 | `live ⊆ routes` + `planned ∉ routes` 双向单测（已实现并验证有效） |
| 适配层语义损失（相对 Go 直达） | 契约测试对齐 Go 关键用例；问题再评估直达 |
| thinking 表达差异 | 默认 `reasoning_content`，不污染 `content`；标签格式后置 |
| 模型解析边界 | 沿用 `resolve_model` 与现有错误文案，不为 OpenAI 端点放宽 |
| 用户把上游 endpoint 当 Base URL | UI 文案强制区分（§9 第 4 段）+ 字段命名纪律（§1.3） |
| Models 鉴权比 Go 严导致客户端探测 401 | D6：面板显式标注需鉴权 |
| web_search 判定放宽后误拦截 | D11：加运行时开关；Anthropic 侧不改 |
| Responses 持久化涉及敏感落盘 | D2：首版不做，单列 change 评估 |

---

## 15. OpenSpec 与同步清单

| Change | 范围 | 状态 |
| --- | --- | --- |
| `public-api-catalog-admin-display` | Phase A | 已实现 |
| `openai-chat-completions-compat` | Phase B | 已实现 |
| `openai-responses-api-compat` | Phase C（含 websearch） | 已实现 |
| `openai-responses-store`（可选） | Phase D store/retrieve | 待建 |
| `anthropic-websearch-non-stream` | D11 既有问题修复 | 已实现 |

协议 / SSE 变更属强制 OpenSpec 场景（见 `AGENTS.md`），每个 change 需含
proposal + design + tasks + specs，并跑 `openspec validate --all`。

门禁流程：`openspec-new-change` / `openspec-propose` → `openspec-superpowers-bridge`
→ 实现 → `spec-compliance-check` → `openspec-verify-change` → `verification-before-completion`。

实现时同步：`README.md`（对外端点与启动日志，Phase A 已同步）、`spec/`（形成长期能力后
补协议支持事实）、`docs/tooling-sources.md`（如引入新依赖需登记）。

---

## 16. 源码索引

**kiro.rs**：`src/main.rs`、`src/public_api/`、
`src/anthropic/{router,handlers,converter,middleware,stream,websearch}.rs`、
`src/admin/{router,handlers,service,types}.rs`、`src/kiro/{provider,parser/decoder}.rs`、
`admin-ui/src/components/{dashboard,settings-panel,public-api-panel}.tsx`、
`admin-ui/src/{api/public-api.ts,types/api.ts}`

**Kiro-Go**（契约参照，非照搬架构）：`proxy/handler.go`（ServeHTTP / handleOpenAIChat /
handleModels）、`proxy/translator.go`（OpenAIToKiro / ParseModelAndThinking /
OpenAITool.UnmarshalJSON）、`proxy/responses_{handler,types,input,store,history}.go`

**sub2api**（web_search 与多协议参照）：
`backend/internal/service/gateway_websearch_emulation.go`、`gateway_forward.go`、
`openai_alpha_search.go`、`backend/internal/pkg/websearch/`

**相关设计文档**：`docs/admin-models-settings-optimization-design.md`、
`docs/model-alias-and-catalog-routing-optimization-design.md`
