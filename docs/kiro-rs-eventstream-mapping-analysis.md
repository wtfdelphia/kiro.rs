# Kiro EventStream 到 Anthropic SSE 映射分析

日期：2026-08-14

## 1. 分析目标

本轮目标不是继续验证代理连通性，而是基于最新 `claude-tap` 正式对话 trace 和 `kiro-rs` 源码，判断：

1. `claude-tap` 对 `kiro-rs -> Kiro` 的采集能力边界。
2. `kiro-rs` 当前是否能覆盖真实 `toolUseEvent` 多分片工具调用。
3. `reasoningContentEvent`、`contextUsageEvent`、`meteringEvent` 在当前兼容层里的处理状态。
4. 下一步优化应优先落在哪个模块。

## 2. 数据来源

最新 trace DB：

```text
/tmp/kiro-rs-ctap-20260814-111602/ctap-data/claude-tap/traces.sqlite3
```

session 摘要：

```text
session_id=5a6a1c50-b8cb-4348-aba4-e3a9a2b7e099
started_at=2026-08-14T03:16:37Z
updated_at=2026-08-14T03:20:07Z
record_count=16
proxy_mode=forward
client=claude
```

源码入口：

| 项目 | 文件 | 结论用途 |
| --- | --- | --- |
| `kiro-rs` | `src/anthropic/handlers.rs` | `/v1/messages` 与 `/cc/v1/messages` 流式入口 |
| `kiro-rs` | `src/anthropic/stream.rs` | Kiro event 到 Anthropic SSE 的状态机 |
| `kiro-rs` | `src/kiro/model/events/base.rs` | Kiro event type 识别 |
| `kiro-rs` | `src/kiro/model/events/tool_use.rs` | `toolUseEvent` payload 模型 |
| `claude-tap` | `claude_tap/proxy.py` | 常规 HTTP 代理流式判定与采集 |
| `claude-tap` | `claude_tap/forward_proxy.py` | CONNECT forward proxy 流式判定与采集 |
| `claude-tap` | `claude_tap/viewer.py` | Bedrock EventStream 解码与 viewer 归一 |

CodeGraph 状态：

```text
Project: /home/openclaw/wtf_workspace/local/kiro.rs
Files: 146
Nodes: 2,869
Edges: 8,385
Index is up to date
```

OpenSpec 状态：

```text
active changes: []
```

## 3. 最新正式对话事件结构

7 条 `POST /generateAssistantResponse` 全部 `HTTP 200`，模型均为 `gpt-5.6-sol`。event-type 统计：

| event | count |
| --- | ---: |
| `toolUseEvent` | 1561 |
| `assistantResponseEvent` | 882 |
| `reasoningContentEvent` | 7 |
| `contextUsageEvent` | 7 |
| `meteringEvent` | 7 |

事件顺序按 record 压缩后是稳定的：

| record | 顺序 |
| ---: | --- |
| 4 | `assistantResponseEvent -> reasoningContentEvent -> contextUsageEvent -> meteringEvent` |
| 6 | `assistantResponseEvent -> toolUseEvent -> reasoningContentEvent -> contextUsageEvent -> meteringEvent` |
| 8 | `assistantResponseEvent -> toolUseEvent -> reasoningContentEvent -> contextUsageEvent -> meteringEvent` |
| 10 | `toolUseEvent -> reasoningContentEvent -> contextUsageEvent -> meteringEvent` |
| 12 | `assistantResponseEvent -> toolUseEvent -> reasoningContentEvent -> contextUsageEvent -> meteringEvent` |
| 14 | `toolUseEvent -> reasoningContentEvent -> contextUsageEvent -> meteringEvent` |
| 16 | `assistantResponseEvent -> reasoningContentEvent -> contextUsageEvent -> meteringEvent` |

这说明本轮没有观察到同一个 `generateAssistantResponse` 内 `toolUseEvent` 之后又追加 `assistantResponseEvent` 的情况，但源码已经有 text block 重开逻辑，后续仍应保留这个测试面。

## 4. toolUseEvent 多分片形态

对 `payload_json.response.body` 做脱敏 JSON payload 扫描后，工具调用结构如下：

