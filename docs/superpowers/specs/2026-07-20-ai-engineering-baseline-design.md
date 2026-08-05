# AI 工程化基线落地设计（L4）

日期：2026-07-20  
项目：kiro-rs  
分支策略：推荐 `codex/ai-engineering-baseline`（实现阶段创建；未要求前不 push/PR/merge）  
成熟度目标：L4（OpenSpec 闭环 + Skills 门禁 + CodeGraph 约定进入工件）  
方案：B — 基建 + 流程自验证试点（零业务代码改动）

## 1. 背景与目标

### 1.1 背景

仓库当前约 L0：

- 有较完整 `README.md` 与业务代码（Rust/Axum Anthropic→Kiro 代理）
- 无 `AGENTS.md`、`spec/`、`openspec/`、项目内 skills
- `.gitignore` 忽略了 `/AGENTS.md` 与 `/CLAUDE.md`（与“项目规则入库”冲突）
- 本机已装最新工具：OpenSpec 1.4.0、CodeGraph 0.9.8、rg 14.0.3；CodeGraph 已有本地索引
- 白皮书在 `docs/AI 辅助开发工程化落地白皮书.md`（未跟踪）

### 1.2 目标

按白皮书 0→1 流程，把 AI 辅助开发落到本仓库，达到 L4：

1. 事实基线化：README + AGENTS + `spec/`
2. 变更规格化：OpenSpec + 试点 change
3. 上下文证据化：CodeGraph 约定 + rg 补盲
4. 执行隔离化：独立分支 / 可选 worktree；不覆盖用户改动
5. 合规审查化：Compliance / Verify 模板与 skills
6. 验证真实化：verification-before-completion
7. 文档同步化：README/AGENTS/spec 同步纪律

### 1.3 非目标

- 不修改 `src/` 业务逻辑
- 不全量复制 ECC / 外部 hooks
- 不引入 Java/DPM-ILDA 专用规则
- 不提交 `config.json` / `credentials.json` / 真实密钥
- 不默认 push、PR、merge 主干
- 不强制用户级全局 MCP/hook 安装
- 不把“未来愿景”写成已实现能力

## 2. 用户已确认决策

| 决策点 | 结论 |
| --- | --- |
| 目标成熟度 | L4 增强档 |
| AGENTS.md 版本控制 | 入库；去掉 `.gitignore` 对 AGENTS/CLAUDE 的忽略 |
| 落地路径 | 方案 B：基建 + 流程自验证试点 `ai-engineering-baseline` |
| ECC | 最小档：结构借鉴，项目化写入 AGENTS/skills，不整包安装 |
| 工具版本 | 使用本机最新核验版本写入 `docs/tooling-sources.md` |

## 3. 产物清单与目录

```text
kiro.rs/
├── AGENTS.md
├── CLAUDE.md                          # 最小，指向 AGENTS.md
├── README.md                          # 增量：SpecCoding 入口
├── .gitignore                         # 修忽略规则
├── docs/
│   ├── AI 辅助开发工程化落地白皮书.md
│   ├── tooling-sources.md
│   └── superpowers/specs/
│       └── 2026-07-20-ai-engineering-baseline-design.md  # 本文
├── spec/
│   ├── requirements.md
│   ├── design.md
│   └── structure.md
├── openspec/
│   ├── project.md
│   ├── specs/                         # 长期 capability（最小或初始化后按 CLI schema）
│   └── changes/
│       └── ai-engineering-baseline/
│           ├── proposal.md
│           ├── design.md
│           ├── tasks.md
│           ├── specs/ai-workflow/spec.md
│           └── evidence/              # Bridge/Compliance/Verify/Completion
└── .codex/skills/
    ├── openspec-new-change/SKILL.md
    ├── openspec-superpowers-bridge/SKILL.md
    ├── spec-compliance-check/SKILL.md
    ├── openspec-verify-change/SKILL.md
    └── verification-before-completion/SKILL.md
```

OpenSpec 1.4 实际目录/schema 以 `openspec init` / `openspec templates` 为准；白皮书模板用于内容，不硬编码过时路径。

## 4. 组件设计

### 4.1 AGENTS.md

入库，中文为主，包含：

1. 回答语言：默认中文
2. 项目上下文：定位、技术栈、关键模块入口
3. Karpathy 四原则
4. OpenSpec 触发条件与豁免
5. Skills 门禁（项目内 `.codex/skills/`；不支持则等价输出证据）
6. CodeGraph 边界与 rg 补盲要求
7. 本项目高风险检查矩阵（见 §5）
8. 验证纪律（只报告真实运行结果）
9. README / AGENTS / spec / openspec/specs 同步纪律
10. 安全：禁止真实 token/密钥入仓与粘贴

### 4.2 CLAUDE.md

最小项目级文件：说明 Claude Code 遵循 `AGENTS.md` + `spec/` + 当前 OpenSpec change，避免与 AGENTS 冲突的第二套规则。

### 4.3 .gitignore

移除：

- `/AGENTS.md`
- `/CLAUDE.md`

新增（若缺失）：

