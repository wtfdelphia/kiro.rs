# Synthetic Trace Correspondence

日期：2026-08-14

## 目的

记录真实 `claude-tap` 观察与本变更 synthetic test 的对应关系。本文只记录事件类型、计数级事实和测试覆盖，不粘贴真实 payload、prompt、工具输入、profile ARN、token、Cookie 或 reasoning signature。

## 真实 trace 脱敏事实

来源：本机最新正式对话 `claude-tap` trace DB 的脱敏分析。

| 观察项 | 脱敏结论 |
| --- | --- |
| 上游端点 | `generateAssistantResponse` |
| HTTP 状态 | 真实对话中出现多条 200 响应 |
| 模型 | trace 中可见请求模型名，不在本文复写真实上下文 |
| event families | `assistantResponseEvent`、`toolUseEvent`、`reasoningContentEvent`、`contextUsageEvent`、`meteringEvent` |
| toolUse 生命周期 | 同一 logical tool call 会拆成多段 `toolUseEvent`，正常应恰好一个 `stop=true` |
| reasoning | payload 含 `text` 与较长 `signature`；signature 按敏感值处理 |
| claude-tap 边界 | 当前 trace 无结构化 Kiro EventStream 事件数组，body 是文本化二进制 |

## Synthetic 覆盖

| 真实观察 | synthetic test / 实现证据 | 结论 |
| --- | --- | --- |
| reasoning event 稳定出现 | `kiro::model::events::base::tests::test_reasoning_event_from_frame`、`reasoning::tests::*` | `reasoningContentEvent` 不再落入 unknown |
| metering event 稳定出现 | `kiro::model::events::base::tests::test_metering_event_from_frame`、`metering::tests::*` | `meteringEvent` 解析 `unit`、`unitPlural`、`usage` |
| 多分片 toolUse | `diagnostics::tests::summarizes_synthetic_frame_stream`、`summarizes_multichunk_tool_without_raw_input` | 多 chunk 按同一 tool id 聚合为一个 lifecycle |
| exactly-one-stop 正常生命周期 | `summarizes_synthetic_frame_stream`、`summarizes_multichunk_tool_without_raw_input` | stop count 为 1 时不产生 anomaly |
| missing/duplicate stop 异常 | `reports_tool_lifecycle_anomalies` | missing stop、duplicate stop、missing id、missing name 进入 anomaly |
| reasoning signature 敏感 | `summarizes_reasoning_without_signature` | 摘要只记录 signature 字符数 |
| unknown event 需容错 | `test_unknown_event_preserves_type_and_payload`、`summarizes_usage_signals_and_unknowns` | 继续处理并计入 unknown count、unknown event type count 与 payload byte count |
| public protocol 不变 | `anthropic::stream::tests::test_reasoning_and_metering_events_are_not_public_sse`、`openai::stream::tests::test_reasoning_and_metering_events_are_not_public_chat_chunks`、`openai::responses_stream::tests::test_reasoning_and_metering_events_are_not_public_responses_events` | 不向 Anthropic/OpenAI 响应插入诊断字段 |

## 安全边界

- 诊断摘要保存长度、计数、hash 与 anomaly，不保存 raw prompt / raw tool input / raw tool output / raw signature。
- tool-use id 只输出 SHA-256 前 8 字节 hex hash；缺失 id 输出固定 `missing` 标记。
- 正常摘要为 debug 日志；生命周期异常为 warn 日志。
- 本 evidence 不包含真实 trace payload。

## 已运行验证

截至本文写入时已运行并通过：

```text
cargo test --release kiro::model::events
cargo test --release anthropic::stream
cargo test --release openai::stream
cargo test --release openai::responses_stream
cargo test --release anthropic::handlers
cargo test --release openai::handlers
git diff --check
```

最终 `openspec validate --all`、`cargo check --release --all-targets`、敏感扫描与 git status 将记录在后续 completion evidence 中。
