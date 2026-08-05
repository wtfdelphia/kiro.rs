# AI 工程化基线落地（L4）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按已批准设计把 kiro-rs 从 L0 接到 L4：规则入库、三层事实、OpenSpec、CodeGraph 约定、5 个门禁 skills、流程自验证试点 change（零业务代码改动）。

**Architecture:** 只改文档/规则/工具配置层。以 AGENTS.md + spec/ 为长期事实，以 openspec/changes/ai-engineering-baseline/ 为本次变更过程，以 .codex/skills/ 固化门禁；CodeGraph 仅约定用法并 git 忽略 .codegraph/。业务代码与真实凭据完全不碰。

**Tech Stack:** OpenSpec CLI 1.4.0、CodeGraph 0.9.8、ripgrep 14.0.3、Git、Markdown；项目本身为 Rust/Axum（本 plan 不改其代码）。

**Spec:** docs/superpowers/specs/2026-07-20-ai-engineering-baseline-design.md

## Global Constraints

- 回答与项目规则默认中文
- 禁止修改 src/**、admin-ui/src/**、Cargo.toml、Cargo.lock（除非用户另行授权）
- 禁止创建或提交真实 config.json / credentials.json / token / apiKey
- 禁止全量安装 ECC；只项目化写入 AGENTS + skills
- 工具版本以 2026-07-20 核验为准：OpenSpec 1.4.0、CodeGraph 0.9.8、rg 14.0.3、Node 25.0.0、Rust/Cargo 1.94.1、pnpm 11.1.3
- OpenSpec 目录细节以本机 openspec init / openspec templates 实际输出为准；内容按本 plan 与 design spec 填充
- 不 push、不建 PR、不 merge；commit 仅在各 Task 指定步骤执行
- 推荐在分支 codex/ai-engineering-baseline 上实施（Task 0 创建）
- 工作区已有未跟踪：docs/AI 辅助开发工程化落地白皮书.md、.codegraph/；白皮书应纳入版本，.codegraph/ 必须忽略
- 实现时完整文件正文以 design spec 与本 plan 各 Task 要求为准；不得提交空壳占位文件

## File Structure

| 路径 | 动作 | 职责 |
| --- | --- | --- |
| .gitignore | Modify | 放行 AGENTS/CLAUDE；忽略 codegraph/worktrees/local settings/logs |
| docs/tooling-sources.md | Create | 工具来源与核验版本 |
| docs/AI 辅助开发工程化落地白皮书.md | Add（已存在未跟踪） | 白皮书入库 |
| spec/requirements.md | Create | 长期需求与边界 |
| spec/design.md | Create | 长期架构与模块边界 |
| spec/structure.md | Create | 目录归属 |
| openspec/** | Create via CLI | OpenSpec 项目元数据与 change |
| openspec/project.md | Modify after init | 项目事实，与 AGENTS 对齐 |
| openspec/changes/ai-engineering-baseline/** | Create | 试点 change 工件 + evidence |
| AGENTS.md | Create | AI 协作总规则 |
| CLAUDE.md | Create | Claude 入口，指向 AGENTS |
| .codex/skills/*/SKILL.md | Create x5 | 门禁 skills |
| README.md | Modify（增量） | SpecCoding 入口，不覆盖原章节 |

---
### Task 0: 保护工作区并创建实施分支

**Files:**
- 无业务文件；仅 git 分支操作

**Interfaces:**
- Produces: 分支 codex/ai-engineering-baseline，基于当前 dev（含设计 spec 提交）

- [ ] **Step 1: 记录工作区基线**

Run:

```powershell
git status --short
git branch --show-current
git log -3 --oneline
```

Expected:
- 当前分支多为 dev
- 未跟踪至少包含 .codegraph/、白皮书（若尚未 add）
- 已有 commit 含 design spec

- [ ] **Step 2: 创建并切换实施分支**

Run:

```powershell
git switch -c codex/ai-engineering-baseline
git branch --show-current
```

Expected: codex/ai-engineering-baseline