| record | tool payloads | unique tool calls | 工具名 | 每个 tool chunks 范围 | 每个 tool input chars 范围 | stop 结论 |
| ---: | ---: | ---: | --- | --- | --- | --- |
| 6 | 154 | 4 | `Bash`, `Glob`, `Read` | 35-42 | 97-123 | 每个 tool 恰好 1 个 `stop=true` |
| 8 | 566 | 10 | `Read`, `Grep` | 41-103 | 118-414 | 每个 tool 恰好 1 个 `stop=true` |
| 10 | 423 | 4 | `Grep`, `Bash` | 40-144 | 139-497 | 每个 tool 恰好 1 个 `stop=true` |
| 12 | 377 | 8 | `Read`, `Grep` | 35-81 | 103-232 | 每个 tool 恰好 1 个 `stop=true` |
| 14 | 41 | 1 | `Read` | 41 | 105 | 每个 tool 恰好 1 个 `stop=true` |

重要观察：

1. Kiro 上游把单个工具调用拆成几十到一百多个 `toolUseEvent`。
2. 中间分片多为未显式携带 `stop` 字段；`kiro-rs` 的 `ToolUseEvent.stop` 使用 `#[serde(default)]`，因此会按 `false` 处理。
3. 每个 `toolUseId` 最终都有且只有一个 `stop=true`。

这与 `src/anthropic/stream.rs` 当前设计基本匹配：

1. 以 `tool_use_id` 复用同一个 content block index。
2. 每个非空 `input` 映射为 `input_json_delta.partial_json`。
3. `stop=true` 时发送 `content_block_stop`。
4. `tool_use` 开始前自动关闭打开的 text block。

结论：本轮真实 trace 不能证明 `toolUseEvent` 映射有缺陷；相反，它给出了当前状态机设计合理的正向证据。

## 5. reasoningContentEvent 是当前主要盲点

本轮 7 条 `reasoningContentEvent` 的 payload 形状均为：

```text
text: str(len=3)
signature: str(len=2082..14078)
```

`kiro-rs` 当前 `EventType::from_str` 只识别：

```text
assistantResponseEvent
toolUseEvent
meteringEvent
contextUsageEvent
```

因此 `reasoningContentEvent` 会进入 `Event::Unknown {}`，随后在 `StreamContext::process_kiro_event` 中被丢弃。

这和现有 thinking 逻辑不是同一个入口：

1. 当前 `thinking` 支持来自 `assistantResponseEvent.content` 中的 `<thinking>...</thinking>` 标签。
2. Kiro 上游真实返回的是单独的 `reasoningContentEvent`。
3. `reasoningContentEvent` 还携带 `signature`，当前没有模型结构承接，也没有 SSE 输出策略。

影响判断：

1. 对本轮 `gpt-5.6-sol`，每轮 reasoning 文本很短，因此用户可见影响可能不大。
2. 对上一轮 `claude-opus-5`，`reasoningContentEvent=3592`，如果这些事件承载真实 reasoning 内容，当前丢弃会更明显。
3. 是否映射为 Anthropic `thinking_delta` 需要先确认客户端契约，尤其是 `signature` 如何处理。

## 6. contextUsageEvent 与 meteringEvent 状态

`contextUsageEvent` 当前已被 `kiro-rs` 用于估算 `input_tokens`：

```text
actual_input_tokens = context_usage_percentage * context_window_size / 100
```

并在上下文达到 100% 时设置：

```text
stop_reason=model_context_window_exceeded
```

本轮 context usage 递增范围：

```text
7.1305% -> 21.2757%
```

`meteringEvent` 当前只在 event type 层被识别为 `Event::Metering(())`，没有解析 payload，也没有进入响应 `usage` 或 Admin 诊断。本轮 payload 形状稳定：

```text
unit
unitPlural
usage
```

结论：

1. `contextUsageEvent` 已参与兼容响应的 token 估算。
2. `meteringEvent` 还没有产品化使用。
3. 若要提升诊断能力，可优先把 metering 聚合为日志或 Admin 只读指标，而不是直接改变 Anthropic API 响应字段。

## 7. claude-tap 对 Kiro EventStream 的支持边界

`claude-tap` 可以稳定代理 `kiro-rs -> Kiro` 流量，但当前不是 Kiro EventStream 专用解码器。

源码证据：

1. `is_capture_only_streaming_request` 只把以下情况判为 streaming：
   - Bedrock EventStream 路径。
   - Vertex `:streamRawPredict`。
   - Gemini `streamGenerateContent`。
   - 请求 body 里显式 `stream=true`。
