## ADDED Requirements

### Requirement: Chat Completions 端点与鉴权

The system MUST serve `POST /v1/chat/completions` as a client-facing endpoint. The endpoint MUST be protected by the same client authentication middleware as the existing Anthropic endpoints, and MUST accept request bodies up to the same size limit as those endpoints. Cross-origin requests MUST be permitted under the same CORS policy.

#### Scenario: 未鉴权请求被拒绝

- **WHEN** `requireApiKey` 为开启且请求未携带有效 client apiKey
- **THEN** 响应 MUST 为 401，且 MUST NOT 触达上游

#### Scenario: 关闭鉴权后放行

- **WHEN** `requireApiKey` 为关闭
- **THEN** 未携带 key 的请求 MUST NOT 因鉴权被拒绝

#### Scenario: 大请求体不被默认限制拦截

- **WHEN** 请求体大于框架默认限制（2MB）但小于本服务配置的上限
- **THEN** 响应 MUST NOT 为 413

### Requirement: 请求契约与参数处理

The endpoint MUST require `model` and a non-empty `messages` array containing at least one user message. It MUST accept `stream`, `stream_options`, `max_tokens`, `max_completion_tokens`, `tools`, and `tool_choice`. It MUST accept `temperature` and `top_p` without error but MUST NOT forward them upstream, because the upstream protocol has no equivalent field. Unrecognized fields MUST be ignored rather than rejected.

#### Scenario: 缺少 user 消息

- **WHEN** `messages` 非空但不含任何 `user` 角色消息
- **THEN** 响应 MUST 为 400 且错误类型为 `invalid_request_error`

#### Scenario: messages 为空

- **WHEN** `messages` 为空数组
- **THEN** 响应 MUST 为 400 且错误类型为 `invalid_request_error`

#### Scenario: max_tokens 与 max_completion_tokens 二者取一

- **WHEN** 请求同时缺省 `max_tokens` 与 `max_completion_tokens`
- **THEN** 系统 MUST 采用文档化的默认上限值，MUST NOT 因缺省而失败

#### Scenario: temperature 被接受但不生效

- **WHEN** 请求携带 `temperature` 或 `top_p`
- **THEN** 请求 MUST NOT 因此被拒绝，且这两个值 MUST NOT 出现在发往上游的请求中

### Requirement: 工具定义双形状兼容

Tool definitions MUST be accepted in both the Chat Completions nested form (`{type, function:{name, description, parameters}}`) and the Responses top-level form (`{type, name, description, parameters}`). Both forms MUST yield the same resolved tool name, description, and parameter schema.

#### Scenario: 嵌套形状

- **WHEN** 工具为 `{"type":"function","function":{"name":"get_weather","parameters":{...}}}`
- **THEN** 解析出的工具名 MUST 为 `get_weather`，参数 schema MUST 等于给定的 `parameters`

#### Scenario: 顶层形状

- **WHEN** 工具为 `{"type":"function","name":"get_weather","parameters":{...}}`
- **THEN** 解析结果 MUST 与嵌套形状等价

### Requirement: 请求映射复用既有转换核

The request MUST be mapped into the internal Anthropic-shaped request structure in memory and then passed through the existing request conversion pipeline, so that upstream-side constraints (prefill handling, tool name shortening, system chunking, thinking prefix injection, tool_use/tool_result pairing and orphan cleanup) apply identically to both protocols. The system MUST NOT implement a second direct-to-upstream converter for this protocol.

#### Scenario: system 消息合并

- **WHEN** 请求含多条 `system`（或 `developer`）角色消息
- **THEN** 它们 MUST 按原顺序进入内部请求的 system 部分，MUST NOT 被丢弃或重排

#### Scenario: assistant 工具调用映射

- **WHEN** `assistant` 消息携带 `tool_calls`
- **THEN** 每个调用 MUST 映射为一个 tool_use 内容块，且其 `arguments` JSON 字符串 MUST 被解析为对象；解析失败时 MUST 退化为空对象而非使整个请求失败

#### Scenario: tool 角色配对与归集

- **WHEN** 出现一条或多条连续的 `tool` 角色消息
- **THEN** 它们 MUST 按 `tool_call_id` 映射为 tool_result 内容块，并归集到同一条 user 消息中

#### Scenario: 图片 part 处理