- [ ] **Step 3: 确认不提交敏感与缓存**

Run:

```powershell
git status --short
```

Expected: 无已跟踪的 config.json / credentials.json；.codegraph/ 仍为未跟踪（下一步 ignore）

---

### Task 1: 修正 .gitignore 并入库白皮书 + tooling-sources

**Files:**
- Modify: .gitignore
- Create: docs/tooling-sources.md
- Add: docs/AI 辅助开发工程化落地白皮书.md（工作区已有）

**Interfaces:**
- Produces: 可提交的 AGENTS/CLAUDE 路径策略；工具版本权威表

- [ ] **Step 1: 重写 .gitignore 为目标内容**

将 .gitignore 完整替换为：

```gitignore
/target
/config.json
/credentials.json
/.idea
/test.json
/admin-ui/node_modules/
/admin-ui/dist/
/admin-ui/tsconfig.tsbuildinfo
/credentials.*
/kiro_balance_cache.json
/kiro_stats.json

# AI engineering local caches / isolation
.codegraph/
.worktrees/
.claude/settings.local.json
*.log
```

注意：必须删除原有的 /CLAUDE.md 与 /AGENTS.md 两行。

- [ ] **Step 2: 验证 gitignore 行为**

Run:

```powershell
'' | Set-Content -LiteralPath 'AGENTS.md' -Encoding utf8
git check-ignore -v AGENTS.md CLAUDE.md .codegraph/codegraph.db 2>&1
Remove-Item -LiteralPath 'AGENTS.md' -Force
```

Expected:
- AGENTS.md 不被 ignore
- .codegraph/ 被 ignore

- [ ] **Step 3: 创建 docs/tooling-sources.md**

完整内容必须包含核验日期 2026-07-20，以及版本表（不得缺行）：

| 工具 | 来源 | 核验版本 | 用途 | 不应提交 |
| --- | --- | --- | --- | --- |
| OpenSpec | https://github.com/Fission-AI/OpenSpec | 1.4.0 | 规格驱动与变更归档 | token、本机缓存 |
| CodeGraph | https://github.com/colbymchenry/codegraph | 0.9.8 | 本地代码图谱与影响面 | .codegraph/ |
| ripgrep | https://github.com/BurntSushi/ripgrep | 14.0.3 | 文本补盲 | 无 |
| Node.js | 本机运行时 | 25.0.0 | 运行 OpenSpec / CodeGraph CLI | 无 |
| Rust / Cargo | 本机工具链 | 1.94.1 | 构建与测试 | target/ |
| pnpm | 本机 | 11.1.3 | admin-ui 构建 | admin-ui/node_modules/ |
| ECC | https://github.com/affaan-m/ECC | 仅参考，不安装 | rules/skills 结构借鉴 | 用户级配置、密钥 |
| Karpathy skills | https://github.com/multica-ai/andrej-karpathy-skills | 仅参考 | 行为纪律项目化 | 未裁剪外部配置 |

并写上核验命令示例、项目内入口（AGENTS、spec、openspec、白皮书、design spec）、企业网络说明（不写个人 token）。正文结构见 design spec 第 4.4 节。

- [ ] **Step 4: 暂存并提交**

Run:

```powershell
git add -- .gitignore docs/tooling-sources.md "docs/AI 辅助开发工程化落地白皮书.md"
git status --short
git commit -m "chore: baseline gitignore and tooling sources for AI engineering"
```

Expected: commit 成功；.codegraph/ 仍不在暂存区

---

### Task 2: 建立 spec/ 三层长期事实

**Files:**
- Create: spec/requirements.md
- Create: spec/design.md
- Create: spec/structure.md

**Interfaces:**
- Produces: 长期事实源，供 AGENTS / OpenSpec project.md 引用

- [ ] **Step 1: 创建 spec/requirements.md**

