# Bridge Plan

Change：`correct-calver-to-month-sequence`
日期：2026-08-10
分支：`dev`（`9d4bdba`）
状态：`openspec status` → `isComplete: true`、`mode: repo-local`；`openspec instructions apply` → `state: ready`（非 blocked）

## 范围与非目标

范围：把正式版本第三段语义由「日历日」修正为「当月发布序号」，恢复同月多版本发布能力。改动集中在
`scripts/check_release_version.py` 的格式判定、其测试、长期规格与三处文档。

非目标：不改写历史 tag 与已发布 Release；不改 `version-gate.yaml` 接线与 caller 的 `needs`；
不改 Docker、资产命名、OCI label 与任何 Rust 源码；不引入自动序号推导；不转 SemVer；不加第四段。

关键设计决策：D1 第三段为当月序号，移除 `dt.date()` 并显式补月份 1-12 校验、序号段放宽上界；
D2 唯一性靠序号单调递增 + git 同名 tag 天然去重，门禁不比较历史 tag；D3 不追溯修正历史 tag；
D4 其余身份约束全部保留。

## 高风险项

| 风险 | 处置 |
| --- | --- |
| 移除 `dt.date()` 后月份失去隐式校验，`v2026.99.1` 可能通过 | **本 change 最关键风险**。原实现靠 `dt.date()` 顺带拒绝非法月份；必须显式补 `1 <= month <= 12`，并加 `v2026.13.1`、`v2026.0.1` 反例（tasks 3.3） |
| 序号段放宽为 `[1-9]\d*` 后正则更宽松 | 仍禁 0 与前导零；配合 Cargo 一致性，畸形序号无法单独通过（tasks 3.4/3.5） |
| `datetime` 导入变为未使用，触发新告警 | 该文件是 Python，不进 `cargo check`；但仍应清理以免 lint 噪声（tasks 2.4）。注意 `dt` 是否被文件内其他位置使用，删前先 rg |
| 规格 MODIFIED 整块替换导致原场景丢失 | 已在建 change 阶段被 `openspec validate` 拦到一次（改场景标题＝删场景）；已核对 MODIFIED 覆盖主规格全部 3 个原场景 |
| 上一轮红/绿 CI 证据失效 | 门禁接线未变但判定规则已变，必须重新取证（tasks 6.1/6.3） |

## CodeGraph 证据

| 命令 | 结论 |
| --- | --- |
| `codegraph status` | 索引存在，有 pending changes（Added 4 / Modified 5 / Removed 1，来自前两个 change 的归档与提交）。本 change 不改 Rust 源码，不依赖索引新鲜度；不执行 `sync` 以免把索引变更混入提交 |
| `rg "CARGO_PKG_VERSION"` | 仅 `src/main.rs:35` 一处（启动版本日志）。它读编译期常量，随 `Cargo.toml` 自动跟随，本 change 不需改动 Rust 代码 |

## rg / 源码补盲

补出 4 处 design.md 未完全列出的影响面：

| 位置 | 事实 | 是否需改动 |
| --- | --- | --- |
| `.github/workflows/build.yaml:9`、`docker-build.yaml:8` | tag 触发器为通配 `'v*'`，不含任何格式假设 | **否**。这是本 change 爆炸半径小的关键原因 |
| `scripts/tests/test_check_release_version.py:75` | `test_rejects_leading_zero_month_or_day` 名称含 "day"，语义过期 | **是**，需改名为 month_or_micro（tasks 3.4 顺带处理） |
| `docs/release-version-governance-remaining-verification.md:25-26` | 「严格 CalVer 不补零」「同一自然日只能有一个正式版本，改期就改日期」表述过期 | **是**，需同步（tasks 4.4 之外的增补项，见下） |
| 同上 `:379-427` | 该手册用 `v2026.8.11` 作红路径失配 tag 示例 | **是**，需换号。`v2026.8.11` 现在是本 change 的绿路径目标版本，若沿用会造成指引冲突 |
| `docker-compose.yml`、`scripts/git-hooks/pre-push` | 无版本格式硬编码 | 否 |

补盲结论：新增两项文档同步（该手册的格式表述与红路径示例号），已在 tasks 4.4 范围内一并处理，无需
改动 workflow 或 Rust 源码。