2. Kiro `generateAssistantResponse` 路径不在这些规则内，且请求体不是 Anthropic/OpenAI 形状的 `stream=true`。
3. 非流式路径会 `await upstream_resp.read()`，再尝试 `json.loads`；失败则用 `decode("utf-8", errors="replace")` 保存为文本。
4. viewer 里的 EventStream 结构化解码函数名为 `_decode_bedrock_eventstream_events`，它识别的是 `bytes/chunk/messageStart/contentBlockStart/contentBlockDelta/...` 这类 Bedrock payload，而不是 Kiro 的 `:event-type=assistantResponseEvent/toolUseEvent/...`。

trace DB 也印证了这一点：

```text
response_keys = body, headers, status
response.sse_events = absent
record_blobs count = 0
Content-Type = application/json
```

因此本轮 `claude-tap` 能用于：

1. 确认链路是否命中本地代理。
2. 聚合端点、状态码、耗时。
3. 通过文本化二进制统计 event-type。
4. 脱敏扫描 JSON payload 形状。

但它不适合直接用于：

1. 精确 AWS EventStream 帧边界验证。
2. CRC/offset 级别解析。
3. 首 token 延迟、真实上游分片节奏分析。
4. 自动重建 Anthropic SSE content block 序列。

## 8. /v1/messages 与 /cc/v1/messages 的差异

真实 PM2 日志显示本轮走的是：

```text
POST /v1/messages model=gpt-5.6-sol stream=true
```

`kiro-rs` 两条 Anthropic 入口差异：

| 入口 | 当前策略 | 影响 |
| --- | --- | --- |
| `/v1/messages` | 立即发送初始 SSE，然后边解码 Kiro EventStream 边输出 | 延迟更低，但 `message_start.usage.input_tokens` 初始值是估算 |
| `/cc/v1/messages` | 等上游流结束，只发 ping 保活，拿到 `contextUsageEvent` 后一次性输出 | input_tokens 更准，但牺牲实时流式体验 |

如果中间经过当前 `claude-tap` forward proxy，Kiro 上游响应可能先被 `claude-tap` 读完再转发给 `kiro-rs`。这会让 `/v1/messages` 在测试时表现得更接近“上游完成后集中输出”，不能完全代表没有 tap 时的真实 streaming 体验。

## 9. 当前结论

可以排除：

1. 代理链路未命中 `claude-tap:18082`。
2. `claude-tap` 出站代理不可用。
3. `toolUseEvent` 多分片一定无法映射。
4. 工具结果回传整体失败。

当前最明确的问题是：

1. `claude-tap` 对 Kiro `generateAssistantResponse` 没有结构化 EventStream 支持。
2. `kiro-rs` 对 `reasoningContentEvent` 没有事件模型和输出策略。
3. `meteringEvent` 没有解析和诊断展示。
4. 经 `claude-tap` 测 `/v1/messages` 时，流式时序可能被 tap 的非流式采集路径改变。

## 10. 推荐下一步

### 10.1 如果目标是继续分析

优先增强采集侧，而不是先改协议转换：

1. 给 `claude-tap` 增加 Kiro EventStream 识别规则：
   - path 包含 `/generateAssistantResponse`。
   - 或响应头/响应体可识别 AWS EventStream prelude 与 `:event-type`。
2. 增加 Kiro event decoder：
   - 输出 `response.sse_events` 类似结构，但事件名保留 Kiro 原始 event type。
   - 对 payload 做字段级脱敏，只保留字段名、长度、计数、tool id hash。
3. 重新跑正式对话，验证真实 chunk 顺序和 stop 边界。

### 10.2 如果目标是优化当前项目

需要新建 OpenSpec change，建议名称：

```text
analyze-kiro-eventstream-diagnostics
```

建议范围：

1. 为 `reasoningContentEvent` 增加模型结构和解析。
2. 增加只读诊断日志或 Admin 指标：
   - event counts
   - tool_use unique count
   - tool_use stop count
   - reasoning text/signature length
   - metering usage
3. 不在第一步直接改变 Anthropic SSE 对外协议，除非先明确客户端契约。
4. 增加单元测试覆盖：
   - 多分片 toolUse，最后 stop 无 input。
   - assistant text -> tool_use -> final usage 顺序。
   - reasoningContentEvent 被计数但不泄露 signature。
   - contextUsageEvent 更正 input_tokens。