完整正文必须写满（禁止空段），覆盖：
- 产品定位：Rust Anthropic->Kiro 代理
- 核心能力：/v1、/cc/v1、SSE、token 刷新、多凭据、thinking/tools/websearch、模型映射、Admin、Region/代理
- 业务边界：研究用途、apiKey 客户端认证、上游 OAuth 凭据
- 非目标：非多租户 SaaS、不存真实凭据、不替代官方客户端、高风险无 OpenSpec 不做
- 质量与协作：高风险可规格化可验证；遵循 AGENTS + OpenSpec

内容以 README 功能列表与 design spec 为准逐条落盘。

- [ ] **Step 2: 创建 spec/design.md**

完整正文必须覆盖：
- 架构风格（Axum 单进程异步代理）
- 模块边界表：main、model、anthropic、kiro、admin、admin_ui、common、http_client、admin-ui
- 关键数据流 6 步（鉴权 -> 转换 -> TokenManager -> 出站 -> 解析 -> 回写 SSE/JSON）
- 安全与机密
- 构建与测试策略（cargo/pnpm/docker/openspec）
- CodeGraph 约定

- [ ] **Step 3: 创建 spec/structure.md**

完整正文必须覆盖：
- 目录树（含 spec、openspec、.codex/skills、AGENTS、CLAUDE、docs）
- 配置文件归属表（example 入库；config/credentials/.codegraph 不入库）

- [ ] **Step 4: 提交**

Run:

```powershell
git add -- spec/requirements.md spec/design.md spec/structure.md
git commit -m "docs(spec): add long-lived requirements, design, and structure facts"
```

---
### Task 3: 初始化 OpenSpec 并填写 project.md

