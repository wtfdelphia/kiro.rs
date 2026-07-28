# Design: OpenAI Responses 兼容端点（含 web_search）

> 范围：Phase C。新增 `POST /v1/responses` 与 Responses 端点的 server-side web_search。
> 不改任何 Anthropic 行为，包括不改 `has_web_search_tool`。
> 设计输入：`docs/multi-protocol-api-design.md`（§7、D2/D10/D11/D12）
> 对照：Kiro-Go `responses_handler.go`、`responses_input.go`、`responses_types.go`；
> sub2api `gateway_websearch_emulation.go`

---

## 1. 模块结构

```
src/openai/
├── responses.rs          # 请求归一（input 三形状 + instructions）
├── responses_types.rs    # ResponsesRequest / ResponsesObject / output items
├── responses_stream.rs   # 语义事件状态机
├── websearch.rs          # Responses 端点的 server-side web_search
└── handlers.rs           # 追加 post_responses（复用 Phase B 的 prepare）
```

## 2. 分层

```
ResponsesRequest
  -> parse_responses_input          -> Vec<ChatMessage>
  -> instructions 作为 system 追加
  -> 拼成 ChatCompletionRequest     -> 复用 Phase B to_messages_request
  -> 复用 Phase B prepare（thinking / tool_name_map / tokens 四项）
  -> KiroProvider（复用）
  -> ResponsesStreamContext 或 build_responses_object（新增）
```

Responses **不写第二套上游调用逻辑**。Phase B 的 `prepare` 已含 D8 的四项前置，
直接复用即可，无需再抄一遍——这是 Phase B 把它抽成 `prepare` 的收益。

## 3. 请求类型

```rust
pub struct ResponsesRequest {
    pub model: Option<String>,          // 缺省时用默认模型
    pub input: serde_json::Value,       // string | array | object
    pub instructions: Option<String>,
    #[serde(default)] pub stream: bool,
    pub tools: Option<Vec<OpenAiTool>>, // 复用 Phase B 的双形状 Deserialize
    pub tool_choice: Option<Value>,     // 接受不透传
    pub previous_response_id: Option<String>,
    pub store: Option<bool>,            // 忽略
    pub temperature: Option<f64>,       // 接受不透传
    pub max_output_tokens: Option<i32>,
    pub metadata: Option<HashMap<String, String>>,
}
```

`model` 缺省时用 `claude-sonnet-4.5`（对齐 Go 的 `defaultResponsesModel`）。
`OpenAiTool` 直接复用 Phase B —— 这正是 Phase B 要求双形状 `Deserialize` 的原因：
Responses 用顶层 `{type,name,parameters}` 形状。

## 4. input 归一（`responses.rs`）

对齐 `responses_input.go`。三种顶层形状：

| 形状 | 处理 |
| --- | --- |
| `"文本"` | 单条 user 消息 |
| `[item, ...]` | 逐 item 转换（下表） |
| `{...}` | 视作单元素数组 |

item 类型分派（`convertResponsesInputItems`）：

| item type | 结果 |
| --- | --- |
| `message` 或（无 type 但有 role） | 按 role 建消息，content 支持 string / parts |
| `function_call` | assistant 消息 + tool_call；**连续多个合并进同一条 assistant 消息**（保持并行工具调用在同一轮，Kiro 的 tool_use/tool_result 配对要求） |
| `function_call_output` / `tool_result` | tool 消息，`call_id` 或 `tool_call_id` 取先有者，`output` 或 `content` 取先有者 |
| `input_text` / `text` | 累积到 pending user parts |
| `input_image` / `image` / `image_url` | 累积到 pending user parts |
| `output_text` | assistant 消息（先 flush pending） |
| 其它但有 role | 按 role 建消息 |

**pending user parts 机制**：裸的 `input_text` / `input_image` item 不带 role，
要累积起来在遇到下一个带 role 的 item（或结尾）时 flush 成一条 user 消息。
漏掉这个机制会让纯 parts 形式的 input 丢失内容。

`instructions` 非空时追加为 **system** 消息，位置在归一后的消息**之前**
（首版无历史展开，等价于 Go 的「置于展开历史之后」）。

校验：归一后为空 → 400；无任何 user 上下文 → 400。

## 5. 无状态语义（D2）

```rust
if req.previous_response_id.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) {
    return Err(OpenAiError::InvalidRequest(
        "previous_response_id is not supported: this service does not \
         enable stateful continuation. Send the full conversation in `input`.".into()
    ));
}
```

返回 **400**（不是 Go 的 404）：Go 有持久化，找不到才 404；kiro.rs 是能力不支持。
**绝不静默丢历史** —— 静默降级会让客户端拿到无上下文的答复且无法察觉。