- **WHEN** `user` 消息的 content 数组含 `image_url` 且其 url 为 base64 data URL
- **THEN** 该 part MUST 映射为内部的 base64 图片来源，保留 media type

#### Scenario: 远程图片 URL 跳过

- **WHEN** `image_url` 的 url 为 http/https 远程地址
- **THEN** 该 part MUST 被跳过并记录告警，MUST NOT 导致请求失败，也 MUST NOT 让上游收到无法解析的引用

### Requirement: thinking 后缀在本端点同样生效

When the requested model carries the thinking suffix, the endpoint MUST apply the same thinking configuration override that the Anthropic endpoints apply, so that the thinking directive reaches the upstream request. Silently dropping the suffix is not acceptable.

#### Scenario: thinking 后缀注入指令

- **WHEN** 请求 model 为带 thinking 后缀的可识别模型
- **THEN** 发往上游的请求 MUST 含 thinking 模式指令；MUST NOT 出现「后缀被忽略、请求退化为普通请求」的情况

### Requirement: 响应中的模型名回显原值

Both streaming and non-streaming responses MUST echo the model string exactly as supplied by the client, not the resolved upstream model id.

#### Scenario: 别名模型回显

- **WHEN** 客户端请求 model 为 `gpt-4o`
- **THEN** 响应的 model 字段 MUST 为 `gpt-4o`，即使实际执行的是映射后的 Claude 模型

### Requirement: 工具名还原

When the conversion pipeline shortens an over-long tool name, the response MUST report the client's original tool name, so that the client can match the call against its own declared tools.

#### Scenario: 超长工具名往返

- **WHEN** 客户端声明的工具名长度超过内部上限并触发缩短
- **THEN** 响应中 `tool_calls[].function.name` MUST 为客户端声明的原始名称，MUST NOT 为内部缩短后的名称

### Requirement: 非流式响应契约

A non-streaming request MUST return a `chat.completion` object with one choice. `finish_reason` MUST be `tool_calls` when the model produced tool calls, `length` when the upstream indicates a content-length or context-window limit, and `stop` otherwise. `usage.prompt_tokens` MUST prefer the value derived from the upstream context-usage signal and fall back to local estimation when that signal is absent.

#### Scenario: 工具调用的 finish_reason

- **WHEN** 上游产生了工具调用
- **THEN** `finish_reason` MUST 为 `tool_calls`

#### Scenario: 上下文超限

- **WHEN** 上游返回内容长度超限异常或上下文使用率达到 100%
- **THEN** `finish_reason` MUST 为 `length`

#### Scenario: prompt_tokens 优先用上游信号

- **WHEN** 上游回传了 context-usage 事件
- **THEN** `usage.prompt_tokens` MUST 由该信号反算得出，而非本地估算值

### Requirement: 思考内容不污染 content

When thinking is enabled, reasoning text MUST be delivered in a dedicated reasoning field, separate from the assistant's answer content. Reasoning text MUST NOT appear inside `content`.

#### Scenario: 非流式分离

- **WHEN** thinking 启用且上游输出含思考段
- **THEN** 思考文本 MUST 出现在专用 reasoning 字段中，`content` MUST 只含最终回答

#### Scenario: 流式分离

- **WHEN** thinking 启用的流式请求
- **THEN** 思考增量 MUST 通过 reasoning 增量字段发送，MUST NOT 混入 content 增量

### Requirement: 流式 chunk 序列

A streaming request MUST respond with `text/event-stream` and emit `chat.completion.chunk` objects. The first chunk MUST carry only the assistant role in its delta. Text and tool-call arguments MUST be sent as deltas. A final chunk MUST carry `finish_reason`. The stream MUST terminate with the `[DONE]` sentinel.

#### Scenario: 首块只带 role

- **WHEN** 流式响应开始
- **THEN** 第一个 chunk 的 delta MUST 只含 role，MUST NOT 含 content

#### Scenario: 工具调用增量

- **WHEN** 上游分片输出某个工具调用的参数
- **THEN** 首个该工具的 chunk MUST 含其 id、name 与稳定的 index，后续 chunk MUST 只含参数片段；同一工具的 index 在整个流中 MUST 保持不变

#### Scenario: 流以 DONE 结束

