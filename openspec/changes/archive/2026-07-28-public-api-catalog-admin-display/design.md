# Design: Public API Catalog 与 Admin 端点展示

> 范围：Phase A。不触碰上游链路（converter / provider / stream），回归面限于新增模块 + Admin 只读接口 + 启动日志 + admin-ui。
> 设计输入：`docs/multi-protocol-api-design.md`（跨 Phase 设计定稿，含 D1–D12 决策记录）
> 对照项目：Kiro-Go（`proxy/handler.go` ServeHTTP switch、`handleModels`）、sub2api（`backend/internal/service/gateway_websearch_emulation.go`、`openai_alpha_search.go`）

---

## 1. 概念切割（贯穿全文的命名纪律）

```
A. Public Client API   客户端 -> 本代理（/v1/messages、/v1/chat/completions …）
B. Upstream Endpoint   本代理 -> 上游 Kiro（ide；Go 侧另有 kiro/cw/amazonq）
```

- 既有 `/api/admin/settings/endpoint` = **B**，本 change 不复用、不修改
- 本 change 新增的一切标识符用 `publicApi` / `public-api` / `publicEndpoints`
- **禁止裸用 `endpoint`** 命名新增字段或路由。反例：`GET /api/admin/endpoints` 会直接制造与 B 的歧义

## 2. 数据模型

`src/public_api/catalog.rs`：

```rust
pub enum EndpointStatus { Live, Beta, Planned }

pub enum AuthKind { ClientApiKey }

pub struct PublicEndpoint {
    pub id: &'static str,                  // "openai.chat.completions"
    pub family: &'static str,              // "claude" | "openai-chat" | "openai-responses" | "models"
    pub method: &'static str,              // "GET" | "POST"
    pub path: &'static str,                // "/v1/chat/completions"
    pub aliases: &'static [&'static str],  // 首版全为空数组（见 D5）
    pub auth: AuthKind,
    pub status: EndpointStatus,
    pub stream: bool,
    pub summary: &'static str,
    pub client_hints: &'static [&'static str],
}
```

全静态常量表，无运行时构造。`catalog()` 返回 `&'static [PublicEndpoint]`。

## 3. Canonical 清单（本 change 初值）

| id | family | method | path | stream | status |
| --- | --- | --- | --- | --- | --- |
| `models.list` | models | GET | `/v1/models` | - | Live |
| `claude.messages` | claude | POST | `/v1/messages` | 是 | Live |
| `claude.count_tokens` | claude | POST | `/v1/messages/count_tokens` | 否 | Live |
| `claude.cc.messages` | claude | POST | `/cc/v1/messages` | 是（缓冲流） | Live |
| `claude.cc.count_tokens` | claude | POST | `/cc/v1/messages/count_tokens` | 否 | Live |
| `openai.chat.completions` | openai-chat | POST | `/v1/chat/completions` | 是 | **Planned** |
| `openai.responses` | openai-responses | POST | `/v1/responses` | 是 | **Planned** |
| `openai.responses.retrieve` | openai-responses | GET | `/v1/responses/{id}` | 否 | **Planned** |

`live` 项与 `src/anthropic/router.rs:49-74` 的实际挂载逐条对应。`planned` 项未挂载，请求返回 404。

> 时点注记：上表为本 change（Phase A）落地时的状态快照。后续 `openai-chat-completions-compat`（Phase B）与 `openai-responses-api-compat`（Phase C）已分别把 `openai.chat.completions` 与 `openai.responses` 翻转为 `Live`，两者的状态契约由各自能力的「与端点注册表状态一致」需求持有。本能力只定义注册表机制与既有 Anthropic 端点的 live 集合，不持有其他协议端点的具体状态。`openai.responses.retrieve` 至今仍为 `Planned`。

## 4. 防漂移契约（本 change 的核心机制）

单测断言两个方向：

- **`live ⊆ routes`**：catalog 中每个 `status == Live` 的 `(method, path)` 在真实 Router 上必须命中（非 404）
- **`planned ∉ routes`**：每个 `status == Planned` 的 `(method, path)` 必须命中不到（404）

实现方式：用 `axum::body::Body::empty()` 构造请求打真实 Router（`create_router_with_provider_and_auth` 产出的 app），只断言"是否 404"，不断言业务响应。鉴权中间件会对无 key 请求返回 401 —— **401 也算命中**，因为它证明路由存在。

这条测试是 planned → live 切换的门禁：Phase B 把 `openai.chat.completions` 改 Live 而忘记挂载路由，测试立即红。

## 5. Admin 接口