`store` 字段读取但忽略，不报错（客户端普遍默认传 `store: true`）。

## 6. 非流式响应

```jsonc
{
  "id": "resp_xxx", "object": "response", "created_at": 1719446400,
  "status": "completed", "model": "<原始请求 model，D9>",
  "output": [
    { "id": "msg_x", "type": "message", "role": "assistant", "status": "completed",
      "content": [{ "type": "output_text", "text": "..." }] },
    { "id": "fc_x", "type": "function_call", "status": "completed",
      "call_id": "...", "name": "...", "arguments": "{...}" }
  ],
  "usage": { "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 },
  "metadata": {...}          // 回显请求的 metadata
}
```

`status` 恒为 `completed`（失败走错误响应，不返回 `status: failed` 的 200）。
`usage.input_tokens` 优先 `context_input_tokens`，`None` 时回落估算（D12）。
thinking 内容不进 `output`（Responses 无 reasoning part 的稳定契约，首版丢弃并 log）。

## 7. 流式语义事件（`responses_stream.rs`）

逐事件对齐 `responses_handler.go:292-580`。SSE 格式为
`event: <name>\ndata: <json>\n\n`（**与 Chat Completions 不同**，后者只有 `data:`）。

文本路径：

```
event: response.created              {"type":..,"response":{...status:"in_progress"}}
event: response.in_progress          {"type":..,"response":{...}}
event: response.output_item.added    {output_index, item:{id,type:"message",role,status:"in_progress",content:[]}}
event: response.content_part.added   {item_id,output_index,content_index,part:{type:"output_text",text:""}}
event: response.output_text.delta    {item_id,output_index,content_index,delta:"增量"}
...
event: response.content_part.done    {item_id,output_index,content_index,part:{type:"output_text",text:"全文"}}
event: response.output_item.done     {output_index,item:{...status:"completed",content:[...]}}
event: response.completed            {"type":..,"response":{...usage...}}
data: [DONE]
```

工具路径（关键：出现工具调用时先关闭当前 message item）：

```
# 若 message item 已开启 -> 先 content_part.done + output_item.done，outputIndex++
event: response.output_item.added    {output_index, item:{id:"fc_x",type:"function_call",
                                      status:"in_progress",call_id,name,arguments:""}}
event: response.function_call_arguments.delta  {item_id:"fc_x",output_index,delta:"{...}"}
event: response.output_item.done     {output_index, item:{...status:"completed",arguments:"{...}"}}
# outputIndex++
```

失败路径：`event: response.failed` + `{response:{id,status:"failed",error:{type,message}}}`。

`output_index` 与 `content_index` 的管理是本状态机最易错处：message item 与每个
function_call item 各占一个 `output_index`，切换时必须先 done 再自增。

## 8. web_search（`websearch.rs`）

### 8.1 判定（D11，宽口径）

对齐 sub2api `isWebSearchToolJSON`（`gateway_websearch_emulation.go:96-105`）：

```rust
fn is_web_search_tool(tool: &OpenAiTool) -> bool {
    let t = tool.tool_type.to_lowercase();
    if t.starts_with("web_search") || t == "google_search" { return true; }
    matches!(tool.name.as_str(),
             "web_search" | "google_search" | "web_search_20250305")
}
```

拦截条件：**恰好一个** tool 且命中上述判定（与 Anthropic 侧
`tools.len() == 1` 同一约束，避免劫持混合工具场景）。

**Anthropic 侧 `has_web_search_tool` 不改**（D11）。已知不一致：
`web_search_20250305` 打 `/v1/messages` 转发上游、打 `/v1/responses` 被代执行。
须写进 catalog 的 `client_hints`。

### 8.2 运行时开关

判定放宽后必须能关闭（对齐 sub2api 的可配置设计）。新增：

- 配置 `webSearchEmulation: bool`（默认 `true`，缺省兼容现网）
- Admin `GET/PUT /api/admin/settings/websearch`（热更新 + 落盘）
- 关闭时：web_search tool 走**正常 tools 路径**（不报错，交给模型自己决定）

### 8.3 复用与新写

| 复用（`websearch.rs` 提 `pub(crate)`，不改实现） | 新写 |
| --- | --- |
| `create_mcp_request` | `extract_search_query`（Anthropic 版剥 `"Perform a web search for the query: "` 前缀，那是 Claude Code 约定） |
| `call_mcp_api` | Responses 事件映射 |
| `parse_search_results` | 非流式 output items |
| `generate_search_summary` | — |

