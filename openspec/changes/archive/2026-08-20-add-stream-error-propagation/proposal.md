# Proposal: add-stream-error-propagation

让 Kiro 上游在流中发出的错误事件（`Event::Error` / 不可映射的 `Event::Exception`）
在 Anthropic、OpenAI Chat、Responses（含 WS ingress）三条协议链路上**可见**：
按各自协议方言渲染为错误事件/错误响应，并且**不再把错误截断包装成正常完成**。

立项依据：`docs/qoder2api-analysis-and-borrowable-patterns.md` 建议 1（P0）。
分析确认现状不是「错误不可见」而是「错误被包装成假成功」：上游错误事件被吞掉后
流照常收尾，`get_stop_reason()` 兜底返回 `end_turn`
（`src/anthropic/stream.rs:341-347`），客户端收到带 `message_stop` 的成功响应，
内容截断在上游出错点。Agent 客户端会当成正常回合结束，带残缺答案继续下游逻辑。

## Why

- **正确性缺陷（静默截断）**：三条协议链路全部吞掉 `Event::Error`：
  - Anthropic 流式：`src/anthropic/stream.rs:658-664` 只打日志返回 `Vec::new()`；
    收尾路径 `src/anthropic/handlers.rs:628-644` 无条件 `generate_final_events()`，
    客户端看到 `stop_reason: end_turn` 的假成功；
  - Anthropic 非流式：`src/anthropic/handlers.rs:824` 附近 `_ => {}` 吞掉，
    返回截断的 200 JSON；
  - OpenAI Chat 流式/非流式：`src/openai/stream.rs:143-170`、
    `src/openai/handlers.rs:429` 同样 `_ => {}`；
  - Responses：`src/openai/responses_stream.rs:168-183` `_ => {}`——而该协议其实
    已有 `fail()` → `response.failed` 机制（`responses_stream.rs:577-600`），
    目前只接了传输层错误（`src/openai/handlers.rs:1510`），流内错误事件没接进去。
- **协议义务**：`openai-responses` spec 已有「上游失败且已开始输出 → MUST 发出
  失败事件，MUST NOT 伪装成正常完成」场景（`openspec/specs/openai-responses/spec.md`
  Requirement「流式语义事件序列」）。流内错误事件属于上游失败，当前实现未覆盖该触发源。
- **遥测前置**：建议 2（首内容前透明故障转移）是否立项取决于「流内凭据类错误」的
  真实频率，而该频率目前无法测量——错误全被吞了。本变更让错误可见并进入诊断摘要，
  为后续决策提供证据。
- 参照系：Qoder2Api 对上游流内错误做协议化渲染（OpenAI error chunk + `[DONE]`、
  Anthropic SSE `error` 事件），kiro-rs 当前连可见性都没有。

## What Changes

### 新增能力：stream-error-propagation（新 spec）

- **统一分类**：新增共享分类逻辑（建议位置 `src/kiro/stream_fault.rs`）：
  `Event::Error` → 硬错误（code = error_code）；`Event::Exception` → 硬错误
  （code = exception_type），但 `ContentLengthExceededException` 保留既有
  length/max_tokens 语义，不算硬错误。首个硬错误生效，后续只记日志。
- **Anthropic**（`/v1/messages`、`/cc/v1/messages`）：流式发出 SSE `error` 事件后
  终止，MUST NOT 再发 `end_turn` 的 `message_delta`/`message_stop`；非流式返回
  502 + Anthropic 错误信封，替代截断的 200。
- **OpenAI Chat**（`/v1/chat/completions`）：流式发出带 `error` 字段、空 `choices`
  的错误 chunk 后跟 `[DONE]`，MUST NOT 发正常 `finish_reason` chunk；非流式返回
  502 + OpenAI 错误信封。
- **Responses**（`POST /v1/responses` SSE 与 WS ingress）：流内硬错误接入既有
  `fail()` → `response.failed`，MUST NOT 发 `response.completed`；非流式返回
  错误信封。
- **诊断摘要**：错误分类（code + 次数）进入既有 per-request 诊断摘要日志，
  不含原始错误消息，为建议 2 的决策提供遥测。
- **安全**：错误渲染 MUST NOT 泄漏凭据、Cookie、profile ARN 或提示词。

### 修改能力：openai-responses（MODIFIED）

- Requirement「流式语义事件序列」补充场景：Kiro 流内错误事件构成上游失败触发源，
  已输出内容时 MUST 走失败事件路径。

## Non-Goals

- 不做首内容前透明故障转移/重放（建议 2，需本变更的遥测证据后再评估）。
- 不做配额重置窗口解析与定时冷却（建议 3，需先验证 Kiro 402 错误体字段）。
- 不改 `provider.rs` 的状态码级重试/凭据切换逻辑。
- 不改请求级（非流内）错误映射。
- 不改成功路径的事件序列与字节形态。
- 不改 `count_tokens` 路径（不消费生成事件流）。
- 不新增配置开关：错误可见性是正确性修复，不做可回退的灰度。

## Impact

- 源码：`src/kiro/`（新增分类模块、诊断摘要扩展）、`src/anthropic/stream.rs`、
  `src/anthropic/handlers.rs`、`src/openai/stream.rs`、`src/openai/handlers.rs`、
  `src/openai/responses_stream.rs`、`src/openai/responses.rs`。
- Spec：新增 `stream-error-propagation`；修改 `openai-responses`。
- 客户端可见行为变化（仅错误路径）：原先收到「截断的假成功」，改后收到协议错误。
  这是本变更的目的，不是回归。
- README/AGENTS：不影响启动、构建、部署、测试命令与 API 入口清单，无需同步。