**Files:**
- Create via CLI: openspec/**
- Modify: openspec/project.md

**Interfaces:**
- Produces: 可用的 OpenSpec 仓库布局；openspec validate 可运行

- [ ] **Step 1: 初始化**

Run:

```powershell
openspec init --tools codex
```

Expected: 创建 openspec/。若提示已存在则停止并人工检查，禁止盲目 --force。

- [ ] **Step 2: 查看模板与结构**

Run:

```powershell
Get-ChildItem -Recurse openspec | Select-Object FullName
openspec templates --json
openspec validate --all
```

- [ ] **Step 3: 填写 openspec/project.md**

若 init 生成 front matter，保留并合并。正文至少包含：
- 项目：kiro-rs Anthropic->Kiro 代理
- 技术栈：Rust 2024、Axum 0.8、Tokio、Reqwest、Serde、admin-ui pnpm、Docker
- 事实源优先级：change 工件 > AGENTS > spec/ > README > 源码
- 约束：无真实凭据；高风险走 OpenSpec；Bridge + 真实验证；详见 AGENTS.md
- 常用验证：openspec validate、cargo test、codegraph、rg 补盲

- [ ] **Step 4: 再次 validate 并提交**

Run:

```powershell
openspec validate --all
git add -- openspec
git status --short
git commit -m "chore(openspec): initialize OpenSpec with codex tool profile"
```

若 init 在根目录生成其它客户端指令文件，检查后按需加入；不要加入 settings.local.json。

---

### Task 4: 创建试点 change 工件 ai-engineering-baseline

**Files:**
- Create: openspec/changes/ai-engineering-baseline/proposal.md
- Create: openspec/changes/ai-engineering-baseline/design.md
- Create: openspec/changes/ai-engineering-baseline/tasks.md
- Create: openspec/changes/ai-engineering-baseline/specs/ai-workflow/spec.md
- Create: openspec/changes/ai-engineering-baseline/evidence/.gitkeep

**Interfaces:**
- Produces: 完整 OpenSpec change；后续 evidence 写入 evidence/

- [ ] **Step 1: 用 CLI 创建 change 目录**

Run:

```powershell
openspec new change ai-engineering-baseline --description "AI engineering L4 baseline and self-validation pilot"
Get-ChildItem -Recurse openspec/changes/ai-engineering-baseline | Select-Object FullName
```

- [ ] **Step 2: 写入 proposal.md**

必须包含完整章节：背景、范围、非目标、假设、影响面、成功标准、风险。
- 范围：AGENTS/CLAUDE/spec/openspec/skills、tooling-sources、gitignore、README 入口、自验证试点
- 非目标：不改 src、不全量 ECC、不用真实凭据、不自动 merge
- 假设：OpenSpec 1.4.0、CodeGraph 0.9.8、design 已批准
- 成功标准：validate 通过、设计第9节验收、四 evidence 齐全

- [ ] **Step 3: 写入 design.md**

必须包含：当前实现、目标设计（指向 design spec）、CodeGraph 影响面（codegraph status；无运行时符号变更）、盲区补充（gitignore/example/Dockerfile/workflows）、异常路径、回滚、验证策略。

- [ ] **Step 4: 写入 tasks.md**

10 项可勾选：
1. gitignore + tooling-sources + 白皮书
2. spec 三件套
3. openspec init + project.md
4. 本 change 工件
5. AGENTS + CLAUDE
6. 五个 skills
7. README SpecCoding 入口
8. openspec validate --all
9. 四门禁 evidence
10. 文档同步判断 + git status 无密钥

- [ ] **Step 5: 写入 specs/ai-workflow/spec.md**

三个 Requirement + Scenario：
1. Agent 可发现项目规则（读 AGENTS 含门禁与安全条款）
2. 高风险变更必须规格化（有 change 工件 + Bridge Plan）
3. 完成前必须真实验证（真实命令或明确 SKIPPED）

- [ ] **Step 6: 创建 evidence 目录并 validate**

Run:

```powershell
New-Item -ItemType Directory -Force -Path 'openspec/changes/ai-engineering-baseline/evidence' | Out-Null
'' | Set-Content -LiteralPath 'openspec/changes/ai-engineering-baseline/evidence/.gitkeep' -Encoding utf8
openspec validate --all
openspec status --change ai-engineering-baseline 2>&1
```

- [ ] **Step 7: 提交**

Run:

```powershell
git add -- openspec/changes/ai-engineering-baseline
git commit -m "docs(openspec): add ai-engineering-baseline change artifacts"
```

---

### Task 5: 创建 AGENTS.md 与 CLAUDE.md

**Files:**
- Create: AGENTS.md
- Create: CLAUDE.md

**Interfaces:**
- Consumes: spec/、tooling-sources、高风险矩阵
- Produces: 全客户端共享规则入口

- [ ] **Step 1: 写入完整 AGENTS.md**

必须包含且写满以下章节（正文见 design spec 第4.1节与已确认设计，禁止省略表格）：
1. 回答语言（默认中文）
2. 项目上下文（定位、技术栈、入口、spec/openspec/tooling/白皮书）
3. Karpathy 四原则 + 一句话规则
4. OpenSpec 条件（协议/SSE/Token/认证/Admin/模型映射/Docker/配置/重构）与豁免
5. Skills 门禁表（5 个 skill 名称与场景）
6. CodeGraph 边界与补盲
7. 高风险检查矩阵表（协议/SSE、Token、认证、Admin、模型映射、Docker、admin-ui、OpenSpec）
8. 验证纪律
9. README/AGENTS/spec 同步纪律
10. 安全（config/credentials 忽略与禁止粘贴真实密钥）

- [ ] **Step 2: 写入 CLAUDE.md**

最小入口：指向 AGENTS.md、spec/、当前 change、docs/tooling-sources.md；声明 AGENTS 为唯一主规则。

- [ ] **Step 3: 确认可被 git 跟踪并提交**

Run:

```powershell
git check-ignore -v AGENTS.md CLAUDE.md 2>&1
git add -- AGENTS.md CLAUDE.md
git commit -m "docs: add AGENTS.md and CLAUDE.md project AI rules"
```

---
### Task 6: 创建五个项目内 Skills

**Files:**
- Create: .codex/skills/openspec-new-change/SKILL.md
- Create: .codex/skills/openspec-superpowers-bridge/SKILL.md
- Create: .codex/skills/spec-compliance-check/SKILL.md
- Create: .codex/skills/openspec-verify-change/SKILL.md
- Create: .codex/skills/verification-before-completion/SKILL.md

**Interfaces:**
- Produces: 可被 Codex 发现的 skill 说明与强制产出模板

- [ ] **Step 1: 创建目录**

Run:

```powershell
$skills = @('openspec-new-change','openspec-superpowers-bridge','spec-compliance-check','openspec-verify-change','verification-before-completion')
foreach ($s in $skills) { New-Item -ItemType Directory -Force -Path ".codex/skills/$s" | Out-Null }
```

- [ ] **Step 2: 写入五个 SKILL.md**

每个文件必须含 YAML front matter（name/description），以及：何时使用、输入、步骤或维度、必产出路径、停止条件。不得提交空文件。

| Skill | 必产出 |
| --- | --- |
| openspec-new-change | openspec/changes/<name>/ 最低工件 |
| openspec-superpowers-bridge | openspec/changes/<name>/evidence/bridge-plan.md |
| spec-compliance-check | openspec/changes/<name>/evidence/spec-compliance-report.md |
| openspec-verify-change | openspec/changes/<name>/evidence/openspec-verify-report.md |
| verification-before-completion | openspec/changes/<name>/evidence/verification-before-completion.md |

内容要点：
- new-change：openspec list/new change；填 proposal/design/tasks/specs；validate；停止于范围/验收不清
- bridge：必读 AGENTS、spec/design、project.md、当前 change 工件；Bridge Plan 含范围/非目标/高风险/CodeGraph/rg/任务映射表/必跑验证/文档同步/停止条件
- compliance：六维 Scope、Design、Scenarios、Project Rules、Verification、README/AGENTS Sync；CRITICAL 必须处理
- verify：Completeness、Correctness、Coherence；未给 change name 时用 openspec list，勿猜测
- verification-before-completion：只报告本会话真实命令；含 Verification、Documentation Sync、Residual Risk；检查 git status

- [ ] **Step 3: 提交**

Run:

```powershell
git add -- .codex/skills
git commit -m "chore(skills): add OpenSpec gate skills for AI engineering workflow"
```

---

### Task 7: README 增量 SpecCoding 入口

**Files:**
- Modify: README.md（仅追加，不删既有业务说明）

- [ ] **Step 1: 在「## 项目结构」之前插入章节**

插入标题「## SpecCoding / OpenSpec 工作流」，正文链接：
- spec/
- openspec/changes/<change-name>/
- AGENTS.md
- CLAUDE.md
- docs/tooling-sources.md
- docs/AI 辅助开发工程化落地白皮书.md

并给出推荐闭环：

```text
openspec new change / 补齐工件
  -> openspec-superpowers-bridge（Bridge Plan）
  -> 小步实现并更新 tasks.md
  -> spec-compliance-check
  -> openspec-verify-change
  -> README/AGENTS/spec 同步判断
  -> verification-before-completion
  -> openspec archive（人工确认后）
```

不得修改「## 项目结构」及之后既有业务文档内容。

- [ ] **Step 2: 提交**

Run:

```powershell
git add -- README.md
git commit -m "docs(readme): add SpecCoding and OpenSpec workflow entry"
```

---

### Task 8: 勾选 change tasks、跑 validate，并写入四门禁 evidence

**Files:**
- Modify: openspec/changes/ai-engineering-baseline/tasks.md
- Create: openspec/changes/ai-engineering-baseline/evidence/bridge-plan.md
- Create: openspec/changes/ai-engineering-baseline/evidence/spec-compliance-report.md
- Create: openspec/changes/ai-engineering-baseline/evidence/openspec-verify-report.md
- Create: openspec/changes/ai-engineering-baseline/evidence/verification-before-completion.md

- [ ] **Step 1: 运行核验命令**

Run:

```powershell
openspec validate --all
openspec list
codegraph status
git status --short
Get-ChildItem AGENTS.md, CLAUDE.md, spec, .codex/skills, openspec/changes/ai-engineering-baseline -Recurse -ErrorAction SilentlyContinue | Select-Object FullName
```

Expected: validate 通过；关键路径存在；无 credentials/config/.codegraph 被 add

- [ ] **Step 2: 写 bridge-plan.md**

按 bridge skill 模板，写入 Step 1 真实命令结论；范围=基建；非目标=不改 src；CodeGraph=status 基线；任务映射对应 tasks 1-10。

- [ ] **Step 3: 写 spec-compliance-report.md**

六维表，本 change 预期 PASS（Scope 无 src 改动；Scenarios 由 AGENTS/skills/validate 证据覆盖）。

- [ ] **Step 4: 写 openspec-verify-report.md**

Completeness/Correctness/Coherence 通过，并引用具体文件路径作为证据。

- [ ] **Step 5: 写 verification-before-completion.md**

必须含：
- Verification 列表（validate、codegraph status、路径检查、git status）
- Documentation Sync 表（README/AGENTS/CLAUDE/spec/openspec/specs/tooling-sources）
- Residual Risk（未 archive、未全量 cargo test、未装 CodeGraph MCP）

- [ ] **Step 6: tasks.md 全部改为 - [x]**

- [ ] **Step 7: 最终 validate 与提交**

Run:

```powershell
openspec validate --all
git add -- openspec/changes/ai-engineering-baseline
git commit -m "docs(openspec): complete baseline pilot evidence and task checklist"
```

---

### Task 9: 最终自检对照设计验收表

**Files:**
- 无强制新文件

- [ ] **Step 1: 逐条对照设计第 9 节**

| # | 标准 | 检查 |
| --- | --- | --- |
| 1 | AGENTS/CLAUDE 可跟踪 | git ls-files AGENTS.md CLAUDE.md |
| 2 | spec 三件套 | 路径存在 |
| 3 | validate 通过 | openspec validate --all |
| 4 | change 工件齐全 | 列目录 |
| 5 | 5 skills | 列 .codex/skills/*/SKILL.md |
| 6 | tooling-sources 版本 | 打开文件核对 1.4.0 / 0.9.8 等 |
| 7 | README 入口 | rg -n "SpecCoding|OpenSpec" README.md |
| 8 | 忽略与无密钥 | git status --short |
| 9 | 四 evidence | 四文件存在 |
| 10 | L4 标志 | OpenSpec+skills+CodeGraph 约定在库中 |

