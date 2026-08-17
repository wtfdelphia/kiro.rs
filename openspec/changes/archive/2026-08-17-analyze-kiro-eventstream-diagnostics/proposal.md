## Why

最新 `claude-tap` 正式对话 trace 已确认代理链路稳定，剩余风险转移到 Kiro `generateAssistantResponse` 事件流的解析、诊断与协议映射质量。当前 `kiro-rs` 能处理 `assistantResponseEvent`、`toolUseEvent`、`contextUsageEvent`，但真实 trace 中稳定出现的 `reasoningContentEvent` 和 `meteringEvent` 还没有结构化建模与安全诊断，后续优化缺少可验证证据面。

## What Changes

- 新增 Kiro EventStream 诊断能力，用于记录每次上游生成响应的脱敏事件摘要。
- 为 `reasoningContentEvent` 建立事件模型，保留文本长度、signature 长度等安全元数据，默认不泄露原始 signature。
- 为 `meteringEvent` 建立最小解析模型，用于诊断与日志聚合，暂不改变 Anthropic/OpenAI 对外 usage 契约。
- 补充工具调用多分片诊断：按 `tool_use_id` 聚合 chunk 数、input 长度、`stop=true` 数量与异常情况。
- 保持现有 `/v1/messages`、`/cc/v1/messages`、OpenAI Chat Completions、OpenAI Responses 的对外响应兼容性，除非后续单独变更明确要求暴露 reasoning。
- 增加单元测试覆盖 reasoning/metering 解析、toolUse 多分片 stop 边界、context usage 回退与敏感字段不泄露。

## Capabilities

### New Capabilities

- `kiro-eventstream-diagnostics`: 描述 Kiro `generateAssistantResponse` 事件解析、脱敏诊断聚合、reasoning/metering 建模和不泄露敏感 payload 的要求。

### Modified Capabilities

- 无。首版仅新增内部诊断能力，不改变既有 public API、Anthropic/OpenAI 响应契约或模型映射要求。

## Impact

- 影响源码：
  - `src/kiro/model/events/*`
  - `src/kiro/parser/*`
  - `src/anthropic/stream.rs`
  - `src/anthropic/handlers.rs`
  - 可能涉及 `src/openai/stream.rs`、`src/openai/handlers.rs`、`src/openai/responses_stream.rs` 的诊断接入点
- 影响测试：
  - Kiro event 解析单元测试
  - Anthropic SSE 状态机测试
  - OpenAI 兼容流式/非流式聚合测试
- 不新增外部服务依赖。
- 不读取、保存或输出真实 token、Cookie、profile ARN、完整工具输入、完整 prompt、signature 原文。
- 实现后必须运行 `openspec validate --all` 与 `cargo check --release --all-targets`，并确认无新增编译告警。