## 任务到执行步骤表

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 2.1 | 正则序号段 `[1-9]\d?` → `[1-9]\d*` | `v2026.8.100` 通过、`v2026.8.011` 被拒 | 前导零放行则停 |
| 2.2 | 删 `dt.date()`，加 `1 <= month <= 12` | `v2026.13.1` 被拒且文案指向月份 | 非法月份放行则停 |
| 2.3 | 文案 `vYYYY.M.D` → `vYYYY.MM.MICRO` | 测试断言更新后全绿 | — |
| 2.4 | rg 确认 `dt` 无其他使用后移除导入 | `python -m unittest` 通过 | 仍有引用则保留导入 |
| 3.1 | `test_rejects_invalid_calendar_date` 改为正例（`v2026.2.30` 通过） | 断言反转后绿 | — |
| 3.2 | 新增同月多版本正例 | `v2026.8.11`、`v2026.8.12` 均通过 | 任一被拒即说明同月限制未清除，停 |
| 3.3 / 3.4 / 3.5 | 月份与序号边界反例、三位序号正例 | 全绿 | — |
| 3.6 | 跑全模块确认既有 28 例未回归 | 用例数不低于基线 | 既有用例红则停 |
| 4.1-4.4 | README 三处 + 设计文档修订记录 + 剩余验证手册两处 | 人工核对无 `vYYYY.M.D` 残留 | — |
| 5.1-5.4 | 准绳、门禁测试、`v2026.8.11` 本地演练、确认 workflow 未改 | 零新增告警；`git diff .github/` 为空 | workflow 被误改则停 |
| 6.1-6.4 | 维护者红路径 + Cargo 升版 PR + 绿路径 `v2026.8.11` | 两条 workflow run 证据 | 红路径产物 job 非 skipped 则停 |

## 必跑验证

- `cargo check --release --all-targets`（准绳；本 change 不改 Rust，预期仍为 0 告警）
- `python -m unittest scripts.tests.test_check_release_version`（用例数须 ≥ 基线 28）
- `python -m unittest scripts.tests.test_release_governance_files scripts.tests.test_release_workflow_graph`（确认未破坏 gate 接线断言）
- `python scripts/check_release_version.py validate --tag v2026.8.11 ...`（本地演练）
- `openspec validate --all`
- `git diff --stat .github/`（须为空，证明接线未动）
- `git status --short --untracked-files=all`

## 实现前基线

| 项 | 值 |
| --- | --- |
| `cargo check --release --all-targets` 告警数 | **0** |
| `test_check_release_version` 用例数 | **28** |
| 门禁三模块合计 | **40** |
| 当前 `Cargo.toml` 版本 | `2026.8.10` |
| 已发布最新正式 tag | `v2026.8.10`（指向 `674c2cc`，**不含** Opus 5） |
| Opus 5 提交 | `9d4bdba`，晚于上述 tag，故需新正式版本 |
| 历史 tag 总数 | 29（其中 `2025.12.1`–`.7` 四天内发布、`2026.2.1`–`.3` 同日发布，证实序号语义） |
| 工作树 | 干净，仅本 change 目录未跟踪 |

## README / AGENTS / spec 同步判断

| 入口 | 判断 |
| --- | --- |
| `README.md` | **需同步**：版本约定格式、同日限制表述、人工发布的 `vYYYY.M.D`、发版清单增补序号查法（tasks 4.1-4.3） |
| `docs/version-governance-optimization-design.md` | **需同步**：增补修订记录，保留原判断与纠正依据（tasks 4.4） |
| `docs/release-version-governance-remaining-verification.md` | **需同步**（补盲新增）：格式表述与红路径示例号 |
| `AGENTS.md` | 无需修改。高风险矩阵已含「Docker / 发布」与「CI / 告警门禁」，本 change 不改变 AI 纪律或验证口径 |
| 顶层 `spec/` | 无需修改。`spec/design.md` 只涉及架构、模块边界与告警门禁，不含版本发布约定 |
| `openspec/specs/release-version-governance/` | 归档时同步 delta（MODIFIED 1 项、ADDED 1 项）；变更过程只写在 change 目录内 |

## 停止条件

- 非法月份（`v2026.13.1`）或序号 0、前导零被放行。
- 既有 28 个门禁用例出现回归。
- `.github/workflows/` 出现非预期改动。
- 红路径中任何产物 job 不是 skipped。
- 绿路径发布出的版本不含 Opus 5 提交 `9d4bdba`。
