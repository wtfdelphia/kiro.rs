# Proposal: ai-engineering-baseline

## 背景

仓库 AI 协作处于 L0：有 README 与代码，缺少 AGENTS、spec、OpenSpec 与可执行门禁，无法审计与迁移 AI 变更过程。

## 范围

- 建立 AGENTS.md / CLAUDE.md / spec/ / openspec/ / skills 门禁
- 记录工具来源与版本
- 修正 gitignore
- README 增加 SpecCoding 入口
- 本 change 作为流程自验证试点

## 非目标

- 不修改 src/ 业务逻辑
- 不全量安装 ECC
- 不接入真实凭据或线上冒烟
- 不自动 merge 主干

## 假设

- 本机 OpenSpec 1.4.0、CodeGraph 0.9.8 可用
- 设计规格已批准：docs/superpowers/specs/2026-07-20-ai-engineering-baseline-design.md

## 影响面

- 入口/文档/规则层；无运行时符号变更
- gitignore 放行 AGENTS/CLAUDE

## 成功标准

- openspec validate --all 通过
- 验收表（设计第9节）全部满足
- evidence 含 Bridge/Compliance/Verify/Completion

## 风险

- 误提交密钥：凭据类持续 ignore + 提交前 status
- OpenSpec schema 差异：以 CLI 为准填充内容
