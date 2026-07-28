# Design: OpenAI Chat Completions 兼容端点

> 范围：Phase B。新增 `POST /v1/chat/completions`，不改任何 Anthropic 行为。
> 设计输入：`docs/multi-protocol-api-design.md`（§6、§8、D1/D8/D9/D12）
> 对照：Kiro-Go `proxy/handler.go:1568`、`translator.go:1096/1155`

---

## 1. 模块结构

```
src/openai/
├── mod.rs          # create_openai_routes(app_state) + 模块声明
├── types.rs        # 请求/响应 serde 类型（含 OpenAiTool 手写 Deserialize）
├── converter.rs    # ChatCompletionRequest -> anthropic::types::MessagesRequest
├── stream.rs       # Event -> chat.completion.chunk 状态机
├── error.rs        # OpenAI error shape
└── handlers.rs     # post_chat_completions（流式 + 非流式）
```

`src/main.rs` 顶部加 `mod openai;`（与 `mod public_api;` 并列）。

## 2. 需从 anthropic 模块导出的项

`src/anthropic/mod.rs` 目前只导出 `resolve_model` 与 `create_router_with_provider_and_auth`。
Phase B 需追加 `pub(crate)` 导出（**只加导出，不改实现**）：

| 项 | 位置 | 用途 |
| --- | --- | --- |
| `convert_request_with_policy` | `converter.rs:471` | 请求转换（D1） |
| `ConversionError` / `ConversionResult` | `converter.rs:368/376` | 错误映射与 `tool_name_map` |
| `get_context_window_size` | `converter.rs:351` | usage 反算（D12） |
| `override_thinking_from_model_name` | `handlers.rs:834` | thinking 后缀（D8） |
| `extract_thinking_from_complete_text` | `stream.rs:182`（已 `pub(crate)`） | 非流式 reasoning 提取 |
| `AppState` / `auth_middleware` / `cors_layer` | `middleware.rs` | 路由挂载 |
| `types::{MessagesRequest, Message, SystemMessage, Tool, ContentBlock, ImageSource, Thinking}` | `types.rs` | 构造中间结构 |

`token::count_all_tokens` 与 `token::estimate_output_tokens` 已是 `pub(crate)`，无需改动。

## 3. 请求类型（`types.rs`）

```rust
#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)] pub stream: bool,
    pub stream_options: Option<StreamOptions>,
    pub max_tokens: Option<i32>,
    pub max_completion_tokens: Option<i32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub tools: Option<Vec<OpenAiTool>>,
    pub tool_choice: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct StreamOptions {
    #[serde(default)] pub include_usage: bool,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,                                  // system|developer|user|assistant|tool
    #[serde(default)] pub content: serde_json::Value,  // string | parts[] | null
    #[serde(default)] pub tool_calls: Option<Vec<OpenAiToolCall>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(default)] pub function: OpenAiFunctionCall,
}

#[derive(Debug, Default, Deserialize)]
pub struct OpenAiFunctionCall {
    #[serde(default)] pub name: String,
    #[serde(default)] pub arguments: String,   // JSON 字符串
}
```

`temperature` / `top_p` 为 best-effort：Kiro 上游不接受这两个参数，`MessagesRequest`
也没有对应字段，因此**接受但忽略**（不报错，避免 SDK 默认值导致 400）。
这一点必须在 spec 里写明，否则会被误当成 bug。

### 3.1 `OpenAiTool` 手写 `Deserialize`（关键）

两种形状都要接（对齐 `translator.go:1096`）：

```jsonc
{"type":"function","function":{"name":"f","description":"d","parameters":{}}}  // Chat
{"type":"function","name":"f","description":"d","parameters":{}}              // Responses
```

实现方式：先反序列化到一个中间结构，`function` 字段存在则取嵌套值，否则取顶层。

```rust
pub struct OpenAiTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    /// 原始 type（用于 Phase C 的 web_search 判定；Chat 端点仅记录不使用）
    pub tool_type: String,
}
```

漏掉任一形状会导致 name 为空、上游返回 400。这也是 Phase C 复用本转换器的前提。

