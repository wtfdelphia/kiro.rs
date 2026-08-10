# OpenSpec Verify Report

Change：`add-claude-opus-5-support`
日期：2026-08-10
结论：**归档条件已满足**——22/23 任务完成，仅余 7.4 归档动作本身待用户确认。

## Completeness

| 项 | 结果 |
| --- | --- |
| `openspec status --change add-claude-opus-5-support --json` | `isComplete: true`，proposal/specs/design/tasks 4/4 done |
| `openspec validate --all` | 22 passed, 0 failed |
| tasks.md | 23 项中 22 项完成；未完成仅 7.4（归档，待用户确认） |
| evidence | `bridge-plan.md`、`spec-compliance-report.md`、`openspec-verify-report.md`、`verification-before-completion.md` 均存在 |

## Correctness

3 个 Requirement 均有可核对的实现落点与测试证据：

- **归一先判具体版本**：`converter.rs` opus 分支首位插入 `base.contains("opus-5")`，位于 4-8/4-7/4-6/4-5
  之前。`test_map_model_opus_5` 覆盖基座与 thinking 变体；
  `test_map_model_opus_4_x_not_captured_by_opus_5` 覆盖四个 4-x 版本的反向断言。
- **Opus 5 上下文与 thinking**：`get_context_window_size` 白名单加 `claude-opus-5`（1M）；
  `is_adaptive_thinking` 加 `contains("opus-5")`。`opus_5_thinking_uses_adaptive_and_high_effort`
  断言 type=adaptive、budget_tokens=20000、output_config.effort=high；
  `opus_4_5_thinking_stays_enabled_without_output_config` 提供反向约束。
- **静态 fallback 含 Opus 5**：`static_fallback_models` 新增两条目，字段风格与既有 Sonnet 5 对齐；
  `static_fallback_models_has_core_ids` 扩展断言；`models_from_catalog` 零改动，四个既有 catalog
  测试全绿，证实动态路径未受影响。

实现前实测基线 `normalize_claude_model("claude-opus-5")` 为 `None`（请求被判 unmapped 拒绝），
实现后归一为 `claude-opus-5`，功能缺口已闭合。

## Coherence

- design.md 的 D1-D3 与实现一一对应，无冲突事实源。
- delta spec 的 3 个 Requirement 均落在 `model-resolution` capability 内，与该规格既有的
  「统一模型解析管线」「Catalog 透传策略」不矛盾：新增约束是对归一子步骤的细化。
- README 模型映射表的行顺序与代码判定顺序一致（`*opus-5*` 在各 `*opus*` 4-x 行之前）。
- AGENTS.md 与顶层 `spec/` 无需修改，理由见 spec-compliance-report。
- 未修改 `model-catalog` 相关规格与实现。

## 与上游实现的关系

本 change 未 cherry-pick 上游 `hank9999/kiro.rs@5ca5703`。本地试算该 cherry-pick 在
`converter.rs` 冲突，根因是三处结构性分叉（归一函数已重构为 `normalize_claude_model`、thinking 后缀
处理方式不同、`/v1/models` 已改为动态 catalog 优先）。改为按语义重新实现，详见 design.md。

值得记录的差异：上游 commit message 所述「置于 4-5 之前以避免 opus-4-5 误匹配」在本项目中并非
必要前提（本项目已剥离 thinking 后缀且 4-5 分支先于宽松判定）。本 change 仍采用前置顺序，理由是
为将来的 `opus-5-x` 留出正确判定空间，并与 sonnet 分支既有的 `sonnet-5` 前置写法一致。该顺序约束
已提升为显式规格要求。

## 未完成项

| 任务 | 状态 | 说明 |
| --- | --- | --- |
| 7.4 归档 | 待用户确认 | 需用户确认实现与证据后执行 `openspec-archive-change` |

## 归档建议

归档时将 delta 的 3 个 Requirement 同步进 `openspec/specs/model-resolution/spec.md` 的
`## Requirements` 段（追加，不覆盖既有 4 个 Requirement），change 目录移入
`openspec/changes/archive/2026-08-10-add-claude-opus-5-support/`。
