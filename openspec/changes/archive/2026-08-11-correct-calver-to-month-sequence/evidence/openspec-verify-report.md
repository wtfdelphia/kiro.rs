# OpenSpec Verify Report

Change：`correct-calver-to-month-sequence`
日期：2026-08-11
结论：**归档条件已满足**——32/34 任务完成，余 7.4（归档动作）与 7.5（归档时同步 Purpose 段）。

## Completeness

| 项 | 结果 |
| --- | --- |
| `openspec status --change correct-calver-to-month-sequence --json` | `isComplete: true`，proposal/specs/design/tasks 4/4 done |
| `openspec validate --all` | 22 passed, 0 failed |
| tasks.md | 34 项中 32 项完成；余 7.4 归档、7.5 归档时同步 Purpose |
| evidence | `bridge-plan.md`、`ci-red-path.md`、`ci-green-path.md`、`spec-compliance-report.md`、`openspec-verify-report.md`、`verification-before-completion.md` |

## Correctness

### MODIFIED：正式版本必须具有唯一且一致的发布身份

格式由 `vYYYY.M.D`（日历日）修正为 `vYYYY.MM.MICRO`（当月发布序号）。实现落点：

- `CALVER_TAG` 序号段 `[1-9]\d?` → `[1-9]\d*`：解除两位上界，仍禁 0 与前导零。
- 移除 `dt.date()`，显式补 `1 <= month <= 12`。**这是本 change 最容易遗漏的一处**：原实现依赖
  `dt.date()` 作为副作用拒绝 13-99 月，直接删除会让 `v2026.99.1` 通过。
- 错误文案改为 `vYYYY.MM.MICRO`，非法月份独立可诊断（`invalid month N; month must be 1-12`）。
- 移除不再使用的 `datetime` 导入。

同日单版本限制已移除，规格改为「同一年月内序号 MUST 严格递增且 MUST NOT 复用」。

### ADDED：发布序号必须由维护者显式决定

门禁不推导也不强制「序号等于上一个加一」。理由：比较历史 tag 会让判定依赖远端 tag 列表完整性
（浅克隆、`fetch-tags: false` 均会误判），而跳号无害，唯一性由 git 保证（同名 tag 无法重复创建）。

### 双向 CI 证据

| 路径 | tag | version-gate | 产物 |
| --- | --- | --- | --- |
| 红 | `v2026.8.900`（三位序号 + Cargo 失配） | failure | `build`/`release`/`manifest` 全 skipped；无 Release、GHCR 无镜像；临时 tag 已删 |
| 绿 | `v2026.8.11`（身份一致） | success | 7 腿产物 + Release（7 资产）+ 双架构镜像 + manifest + `latest` |

红路径的证据设计值得记录：三位序号在旧门禁下必被格式判定拒绝，实际报 Cargo 失配，故单次实验同时
证明「上界解除生效」与「一致性判定未被破坏」。

## Coherence

- design.md 的 D1-D4 与实现一一对应。
- delta 的 MODIFIED 完整覆盖主规格该 Requirement 的 3 个原场景（建 change 阶段被
  `openspec validate` 拦到一次：改场景标题等于删场景，已修正），另加 2 个新场景。
- `release-version-governance` 的其余 6 个 Requirement（main 落点、人工发布约束、运行时与镜像
  可观测性、非正式构建追溯、MSRV、admin-ui 策略）未被本 change 触碰。
- 文档层次一致：README 给操作口径，`docs/version-governance-optimization-design.md` 保留原判断并
  标注推翻依据与成因，`docs/release-version-governance-remaining-verification.md` 加纠正说明。
- `.github/workflows/` 零改动，故上一轮的 gate 接线证据继续有效，两轮证据互补不重复。

## 未完成项

| 任务 | 状态 | 说明 |
| --- | --- | --- |
| 7.4 归档 | 待用户确认 | 执行 `openspec-archive-change` |
| 7.5 Purpose 段同步 | 归档时执行 | delta 只含 Requirements 段，主规格 `## Purpose` 的 `vYYYY.M.D` 需手动修正，否则遗留旧格式 |

## 归档建议

同步 delta 到 `openspec/specs/release-version-governance/spec.md`：MODIFIED 替换「正式版本必须具有
唯一且一致的发布身份」整块（含 5 个场景），ADDED 追加「发布序号必须由维护者显式决定」。**同时手动
修正 `## Purpose` 段的 `vYYYY.M.D` 为 `vYYYY.MM.MICRO`**。change 目录移入
`openspec/changes/archive/2026-08-11-correct-calver-to-month-sequence/`。
