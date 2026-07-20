# Bridge Plan: ai-engineering-baseline

日期：2026-07-20
状态：WARN（本会话恢复时部分工件已落盘；本文件作为恢复后的执行桥接证据写入。后续高风险 change 必须在实现前产出。）

## 范围

- 基建与规则层：AGENTS.md、CLAUDE.md、spec/、openspec/、.codex/skills/、README、docs/tooling-sources.md、.gitignore。
- 本 change 只落地 AI 辅助开发工程化 L4 基线和自验证试点。

## 非目标

- 不修改 src/**、admin-ui/src/**、Cargo.toml、Cargo.lock。
- 不提交 config.json、credentials.json、credentials.*、真实 token、Cookie、API Key。
- 不 push、不 merge、不 PR、不 archive。
- 不全量安装 ECC；只项目化写入规则和 skills。

## 关键设计决策

| 决策 | 结论 |
| --- | --- |
| 成熟度 | L4 增强档 |
| 事实源 | change 工件 > AGENTS.md > spec/ > README.md > 源码 |
| 变更形态 | 文档、规则、工具配置；零业务代码改动 |
| 门禁 | OpenSpec + 项目内 5 个 skills + verification-before-completion |
| CodeGraph | 用于上下文和影响面，不替代 rg、源码精读和测试 |

## 高风险项

| 风险 | 处理 |
| --- | --- |
| 误提交凭据 | .gitignore 保持 config/credentials 忽略；提交前 git status 与 check-ignore |
| OpenSpec schema 差异 | 使用本机 OpenSpec 1.4.0 的 status/templates/validate 输出 |
| 子代理流程不可用 | 当前工具列表无 spawn_agent/followup_task/wait_agent，且终端无 codeagent-wrapper；按 SDD 切片和证据门禁降级执行，最终标 WARN |
| Bridge 证据晚于部分实现 | 本次为恢复执行；后续 change 必须实现前生成 |

## CodeGraph 证据

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| codegraph status | Exit 0；85 files、1,281 nodes、3,297 edges；[OK] Index is up to date | 本试点无运行时符号变更，CodeGraph 作为基线可用 |

## rg / 补盲

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| rg --files AGENTS.md CLAUDE.md spec openspec .codex docs .superpowers | 找到 AGENTS、CLAUDE、spec 三件套、OpenSpec change、skills、tooling-sources、白皮书和 SDD scratch | 关键路径存在 |
| git diff --name-only b9e757e..HEAD | rg "^(src/|admin-ui/src/|Cargo\.toml$|Cargo\.lock$)" | Exit 1，无匹配 | 未修改受限业务路径 |
| git check-ignore -v config.json credentials.json credentials.local.json .codegraph\codegraph.db | Exit 0；命中 .gitignore | 敏感/本地缓存忽略规则有效 |

## 任务到执行步骤

| Task | 执行步骤 | 状态 |
| --- | --- | --- |
| 1 | .gitignore、白皮书、tooling-sources 入库 | 已完成，commit ef35fb9 |
| 2 | spec/requirements、design、structure | 已完成，commit 4f047f3 |
| 3 | openspec init 与 project.md | 已完成，commit 80c4d82 |
| 4 | ai-engineering-baseline change 工件 | 已完成，commit 93be424 |
| 5 | AGENTS.md 与 CLAUDE.md | 已完成，commit c38284d |
| 6 | 五个项目门禁 skills | 已完成，commit f164146 |
| 7 | README SpecCoding 入口 | 已完成，commit 7261ae9 |
| 8 | OpenSpec validate | 已运行，PASS |
| 9 | 四门禁 evidence | 本文件及同目录报告 |
| 10 | 文档同步与 git status 无密钥 | completion evidence 覆盖 |

## 必跑验证

| 命令 | 结果 |
| --- | --- |
| openspec validate --all | PASS：1 passed, 0 failed |
| openspec list | ai-engineering-baseline：写 evidence 前为 7/10 tasks |
| codegraph status | PASS：[OK] Index is up to date |
| git status --short | 仅 .superpowers/ 未跟踪 scratch |
| 路径存在性检查 | AGENTS、CLAUDE、spec、skills、change 工件存在 |
| 受限路径检查 | 无 src/admin-ui/src/Cargo 改动 |
| 忽略规则检查 | config/credentials/.codegraph 被 ignore |

## README/AGENTS/spec 同步判断

| 入口 | 判断 |
| --- | --- |
| README.md | 已新增 SpecCoding / OpenSpec 工作流入口 |
| AGENTS.md | 已新增项目 AI 主规则 |
| CLAUDE.md | 已新增最小客户端入口 |
| spec/ | 已新增三层长期事实 |
| openspec/changes/ai-engineering-baseline | 已新增试点 change 与 evidence |
| docs/tooling-sources.md | 已记录工具来源与版本 |

## 停止条件

- OpenSpec validate 失败。
- 发现受限业务路径被修改。
- git status 显示 config/credentials/.codegraph 等敏感或本地缓存将被提交。
- 用户要求 archive、push、merge，但未明确授权。