## 4. 请求映射（`converter.rs`）

```rust
pub fn to_messages_request(req: &ChatCompletionRequest)
    -> Result<MessagesRequest, OpenAiError>
```

步骤：

1. **model 原样传入**（D8：thinking 后缀不在此处剥离）
2. `system` / `developer` role → `MessagesRequest.system`（多条按顺序，各成一个 `SystemMessage`）
3. `user` / `assistant` → `messages`：
   - content 为 string → 直接作为文本
   - content 为 parts 数组 → 逐 part 转 Anthropic block：
     - `{"type":"text","text":...}` → `{"type":"text","text":...}`
     - `{"type":"image_url","image_url":{"url":"data:image/png;base64,XXX"}}` →
       `{"type":"image","source":{"type":"base64","media_type":"image/png","data":"XXX"}}`
     - 非 data URL 的 image_url（http/https）→ **跳过并 warn**（Kiro 上游不拉取远程图片）
4. `assistant.tool_calls` → `tool_use` block（`arguments` 字符串解析为 JSON object，
   解析失败退化为 `{}` 并 warn，与 `translator.go:1207` 同一策略）
5. `tool` role → `tool_result` block，按 `tool_call_id` 配对，挂在紧随其后的 user 消息上
6. `tools[]` → Anthropic `Tool { name, description, input_schema }`
7. `max_tokens` / `max_completion_tokens` 取先出现的非空值；都缺省填 **64000**
8. 校验：`messages` 为空 → 400；无任何 user 消息 → 400（对齐
   `validateOpenAIRequestShape`，`handler.go:176`）

**消息合并规则**：Anthropic 要求 user/assistant 交替，`convert_request_with_policy`
内部已有 `merge_assistant_messages` 与孤儿清理。本映射只做结构翻译，
不重复实现合并逻辑——这正是 D1 选适配层的收益。

### 4.1 tool_result 的归属

OpenAI 的 `tool` role 是独立消息；Anthropic 的 `tool_result` 是 user 消息里的 block。
连续多条 `tool` 消息要归集到**同一个** user 消息（对齐 `translator.go:1234-1262`）。
若 `tool` 消息是最后一条，其 `tool_result` 构成当前轮 user 消息。

## 5. 非流式响应

聚合 `Event`，复用 `handle_non_stream_request`（`handlers.rs:646`）的解码模式：
`EventStreamDecoder::feed` → `decode_iter` → `Event::from_frame` → match。

```jsonc
{
  "id": "chatcmpl-<uuid>",
  "object": "chat.completion",
  "created": 1719446400,
  "model": "<原始请求 model，见 D9>",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "...",                 // 无文本时为 null
      "reasoning_content": "...",        // 仅 thinking 启用且提取到时出现
      "tool_calls": [{                   // 仅有工具调用时出现
        "id": "...", "type": "function",
        "function": { "name": "...", "arguments": "{...}" }
      }]
    },
    "finish_reason": "stop"
  }],
  "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 }
}
```

- `finish_reason`：有 tool_use → `tool_calls`；`ContentLengthExceededException` 或
  上下文 100% → `length`；否则 `stop`
- `prompt_tokens`：优先 `context_input_tokens`（`contextUsageEvent` 反算），
  `None` 时回落 `count_all_tokens`（D12）
- `completion_tokens`：`estimate_output_tokens`
- **工具名还原**：`tool_name_map.get(&event.name)` 取原名（D8 第二项）
- **thinking**：`thinking_enabled` 时用 `extract_thinking_from_complete_text` 分离，
  thinking 部分放 `reasoning_content`，**不污染 `content`**

## 6. 流式响应（`stream.rs`）

`Content-Type: text/event-stream`。`OpenAiStreamContext` 职责对应 Anthropic
`StreamContext` 但产出 OpenAI chunk。**不复用** `StreamContext`（它产出 Anthropic 事件）。

chunk 序列：

