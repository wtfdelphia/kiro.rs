# Spec Compliance Report

Change：`add-claude-opus-5-support`
日期：2026-08-10
范围：任务 1.1-6.3 实现后、归档前审查
总体状态：**PASS**（实现与验证完整；剩余风险为不可本地验证的上游行为假设）

## 六维审查

| 维度 | 状态 | 依据 |
| --- | --- | --- |
| Scope | PASS | `git diff --stat` 仅 `README.md`(+1)、`src/anthropic/converter.rs`(+39/-2)、`src/anthropic/handlers.rs`(+59/-1)，与 proposal 的 Impact 完全一致。未触碰非目标：`model-catalog` 缓存/刷新、`token_manager`、`stream.rs`、`src/openai/`、Cargo.toml/lock、admin-ui 均未修改 |
| Design | PASS | D1 opus-5 判定置于 opus 分支首位；D2 1M 白名单与 adaptive thinking 沿用 Sonnet 5 处理；D3 仅补静态 fallback，`models_from_catalog` 零改动 |
| Scenarios | PASS | 7 个 Scenario 全部有对应实现与测试证据，见下方映射表 |
| Project Rules | PASS | 走完整 OpenSpec 流程；`cargo check --release --all-targets` 告警 0，相对基线零新增；bridge plan 含 CodeGraph 与 rg 补盲；无真实凭据、token 或 `.codegraph/` 进入候选提交 |
| Verification | PASS | 全部命令本会话真实运行：`cargo check --release --all-targets`(0 告警)、`cargo test --release --locked`(728 passed)、定向模块测试、`openspec validate --all`(22 passed)。无 SKIPPED 项 |
| README/AGENTS Sync | PASS | README 模型映射表已加 `*opus-5*` 行，位置与代码判定顺序一致；AGENTS.md 无需修改（不改 AI 纪律、告警门禁口径或高风险矩阵）；顶层 `spec/` 无需修改（单模型映射不改变架构事实） |

## Scenario 到证据映射

| Requirement | Scenario | 证据 |
| --- | --- | --- |
| 归一先判具体版本 | opus-5 归一为上游 id | `converter.rs` opus 分支首位 `base.contains("opus-5")`；`test_map_model_opus_5` |
| 归一先判具体版本 | thinking 变体归一到同一基座 | `normalize_claude_model` 开头 `strip_suffix(THINKING_SUFFIX)`；`test_map_model_opus_5` 第二条断言 |
| 归一先判具体版本 | opus-4-5 不被截获 | `test_map_model_opus_4_x_not_captured_by_opus_5` 覆盖 4-5/4-6/4-7/4-8 四个版本 |
| Opus 5 上下文与 thinking | opus-5 使用 1M 上下文 | `get_context_window_size` 白名单新增 `claude-opus-5`；`test_map_model_opus_5` 第三条断言 |
| Opus 5 上下文与 thinking | opus-5 使用 adaptive thinking | `is_adaptive_thinking` 新增 `contains("opus-5")`；`opus_5_thinking_uses_adaptive_and_high_effort` 断言 type=adaptive、budget=20000、effort=high |
| 静态 fallback 含 Opus 5 | 空 catalog 时含 opus-5 | `static_fallback_models` 新增两条目；`static_fallback_models_has_core_ids` 扩展断言；`static_fallback_models_all_mappable` 通过（证实归一已先落地） |
| 静态 fallback 含 Opus 5 | 非空 catalog 不受影响 | `models_from_catalog` 零改动；`models_from_catalog_adds_thinking_variants`、`_passthrough_when_enabled`、`_skips_unmapped`、`_empty` 四个既有测试全绿 |

## 发现项

1. **超出 design 的调用方已由 rg 补盲确认无需改动**：`stream.rs:640`、`src/openai/stream.rs:11` 复用
   `get_context_window_size`，白名单集中在 converter 故自动生效；`token_manager.rs:1273` 用
   `contains("opus")` 判 opus 订阅等级，`claude-opus-5` 自动落入，free tier 仍被正确拦截。
   三处均无需修改，已记入 bridge plan。非阻塞。
2. **新增反向测试超出 tasks 最低要求**：除 opus-5 正例外，另加
   `opus_4_5_thinking_stays_enabled_without_output_config`，断言非 adaptive 模型不因 opus-5 分支被
   误纳入 adaptive。属规格「更具体先于更宽松」约束的反向验证，仍在范围内。
3. **`openspec/config.yaml` 为新增未跟踪文件**：由 `openspec new change` 生成，内容仅
   `schema: spec-driven`，无敏感信息，应随本 change 提交。
4. **`codegraph sync` 未执行**：索引有 pending changes（来自上一 change 的归档移动）。本 change 的
   改动点已由 rg 精确定位，不依赖索引新鲜度；不 sync 以免把索引变更混入提交。非阻塞。

## 剩余风险

- Kiro 侧 `claude-opus-5` 可用性由维护者确认，本会话无凭据探活。若实际不可用，请求会打到不存在的
  上游模型；回滚成本为四处独立改动，无状态迁移。
- opus-5 为 1M 上下文、走 adaptive thinking 这两项是沿用上游实现与 Sonnet 5 惯例的假设，未经真实
  上游响应验证。若有误，仅影响 opus-5 单模型的请求参数。
- 静态 fallback 的 `created: 1782777600` 沿用 Sonnet 5 时间戳，非 opus-5 真实发布日期；该字段仅用于
  列表展示，不参与解析或路由决策。
- 未 push、未创建 PR、未合并、未归档。