- `.codegraph/`
- `.worktrees/`
- `.claude/settings.local.json`
- `*.log`

保持：`/target`、`config.json`、`credentials.json`、`credentials.*`、`admin-ui/node_modules/`、`admin-ui/dist/` 等。

### 4.4 docs/tooling-sources.md

核验日期 2026-07-20：

| 工具 | 来源 | 核验版本 | 用途 | 不提交 |
| --- | --- | --- | --- | --- |
| OpenSpec | https://github.com/Fission-AI/OpenSpec | 1.4.0 | 规格驱动与变更归档 | token、本机缓存 |
| CodeGraph | https://github.com/colbymchenry/codegraph | 0.9.8 | 本地图谱与影响面 | `.codegraph/` |
| ripgrep | https://github.com/BurntSushi/ripgrep | 14.0.3 | 文本补盲 | 无 |
| Node | 本机 | 25.0.0 | 运行上述 CLI | 无 |
| Rust/Cargo | 本机 | 1.94.1 | 构建与测试 | `target/` |
| pnpm | 本机 | 11.1.3 | admin-ui | `node_modules/` |
| ECC | https://github.com/affaan-m/ECC | 仅参考，不安装 | rules/skills 结构借鉴 | 用户级配置 |
| Karpathy skills | https://github.com/multica-ai/andrej-karpathy-skills | 仅参考 | 行为纪律 | 未裁剪外部配置 |

### 4.5 spec/ 三层长期事实

只写当前真实事实：

- `requirements.md`：产品定位、能力边界、研究用途、非目标
- `design.md`：模块边界（anthropic/kiro/admin/admin_ui/model）、认证与凭据流、流式转换、构建测试策略
- `structure.md`：目录与模块归属（与 README 结构对齐）

不确定处标「待核验」。

### 4.6 OpenSpec

命令：

```bash
openspec init --tools codex
openspec validate --all
```

要求：

- `openspec/project.md` 与 `AGENTS.md` 不冲突
- 试点 change：`ai-engineering-baseline`
- 最低工件：proposal / design / tasks / specs/<capability>/spec.md
- 证据目录：`evidence/` 存放四门禁产出

### 4.7 CodeGraph

- `.codegraph/` 本地已索引（约 85 files / 1281 nodes），必须 git 忽略
- 约定命令：`status` / `context` / `query` / `callers` / `callees` / `impact` / `sync`
- Bridge Plan 与 design 必须含 CodeGraph 影响面 + rg 补盲
- 本试点无运行时符号变更，影响面写明“文档与规则层”
- `codegraph install`（MCP）为可选，不写入必做

### 4.8 项目内 Skills

| Skill | 触发 | 必产出 | 停止条件 |
| --- | --- | --- | --- |
| openspec-new-change | 新需求/跨模块/高风险 | `openspec/changes/<name>/` | 范围/非目标/验收不清 |
| openspec-superpowers-bridge | 实现 change 前 | Bridge Plan | 工件缺失/矛盾/blocked |
| spec-compliance-check | 实现后/审查前/归档前 | Spec Compliance Report | CRITICAL 未处理 |
| openspec-verify-change | 归档前 | Completeness/Correctness/Coherence | tasks 未完成或缺证据 |
| verification-before-completion | 最终回复/PR/归档/合并前 | Verification + Doc Sync + Residual Risk | 必要验证未跑且无理由 |

每个 skill 仅 `SKILL.md`（触发、输入、模板、停止条件）。不写 hook 脚本。

### 4.9 README 增量

在不覆盖原业务说明的前提下，增加 SpecCoding/OpenSpec 入口：

- 指向 `AGENTS.md`、`spec/`、`openspec/`、`docs/tooling-sources.md`、白皮书
- 推荐闭环：new → bridge → 实现 → compliance → verify → 文档同步 → verification-before-completion → archive

## 5. 本项目高风险矩阵

替换白皮书 Java/DPM 模板：

| 变更类型 | 验证思路 |
| --- | --- |
| 协议转换 / 流式 SSE | `cargo test` 相关模块；必要时本地 curl 冒烟 |
| Token 刷新 / 多凭据 | token_manager 相关测试；配置示例完整性 |
| 认证中间件 / API Key | common/auth、middleware 测试 |
| Admin API / 凭据 CRUD | admin 模块测试；严禁真实凭据入库 |
| 模型映射 | converter / 映射相关测试 |
| Docker / 发布 | Dockerfile、compose、CI workflow 检查 |
| 前端 admin-ui | `pnpm` build（及已有 test） |
| OpenSpec 工件 | `openspec validate --all` |

## 6. 试点 Change：ai-engineering-baseline

### 6.1 范围

完成 §3 全部基建产物；产出四门禁证据；`openspec validate --all` 通过。

### 6.2 非目标

见 §1.3。

### 6.3 tasks（实现阶段可勾选）

