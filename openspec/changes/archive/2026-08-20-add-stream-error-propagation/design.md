# Design: add-stream-error-propagation

## 1. 当前实现

### 1.1 上游错误事件的形状

`src/kiro/model/events/base.rs:88-95` 定义两类错误事件：

- `Event::Error { error_code, error_message }`：解析自上游错误帧，缺失时兜底
  `UnknownError`（base.rs:154-158）；
- `Event::Exception { exception_type, message }`：兜底 `UnknownException`
  （base.rs:169-173）。

### 1.2 三条协议链路的现状（全部静默）

| 链路 | 位置 | 现状 |
| --- | --- | --- |
| Anthropic 流式 | `src/anthropic/stream.rs:658-676` | `Event::Error` 打日志返回空；`Event::Exception` 仅 `ContentLengthExceededException` 映射为 `max_tokens`，其余打日志吞掉 |
| Anthropic 流式收尾 | `src/anthropic/handlers.rs:628-644` | 流结束/读取失败都无条件 `generate_final_events()`；`get_stop_reason()` 兜底 `end_turn`（stream.rs:341-347）→ **假成功** |
| Anthropic 非流式 | `src/anthropic/handlers.rs:824` 附近聚合循环 | `Event::Error` 落入 `_ => {}`，返回截断的 200 JSON |
| OpenAI Chat 流式 | `src/openai/stream.rs:143-170` | `Event::Error` 落入 `_ => {}`；`finish()`（stream.rs:325-364）兜底 `stop` → 假成功 |
| OpenAI Chat 非流式 | `src/openai/handlers.rs:429` 附近聚合 | 同上吞掉 |
| Responses 流式（SSE/WS 共用事件源） | `src/openai/responses_stream.rs:168-183` | `_ => {}` 吞掉；已有 `fail()`（responses_stream.rs:577-600）只接传输层错误（`src/openai/handlers.rs:1510`） |
| Responses 非流式 | `src/openai/responses.rs` 聚合 | 吞掉 |

### 1.3 已有可复用的机制

- Responses 的 `fail()` 产出 `response.failed`（status=failed + `ResponsesError`），
  正是本变更需要的语义，缺的只是触发源接线；
- 诊断摘要（`src/kiro/model/events/diagnostics.rs`）已 `observe` 所有事件
  （diagnostics.rs:137 对 Error/Exception 不做工具生命周期统计，但计入观察），
  per-request `log_summary` 是现成的遥测落点；
- 请求级错误信封三套均已存在：Anthropic 错误信封、OpenAI 错误信封
  （`src/openai/error.rs`）、`ResponsesError`。

## 2. 目标设计

### 2.1 共享分类：StreamFault

新增 `src/kiro/stream_fault.rs`（或并入 `src/kiro/model/events/`，实现时择一）：

```rust
/// 上游生成流中的硬性错误（区别于已有语义映射的异常）。
pub struct StreamFault {
    pub code: String,    // 上游 error_code 或 exception_type
    pub message: String, // 上游错误消息（仅用于客户端可见错误与日志）
}

/// 将 Kiro 事件分类为硬错误；None 表示非硬错误（含 ContentLengthExceededException）。
pub fn classify_stream_fault(event: &Event) -> Option<StreamFault>;
```

分类规则：

| 事件 | 分类 | 理由 |
| --- | --- | --- |
| `Event::Error { .. }` | 硬错误 | 上游明确报错 |
| `Event::Exception` 且 type ≠ `ContentLengthExceededException` | 硬错误 | 无既有语义映射的异常 |
| `Event::Exception` 且 type = `ContentLengthExceededException` | **非硬错误** | 保留既有 length/max_tokens 映射（已被测试与 spec 覆盖的行为） |
| 其他事件 | 非硬错误 | — |

**首个硬错误生效**：各协议处理器新增 `stream_failed: bool`（或等价状态）。已失败
后再到的错误事件只打日志，不再产出客户端可见错误，避免重复错误事件。

### 2.2 Anthropic 渲染

- **流式**：`process_kiro_event` 遇硬错误时：
  1. 为所有已开启未关闭的内容块产出 `content_block_stop`（防御性收尾，避免客户端
     解析器挂在未闭合块上）；
  2. 产出 SSE `error` 事件：`{"type":"error","error":{"type":"api_error","message":"Kiro upstream error (<code>): <message>"}}`
     （错误 type 选 `api_error`，上游 code 编入 message，保留可扩展映射点）；
  3. 置 `stream_failed`。
  `generate_final_events()` 在 `stream_failed` 时返回空（块已关闭、错误已发），
  收尾路径（handlers.rs:628-644）无需改动即不再产出 `end_turn`/`message_stop`。
  `/cc/v1/messages` 的缓冲流式路径（`handle_stream_request_buffered`，
  handlers.rs:1107；`BufferedStreamProcessor::process_and_buffer` →
  `finish_and_get_all_events`，stream.rs:1200/1219）复用同一 StreamContext：
  错误事件随缓冲产出，`stream_failed` 时终态事件被抑制，客户端收到的仍是
  含 `error` 事件的 SSE（与直流式统一语义，不返回 502）。
