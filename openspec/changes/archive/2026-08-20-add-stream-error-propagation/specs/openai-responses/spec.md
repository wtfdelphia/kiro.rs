## MODIFIED Requirements

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

#### Scenario: 流内错误事件构成上游失败

- **WHEN** 上游生成流中发出 Kiro 错误事件（error 事件或无语义映射的异常事件）
- **THEN** 系统 MUST 将其视为上游失败并按失败路径收尾，MUST NOT 发出完成事件

#### Scenario: 流以 DONE 结束

- **WHEN** 流式响应正常结束
- **THEN** 最后发送的数据 MUST 为 `[DONE]` 标记

#### Scenario: 保活不破坏协议

- **WHEN** 上游长时间无输出
- **THEN** 系统 MAY 发送 SSE 层面的保活数据，但该数据 MUST NOT 被客户端误解析为语义事件
