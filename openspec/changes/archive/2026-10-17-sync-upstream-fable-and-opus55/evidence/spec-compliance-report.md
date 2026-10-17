# Spec Compliance Report: sync-upstream-fable-and-opus55

审查时点：实现完成后、提交前。范围：工作区未提交 diff（3 文件，+212/-9）+
OpenSpec 工件。

## 六维结论

| 维度 | 结论 | 依据 |
| --- | --- | --- |
| Scope | PASS | diff 仅含 `src/anthropic/converter.rs`、`src/anthropic/handlers.rs`、`README.md`，与 proposal Impact 一致。未触碰凭据、认证、Admin、OpenAI 层、CI、版本号 |
| Design | PASS | 六项决策逐条落地：语义移植（无 merge 提交）、fable 置首无条件映射、opus-5.5 先于 opus-5、`extract_tool_result_content` 改签名、静态条目连字符 id、未移植 `22f36e6` 负向断言 |
| Scenarios | PASS | delta 4 个 Requirement 9 个场景全部有测试对应（见下表） |
| Project Rules | PASS | 未提交凭据；走 OpenSpec 流程；告警门禁真实运行；未引入新依赖 |
| Verification | PASS | 本会话真实运行 `cargo check --release --all-targets`（0 告警）与 `cargo test --release --locked`（877 passed，0 failed） |
| README/AGENTS Sync | PASS | README 模型映射表已同步；AGENTS.md 无需修改（无新命令/纪律/入口） |

## 场景到测试映射

| 场景 | 测试 |
| --- | --- |
| opus-5.5 连字符与点号归一 | `test_map_model_opus_5_5` |
| opus-5.5 thinking/日期变体归一 | `test_map_model_opus_5_5` |
| opus-5 不被 5.5 截获 | `test_map_model_opus_5_not_captured_by_5_5` |
| opus-4-5 不被 5/5.5 截获 | `test_map_model_opus_5_not_captured_by_5_5` |
| fable 连字符与点号归一 | `test_map_model_fable_5_1` |
| fable thinking 变体归一 | `test_map_model_fable_5_1` |
| opus-5.5 与 fable 1M 上下文 | `test_map_model_opus_5_5`、`test_map_model_fable_5_1` |
| fable adaptive thinking | `fable_thinking_uses_adaptive_and_high_effort` |
| 静态 fallback 含新条目且可归一 | `static_fallback_models_has_core_ids`（扩展）、`static_fallback_models_all_mappable` |

补充测试（规格外，缺陷修复回归）：`opus_5_5_thinking_uses_adaptive_via_opus_5_match`、
`test_tool_result_images_are_extracted`、`test_tool_result_without_images_unchanged`。

## 验证记录（本会话真实运行）

- `cargo check --release --all-targets`：改动前基线 0 告警；改动中引入 1 个
  `unused_variables` 告警（新测试未用变量），已修；改动后 0 告警，零新增
- `cargo test --release --locked`：877 passed, 0 failed, 0 ignored
  （基线 870，新增 7 个测试）
- `openspec validate --all`：30 passed, 0 failed
- `git status --short`：仅 3 个修改文件 + change 目录，无凭据、无 `.codegraph/`

## 发现项与处置

1. 实现中发现 `extract_tool_result_content` 新测试的解构变量 `_text` 未用，
   触发告警门禁，已加下划线前缀修复。
2. 静态条目 `created: 1756684800` 的上游注释 "Sep 1, 2026" 经实测为
   2025-09-01，本地注释已按真实日期写并注明上游注释有误。

## 剩余风险

- fable / opus-5.5 上游 id 未经本地真实凭据探活（proposal Assumptions 已声明）。
- 未做端到端真实上游请求验证，行为等价性依据为上游提交注释与语义对照。

## 总体状态

PASS