```http
GET /api/admin/public-api      # admin_auth_middleware 保护
```

响应：

```jsonc
{
  "server": {
    "listenHost": "0.0.0.0",
    "port": 8080,
    "requireApiKey": true,
    "apiKeyMask": "sk-ab***",          // 永不返回完整值
    "authHeaders": ["x-api-key", "Authorization: Bearer"],
    "suggestedBaseUrl": null            // publicBaseUrl 未配置时为 null
  },
  "families": [
    {
      "family": "openai-chat",
      "label": "OpenAI Chat Completions",
      "endpoints": [{
        "id": "openai.chat.completions",
        "method": "POST",
        "path": "/v1/chat/completions",
        "aliases": [],
        "status": "planned",
        "stream": true,
        "summary": "OpenAI Chat Completions 兼容入口",
        "clientHints": ["OPENAI_BASE_URL 需带 /v1 后缀"],
        "examples": { "curl": "..." }
      }]
    }
  ]
}
```

约束：

- apiKey 只给 mask，掩码规则沿用 `src/main.rs:212` 现有写法（前半 + `***`）
- 示例中的 key 一律占位符 `API_KEY`，**禁止把 admin key 填进 client 示例**
- `suggestedBaseUrl` 为 `null` 时前端回落 `window.location.origin`

## 6. 启动日志

`src/main.rs:214-218` 现在手写三行可用 API。改为遍历 catalog 中 `status == Live` 项打印 `method path`。Admin 段落（`:220-234`）本 change 保留手写，加注释指向 catalog —— Admin 路由不属于 Public API Catalog 的范围（catalog 只管 A 类）。

## 7. Admin UI

入口：Dashboard 顶栏「API 端点」按钮，与「运行时设置」并列（沿用 `admin-ui/src/components/dashboard.tsx` 现有 header 按钮模式）→ Dialog。

面板四段：

1. **服务概要**：Base URL（默认 `window.location.origin`，可本地覆盖且仅影响复制文本）、`requireApiKey`、apiKey mask
2. **协议分组卡**：METHOD + path + status badge + stream 徽章 + 复制按钮
3. **客户端配方**：

| 客户端 | Base URL | 主路径 | 鉴权 |
| --- | --- | --- | --- |
| Anthropic SDK | `http://host:port` | `/v1/messages` | `x-api-key` 或 Bearer |
| Claude Code | `http://host:port` | `/cc/v1/messages` | 同上 |
| OpenAI SDK (Chat) | `http://host:port/v1` | `/chat/completions` | Bearer（planned） |
| OpenAI SDK (Responses) | `http://host:port/v1` | `/responses` | Bearer（planned） |
| Models | `http://host:port/v1` | `/models` | 需鉴权（见 D6） |

4. **注意区**：显式区分「对外 Public API」与「Kiro 上游 endpoint」；planned 端点标注尚未启用；说明 `/cc/v1` 与 `/v1` 的流式差异（缓冲 vs 增量）；Models 标注需鉴权

复用现有 `card` / `badge` / `dialog` 组件，遵循 `settings-panel.tsx` 的暗色配色约定。

---

## 8. 关键决策记录

前序两份设计稿在以下几点结论冲突，逐条定稿。D1–D6 来自设计稿，D7–D12 来自本次设计评审（grilling）中对源码复核后的修正。

### D1 转换路线：OpenAI → `MessagesRequest`（内存）→ 复用 `convert_request_with_policy`

（约束 Phase B/C，本 change 不实现）

Kiro-Go 为每种协议写直达 Kiro 的转换器（`ClaudeToKiro` / `OpenAIToKiro`）。kiro.rs 不照搬：`convert_request_with_policy` 里已沉淀的 prefill 处理、工具名缩短、system 分块、thinking 前缀、tool_use/tool_result 配对校验、孤儿清理，都是 **Kiro 侧约束而非 Anthropic 协议特性**。重写等于把这批边界条件复制一份并各自演化。

响应侧无法复用 Anthropic `StreamContext`（它产出 Anthropic 事件），必须平行实现输出状态机。

### D2 Responses 首版无状态，`previous_response_id` 明确报 400

`store` 忽略；`previous_response_id` 存在时返回 400（OpenAI error shape）。**不静默丢历史** —— 静默降级会让客户端拿到无上下文的答复且无法察觉。

Kiro-Go 有完整持久化（`responses_store.go` 落盘 + `responses_history.go` 展开），返回 404。kiro.rs 首版不做，作为独立 change 评估：涉及敏感内容落盘、内存增长、多实例一致性三类风险。

### D3 Admin 接口命名 `GET /api/admin/public-api`