```
data: {"id":"chatcmpl-x","object":"chat.completion.chunk","created":N,"model":"M",
       "choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {...,"choices":[{"index":0,"delta":{"content":"增量"},"finish_reason":null}]}
data: {...,"choices":[{"index":0,"delta":{"reasoning_content":"思考增量"},...}]}

data: {...,"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"...",
       "type":"function","function":{"name":"f","arguments":""}}]},...}]}
data: {...,"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,
       "function":{"arguments":"片段"}}]},...}]}

data: {...,"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}
data: {...,"choices":[],"usage":{...}}          # 仅 include_usage=true（D12）
data: [DONE]
```

要点：

- **首块只带 role**，不带 content
- **tool_calls 增量**：首次出现某 tool_use_id 时发带 `id` + `name` 的 chunk 并分配
  `index`（从 0 递增），后续只发 `arguments` 片段。Kiro 的 `ToolUseEvent.input`
  本身就是增量字符串（`tool_use.rs:139`），直接透传即可
- **thinking 标签解析**：Kiro 把思考内容以 `<thinking>...</thinking>` 混在
  `AssistantResponseEvent.content` 里。需复用 Anthropic 侧同样的跨 chunk 标签检测
  策略（`stream.rs:707-762`），把 thinking 内容路由到 `reasoning_content`，
  **绝不混进 `content`**
- **usage chunk**：`include_usage` 为 true 时，在 `finish_reason` chunk 之后、
  `[DONE]` 之前发一个 `choices: []` 的 chunk。为 false 时**不发**（协议要求）
- **ping 保活**：OpenAI 协议没有 ping 事件。用 SSE 注释行 `: keepalive\n\n`
  （合法且被 SDK 忽略），沿用 25s interval

## 7. 错误方言（`error.rs`）

```jsonc
{ "error": { "message": "...", "type": "invalid_request_error", "code": null } }
```

映射：

| 场景 | HTTP | type |
| --- | --- | --- |
| JSON 反序列化失败 | 400 | `invalid_request_error` |
| messages 为空 / 无 user 消息 | 400 | `invalid_request_error` |
| 模型无法 resolve（`ConversionError::UnsupportedModel`） | 400 | `invalid_request_error` |
| provider 未配置 | 503 | `server_error` |
| 上游调用失败 | 502 | `api_error` |
| 序列化内部错误 | 500 | `server_error` |

沿用现有错误文案（不为 OpenAI 端点放宽 resolve 策略）。`map_provider_error`
（`handlers.rs:39`）产出 Anthropic shape，OpenAI 侧需**平行实现**一份而非复用。

## 8. handler 前置逻辑（D8：各自捞，不抽共享层）

`post_chat_completions` 必须逐项做全，四项漏一个都是静默错误：

```rust
// 1. provider 检查（同 handlers.rs:386-399）
// 2. 转成 MessagesRequest
let mut msg_req = to_messages_request(&req)?;
// 3. thinking 后缀 —— 必须显式调用（D8 第一项）
override_thinking_from_model_name(&mut msg_req);
// 4. 转换（拿到 tool_name_map —— D8 第二项）
let (policy, catalog_set) = resolution_context_from_state(&state);
let conversion = convert_request_with_policy(&msg_req, &policy, catalog_set.as_ref())?;
// 5. input_tokens —— 必须在 convert 之后（按值消费 —— D8 第三项）
let input_tokens = count_all_tokens(msg_req.model.clone(), msg_req.system.clone(),
                                    msg_req.messages.clone(), msg_req.tools.clone()) as i32;
// 6. thinking_enabled —— D8 第四项
let thinking_enabled = msg_req.thinking.as_ref().map(|t| t.is_enabled()).unwrap_or(false);
```

**注意**：Anthropic 侧的 websearch 分支（`handlers.rs:405`）**不抄**（D10）。
`web_search` 的 server-side 能力仅在 Phase C 的 Responses 端点提供；
Chat 端点上名为 `web_search` 的普通 function tool 走正常 tools 路径。
这个「故意不抄」由 spec scenario + 单测锁定，防止后来被当成 bug 补上。

**model 回显**：`OpenAiStreamContext.model` 与非流式响应的 `model` 字段都存
**原始请求 model**（`req.model`），不是 `resolve_model` 后的 id（D9）。

