# Tasks: add-stream-error-propagation

## 1. 共享分类层

- [x] 1.1 新增 `StreamFault` 与 `classify_stream_fault`（`src/kiro/stream_fault.rs` 或 `src/kiro/model/events/` 内），实现分类规则：`Event::Error` → 硬错误；`Event::Exception` → 硬错误，但 `ContentLengthExceededException` 排除
- [x] 1.2 分类层单元测试：Error / 普通 Exception / ContentLengthExceededException / 其他事件四类输入

## 2. Anthropic 协议

- [x] 2.1 `src/anthropic/stream.rs`：`process_kiro_event` 硬错误分支——关闭未闭合内容块、产出 SSE `error` 事件（api_error + 上游 code/message）、置 stream_failed；`generate_final_events` 在 stream_failed 时返回空
- [x] 2.2 处理器测试（合成事件）：内容前出错、内容后出错两个场景，断言 `error` 事件发出且无 `end_turn`/`message_stop`；直流式与缓冲流式（`BufferedStreamProcessor`，`/cc` 路径）都要覆盖；成功路径既有测试不回归
- [x] 2.3 `src/anthropic/handlers.rs` 非流式聚合：记录 fault，存在时返回 502 + Anthropic 错误信封；补非流式测试

## 3. OpenAI Chat 协议

- [x] 3.1 `src/openai/stream.rs`：硬错误分支产出 error chunk（空 choices + error 对象，type=server_error，code=上游 code）+ `[DONE]`，置 stream_failed 后 `finish()` 不再产出 finish_reason chunk
- [x] 3.2 处理器测试：错误 chunk 形状、`[DONE]` 结尾、无 finish_reason chunk；成功路径不回归
- [x] 3.3 `src/openai/handlers.rs` 共享 `aggregate()`/`Aggregated` 扩展 fault 字段：Chat 非流式调用点（handlers.rs:297）fault → 502 + OpenAI 错误信封；补测试

## 4. Responses 协议（SSE + WS ingress）

- [x] 4.1 `src/openai/responses_stream.rs`：`process_kiro_event` 硬错误接入既有 `fail()`，确保之后不产出 `response.completed`
- [x] 4.2 流式测试：SSE 路径硬错误 → `response.failed` 且无 `response.completed`；WS ingress 经共用事件源同路径覆盖（补事件源级测试）
- [x] 4.3 Responses 非流式（复用 3.3 的 `aggregate()` 扩展，调用点 handlers.rs:1557）：fault → 502 + OpenAI 错误信封；补测试

## 5. 诊断遥测与安全

- [x] 5.1 诊断摘要新增流内硬错误 code 列表与计数，`log_summary` 输出；不含原始错误消息；补测试
- [x] 5.2 安全测试：构造含敏感串的 fault，断言渲染输出只含 code+message，不含凭据/ARN 等额外上下文

## 6. 门槛验证

- [x] 6.1 `cargo test` 全量通过（含新增测试与既有成功路径 parity 测试）
- [x] 6.2 `cargo check --release --all-targets` 零新增告警
- [x] 6.3 `openspec validate --all` 通过