DTO 字段用 `publicApi` / `publicEndpoints`。理由见 §1。

### D4 阶段顺序：先 Catalog + Admin，再 OpenAI 协议

Phase A 不触碰上游链路，回归风险接近零，且立刻交付可见价值。代价是需要 `status` 字段区分 planned/live —— 这个字段本身就是防漂移机制的一部分。

### D5 路径别名推迟

Kiro-Go 支持 `/messages`、`/chat/completions`、`/anthropic/v1/messages` 等别名（`handler.go:341` ServeHTTP switch）。收益是兼容硬编码客户端，成本是路由表膨胀 + 与 `/admin` nest 的潜在通配冲突。

两份设计稿在此冲突：catalog 稿主张首版 strict、unified 稿主张默认 compat。**定稿取 strict**：unified 稿自己的风险表里也承认「别名与 Admin UI 通配冲突」，默认开 compat 与该风险自相矛盾。catalog 预留 `aliases` 字段（首版全为空数组），需要时再开 `aliasMode = strict|compat`。

### D6 `/v1/models` 鉴权口径：与其它 public API 一致

保持 kiro.rs 现状（受 `require_api_key` 约束）。**这是与 Kiro-Go 的第一处已知行为差异**（Go 侧 `handleModels` 不要求 key）：某些客户端会在未配置 key 时先探测模型列表并因 401 失败。Admin 面板必须在 Models 卡片上显式标注「需鉴权」。

### D7 planned 端点统一返回 404，不做 501 占位

两份稿冲突：catalog 稿定 404（未挂载的自然结果），unified 稿写「404/501 + body 说明未启用」。

**定稿取 404**。501 需要真实挂一个占位 handler，这与 §4 的 `planned ∉ routes` 断言直接矛盾 —— 一个能返回 501 的路径就是已挂载的路径。要么可命中要么不可命中，不能既标 planned 又占着路由表。

### D8 OpenAI 侧不抽共享前置层，每个 handler 各自捞状态

（约束 Phase B/C）

`post_messages`（`handlers.rs:401-500`）与 `post_messages_cc`（`handlers.rs:924-1023`）的前置段目前是**逐字拷贝的两份**。加 Chat 变三份，加 Responses 变四份。

评审结论：**不抽 `prepare_messages_request` 共享函数**，OpenAI handler 各自捞。理由是抽取会改动 Anthropic 现有链路，违反「anthropic 零行为侵入」。

代价必须靠测试兜住 —— 以下四项漏捞全部是**静默错误，编译器不报**：

| 项 | 来源 | 漏了的症状 |
| --- | --- | --- |
| `override_thinking_from_model_name(&mut payload)` | `handlers.rs:834` | `-thinking` 后缀失效，`reasoning_content` 永远为空 |
| `tool_name_map` | `ConversionResult` 第二字段（`converter.rs:372`） | 超长工具名回显哈希短名，工具调用链断 |
| `input_tokens` | `token::count_all_tokens(model, system, messages, tools)` | usage 全 0 |
| `thinking_enabled` | `payload.thinking.as_ref().map(is_enabled)` | 流式不解析 `<thinking>` 标签，思考内容混进 content |

**两份设计稿都误述了 thinking 的生效点**（写成"由下游 `resolve_model` 处理"）。实际：`resolve_model` 返回的 `thinking_requested`（`converter.rs:129,267`）**全项目无消费者**；真正生效的是 handler 层的 `override_thinking_from_model_name`，它写 `payload.thinking` 与 `payload.output_config.effort`（adaptive 时），再由 `generate_thinking_prefix`（`converter.rs:875`）读取生成 `<thinking_mode>` 标签。

Kiro-Go 用的是纯函数 + bool 参数（`ParseModelAndThinking` → `OpenAIToKiro(&req, thinking)`），三个协议入口各调一次（`handler.go:1592`、`handler.go:3426`、`responses_handler.go:107`），漏调编译不过。kiro.rs 是副作用式改写请求字段，漏调静默降级 —— 这是 Go 那套结构不会有的失效模式。

Phase B 必须包含的两条锁定测试：

1. 带 `-thinking` 后缀的 OpenAI 请求 → 断言最终 `ConversationState` 的 system 含 `<thinking_mode>`
2. 超长工具名 → 断言输出 `tool_calls[].function.name` 是原名而非哈希短名

`count_all_tokens` 按值消费 `payload.system/messages/tools`（`handlers.rs:462-467`），调用顺序被锁死在 `convert_request_with_policy` 之后。

### D9 model 回显原始公开名，不覆写为 resolve 后的 id