### 10.3 实现前门禁

上述任意代码改动都会触发协议/SSE/工具调用行为变化，必须先走 OpenSpec，并在实现后运行：

```text
cargo check --release --all-targets
```

若只继续输出分析文档，则不需要运行 cargo check。

## 11. 2026-08-14 实现后诊断能力

已在 `analyze-kiro-eventstream-diagnostics` 变更中把真实 trace 发现转化为 `kiro-rs` 内部诊断能力。

### 11.1 已建模的 Kiro 事件

当前 `src/kiro/model/events/` 已能区分：

| Kiro event type | 当前用途 | 对外协议影响 |
| --- | --- | --- |
| `assistantResponseEvent` | 继续转换为 Anthropic/OpenAI 文本增量 | 保持既有行为 |
| `toolUseEvent` | 继续转换为工具调用增量，并进入 tool lifecycle 诊断 | 保持既有行为 |
| `reasoningContentEvent` | 解析 `text` 与 `signature`，诊断只记录长度 | 不暴露为 `thinking_delta` / `reasoning_content` |
| `contextUsageEvent` | 继续用于 input token 回退/修正，并进入诊断 | 保持既有行为 |
| `meteringEvent` | 解析 `unit`、`unitPlural`、`usage`，仅用于诊断 | 不改变 public usage 字段 |
| unknown event | 保留类型与 payload 供内部计数，摘要不打印 payload | 请求继续处理 |

### 11.2 脱敏摘要字段

新增 request-scoped `EventStreamDiagnostics`，处理路径包括：

1. Anthropic streaming：`src/anthropic/stream.rs`
2. Anthropic non-stream：`src/anthropic/handlers.rs`
3. OpenAI Chat streaming：`src/openai/stream.rs`
4. OpenAI Chat non-stream：`src/openai/handlers.rs`
5. OpenAI Responses streaming：`src/openai/responses_stream.rs`

摘要只包含以下安全元数据：

- event type 计数与 unknown event 计数。
- unknown event type 计数与 unknown payload 总字节数。
- context usage percentage。
- metering 的单位与 usage 数值。
- reasoning event 数、`text` 字符数、`signature` 字符数。
- tool-use id hash、工具名、chunk 数、input 字符数、stop 数。
- lifecycle anomaly：missing id、missing name、missing stop、duplicate stop。

摘要不会保存或输出：

- raw prompt。
- raw tool input / output。
- raw reasoning signature。
- token、Cookie、profile ARN。

正常摘要走 `debug` 日志；发现 lifecycle anomaly 时走 `warn` 日志。

### 11.3 与真实 claude-tap trace 的对应关系

真实正式对话中观察到的稳定信号：

- `generateAssistantResponse` 返回 200。
- 同一会话包含 `assistantResponseEvent`、大量多分片 `toolUseEvent`、`reasoningContentEvent`、`contextUsageEvent`、`meteringEvent`。
- 每个实际工具调用由多个 `toolUseEvent` 分片组成，且正常生命周期应恰好一个 `stop=true`。
- `reasoningContentEvent.signature` 长度明显大于普通文本，必须按敏感值处理。

当前实现用 synthetic Frame/Event 测试覆盖这些形态，但不把真实 trace DB 作为测试 fixture，原因是：

1. `claude-tap` 当前保存的是文本化二进制响应，不是精确 AWS EventStream raw frame。
2. 真实 trace 可能含本机路径、提示词、工具参数或签名，不适合进入仓库。
3. synthetic fixture 足以稳定验证事件分类、工具分片聚合、异常检测和 public protocol 不变。

### 11.4 当前仍未改变的边界

本次变更没有把 `reasoningContentEvent` 暴露给客户端。若后续要映射为 Anthropic `thinking_delta` 或 OpenAI `reasoning_content`，需要另开 OpenSpec，至少确认：

1. 只有客户端明确启用 thinking/reasoning 时才暴露。
2. signature 如何处理，是否需要随流转发或完全丢弃。
3. 与现有 `<thinking>...</thinking>` 文本标签提取逻辑如何兼容。
4. 对 OpenAI Responses 是否存在稳定 reasoning event 契约。

`claude-tap` 仍可作为代理和采集工具，但 `kiro-rs` 的事件诊断现在不依赖它理解 Kiro EventStream。
