# Spec Compliance Report: clean-release-build-warnings

- 日期：2026-08-04
- 运行：spec-compliance-check skill（Codex，实现后/审查前）
- 审查范围：工作树 diff（12 个 src 文件 + 5 个规范入口 + docs + 本 change 工件），基线 `dev` @ `e8c5eda`
- 总体状态：**PASS**

## 六维审查

| 维度 | 检查点 | 结果 | 证据 |
|---|---|---|---|
| Scope | 改动只覆盖 proposal/design/tasks 范围；未触碰非目标 | ✓ | `git diff --stat src` 恰为 proposal 所列 12 个文件；无 CI / admin-ui / Cargo.toml / Cargo.lock / 逻辑重构改动 |
| Design | 实现符合 change design.md 与长期 spec/design.md | ✓ | A 类 6 项仅加 `#[cfg(test)]`（`default_resolution_policy` 与 `convert_request` 同批补丁）；D 类双层 `super::super::converter::map_model`；B 类 6 项连同注释整体删除；E 类 variant 级 allow 并引用生效 spec |
| Scenarios | 每个 Requirement 的 Scenario 有对应实现或证据 | ✓ | 见下方 Requirement 对应表 |
| Project Rules | AGENTS.md 的 OpenSpec / CodeGraph / 验证 / 安全纪律 | ✓ | 已建 OpenSpec change（命中多条强制项）；bridge-plan 与 codegraph callers ×4 证据在案；验证命令全部真实运行；git status 无敏感文件 |
| Verification | 只报告真实运行命令；失败与 SKIPPED 明示 | ✓ | 全部验证命令于本会话实跑，无 SKIPPED 项 |
| README/AGENTS Sync | 入口按影响同步或说明无需同步 | ✓ | AGENTS.md 等五个入口已同步（本 change 交付物）；README / CLAUDE.md 无需改动（理由见 verification-before-completion.md） |

## Requirement ↔ 实现对应（build-warning-hygiene）

| Requirement | 实现 / 证据 |
|---|---|
| 任何代码实现不得引入新的编译告警 | 基线 14 项唯一告警行 → 0 项；分步门槛 14→12→6→1→0 每一档无新增告警 |
| 告警判定以 `--all-targets` 为准绳 | 全部门槛使用 `cargo check --release --all-targets`，计数口径为全行 `sort -u` 唯一告警行 |
| 消除告警必须修正真实问题 | 移动导入 1 项、删除死符号 6 项、`#[cfg(test)]` 收敛 6 项、删多余 `mut` 1 项；无 crate/module 级抑制；无伪造数据/调用点 |
| 窄范围抑制仅在删除会违反规格或契约时允许 | 仅 `EndpointStatus::Beta` 一例：`#[allow(dead_code)]` 附于 variant，紧邻注释引用 `openspec/specs/public-api-catalog/spec.md:39` 与 DTO 序列化契约 |

## 发现项

无。无越界改动、无未授权范围修改、无规格与实现无法对应之处。

## 剩余风险

1. CI 告警门禁未落地（独立后续 change；见 design/proposal Non-Goals 与 bridge-plan）
2. 协议语义无集成测试覆盖（项目级既有缺口，不属本 change 范围）