（约束 Phase B/C）

Kiro-Go 的写法是 `req.Model = actualModel`（`handler.go:1593`）后传给流，回显 resolve 后的 id。kiro.rs 的 `StreamContext.model` 存的是**原始公开模型名**用于回显，resolve 后的 id 只存在于 `conversation_state`。

**定稿：OpenAI 侧保持 kiro.rs 现有行为，回显原值。** 这是与 Kiro-Go 的第二处已知行为差异。

副作用：客户端传 `gpt-4o` 时回显 `gpt-4o`，实际跑 Claude。OpenAI SDK 不校验该字段，但按回显 model 归类计费的中间层（LiteLLM、OpenWebUI 用量统计）会归到 `gpt-4o`。须写进 catalog 的 `client_hints`。

### D10 web_search：只挂 Responses，不挂 Chat Completions

（约束 Phase C）

kiro.rs 现有 websearch 是 Anthropic 专用的完整实现：`has_web_search_tool`（`websearch.rs:108`，条件 `tools.len() == 1 && name == "web_search"`）拦截后走 MCP（`call_mcp`），`generate_websearch_events`（`websearch.rs:246-441`）产出 11 段硬编码 Anthropic SSE，含 `server_tool_use`、`web_search_tool_result` 两种 block 与 `usage.server_tool_use.web_search_requests`。

对照结论：

- **Kiro-Go 完全没有 web_search**（`grep -rn "web_search" proxy/*.go` 无匹配），MCP 那套是 kiro.rs 独有
- **sub2api 有两套且互不相通**：`gateway_websearch_emulation.go`（代理自己调第三方搜索 API，输出 Anthropic 形状，拦截点在 `gateway_forward.go:90`）与 `openai_alpha_search.go`（把请求转成 `tools:[{"type":"web_search"}]` 的 Responses 请求交给**上游 hosted web_search**）。`grep -rln "shouldEmulateWebSearch|handleWebSearchEmulation"` 只有定义处与 `gateway_forward.go` 两个文件 —— **emulation 没有接进任何 OpenAI 端点**，尽管 sub2api 有 200+ 个 `openai_*.go`

kiro.rs 上游是 `generateAssistantResponse`，没有 hosted web_search，所以 sub2api 的 OpenAI 路线（走上游原生）不可行，只能走 emulation 路线。

**定稿：web_search 在 `/v1/responses` 支持，`/v1/chat/completions` 不支持。** Chat Completions 协议里没有任何字段能诚实表达"服务端已代你执行了搜索"；往 `content` 塞 markdown 或伪造 `tool_calls` 都是编造语义。Responses 有一一对应的映射：

```
server_tool_use        → response.output_item.added { type: "web_search_call" }
web_search_tool_result → response.output_item.done  { status: "completed" }
text                   → response.content_part.added + response.output_text.delta
```

### D11 OpenAI 侧 web_search 判定用 sub2api 宽口径；Anthropic 侧不改

（约束 Phase C）

OpenAI 侧判定照 sub2api `isWebSearchToolJSON`（`gateway_websearch_emulation.go:96-105`）：`type` 前缀匹配 `web_search` 或等于 `google_search`，**或** `name` 命中 `web_search` / `google_search` / `web_search_20250305`。

kiro.rs 现有 Anthropic 侧的 `name == "web_search"` 精确匹配接不住 `web_search_20250305`（Anthropic 带日期的官方 tool 名）。

**Anthropic 侧 `has_web_search_tool` 本 change 与 Phase B/C 均不改。** 放宽它会让现在走正常 tools 路径转发上游的 `web_search_20250305` 请求变成被代执行 —— 行为反转，属于必须独立走 OpenSpec 的既有协议变更，且会让 Phase C 回归面失控。

已知不一致（须写进 `client_hints` 与 Phase C 的 non-goal）：同一个 `web_search_20250305` 请求，打 `/v1/messages` 转发上游，打 `/v1/responses` 被代执行。

相关既有问题（**不并入本 change 也不并入 Phase B/C**）：`handle_websearch_request`（`websearch.rs:513-519`）无条件返回 `text/event-stream`，不看 `payload.stream`。Anthropic 客户端发 `stream: false` 时拿不到可解析的响应。sub2api 两条路径都齐（`writeWebSearchStreamResponse` / `writeWebSearchNonStreamResponse`，`:211` / `:322`）。建议单列 change 修复。

Phase C 的 websearch 非流式必须新写，照 sub2api 的三 block 并列结构（`gateway_websearch_emulation.go:329-347`）。`extract_search_query` 不能复用 —— Anthropic 版剥的是 `"Perform a web search for the query: "` 前缀（`websearch.rs:137`），那是 Claude Code 客户端约定。