## 9. 路由挂载

```rust
// src/openai/mod.rs
pub fn create_openai_routes(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/chat/completions", post(post_chat_completions))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .nest("/v1", v1)
        .layer(cors_layer())
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))   // 50MB
        .with_state(state)
}
```

```rust
// src/main.rs
let openai_app = openai::create_openai_routes(app_state.clone());
let app = anthropic_app.merge(openai_app);
// 后续 admin / admin_ui nest 保持不变
```

**`merge` 只合并路由表，不传播 layer**。三个 layer 必须在 `openai_app` 上各自挂：

| 遗漏 | 后果 |
| --- | --- |
| `auth_middleware` | **端点裸奔，任何人可调用（安全事故）** |
| `cors_layer()` | 浏览器端客户端全部被 CORS 拦截 |
| `DefaultBodyLimit::max(50MB)` | 退回 axum 默认 2MB，带图片请求 413 |

第三项症状最像上游问题，最容易漏。`MAX_BODY_SIZE` 需从 `anthropic/router.rs:20`
提为 `pub(crate)` 或在 openai 侧同值定义（倾向前者，避免两处漂移）。

## 10. 测试

| 范围 | 断言 |
| --- | --- |
| tool 双形状 | Chat 嵌套形状与 Responses 顶层形状均解析出正确 name/parameters |
| system 提取 | 多条 system/developer 按顺序合并进 `MessagesRequest.system` |
| tool_calls 映射 | `assistant.tool_calls` → `tool_use` block，arguments 字符串解析为 object |
| tool_result 配对 | 连续多条 `tool` 消息归集到同一 user 消息，按 `tool_call_id` 配对 |
| image parts | data URL → `ImageSource{base64}`；http URL 跳过 |
| max_tokens 优先级 | `max_tokens` 与 `max_completion_tokens` 取先出现的非空值；都缺省填 64000 |
| **thinking 锁定（D8）** | `model: "claude-sonnet-4.5-thinking"` → 最终 `ConversationState` 的 system 含 `<thinking_mode>` |
| **tool_name_map 锁定（D8）** | 超长工具名 → 响应 `tool_calls[].function.name` 是原名而非哈希短名 |
| **model 回显（D9）** | 请求 `gpt-4o` → 响应 `model` 字段为 `gpt-4o` |
| **web_search 不劫持（D10）** | 单个名为 `web_search` 的 function tool → 走正常 tools 路径，生成 `tool_use` |
| 流式序列 | 给定 Event 序列断言 chunk 顺序、首块仅 role、tool_calls 增量 index、`[DONE]` |
| **include_usage（D12）** | true → `[DONE]` 前有 `choices:[]` + usage 的 chunk；false → 无该 chunk |
| finish_reason | tool_use → `tool_calls`；ContentLengthExceeded → `length`；否则 `stop` |
| error shape | OpenAI 端点返回 `{error:{message,type}}`，非 Anthropic shape |
| **auth 矩阵** | `requireApiKey` 开/关 × 有/无 key，确认 layer 已挂 |
| **body limit** | 大于 2MB 小于 50MB 的请求不返回 413 |
| catalog 防漂移 | status 改 Live 后 `live ⊆ routes` 通过 |

## 11. 风险

| 风险 | 缓解 |
| --- | --- |
| **漏挂 auth layer（安全）** | §9 三 layer 清单 + auth 矩阵测试作为合入门禁 |
| **漏捞前置状态（静默）** | §8 六步清单 + 两条锁定测试（thinking / tool_name_map） |
| 流式 thinking 标签跨 chunk 分割 | 复用 Anthropic 侧已验证的检测策略，不自己重写边界判断 |
| tool_calls 增量 index 错乱 | 单测给定多工具并发的 Event 序列断言 index 稳定 |
| `temperature` 被忽略引发误解 | spec 明确写「接受但不透传」，不静默报错也不假装生效 |
| Anthropic 侧回归 | 只加 `pub(crate)` 导出，不改任何实现；全量 `cargo test` 门禁 |
