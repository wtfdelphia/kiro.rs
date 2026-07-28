## ADDED Requirements

### Requirement: Responses 端点与鉴权

The system MUST serve `POST /v1/responses` as a client-facing endpoint under the same client authentication, body size limit, and CORS policy as the other OpenAI-compatible endpoints.

#### Scenario: 未鉴权请求被拒绝

- **WHEN** `requireApiKey` 开启且请求未携带有效 client apiKey
- **THEN** 响应 MUST 为 401，且 MUST NOT 触达上游

#### Scenario: 大请求体不被默认限制拦截

- **WHEN** 请求体大于框架默认限制（2MB）但小于本服务配置的上限
- **THEN** 响应 MUST NOT 为 413

### Requirement: input 三种形状归一

The `input` field MUST accept a plain string, an array of input items, or a single input item object, and normalize all three into an ordered message sequence. A plain string MUST become a single user message. A single object MUST behave identically to a one-element array.

#### Scenario: 字符串输入

- **WHEN** `input` 为 `"hello"`
- **THEN** 归一结果 MUST 为一条 user 消息，内容为 `hello`

#### Scenario: 单对象等价于单元素数组

- **WHEN** `input` 为一个 item 对象
- **THEN** 归一结果 MUST 与把该对象放入单元素数组时一致

#### Scenario: 空输入被拒绝

- **WHEN** `input` 缺省、为 null、为空字符串或归一后不含任何消息
- **THEN** 响应 MUST 为 400，错误类型为 `invalid_request_error`

### Requirement: input item 类型分派

Array items MUST be dispatched by their `type` (falling back to `role` when `type` is absent). Message items MUST become messages of the stated role. Function-call items MUST become assistant tool calls. Function-call-output items MUST become tool messages carrying the originating call id. Bare text and image items MUST be accumulated into a user message.

#### Scenario: message item

- **WHEN** item 为 `{"type":"message","role":"user","content":"hi"}`
- **THEN** 归一出一条 user 消息

#### Scenario: function_call_output 配对

- **WHEN** item 为 `{"type":"function_call_output","call_id":"c1","output":"result"}`
- **THEN** 归一出一条 tool 消息，其 call id MUST 为 `c1`，内容 MUST 为 `result`

#### Scenario: call_id 与 tool_call_id 兼容

- **WHEN** function-call-output item 只提供 `tool_call_id` 而非 `call_id`
- **THEN** 该值 MUST 被用作配对 id

#### Scenario: 连续 function_call 合并进同一条 assistant 消息

- **WHEN** input 数组含两个相邻的 `function_call` item
- **THEN** 它们 MUST 归入同一条 assistant 消息的工具调用列表，MUST NOT 拆成两条 assistant 消息（并行工具调用必须留在同一轮，否则上游要求的工具调用与结果配对会被破坏）

#### Scenario: 裸文本与图片 item 归集为 user 消息

- **WHEN** input 数组含不带 role 的 `input_text` 与 `input_image` item
- **THEN** 它们 MUST 被归集进同一条 user 消息，MUST NOT 被丢弃

#### Scenario: 归集在遇到带 role 的 item 时结束

- **WHEN** 裸 `input_text` item 之后紧随一个带 role 的 message item
- **THEN** 先前累积的内容 MUST 先成为一条独立 user 消息，顺序 MUST 保持

#### Scenario: output_text item 归为 assistant

- **WHEN** item 为 `{"type":"output_text","text":"prev answer"}`
- **THEN** 归一出一条 assistant 消息

### Requirement: instructions 作为当前轮系统指令

A non-empty `instructions` field MUST be applied as a system instruction for the current turn, positioned so that it governs the normalized input messages.

#### Scenario: instructions 生效

- **WHEN** 请求携带非空 `instructions`
- **THEN** 该文本 MUST 作为系统指令进入发往上游的请求

#### Scenario: instructions 缺省不产生空指令

- **WHEN** `instructions` 缺省或为空白
- **THEN** MUST NOT 产生空的系统指令

### Requirement: 首版无状态，previous_response_id 明确报错

This endpoint MUST NOT support stateful continuation in this change. A non-empty `previous_response_id` MUST be rejected with a client error whose message states that stateful continuation is unavailable and that the full conversation should be sent in `input`. The request MUST NOT be silently downgraded to a stateless one.

#### Scenario: previous_response_id 被拒绝

- **WHEN** 请求携带非空 `previous_response_id`
- **THEN** 响应 MUST 为 400，错误信息 MUST 说明本服务未启用有状态续接，且 MUST NOT 发起上游调用

#### Scenario: 不静默丢历史

- **WHEN** 请求携带 `previous_response_id`
- **THEN** 系统 MUST NOT 忽略该字段并照常返回一个缺少上下文的答复

#### Scenario: store 被忽略但不报错

- **WHEN** 请求携带 `store: true`
- **THEN** 请求 MUST NOT 因此被拒绝，且该值 MUST NOT 导致任何持久化行为

### Requirement: model 缺省与回显

When `model` is absent or blank, a documented default model MUST be used. Both streaming and non-streaming responses MUST echo the model string as supplied by the client (or the default when none was supplied), not the resolved upstream id.

#### Scenario: model 缺省

- **WHEN** 请求未提供 `model`
- **THEN** 系统 MUST 使用文档化的默认模型，MUST NOT 因缺省而失败

#### Scenario: 别名模型回显

- **WHEN** 客户端请求 model 为 `gpt-4o`
- **THEN** 响应中的 model 字段 MUST 为 `gpt-4o`

### Requirement: 非流式响应对象