1. 修 `.gitignore`
2. 写 `docs/tooling-sources.md`
3. 写 `spec/{requirements,design,structure}.md`
4. `openspec init` + 填写 `project.md`
5. 创建并填写试点 change 工件
6. 写 `AGENTS.md` + `CLAUDE.md`
7. 建 5 个 skills
8. README 增量
9. `openspec validate --all`
10. 产出 Bridge / Compliance / Verify / Completion 证据
11. 文档同步判断 + `git status` 无密钥误入

### 6.4 回滚

删除新增规则/规格/skills 文件，恢复 `.gitignore` 相关行；不涉及数据迁移。

## 7. 数据流与协作模型

```text
人类需求
  -> Gate0 分类（是否 OpenSpec）
  -> 读 AGENTS + spec + 当前 change
  -> CodeGraph / rg 收集影响面
  -> Bridge Plan
  -> 小步实现（本试点仅文档/规则）
  -> Compliance
  -> Verify
  -> Doc Sync + Verification Before Completion
  -> 人工确认 archive / merge
```

多客户端：Codex 以 `AGENTS.md` + `.codex/skills/` 为主；Claude 以 `CLAUDE.md` 指向同一事实；不维护互相冲突的第二套长期规则。

## 8. 错误处理与停止条件

停止并请求人工确认，当：

- 需求多解且影响接口/凭据/协议/发布
- 无法判断改动边界或验证方式
- 多个活跃 change 目标不清
- OpenSpec 工件互相矛盾或 validate 失败且原因不明
- 发现工作区含用户未说明的敏感文件将被提交
- 用户要求跳过必要验证但未明确承担风险

## 9. 验收标准

| # | 标准 | 验证 |
| --- | --- | --- |
| 1 | AGENTS.md、CLAUDE.md 存在且可被 git 跟踪 | 文件 + gitignore |
| 2 | spec 三件套存在 | 路径 |
| 3 | OpenSpec 初始化且 validate 通过 | 真实命令 |
| 4 | 试点 change 工件齐全 | proposal/design/tasks/specs |
| 5 | 5 个 skills 存在 | 路径 |
| 6 | tooling-sources 记录最新版本 | 内容 |
| 7 | README 含 SpecCoding 入口且未覆盖原文 | diff |
| 8 | .codegraph 忽略；无密钥入候选提交 | git status |
| 9 | 四门禁证据在 change/evidence | 文件 |
| 10 | 达到 L4 标志项 | 自检清单 |

## 10. 实施顺序

```text
保护工作区检查
  -> 修 gitignore
  -> tooling-sources
  -> spec/ 三件套
  -> openspec init + project.md
  -> 试点 change 工件
  -> AGENTS.md + CLAUDE.md
  -> .codex/skills/*
  -> README 增量
  -> validate + 四门禁证据
  -> 同步判断 + 最终验证报告
  ->（用户确认后）commit / 可选 archive
```

## 11. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| gitignore 放行 AGENTS 后误写密钥 | AGENTS 安全条款；提交前 git status；credentials 仍忽略 |
| OpenSpec 1.4 schema 与白皮书示例差异 | 以本机 CLI templates/instructions 为准 |
| 文档与长期事实重复/漂移 | 变更过程只放 openspec/changes；长期事实只放 spec/ |
| 把接入做成业务大改 | 非目标写死；零 src 改动 |
| CodeGraph 索引陈旧 | 文档要求变更后 sync；本试点无代码符号变更 |

## 12. 测试与验证策略（本 change）

必跑：

- `openspec validate --all`
- 关键存在性检查（AGENTS、spec、skills、change 工件、evidence）
- `git status --short`（无密钥、无 .codegraph）
- `codegraph status`（确认可用；记录基线）

不强制：

- 全量 `cargo test` / `cargo build --release`（无 src 变更；若用户要求可加）
- admin-ui 构建
- 真实 API 冒烟（需要凭据，禁止用真实密钥做演示）

## 13. 文档同步判断（本 change 预期）

| 文件 | 是否更新 | 原因 |
| --- | --- | --- |
| README.md | 是 | AI 开发流程入口、工具入口变化 |
| AGENTS.md | 是（新建） | 项目 AI 规则基线 |
| CLAUDE.md | 是（新建） | 多客户端入口 |
| spec/ | 是（新建） | 长期事实基线 |
| openspec/specs/ | 视 init/归档 | 最小 capability 或归档时回写 |
| docs/tooling-sources.md | 是（新建） | 工具来源与版本 |

## 14. Spec 自检记录

- Placeholder：无 TBD/TODO 未决项（OpenSpec 细路径以 CLI 为准已写明）
- 一致性：方案 B / L4 / 零业务改动 / AGENTS 入库全文一致
- 范围：单次可实施的基建 + 自验证试点，无需再拆子项目
- 歧义：归档与 commit 需用户确认后执行——已明确

## 15. 参考

- `docs/AI 辅助开发工程化落地白皮书.md`
- OpenSpec：https://github.com/Fission-AI/OpenSpec
- CodeGraph：https://github.com/colbymchenry/codegraph
- ECC：https://github.com/affaan-m/ECC
- andrej-karpathy-skills：https://github.com/multica-ai/andrej-karpathy-skills

