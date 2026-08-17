# Bridge Plan: analyze-kiro-eventstream-diagnostics

日期：2026-08-14

## 范围

本变更实现 Kiro `generateAssistantResponse` EventStream 的安全诊断能力：

- 识别 `reasoningContentEvent` 与 `meteringEvent`，避免继续落入 unknown。
- 为每次生成响应生成 request-scoped 脱敏诊断摘要。
- 聚合多分片 `toolUseEvent` 生命周期，检测 missing/duplicate stop 等异常。
- 不改变 Anthropic/OpenAI 对外响应 body 或 SSE 契约。

## 非目标

- 不把 `reasoningContentEvent` 映射为 Anthropic `thinking_delta`。
- 不新增 Admin API/UI。
- 不持久化真实 trace 或 raw payload。
- 不依赖 `claude-tap` 增强。

## 关键设计决策

| 决策 | 结论 |
| --- | --- |
| reasoning/metering | 先建模与诊断，不改变 public protocol |
| 敏感字段 | 只记录长度、计数、异常标记，不记录原文 |
| 日志 | 只输出摘要，异常可 warn/debug，禁止 raw prompt/tool/signature |
| 测试 | 使用 synthetic EventStream/Frame，不依赖真实 Kiro 登录态 |

## 高风险项

| 风险 | 缓解 |
| --- | --- |
| Event enum 影响面宽，Anthropic/OpenAI/debug/admin 测试路径都会编译受影响 | 保持新增事件旁路处理，默认不改变响应；运行相关单测与 `cargo check --release --all-targets` |
| 诊断日志泄露工具输入或 reasoning signature | 诊断结构只存长度；新增测试断言敏感字符串不出现在摘要 JSON |
| metering payload 形状变化 | 解析字段容错，未知字段不失败 |
| `claude-tap` trace 是文本化二进制，不能证明精确帧边界 | 代码测试使用 synthetic frame，不使用 trace DB 作为原始帧 fixture |

## CodeGraph 证据

命令：

```text
codegraph status
codegraph impact StreamContext
codegraph impact Event
codegraph impact EventStreamDecoder
```

结论：

- `codegraph status`：索引可用，146 files / 2,869 nodes / 8,385 edges；因为新增 OpenSpec 文件，提示 pending added 1 file。
- `StreamContext` 影响面集中在 `src/anthropic/stream.rs`、`src/anthropic/handlers.rs` 及已有 stream tests。
- `Event` 影响面覆盖 `src/kiro/model/events/base.rs`、Anthropic/OpenAI stream handlers、debug、admin service 的编译路径。
- `EventStreamDecoder` 影响面覆盖 parser 自身、Anthropic/OpenAI streaming/non-streaming 聚合路径。

## rg / 源码补盲

已检查：

- `AGENTS.md`：协议/SSE 变更必须 OpenSpec；代码改动必须报告 `cargo check --release --all-targets` 告警数。
- `spec/design.md`：关键数据流为 client -> Anthropic/OpenAI 兼容层 -> Kiro provider -> parser -> stream/converter。
- `src/kiro/model/events/base.rs`：当前仅识别 assistant/tool/metering/context/unknown，其中 metering 无 payload。
- `src/anthropic/stream.rs`：已有 toolUse block index 复用和 stop 关闭逻辑。
- `src/anthropic/handlers.rs`、`src/openai/handlers.rs`：非流式聚合路径也会读取 Kiro events。
- 凭据路径规则：不得提交 `config.json`、`credentials.json`、`credentials.*`、`.codegraph/`。

## 实现前基线

命令：

```text
cargo check --release --all-targets
```

结果：

```text
Finished `release` profile [optimized] target(s) in 3.52s
```

告警数：0。

## 任务到执行步骤

| tasks | 执行步骤 | 验证 |
| --- | --- | --- |
| 1.x | 已完成 bridge、CodeGraph、基线、git status 检查 | 本文件、cargo check 基线 |
| 2.x | 新增 reasoning/metering event 模型，扩展 EventType/Event | event 模块单测 |
| 3.x | 新增脱敏诊断摘要与 tool lifecycle 聚合，并接入 Anthropic/OpenAI 处理路径 | 摘要单测、敏感字符串断言 |
| 4.x | 构造 synthetic frame/事件测试流式与非流式路径 | `cargo test` 相关模块 |
| 5.x | 更新 docs 与 evidence | 文档扫描 |
| 6.x | 运行验证命令 | `openspec validate --all`、`cargo check --release --all-targets` |
| 7.x | 运行合规与完成门禁 | spec-compliance、verify、verification evidence |

## 必跑验证

实现后至少运行：

```text
cargo test --release kiro::model::events
cargo test --release anthropic::stream
cargo test --release openai::stream
cargo test --release openai::responses_stream
openspec validate --all
cargo check --release --all-targets
git status --short
```

如实际改动影响 `handlers.rs` 聚合逻辑，再追加：

```text
cargo test --release anthropic::handlers
cargo test --release openai::handlers
```

## 文档同步判断

| 文档 | 是否需要 | 理由 |
| --- | --- | --- |
| README | 暂不需要 | 首版不新增用户可见 API/启动命令 |
| AGENTS | 不需要 | 项目纪律未变 |
| spec/ | 不直接改 | 通过 OpenSpec delta 归档后同步 |
| openspec/specs | 不直接改 | 通过本 change 的 delta 管理 |
| docs/kiro-rs-eventstream-mapping-analysis.md | 需要 | 实现后补充诊断能力与限制 |

## 停止条件

- 发现需要改变 Anthropic/OpenAI public response 契约。
- 诊断实现需要输出 raw prompt、raw tool input、raw tool output、Cookie、token、profile ARN 或 reasoning signature。
- `cargo check --release --all-targets` 出现新增告警。
- `openspec validate --all` 失败且无法在本变更范围内修复。
- `git status --short` 出现 `config.json`、`credentials.json`、`credentials.*`、`.codegraph/` 候选变更。