A non-streaming request MUST return a response object whose `output` is an ordered list of items. Assistant text MUST appear as a message item containing an `output_text` content part. Each tool call MUST appear as its own function-call item carrying the call id, tool name, and argument JSON string. `usage.input_tokens` MUST prefer the value derived from the upstream context-usage signal and fall back to local estimation.

#### Scenario: 文本输出结构

- **WHEN** 上游只产生文本
- **THEN** `output` MUST 含一个 message item，其 content MUST 含一个 `output_text` part

#### Scenario: 工具调用输出结构

- **WHEN** 上游产生了工具调用
- **THEN** 每个调用 MUST 是一个独立的 function-call item，携带 call id、工具名与参数 JSON 字符串

#### Scenario: 工具名还原

- **WHEN** 转换管线缩短了超长工具名
- **THEN** 输出中的工具名 MUST 为客户端声明的原始名称

#### Scenario: input_tokens 优先用上游信号

- **WHEN** 上游回传了 context-usage 事件
- **THEN** `usage.input_tokens` MUST 由该信号反算得出

#### Scenario: metadata 回显

- **WHEN** 请求携带 `metadata`
- **THEN** 响应 MUST 回显该 metadata

### Requirement: 流式语义事件序列

A streaming request MUST respond with `text/event-stream` using named SSE events. The stream MUST begin with a creation event and an in-progress event, MUST wrap assistant text in item-added, content-part-added, text-delta, content-part-done and item-done events, MUST end with a completion event carrying the final response object, and MUST terminate with the `[DONE]` sentinel.

#### Scenario: 事件为命名 SSE 事件

- **WHEN** 流式响应发出任一语义事件
- **THEN** 该事件 MUST 同时包含 SSE 的事件名行与数据行（与 Chat Completions 的纯数据行不同）

#### Scenario: 文本路径事件顺序

- **WHEN** 上游只产生文本
- **THEN** 事件顺序 MUST 为：创建 → 进行中 → item 添加 → content part 添加 → 文本增量（一或多次）→ content part 完成 → item 完成 → 完成 → `[DONE]`

#### Scenario: 工具调用事件序列

- **WHEN** 上游产生一个工具调用
- **THEN** MUST 依次发出该 function-call item 的添加事件（状态为进行中）、参数增量事件、完成事件（状态为已完成）

#### Scenario: 文本后接工具调用时先关闭文本 item

- **WHEN** 上游先输出文本再发起工具调用
- **THEN** 在发出 function-call item 的添加事件之前，MUST 先关闭已开启的 message item（发出其 content part 完成与 item 完成事件）

#### Scenario: output 索引不重复

- **WHEN** 一次响应中出现 message item 与一个或多个 function-call item
- **THEN** 每个 item MUST 拥有各自不同的 output 索引，且索引 MUST 随 item 顺序递增

#### Scenario: 上游失败且已开始输出

- **WHEN** 上游在已经发出内容之后失败
- **THEN** MUST 发出失败事件，其中携带状态为 failed 的响应与错误信息，MUST NOT 伪装成正常完成

#### Scenario: 流以 DONE 结束

- **WHEN** 流式响应正常结束
- **THEN** 最后发送的数据 MUST 为 `[DONE]` 标记

#### Scenario: 保活不破坏协议

- **WHEN** 上游长时间无输出
- **THEN** 系统 MAY 发送 SSE 层面的保活数据，但该数据 MUST NOT 被客户端误解析为语义事件

### Requirement: OpenAI 错误方言

Errors from this endpoint MUST use the OpenAI error envelope. The Anthropic error shape MUST NOT be returned.

#### Scenario: 非法 JSON

- **WHEN** 请求体不是合法 JSON
- **THEN** 响应 MUST 为 400，响应体 MUST 为 OpenAI 错误信封

#### Scenario: 模型无法解析

- **WHEN** 请求 model 无法被模型解析管线接受
- **THEN** 响应 MUST 为 400，使用 OpenAI 错误信封，且 MUST NOT 为该端点放宽解析策略

#### Scenario: 上游不可用

- **WHEN** 无可用凭据或上游调用失败且尚未开始输出
- **THEN** 响应 MUST 使用 OpenAI 错误信封，且 MUST NOT 泄漏凭据信息

### Requirement: thinking 后缀在本端点同样生效

When the requested model carries the thinking suffix, this endpoint MUST apply the same thinking configuration override as the other endpoints, so that the thinking directive reaches the upstream request.

#### Scenario: thinking 后缀注入指令

- **WHEN** 请求 model 为带 thinking 后缀的可识别模型
- **THEN** 发往上游的请求 MUST 含 thinking 模式指令

### Requirement: 与端点注册表状态一致

Once implemented and mounted, the Responses entry in the public API registry MUST be `live`, while the retrieve-by-id entry MUST remain `planned`. The existing drift guard MUST pass without being weakened.

#### Scenario: Responses 登记为 live 且可命中

- **WHEN** 读取端点注册表
- **THEN** `POST /v1/responses` 的 status MUST 为 live，且对它发起请求 MUST NOT 得到 404

#### Scenario: retrieve 端点仍为 planned

- **WHEN** 读取端点注册表
- **THEN** `GET /v1/responses/{id}` MUST 仍为 planned，且请求它 MUST 返回 404

### Requirement: 既有端点零回归

Adding this endpoint MUST NOT change the behavior of the Anthropic endpoints or the Chat Completions endpoint. Shared code MAY gain visibility for reuse but MUST NOT be modified for this protocol's convenience.

#### Scenario: 既有端点行为不变

- **WHEN** 本能力实现完成
- **THEN** `/v1/messages`、`/cc/v1/messages`、`/v1/models`、`/v1/chat/completions` 的请求契约、响应结构与事件序列 MUST 与实现前一致
