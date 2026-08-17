## 1. 基线与实施桥接

- [x] 1.1 运行 `openspec-superpowers-bridge`，记录规格、风险、影响面、工具与验证命令映射
- [x] 1.2 运行 `codegraph status` 并确认索引可用于 `src/kiro/model/events/*`、`src/anthropic/stream.rs`、`src/anthropic/handlers.rs` 影响面分析
- [x] 1.3 记录实现前 `cargo check --release --all-targets` 告警基线
- [x] 1.4 确认 `config.json`、`credentials.json`、`credentials.*`、`.codegraph/` 不进入本次变更

## 2. Kiro 事件建模

- [x] 2.1 为 `reasoningContentEvent` 增加事件 payload 模型，解析 `text` 与 `signature`，字段缺失时保持容错
- [x] 2.2 为 `meteringEvent` 增加最小 payload 模型，解析 `unit`、`unitPlural`、`usage`，未知字段不导致失败
- [x] 2.3 扩展事件类型识别，让 reasoning 与 metering 不再落入 unknown 分支
- [x] 2.4 增加单元测试覆盖 reasoning、metering、unknown event 的分类与容错

## 3. 脱敏诊断聚合

- [x] 3.1 新增 request-scoped EventStream 诊断摘要结构，包含 event counts、unknown count、context usage、metering usage
- [x] 3.2 按 tool-use id 聚合 chunk count、input length、stop count、工具名与 lifecycle anomaly
- [x] 3.3 为 reasoning 只记录 text length 与 signature length，不保存原文
- [x] 3.4 在 Anthropic 与 OpenAI 处理路径接入诊断聚合，但不改变对外响应 body 或 SSE 契约
- [x] 3.5 为异常诊断输出选择低噪声日志策略，确保不打印 raw prompt、raw tool input、raw tool output 或 signature

## 4. 流式与非流式回归测试

- [x] 4.1 使用 synthetic EventStream/Frame fixture 覆盖 assistant text + 多分片 toolUse + reasoning + contextUsage + metering 的完整流
- [x] 4.2 断言每个多分片 toolUse 聚合为一个 logical tool lifecycle，并正确识别 exactly-one-stop
- [x] 4.3 断言 missing stop、duplicate stop、missing id、missing name 会进入 anomaly 但不泄露 raw input
- [x] 4.4 断言现有 Anthropic `/v1/messages` SSE 序列不新增诊断字段
- [x] 4.5 断言 OpenAI Chat Completions 与 Responses 兼容路径不新增诊断字段

## 5. 文档同步

- [x] 5.1 更新 `docs/kiro-rs-eventstream-mapping-analysis.md`，补充实现后的诊断能力与使用限制
- [x] 5.2 如新增日志或运行时诊断开关，更新 README 或相关 docs；若未新增用户入口，在完成报告说明无需同步
- [x] 5.3 更新 OpenSpec evidence，记录真实 trace 与 synthetic test 的对应关系，禁止粘贴真实 payload

## 6. 验证

- [x] 6.1 运行相关 Rust 单元测试，覆盖 Kiro event parsing、Anthropic stream、OpenAI stream/handlers 受影响路径
- [x] 6.2 运行 `openspec validate --all`
- [x] 6.3 运行 `cargo check --release --all-targets`，报告告警数并确认无新增告警
- [x] 6.4 运行敏感信息扫描，确认文档与 evidence 不含 Bearer、Cookie、profile ARN、真实 token、raw tool input、raw signature

## 7. 合规与完成门禁

- [x] 7.1 运行 `spec-compliance-check` 并修复范围、设计、场景、项目规则、验证与文档同步问题
- [x] 7.2 运行 `openspec-verify-change`，产出归档前验证报告
- [x] 7.3 运行 `verification-before-completion`，记录真实命令、告警数、文档同步、`git status --short` 与剩余风险
- [x] 7.4 用户确认后再运行 `openspec-archive-change`（用户已于 2026-08-17 确认「同步并归档所有变更」）
