# Capability: stream-error-propagation

## Purpose

Surface hard error events arriving inside a Kiro `generateAssistantResponse` EventStream
(`Event::Error`, and `Event::Exception` without an existing semantic mapping) as
protocol-appropriate errors on every client-facing protocol — Anthropic Messages,
OpenAI Chat Completions, and OpenAI Responses (SSE and WebSocket ingress). An in-stream
upstream error MUST NOT be rendered as a successful completion with truncated content.

## Requirements

### Requirement: 流内硬错误统一分类

The system MUST classify Kiro generation events into hard in-stream faults versus normal
events. An `Event::Error` MUST be classified as a hard fault carrying the upstream error
code and message. An `Event::Exception` MUST be classified as a hard fault carrying the
exception type and message, EXCEPT `ContentLengthExceededException`, which MUST keep its
existing length/max_tokens mapping and MUST NOT be treated as a fault. When multiple hard
faults occur in one stream, the first fault MUST determine the client-visible error and
subsequent faults MUST only be logged.

#### Scenario: Error 事件分类为硬错误

- **WHEN** 上游生成流包含携带 code 与 message 的错误事件
- **THEN** 系统 MUST 将其分类为硬错误并保留上游 code 与 message

#### Scenario: 无映射异常分类为硬错误

- **WHEN** 上游生成流包含类型不是 `ContentLengthExceededException` 的异常事件
- **THEN** 系统 MUST 将其分类为硬错误，code 为异常类型

#### Scenario: 内容超长异常保留既有语义

- **WHEN** 上游生成流包含 `ContentLengthExceededException`
- **THEN** 系统 MUST 保留既有的 length/max_tokens 映射，MUST NOT 产出协议错误事件

#### Scenario: 首个硬错误生效

- **WHEN** 一次流中出现多个硬错误事件
- **THEN** 客户端可见错误 MUST 由首个硬错误决定，后续硬错误 MUST 仅记录日志

### Requirement: Anthropic 协议流内错误渲染

For Anthropic Messages endpoints (including the `/cc` variant), when a hard in-stream
fault occurs, a streaming response MUST close any open content blocks, emit an SSE
`error` event whose error object carries an error type and a message including the
upstream code, and then terminate; it MUST NOT emit a `message_delta` with a normal
stop reason or a `message_stop` afterwards. A non-streaming response MUST return an HTTP
502 with the Anthropic error envelope instead of a truncated success body. The HTTP
status of an already-committed SSE response is unchanged.

#### Scenario: 内容之后出错

- **WHEN** 流式响应已发出部分文本后上游发出硬错误事件
- **THEN** 系统 MUST 关闭未闭合内容块、发出 SSE `error` 事件并终止流
- **AND** MUST NOT 再发出 `stop_reason: end_turn` 的 `message_delta` 或 `message_stop`

#### Scenario: 内容之前出错

- **WHEN** 上游在发出任何内容前发出硬错误事件
- **THEN** 系统 MUST 发出 SSE `error` 事件并终止流，客户端 MUST NOT 收到成功完成序列

#### Scenario: 非流式返回错误信封

- **WHEN** 非流式请求的上游流中出现硬错误事件
- **THEN** 响应 MUST 为 502 且 body 为 Anthropic 错误信封，MUST NOT 返回截断的成功消息

### Requirement: OpenAI Chat 协议流内错误渲染

For the OpenAI Chat Completions endpoint, when a hard in-stream fault occurs, a streaming
response MUST emit an error chunk carrying an `error` object (message, type
`server_error`, upstream code) with empty `choices`, followed by the `[DONE]` sentinel;
it MUST NOT emit a chunk with a normal `finish_reason`. A non-streaming response MUST
return an HTTP 502 with the OpenAI error envelope instead of a truncated success body.

#### Scenario: 流式错误 chunk

- **WHEN** 流式响应进行中上游发出硬错误事件
- **THEN** 系统 MUST 发出携带 `error` 对象且 `choices` 为空的错误 chunk，随后发出 `[DONE]`
- **AND** MUST NOT 发出携带正常 `finish_reason` 的 chunk

#### Scenario: 非流式返回错误信封

- **WHEN** 非流式请求的上游流中出现硬错误事件
- **THEN** 响应 MUST 为 502 且 body 为 OpenAI 错误信封，MUST NOT 返回截断的成功补全

### Requirement: Responses 协议流内错误渲染

For the Responses endpoint across both SSE and WebSocket ingress, a hard in-stream fault
MUST be treated as an upstream failure: the stream MUST emit a `response.failed` event
carrying a response with status `failed` and an error object, and MUST NOT emit
`response.completed`. A non-streaming response MUST return an HTTP 502 with the OpenAI
error envelope.

#### Scenario: SSE 路径失败事件

- **WHEN** SSE 流式响应进行中上游发出硬错误事件
- **THEN** 系统 MUST 发出 `response.failed` 事件，MUST NOT 发出 `response.completed`

#### Scenario: WS ingress 同语义

- **WHEN** WebSocket ingress 的一个 turn 执行中上游发出硬错误事件
- **THEN** 系统 MUST 经共用事件源发出 `response.failed`，连接收尾语义与既有传输层失败一致

#### Scenario: 非流式返回错误信封

- **WHEN** Responses 非流式请求的上游流中出现硬错误事件
- **THEN** 响应 MUST 为 502 且 body 为 OpenAI 错误信封

### Requirement: 成功路径不变

Streams containing no hard in-stream fault MUST produce the same event sequence and
response shapes as before this change. Error artifacts (error events, error chunks,
failed terminal events) MUST NOT appear in successful responses.

#### Scenario: 正常 Anthropic 流不变

- **WHEN** 上游流只包含正常事件并以正常方式结束
- **THEN** SSE 事件序列 MUST 与本变更前一致，以正常 `message_delta`/`message_stop` 收尾

#### Scenario: 正常 OpenAI 与 Responses 流不变

- **WHEN** Chat Completions 或 Responses 的上游流不含硬错误事件
- **THEN** 响应形状与事件序列 MUST 与本变更前一致，MUST NOT 出现错误 chunk 或失败事件

### Requirement: 错误渲染安全与遥测

Client-visible error rendering MUST NOT leak credentials, cookies, profile ARNs, or
prompts; it MUST only compose the upstream code and message with fixed protocol text.
The per-request diagnostic summary MUST record hard in-stream faults by code and count,
and MUST NOT include raw error messages.

#### Scenario: 错误渲染不泄漏敏感信息

- **WHEN** 硬错误被渲染为任一协议的错误事件或错误响应
- **THEN** 输出 MUST 仅由上游 code、message 与固定协议文案组成，MUST NOT 附带凭据或 profile ARN

#### Scenario: 诊断摘要记录错误分类

- **WHEN** 一次请求的流中出现硬错误事件
- **THEN** 诊断摘要 MUST 包含错误 code 与次数，MUST NOT 包含原始错误消息

### Requirement: 无真实凭据可验证

All behavior above MUST be testable with synthetic Kiro events or EventStream fixtures.
Tests MUST NOT require real Kiro credentials, login state, or live upstream access.

#### Scenario: 合成事件驱动测试

- **WHEN** 测试以合成的错误事件/异常事件驱动任一协议处理器
- **THEN** 系统 MUST 产出对应协议的错误渲染，且测试 MUST NOT 依赖真实上游
