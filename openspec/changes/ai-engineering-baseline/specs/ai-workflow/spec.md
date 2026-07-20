# Capability: ai-workflow

## ADDED Requirements

### Requirement: Agent 可发现项目规则

The repository MUST provide a root AGENTS.md that agents can discover, covering OpenSpec triggers, skills gates, verification discipline, and safety rules.

#### Scenario: 打开 AGENTS

- GIVEN 仓库已落地基线
- WHEN Agent 读取 AGENTS.md
- THEN 文件包含 OpenSpec 触发条件、skills 门禁、验证纪律与安全条款

### Requirement: 高风险变更必须规格化

High-risk changes (protocol, credentials, auth, Admin, model mapping, Docker/release, cross-module) MUST have openspec/changes/<name>/ artifacts before implementation begins.

#### Scenario: 高风险改动前

- GIVEN 变更属于高风险矩阵
- WHEN 开始实现
- THEN 已存在 proposal/design/tasks/specs 或等价完整工件，并完成 Bridge Plan

### Requirement: 完成前必须真实验证

Final delivery MUST report only verification commands actually run in the session, or MUST explicitly mark SKIPPED with reason and residual risk.

#### Scenario: 完成报告

- GIVEN 变更实现结束
- WHEN 输出最终报告
- THEN 包含本次真实命令与结果，或明确 SKIPPED 原因与剩余风险