`extract_search_query` 取归一后消息中**最后一条 user 消息**的文本
（对齐 sub2api `extractSearchQueryFromBody`，`:106-122`）。

### 8.4 输出映射

非流式（三个 output item，对齐 sub2api 的三 block 结构）：

```jsonc
"output": [
  { "id":"ws_x", "type":"web_search_call", "status":"completed",
    "action": { "type":"search", "query":"..." } },
  { "id":"msg_x", "type":"message", "role":"assistant", "status":"completed",
    "content":[{ "type":"output_text", "text":"<搜索结果摘要>" }] }
]
```

流式：

```
response.created / response.in_progress
event: response.output_item.added   {item:{type:"web_search_call",status:"in_progress",...}}
event: response.output_item.done    {item:{type:"web_search_call",status:"completed",...}}
event: response.output_item.added   {item:{type:"message",...}}
event: response.content_part.added  {part:{type:"output_text"}}
event: response.output_text.delta   {delta:"摘要分片"}
event: response.content_part.done / response.output_item.done
event: response.completed
data: [DONE]
```

**流式与非流式两条路径都要写**（sub2api 两条都齐，`:211` / `:322`）。
kiro.rs 的 Anthropic 侧只有流式（`websearch.rs:513` 无条件返回 SSE），
那是既有问题，不在本 change 修（D11）。

web_search 路径的 usage：`input_tokens` 用估算值（不经上游 generate，
没有 `contextUsageEvent`），`output_tokens` 按摘要长度估算。

## 9. 路由

```rust
// src/openai/mod.rs
let v1_routes = Router::new()
    .route("/chat/completions", post(handlers::post_chat_completions))
    .route("/responses", post(handlers::post_responses))       // 新增
    .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));
```

三个 layer 已由 Phase B 挂齐，新路由挂在同一 `v1_routes` 上自动继承。
Phase B 的 auth 矩阵与 body limit 测试需扩展到新路径。

## 10. 测试

| 范围 | 断言 |
| --- | --- |
| input 字符串形状 | `"hi"` → 一条 user 消息 |
| input 数组形状 | message / function_call / function_call_output / input_text / output_text 各自正确 |
| input 单对象形状 | 等价于单元素数组 |
| **连续 function_call 合并** | 两个连续 function_call → 一条 assistant 消息含两个 tool_call |
| **pending user parts flush** | 裸 input_text + input_image → 一条 user 消息含两个 block |
| instructions 位置 | 成为 system 消息 |
| **previous_response_id 报错** | 非空 → 400，message 说明不支持有状态续接 |
| store 忽略 | `store:true` 不报错 |
| model 缺省 | 无 model → 用默认模型 |
| 非流式 output items | message item + function_call item 结构与顺序 |
| **流式事件顺序** | created → in_progress → output_item.added → content_part.added → output_text.delta → content_part.done → output_item.done → completed → [DONE] |
| **SSE 格式** | 含 `event:` 行（与 Chat 的纯 `data:` 不同） |
| **output_index 管理** | 文本后跟工具调用时，message item 先 done，function_call 用新 index |
| function_call 事件 | added(in_progress) → arguments.delta → done(completed) |
| response.failed | 上游失败且已开始输出 → failed 事件 |
| usage 优先级 | contextUsage 优先，回落估算 |
| **web_search 宽判定** | type 前缀 / name 三值各自命中；混合工具不拦截 |
| **web_search 开关** | 关闭时不拦截，走正常 tools 路径 |
| web_search 输出 | 非流式三 item；流式事件序列 |
| **Anthropic 侧不受影响** | `has_web_search_tool` 行为与实现前一致 |
| auth / body limit | 新路径纳入 Phase B 的矩阵 |
| catalog | responses live、retrieve 仍 planned、防漂移通过 |

## 11. 风险

| 风险 | 缓解 |
| --- | --- |
| **output_index / content_index 错乱** | 单测覆盖「文本 → 工具 → 文本」的 index 序列 |
| **event: 行漏写** | SSE 格式单测；Chat 与 Responses 两种格式分别断言 |
| web_search 判定放宽后误拦截 | 恰好一个 tool 的约束 + 运行时开关 + 混合工具不拦截单测 |
| 两端点 web_search 行为不一致 | 有意选择（D11），写进 client_hints 与 spec non-goal |
| 连续 function_call 未合并导致配对失败 | 专项单测（Kiro 要求 tool_use/tool_result 同轮配对） |
| Anthropic 侧回归 | websearch.rs 只提可见性不改实现；全量 `cargo test` |
| 无凭据无法端到端验证 | 与 Phase B 同：单测覆盖事件序列，evidence 明确标注未跑项 |