- [ ] **Step 2: 向用户报告 PASS/风险；默认不 archive、不 push**

- [ ] **Step 3: 仅当用户明确要求时 archive**

```powershell
openspec archive ai-engineering-baseline
openspec validate --all
git add -- openspec
git commit -m "docs(openspec): archive ai-engineering-baseline change"
```

---

## Plan Self-Review

### Spec coverage

| 设计要点 | Task |
| --- | --- |
| gitignore 修正 | 1 |
| tooling-sources + 白皮书 | 1 |
| spec 三件套 | 2 |
| openspec init + project.md | 3 |
| 试点 change | 4 |
| AGENTS + CLAUDE | 5 |
| 5 skills | 6 |
| README 增量 | 7 |
| 四门禁 evidence | 8 |
| 验收 / 不自动 archive | 9 |
| 零 src 改动 | Global Constraints |
| 分支 codex/ai-engineering-baseline | 0 |
| CodeGraph 约定 | 1,5,8 |
| 高风险矩阵项目化 | 5 |

### Placeholder scan

无 TBD/TODO/implement later。OpenSpec 细路径以 CLI 为准处已给出覆盖/合并动作。Task 4/5/6 要求写入完整正文（以 design spec 与已确认模板为准），实现 agent 不得留空文件。

### Consistency

- change 名：ai-engineering-baseline
- evidence 目录统一：openspec/changes/ai-engineering-baseline/evidence/
- 分支名：codex/ai-engineering-baseline
- 工具版本与 design 一致

---

## Execution Handoff

实现时 REQUIRED：subagent-driven-development（推荐）或 executing-plans。
