# Bridge Plan

Change：`add-claude-opus-5-support`
日期：2026-08-10
分支：`dev`（`b746607`）
状态：`openspec status` → `isComplete: true`，`mode: repo-local`，非 blocked

## 范围与非目标

范围：让 `claude-opus-5` 走通四个面——归一、1M 上下文、adaptive thinking、`/v1/models` 静态 fallback。

非目标：不改 `model-catalog` 缓存/刷新/预热；不把模型能力配置化；不把 `contains` 匹配重构为结构化
版本解析；不做真实凭据探活。

关键设计决策：D1 opus-5 判定置于 opus 分支最前（沿用 sonnet 分支的 `sonnet-5` 前置惯例，并为将来的
`opus-5-1` 留出正确顺序）；D2 上下文与 thinking 沿用 Sonnet 5 处理；D3 只补静态 fallback，动态
catalog 路径不动。

## 高风险项

| 风险 | 处置 |
| --- | --- |
| Kiro 侧 opus-5 实际不可用 | 维护者已确认可用；回滚成本为四处独立改动，无状态迁移 |
| 1M 上下文 / adaptive thinking 假设有误 | 仅影响 opus-5 单模型请求参数；记入 proposal Assumptions |
| `opus-4-5` 被 opus-5 分支误截获 | 由 2.3 回归单测固定；本项目已剥离 thinking 后缀，`4-5` 分支先于 opus-5 之外的宽松判定 |
| 静态 fallback 新增条目破坏既有断言 | `static_fallback_models_all_mappable` 要求每个 id 可映射，故必须先完成 2.1 归一再加 4.1 条目 |

## CodeGraph 证据

| 命令 | 结论 |
| --- | --- |
| `codegraph status` | 索引存在但有 pending changes（Added 3 / Modified 3 / Removed 1，来自上一个 change 的归档移动）。本 change 不依赖索引新鲜度，改动点已由 rg 精确定位，故不执行 `codegraph sync` 以免把索引变更混入本次提交 |

## rg / 源码补盲

`rg` 补出三处 design.md 未列出的调用方与影响面：

| 位置 | 事实 | 是否需改动 |
| --- | --- | --- |
| `src/anthropic/stream.rs:640` | 流式路径调用 `get_context_window_size(&self.model)` | 否。白名单集中在 converter，改一处即全局生效 |
| `src/openai/stream.rs:11` | OpenAI 兼容层复用 `get_context_window_size` | 否。同上，自动受益 |
| `src/kiro/token_manager.rs:1273` | 凭据选择用 `contains("opus")` 判断是否需要 opus 订阅等级 | 否。`claude-opus-5` 自动落入该判定，free tier 仍被正确拦截 |
| `src/anthropic/handlers.rs:1180` | `static_fallback_models_all_mappable` 断言静态表每个 id 必须 `map_model` 成功 | 否，但**约束实现顺序**：必须先加归一分支，否则新增条目会让该测试失败 |

补盲结论：本 change 的四处改动足以覆盖全部消费方，无需触碰 stream、openai 或 token_manager。

配置 / Docker / workflow / example 凭据路径：`rg` 确认无 opus 版本硬编码，无需同步。

## 任务到执行步骤表

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 2.1 | `converter.rs` opus 分支首位插入 `base.contains("opus-5")` | `cargo test` converter 模块 | 插入位置若导致 `4-x` 分支不可达则停 |
| 2.2 / 2.3 | 新增 `test_map_model_opus_5`，含 opus-5、thinking 变体、`opus-4-5-20251101` 反例 | 三条断言全绿 | 反例失败即说明顺序错误，停 |
| 3.1 / 3.2 | 1M 白名单加 `claude-opus-5`，更新注释 | `get_context_window_size("claude-opus-5") == 1_000_000` | — |
| 3.3 / 3.4 | `is_adaptive_thinking` 加 `opus-5`；单测断言 adaptive + high effort | handlers 模块测试 | — |
| 4.1 / 4.2 | 静态 fallback 加两条目，扩展 `has_core_ids` 断言 | `all_mappable` 与 `has_core_ids` 均绿 | `all_mappable` 失败说明 2.1 未完成，停 |
| 4.3 | 不改 `models_from_catalog`，跑既有测试确认 | `models_from_catalog_*` 测试绿 | — |
| 5.1 | README 映射表加 `*opus-5*` 行，置于 `*opus*` 各 4-x 行之前 | 人工核对表格顺序 | — |
| 6.1 / 6.2 | 准绳与全量测试 | 零新增告警；测试无回归 | 告警数高于基线即视为未完成 |

## 必跑验证

- `cargo check --release --all-targets`（准绳，须零新增告警）
- `cargo test --release --locked`（converter 与 handlers 模块无回归）
- `openspec validate --all`
- `git status --short --untracked-files=all`（防敏感文件与 `.codegraph/` 混入）

## 实现前基线

| 项 | 值 |
| --- | --- |
| `cargo check --release --all-targets` 告警数 | **0**（本会话多次确认） |
| `normalize_claude_model("claude-opus-5")` | `None`（探针实测，请求会被判 unmapped 拒绝） |
| `normalize_claude_model("claude-opus-4-5-20251101")` | `Some("claude-opus-4.5")` |
| 工作树 | 干净，仅本 change 目录与 `openspec/config.yaml` 未跟踪 |

`openspec/config.yaml` 为 `openspec new change` 生成的项目配置，内容仅 `schema: spec-driven`，
无敏感信息，应随本 change 提交。

## README / AGENTS / spec 同步判断

| 入口 | 判断 |
| --- | --- |
| `README.md` | **需同步**：模型映射表加一行（任务 5.1） |
| `AGENTS.md` | 无需修改。本 change 不改变 AI 纪律、告警门禁口径或高风险矩阵 |
| `spec/` | 无需修改。顶层 `spec/` 是长期需求/设计/结构，单个模型映射不改变架构事实 |
| `openspec/specs/model-resolution/` | 归档时同步 delta；本次变更过程只写在 change 目录内 |
| `docs/tooling-sources.md` | 无需修改，与本 change 无关 |

## 停止条件

- `opus-4-5` 反例失败（顺序错误）。
- `static_fallback_models_all_mappable` 失败（归一未先落地）。
- 告警数高于基线 0。
- 发现 opus-5 需要 token_manager 或 catalog 侧额外改动（超出当前规格范围）。
