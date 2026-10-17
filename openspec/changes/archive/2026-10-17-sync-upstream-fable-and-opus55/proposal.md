## Why

dev 分支落后上游 `hank9999/kiro.rs` master 7 个提交，其中除已被 `9d4bdba` 语义覆盖的
Opus 5 外，剩余净增量为三块：Opus 5.5 支持（`9741858`）、tool_result 图片丢失修复 +
Claude Fable 5.1 支持（`fcb6e7a`）、过时模型测试更新（`22f36e6`）。客户端已可用
`claude-opus-5.5` 与 `claude-fable-5.1`，但本项目归一逻辑无对应分支，请求会被判
unmapped 拒绝；tool_result 中的 image 块目前被静默丢弃，多轮工具链场景（截图、读图）
模型看不到图片。

两侧代码在分叉点 `606c6bc` 后结构性分叉：本项目把归一重构为
`normalize_claude_model` + `resolve_model`（含别名、catalog 透传、thinking 后缀），
并把 `/v1/models` 改为 catalog 优先 + 静态 fallback；上游仍是旧式 `map_model` contains
链。直接 `git merge` 在 `converter.rs`、`handlers.rs`、`README.md`、`Cargo.toml/lock`
五个文件冲突（已实测），标记冲突无法靠机械解法收敛，需按语义把上游增量移植进本项目的
解析架构，与 `9d4bdba` 处理 Opus 5 的方式一致。

## What Changes

- `normalize_claude_model`：新增 `fable` 分支（无条件映射 `claude-fable-5.1`，置于
  sonnet/opus 之前，与上游一致）；opus 分支新增 `opus-5.5`/`opus-5-5` 判定，置于
  `opus-5` 之前（`contains("opus-5")` 会同时匹配 5.5，顺序错了会被吞）。
- `get_context_window_size`：`claude-fable-5.1` 与 `claude-opus-5.5` 纳入 1M 上下文白名单。
- `override_thinking_from_model_name`：`fable` 纳入 adaptive thinking 判定（与上游
  `handlers.rs` 改动对齐；opus-5.5 沿用既有 `opus-5` 包含匹配）。
- `/v1/models` 静态 fallback：新增 `claude-opus-5.5`、`claude-opus-5.5-thinking`、
  `claude-fable-5.1`、`claude-fable-5.1-thinking` 条目；动态 catalog 路径不动。
- 修复 `extract_tool_result_content` 静默丢弃 image 块：返回值改为
  `(String, Vec<KiroImage>)`，提取 `type=image` 的 base64 块，由调用方合并进
  `UserInputMessage.images` 上送。
- 测试：fable/opus-5.5 归一、1M、adaptive thinking、静态 fallback 覆盖；
  tool_result 图片提取回归测试；对齐上游 `22f36e6` 的过时模型断言更新。
- README 模型映射表补 fable 与 opus-5.5 两行。
- 不引入上游版本号（保留 `2026.9.1` 版本治理）、不引入上游 `quality-gates.yaml`
  （本项目已有 warning-gate 双防线，上游该文件与既有流水线关系待另行评估，不在本变更内）。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `model-resolution`: 归一规则扩展覆盖 fable 与 opus-5.5；1M 上下文与 adaptive thinking
  集合同步扩展；静态模型 fallback 要求补充新条目。

tool_result 图片修复属 Anthropic 请求转换的缺陷修复，既有规格中无对应 capability
条款，不为满足校验而新造需求，以代码修复加回归测试承载。

## Impact

- 代码：`src/anthropic/converter.rs`、`src/anthropic/handlers.rs` 及其测试。
- 文档：`README.md` 模型映射表。
- 规格：`model-resolution` delta；归档后同步主规格。
- 不影响：凭据、认证、Admin API、Docker/发布、配置 schema、admin-ui、
  `model-catalog` 缓存机制、OpenAI 兼容层。

## Assumptions

- Kiro 侧 `claude-opus-5.5` 与 `claude-fable-5.1` 上游 id 可用（上游提交含实测依据：
  fable 点号形式实测通过，连字符形式被拒）。
- fable 与 Sonnet 5 / Opus 5.5 同为 1M 上下文、同走 adaptive thinking，沿用上游实现，
  未经本地真实凭据探活。
- 静态 fallback 的 `created` 时间戳沿用上游取值（opus-5.5: 1790035200，
  fable: 1756684800），仅用于展示。

## Success Criteria

- `claude-opus-5.5` / `claude-opus-5-5` / `claude-fable-5.1` / `claude-fable-5-1` 及
  thinking 变体均归一到正确上游 id；`claude-opus-5` 不被 5.5 分支截获，
  `claude-opus-4-5-20251101` 不被 5/5.5 截获。
- 新模型上下文窗口返回 1_000_000；fable 请求走 adaptive thinking。
- tool_result 中的 image 块被提取并随 UserInputMessage.images 上送，不再丢弃。
- `cargo check --release --all-targets` 零新增告警；相关测试全部通过。