- **WHEN** 流式响应结束
- **THEN** 最后发送的数据 MUST 为 `[DONE]` 标记

#### Scenario: 保活不破坏协议

- **WHEN** 上游长时间无输出
- **THEN** 系统 MAY 发送 SSE 层面的保活数据，但该数据 MUST NOT 是一个会被 OpenAI 客户端误解析为 chunk 的 JSON 事件

### Requirement: usage 仅在客户端请求时随流返回

Streaming usage MUST be emitted only when the client sets `stream_options.include_usage` to true. When emitted, it MUST appear in a chunk with an empty choices array, placed after the final `finish_reason` chunk and before the `[DONE]` sentinel.

#### Scenario: 请求 usage

- **WHEN** 流式请求携带 `stream_options: {"include_usage": true}`
- **THEN** `[DONE]` 之前 MUST 存在一个 choices 为空数组且携带 usage 的 chunk

#### Scenario: 未请求 usage

- **WHEN** 流式请求未携带 `stream_options.include_usage` 或其值为 false
- **THEN** 响应流中 MUST NOT 出现携带 usage 的 chunk

### Requirement: OpenAI 错误方言

Errors from this endpoint MUST use the OpenAI error envelope (an `error` object carrying `message` and `type`). The Anthropic error shape MUST NOT be returned from this endpoint, and this endpoint's shape MUST NOT be returned from the Anthropic endpoints.

#### Scenario: 非法 JSON

- **WHEN** 请求体不是合法 JSON
- **THEN** 响应 MUST 为 400，且响应体 MUST 为 OpenAI 错误信封，类型为 `invalid_request_error`

#### Scenario: 模型无法解析

- **WHEN** 请求 model 无法被模型解析管线接受
- **THEN** 响应 MUST 为 400，MUST 使用 OpenAI 错误信封，且 MUST NOT 为该端点放宽解析策略

#### Scenario: 上游不可用

- **WHEN** 无可用凭据或上游调用失败
- **THEN** 响应 MUST 使用 OpenAI 错误信封，状态码反映服务端错误，且 MUST NOT 泄漏凭据信息

### Requirement: 本端点不提供服务端 web_search

This endpoint MUST NOT intercept requests that declare a tool named `web_search` and MUST NOT perform a server-side search on the client's behalf. Such a tool MUST be treated as an ordinary function tool and forwarded through the normal tool path, because the Chat Completions protocol has no faithful way to represent a proxy-executed search.

#### Scenario: web_search 作为普通工具

- **WHEN** 请求声明单个名为 `web_search` 的 function 工具
- **THEN** 该工具 MUST 走正常工具路径（模型可返回对应的 tool call），系统 MUST NOT 代替客户端执行搜索，也 MUST NOT 返回搜索结果内容块

### Requirement: 与端点注册表状态一致

Once this endpoint is implemented and mounted, its entry in the public API registry MUST be updated from `planned` to `live`, and the existing drift guard (live entries must be routable, planned entries must not) MUST pass without modification to the guard itself. Endpoints still unimplemented MUST remain `planned`.

#### Scenario: Chat Completions 登记为 live 且可命中

- **WHEN** 读取端点注册表
- **THEN** `POST /v1/chat/completions` 的 status MUST 为 live，且对它发起请求 MUST NOT 得到 404

#### Scenario: 防漂移断言不被削弱

- **WHEN** 本能力实现完成
- **THEN** 既有的 live/planned 双向路由断言 MUST 仍然生效且未被放宽或跳过

### Requirement: 既有 Anthropic 端点零回归

Adding this endpoint MUST NOT change the behavior of the existing Anthropic endpoints. Shared code MAY gain new visibility so the new module can reuse it, but the shared implementations MUST NOT be modified for the new protocol's convenience.

#### Scenario: Anthropic 端点行为不变

- **WHEN** 本能力实现完成
- **THEN** `/v1/messages`、`/cc/v1/messages`、`/v1/messages/count_tokens`、`/v1/models` 的请求契约、响应结构、SSE 事件序列与鉴权行为 MUST 与实现前一致

#### Scenario: 共享逻辑不为新协议改写

- **WHEN** 新端点需要复用既有转换、thinking 处理或流式辅助逻辑
- **THEN** 复用方式 MUST 限于扩大可见性或调用现有函数，MUST NOT 修改其实现或调用契约
