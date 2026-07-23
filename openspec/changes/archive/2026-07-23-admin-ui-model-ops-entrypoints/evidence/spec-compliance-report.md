# Spec Compliance Report: admin-ui-model-ops-entrypoints

Date: 2026-07-23  
Reviewer: Codex (spec-compliance-check)  
Change status: tasks 17/17 complete; openspec validate --all green

## Overall Status: **PASS**

满足 proposal/design/tasks/spec 范围；无越界后端改动；无密钥入库风险。  
存在可接受剩余风险（未做浏览器真机点击、运行中二进制未嵌入新 dist），记为 WARN 级非阻塞项，不改变总体 PASS。

## Six-Dimension Table

| 维度 | 状态 | 结论 |
| --- | --- | --- |
| Scope | PASS | 仅 admin-ui 源码 + README 一句 + openspec change 工件；未改 src/**、Cargo.*、协议/选号 |
| Design | PASS | 符合 D1–D6：Dashboard+Card 入口、Dialog 查看/测试、axios+x-api-key、全量失败明细、不改后端；live 开关已做 |
| Scenarios | PASS | 4 条 ADDED Requirements / 全部 Scenario 均有源码对应 |
| Project Rules | PASS | OpenSpec 工件齐全；bridge/apply evidence 存在；Surgical；无 credentials/config 提交；validate 通过 |
| Verification | PASS (WARN residual) | 本会话真实跑过 tsc/vite build/validate/静态冒烟；浏览器真机与 cargo 嵌入 SKIPPED 已明示 |
| README/AGENTS Sync | PASS | README Admin UI 已补入口；AGENTS/spec 长期架构无需改；主 specs 归档时再 sync delta |

## Scenario Traceability

### R1 Dashboard 全量模型刷新

| Scenario | 证据 |
| --- | --- |
| 全量刷新按钮可见 | dashboard.tsx 文案「刷新全部模型」；dist JS 含同文案 |
| 部分失败展示 | handleRefreshAllModels toast + ModelsRefreshResultDialog 展示 rrors[{credentialId,error}] |
| 进行中防重复 | 
efreshingAllModels early-return；按钮 disabled={refreshingAllModels || !credentials.length} |

### R2 卡片模型查看与刷新

| Scenario | 证据 |
| --- | --- |
| 查看模型缓存 | CredentialModelsDialog → getCredentialModels(id, false)；展示 models/updatedAt/lastError |
| 单凭据刷新成功 | 卡片「刷新模型」与 Dialog 内刷新 → 
efreshCredentialModels；toast 含 count |
| 刷新失败可诊断 | xtractErrorMessage toast/error 区；不渲染 token 字段 |

### R3 卡片真实推理测试

| Scenario | 证据 |
| --- | --- |
| 默认模型 | 	estCredential(id, undefined) → body {}（model.trim() 空则省略） |
| 指定模型 | Input 非空 → { model: trimmed } |
| 失败不泄密 | 仅 xtractErrorMessage / 业务 reply 字段 |

### R4 前端 API 封装

| Scenario | 证据 |
| --- | --- |
| 路径与鉴权 | credentials.ts：/credentials/models/refresh、/{id}/models/refresh、/{id}/models、/{id}/test；拦截器 x-api-key |

## Scope Diff Inventory

**Modified tracked:**

- README.md (+1 Admin UI bullet)
- dmin-ui/src/types/api.ts
- dmin-ui/src/api/credentials.ts
- dmin-ui/src/components/dashboard.tsx
- dmin-ui/src/components/credential-card.tsx

**New untracked (expected):**

- dmin-ui/src/components/credential-models-dialog.tsx
- dmin-ui/src/components/credential-test-dialog.tsx
- dmin-ui/src/components/models-refresh-result-dialog.tsx
- openspec/changes/admin-ui-model-ops-entrypoints/**

**Not modified (good):**

- src/**（含 admin handlers/router/service）
- Cargo.toml / Cargo.lock
- dmin-ui/src/hooks/use-credentials.ts（task 1.3 可选；组件本地 state 等价）
- online-auth / import 对话框

## Non-Goals Check

| Non-Goal | 遵守？ |
| --- | --- |
| 不新增后端 endpoints | 是 |
| 不重做 online-auth/import | 是 |
| 不做批量并发 test | 是 |
| 不改选号 / /v1/models 后端 | 是 |
| 不强制上游 test 200 | 是（验收为入口+错误可展示） |

## Project Rules

- OpenSpec：proposal/design/tasks/specs + bridge-plan + apply-session + 本报告
- Surgical：仅 UI 消费面
- 安全：git status 无 config.json/credentials.json/token 探测残留
- 验证纪律：只报告真实命令；SKIPPED 写明

## Verification Evidence (this / apply session)

| Command / Check | Result |
| --- | --- |
| pnpm --dir admin-ui exec tsc -b --pretty false | exit 0 |
| pnpm --dir admin-ui exec vite build | exit 0；dist 生成 |
| dist 文案静态检查 | 含「刷新全部模型」「查看模型」「开始测试」 |
| openspec validate --all | 9 passed, 0 failed（本审查会话复跑） |
| git status --short | 仅预期 admin-ui/README/openspec；无密钥文件 |
| 浏览器真机点击运行中 /admin | **SKIPPED** — 运行二进制未嵌入新 dist；上游账号或 suspended |
| cargo build 嵌入 dist | **SKIPPED** — 本 change 默认不强制；运维需 rebuild 后才在进程内可见 |

## Findings

### PASS notes

1. API 路径与后端 src/admin/router.rs 契约对齐（camelCase types 对齐 serde）。
2. 禁用凭据：仍可「查看模型」；「刷新模型/测试」disabled。
3. live 拉取：Dialog「实时拉取」→ getCredentialModels(id, true)。

### WARN (non-blocking)

1. **W1** 运行中 kiro-rs 进程未嵌入新 UI：用户访问当前 127.0.0.1:18990/admin 仍可能是旧前端，直到 pnpm build + 重编二进制。
2. **W2** 无浏览器 E2E：依赖源码+dist 静态证据；交互回归留给本地 rebuild 后人工点一次。
3. **W3** task 1.3 hooks 未改：用组件 state，功能等价，符合可选任务。

### FAIL

无。

## CRITICAL

无。

## Remaining Risks

1. 发布物若只提交源码未 rebuild 嵌入，运维二进制 UI 滞后。  
2. 上游账号 suspended 时全量刷新/test 仍失败——属账号态，非 UI 合规缺口。  
3. 归档前若需主规格 openspec/specs/admin-ui-model-ops，走 archive/sync，本审查不强制已 sync。

## Evidence Paths

- openspec/changes/admin-ui-model-ops-entrypoints/proposal.md
- openspec/changes/admin-ui-model-ops-entrypoints/design.md
- openspec/changes/admin-ui-model-ops-entrypoints/tasks.md
- openspec/changes/admin-ui-model-ops-entrypoints/specs/admin-ui-model-ops/spec.md
- openspec/changes/admin-ui-model-ops-entrypoints/evidence/bridge-plan.md
- openspec/changes/admin-ui-model-ops-entrypoints/evidence/apply-session.md
- openspec/changes/admin-ui-model-ops-entrypoints/evidence/spec-compliance-report.md（本文件）

## Recommendation

**可以进入 verify-before-completion / archive 流程。**  
建议归档前（或发布前）执行一次：pnpm --dir admin-ui build + cargo build --release，用新二进制打开 /admin 点四入口做最终人工确认。