- **非流式**：聚合循环记录 `Option<StreamFault>`；存在时返回 HTTP 502 +
  Anthropic 错误信封（`type: api_error`），不构建截断消息。

### 2.3 OpenAI Chat 渲染

- **流式**：遇硬错误时产出错误 chunk——`chat.completion.chunk` 形状，`choices: []`
  且携带 `error` 对象（`message`、`type: server_error`、`code: <上游 code>`），
  随后 `[DONE]`；置 `stream_failed` 后 `finish()` 不再产出 finish_reason chunk。
  该形状符合 OpenAI 流式错误惯例（错误 chunk + DONE）。
- **非流式**：共享聚合函数 `aggregate()`（openai/handlers.rs:315，生产调用点为
  Chat 非流式 handlers.rs:297 与 Responses 非流式 handlers.rs:1557）的 `Aggregated`
  结果新增 fault 字段；存在时返回 HTTP 502 + OpenAI 错误信封。

### 2.4 Responses 渲染（SSE 与 WS ingress）

- **流式**：`ResponsesEventSource::process_kiro_event` 遇硬错误时调用既有
  `fail(format!("Kiro upstream error (<code>): <message>"))`，产出
  `response.failed`（status=failed + `ResponsesError { server_error }`），
  并保证之后 MUST NOT 产出 `response.completed`。WS ingress 与 SSE 共用事件源，
  一次接线两处生效；WS 侧连接收尾沿用现有 fail 后的关闭语义，不改关闭码。
- **非流式**：聚合记录 fault；存在时返回 HTTP 502 + OpenAI 错误信封
  （Responses 非流式沿用 OpenAI 错误方言，见既有 spec）。

### 2.5 诊断遥测

诊断摘要新增安全字段：流内硬错误的 code 列表与次数（如
`stream_error_codes=["InternalServerException"]`）。**不**记录原始错误消息
（消息可能回显敏感内容，遵循 kiro-eventstream-diagnostics 的安全边界）。
`log_summary` 随之输出，为建议 2 的立项决策提供频率证据。

## 3. 数据流 / 影响面

```
Kiro EventStream 帧
  -> Event::from_frame（不变）
  -> classify_stream_fault（新增）
       ├─ None  -> 既有事件处理路径（不变）
       └─ Some(fault)
            ├─ diagnostics.observe 记 code（扩展）
            ├─ Anthropic: content_block_stop* + SSE error + stream_failed
            ├─ OpenAI:    error chunk + [DONE] + stream_failed
            └─ Responses: fail() -> response.failed + stream_failed
收尾：stream_failed 时抑制正常终态事件（end_turn / stop / completed）
```

影响面：

- 仅错误路径行为变化；成功路径事件序列与字节不变（既有测试把关）；
- `count_tokens` 不消费生成事件流，不受影响；
- `provider.rs` 状态码级重试/凭据切换不变；
- WS ingress 的连接生命周期语义不变（复用既有 fail 路径）。

## 4. 异常路径

| 场景 | 行为 |
| --- | --- |
| 错误事件先于任何内容 | 流式：SSE 已承诺 200，直接发协议错误事件后终止（HTTP 状态不变，透明重放属建议 2 范围）；非流式：502 错误信封 |
| 错误事件出现在内容之后 | 关闭已开启块/item → 发协议错误事件 → 终止；已发内容保留在错误事件之前 |
| 多个错误事件 | 首个生效，后续仅日志与诊断计数 |
| 错误事件后上游又发正常事件 | 已置 stream_failed，正常事件不再产出客户端内容（避免错误后又冒内容的错乱序列） |
| `ContentLengthExceededException` | 保持现状：Anthropic max_tokens / OpenAI length，不算硬错误 |
| 错误消息为空 | message 回退为 code 描述，不产出空 message 字段 |
| 上游错误消息包含异常内容 | 仅原样编入客户端错误 message 与日志；不进诊断摘要（摘要只留 code） |

## 5. 回滚

变更为纯错误路径行为修复，无配置 schema、无持久化格式变化。回滚 = revert 提交，
无需迁移。若线上发现某客户端无法处理协议错误事件，可按协议单独回退渲染层
（分类层保留），但默认不做开关。

## 6. 验证策略

- **单元测试**：`classify_stream_fault` 三类输入（Error / 普通 Exception /
  ContentLengthExceededException）；首个生效语义。
- **处理器测试（合成事件，不需要真实凭据）**：
  - Anthropic：硬错误 → `content_block_stop` + `error` 事件，且
    `generate_final_events` 不再产出 `end_turn`/`message_stop`；
    既有 `test_thinking_only_sets_max_tokens_stop_reason` 等成功路径测试不回归；
  - OpenAI Chat：硬错误 → error chunk + `[DONE]`，无 finish_reason chunk；
  - Responses：硬错误 → `response.failed`，无 `response.completed`。
- **非流式测试**：三协议聚合出 fault 时返回 502 错误信封而非截断 200。
- **诊断测试**：摘要含错误 code 与计数，不含原始消息。
- **安全断言**：错误渲染输出不含凭据/ARN 等敏感串（构造含敏感串的 fault 验证
  只透传 code+message，不引入其他上下文）。
- **门槛**：`cargo test` 全量；`cargo check --release --all-targets` 零新增告警；
  `openspec validate --all` 通过。
