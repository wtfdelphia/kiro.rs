## Context

dev 与上游 `hank9999/kiro.rs` master 在 `606c6bc` 分叉。上游 7 个提交里，
`5ca5703`（Opus 5）已由 `9d4bdba` 按语义重实现并归档；`e09625c`/`cbcee8a` 是上游自有
CI workflow，与本项目 warning-gate 双防线无接线需求；`9da05bc` 是上游版本号，本项目走
独立版本治理（`2026.9.1`）。剩余净增量为 `9741858`（Opus 5.5）、`fcb6e7a`（tool_result
图片修复 + Fable 5.1）、`22f36e6`（测试更新）。

实测 `git merge upstream/master` 冲突五个文件：`src/anthropic/converter.rs`（4 处）、
`src/anthropic/handlers.rs`（1 处）、`README.md`、`Cargo.toml`、`Cargo.lock`。冲突根因
与 `9d4bdba` 记录的一致：归一函数已重构（`normalize_claude_model` + `resolve_model`
管线，含别名、catalog 透传、ResolveKind），thinking 后缀处理不同，`/v1/models` 已改
catalog 优先 + `static_fallback_models`。上游代码基于旧式 `map_model` contains 链，
不能机械套用。

## Goals / Non-Goals

**Goals:**

- 把上游三块净增量按语义移植进 dev 的解析架构，行为与上游等价。
- 修复 tool_result 图片静默丢弃。
- 保持 `cargo check --release --all-targets` 零告警，测试全绿。

**Non-Goals:**

- 不执行真实 `git merge`：冲突无法机械收敛，且会把上游版本号与无关 CI 文件带进来。
  采用语义移植，冲突文件全部以 dev 结构为基底手写合入。
- 不引入上游 `quality-gates.yaml`；其与本项目双防线的关系另行评估。
- 不改凭据、路由、Admin、OpenAI 兼容层。
- 不移植上游 `fcb6e7a` 附带的 README「常见排错」章节：dev README 已有等价的
  profileArn 文档（见 README 423-499 行范围），不重复引入上游版本。

## Decisions

### 决策 1：语义移植而非 git merge / cherry-pick

cherry-pick 三笔上游提交会命中与整仓 merge 相同的结构冲突，且上游提交粒度与 dev
解析架构对不上（`map_model` 不存在于 dev）。merge 还会把上游版本号、上游 CI 文件、
上游 README 格式一并带入，需要逐项回退，成本高于直接移植。按 `9d4bdba` 先例，以
dev 架构为基底，按上游语义逐点实现。

### 决策 2：fable 分支放在 sonnet/opus 之前，无条件映射

与上游一致。Kiro 仅上线 5.1 一个 Fable 版本，无版本分支需求；上游 id 为点号形式
`claude-fable-5.1`，客户端连字符拼写 `claude-fable-5-1` 也归一到点号形式（实测连字符
形式被上游拒绝，见 `fcb6e7a` 提交注释）。

### 决策 3：opus-5.5 判定先于 opus-5

`contains("opus-5")` 同时匹配 `opus-5.5` 与 `opus-5-5`，若 5 分支在前会把 5.5 请求吞掉。
上游用 `opus-5-5`/`opus-5.5` 双拼写先判，本移植保持一致。既有「具体版本先于宽松版本」
规格约束（2026-08-10 归档）已覆盖这一模式，本次是同一约束的第二个实例。

### 决策 4：tool_result 图片修复直接改签名

`extract_tool_result_content` 返回值从 `String` 改为 `(String, Vec<KiroImage>)`，调用方
`process_message_content` 把提取的图片合并进既有 `images` 返回。`KiroImage::from_base64`
与 `get_image_format` 在 dev 已存在，直接复用。不引入新类型。

### 决策 5：静态 fallback 与测试基线

`static_fallback_models` 新增四个条目（opus-5.5、opus-5.5-thinking、fable-5.1、
fable-5.1-thinking），`created` 时间戳沿用上游取值（1790035200）。测试侧：
- 新增归一、1M、adaptive thinking、静态 fallback 测试（对齐上游 `22f36e6` 的断言风格）。
- dev 侧 `test_map_model_unsupported` 等既有测试中若含 `gpt-4` 类断言，需与
  `resolve_model` 别名管线区分：`map_model`/`normalize_claude_model` 对 gpt-4 返回 None
  是正确行为（别名走 `resolve_model`），不按上游测试改写语义，只更新确有变化的断言。

### 决策 6：不移植上游 `22f36e6` 的负向断言

上游 `22f36e6` 把 `claude-sonnet-4-20250514`、`claude-3-5-sonnet-20241022`、
`claude-opus-4`、`claude-opus-4-20250514` 断言为 `map_model` 不支持。但 dev 的
`normalize_claude_model` 在 `385156a` 重构时有意保留宽松兼容分支：sonnet 分支
`3-5`/`3.5`/`sonnet-4` 归一到 4.5，opus 分支 `opus-4` 归一到 4.5（已用本地试算验证：
上述四个输入在 dev 均返回 Some）。这些宽松分支是 dev 与上游的有意分叉，移植上游负向
断言会要求收紧归一行为，超出本 change 范围。`22f36e6` 中其余内容（把测试请求里的
过时模型名换成现行模型名）属测试卫生，随 5.1 的正向断言一并处理即可，不单列任务。

## Risks / Trade-offs

- [fable/opus-5.5 上游 id 实际不可用] → 映射会让请求打到不存在的上游模型。缓解：上游
  提交含实测依据；若本地探活失败，回滚本 change 即可，影响面仅限归一分支。
- [adaptive thinking 假设未经本地验证] → fable 沿用 Sonnet 5 模式，与上游一致；若上游
  后续调整，本 change 行为随之过时。缓解：测试锁定当前行为，变更时走规格流程。
- [语义移植遗漏上游细节] → 逐提交对照实现，测试覆盖每个新分支的正反向断言；
  `cargo test` 全量回归兜底。
- [上游 `22f36e6` 测试更新与 dev 语义不一致] → 见决策 6：负向断言不移植，只移植
  正向断言；测试请求中的过时模型名替换随 5.1 处理。

## Open Questions

无。上游行为语义明确，决策点均已收敛。