同时 Phase C 应加运行时开关：sub2api 的 emulation 是三层可配的（全局 setting → account mode → channel config，`:53-79`），kiro.rs 现在是硬编码拦截。判定放宽后必须有关闭手段。

### D12 实现 `stream_options.include_usage`

（约束 Phase B/C）

Anthropic 与 OpenAI 在 usage 位置上恰好相反：

- Anthropic：`usage.input_tokens` 必须在**首个**事件（`message_start`）
- OpenAI 流式：`usage` 默认不发；传 `stream_options: {"include_usage": true}` 时在 `[DONE]` 前的最后一个 chunk 发（该 chunk `choices` 为空数组）

而 `input_tokens` 的准确值来自 `contextUsageEvent`，它在流的中后段才到（`stream.rs:638-644`：`context_usage_percentage × get_context_window_size(model) / 100`）。

这个时序冲突是 `/cc/v1` 缓冲流存在的**唯一原因**：`handle_stream_request_buffered`（`handlers.rs:1036`）全程只发 ping，等流结束拿到真值回填 `message_start`（`stream.rs:1206-1221`），代价是完全失去增量。`/v1/messages` 则用估算值先发，真值到了只更新 `message_delta`。

**OpenAI 端点不需要缓冲流** —— usage 在末尾，`contextUsageEvent` 天然赶得上。三个协议里唯一没有这个矛盾的。

定稿：实现 `stream_options.include_usage`（约 30 行）。不实现等于白扔一个已算好的准确值，而 Anthropic 侧为拿同一个值付出了整条缓冲流的代价。

非流式（Chat 与 Responses 同）：`prompt_tokens` 优先取 `context_input_tokens`，`None` 时回落 `count_all_tokens` —— 与 `stream.rs:1206-1209` 同一套逻辑。

---

## 9. Phase B/C 的路由挂载约束（本 change 不实现，但 catalog 必须为其留位）

`main.rs` 后续改动形态：

```rust
let openai_app = openai::create_openai_routes(app_state.clone());
let app = anthropic_app.merge(openai_app);
```

**`merge` 只合并路由表，不传播已应用的 layer。** `src/anthropic/router.rs:69-74` 在 `anthropic_app` 上挂了三样，`openai_app` 必须各自补齐：

| 遗漏 | 后果 |
| --- | --- |
| `auth_middleware`（`router.rs:55-58` 模式） | **OpenAI 端点裸奔，任何人可调用（安全事故）** |
| `cors_layer()` | 浏览器端客户端全部被 CORS 拦截 |
| `DefaultBodyLimit::max(MAX_BODY_SIZE)`（50MB） | 退回 axum 默认 2MB，带图片请求 413 |

第三项症状最像上游问题，最容易漏。unified 稿只提了 auth 一项。

`/v1/models` 已由 Anthropic 侧注册，OpenAI 端**不重复注册** —— 其响应结构（`ModelsResponse { object: "list", data: [...] }`，`types.rs:43-59`）是 OpenAI list shape 的超集，SDK 可直接消费。

---

## 10. 测试

| 范围 | 断言 |
| --- | --- |
| catalog 完备性 | id 唯一；`(method, path)` 组合唯一；Live 项字段非空 |
| **`live ⊆ routes`** | 每个 Live 的 `(method, path)` 打真实 Router 非 404（401 算命中） |
| **`planned ∉ routes`** | 每个 Planned 的 `(method, path)` 打真实 Router 得 404 |
| Admin DTO | 正则断言不含完整 apiKey；示例中 key 为 `API_KEY` 占位符 |
| Admin 鉴权 | 未带 admin key 请求 `GET /api/admin/public-api` 得 401 |
| admin-ui | `pnpm build`（tsc + vite） |

## 11. 风险

| 风险 | 缓解 |
| --- | --- |
| catalog 与实际路由漂移 | `live ⊆ routes` + `planned ∉ routes` 双向单测，本 change 即建立 |
| 用户把上游 endpoint 当 Base URL | UI 文案强制区分（§7 第 4 段）；字段命名纪律（§1） |
| Models 鉴权比 Go 严导致客户端探测 401 | D6：面板显式标注需鉴权 |
| 密钥泄漏 | 只回 mask；示例用占位符；正则单测 |
| Phase B 漏挂 layer | §9 三 layer 清单 + Phase B 的 auth 矩阵测试作为门禁 |
| Phase B/C 漏捞前置状态 | D8 的四项表 + 两条锁定测试 |
