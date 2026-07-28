# OpenSpec Verify Report: public-api-catalog-admin-display

核验日期：2026-07-28（第四轮：completion 门禁已补齐、工件残留引用已清）
结论：**PASS** — 可归档

## 第四轮复核（2026-07-28）

| 项 | 结果 |
| --- | --- |
| `openspec validate public-api-catalog-admin-display --strict` | **change is valid** |
| `openspec validate --all` | 15 passed, 0 failed |
| `cargo test` | **500 passed, 0 failed** |
| tasks | 29/29，`[ ]` = 0、`[~]` = 0 |
| Requirement→Scenario | 9 req / 22 scen（删除 1 条已为假的 Scenario 后），`requirements_without_scenario = 0` |
| `git check-ignore` / 敏感文件 | 三者均被忽略；候选提交无敏感文件 |
| catalog 实测 | 7 live / 1 planned，planned 为 `GET /v1/responses/{id}` |

**本轮修正的工件残留引用**（第三轮删除 Scenario 后遗留的矛盾表述）：

- `proposal.md:20` 的 What Changes「OpenAI 端点先登记为 planned」加时点注记，说明 Phase B/C 已翻转、本能力的 spec 不再断言其具体状态
- `tasks.md:5`（1.3 登记 3 条 planned）加时点注记：现存 planned 仅 1 条

**改写后的表述有实例支撑**：第三轮把 Requirement 正文改为「其他协议端点未挂载时登记为 planned」，本轮核对 `catalog.rs` 仍有 1 个 `EndpointStatus::Planned` 实例（`:192`），且防漂移测试 `test_planned_endpoints_are_not_mounted` 遍历该集合非空——不是空集假通过。

**completion 门禁已补齐**：`evidence/verification-before-completion.md` 已写入，内含防漂移契约的运行时闭环（7 live 逐条 200 / planned 404 / 别名 404 / 无 key 401）、密钥不泄漏的程序化断言、文档同步表与 5 项剩余风险。

---

## 第三轮处置记录（2026-07-28）

发现 1 的阻塞项已按选项 2 处置：

- 删除 `specs/public-api-catalog/spec.md` 的 Scenario「OpenAI 端点登记为 planned」（原 `:73-76`，断言 chat 与 responses 均为 planned，已为假）
- 改写该 Requirement 正文：明确本能力只持有「注册表机制」与既有 Anthropic 端点的 live 集合，其他协议端点的具体状态归实现它的 change
- `design.md:58` 后加时点注记，说明 canonical 表为 Phase A 快照、Phase B/C 已把两者翻转为 Live（表格作为历史设计记录保留不改）

复核：`openspec validate --all` **15 passed, 0 failed**；`cargo test` **500 passed, 0 failed**。

另同轮补齐：`spec/requirements.md` 增加 OpenAI 两端点与端点注册表能力条目、`spec/structure.md` 目录树增加 `src/openai/` 与 `src/public_api/`（原发现于 chat change 报告发现 3）。

运行时闭环验证（本轮实测，原报告缺失项）：catalog 报 7 live / 1 planned，7 个 live 端点逐个实测可路由（全 200），planned 的 `GET /v1/responses/{id}` 实测 404，别名 `/messages`、`/chat/completions` 均 404，无 key 401。`GET /api/admin/public-api` 无 key 401、带 adminApiKey 200，响应体不含完整 key（`apiKeyMask: sk-k***3456`），`aliases` 全空，`suggestedBaseUrl` 为 `null`。防漂移契约在运行时成立。

---

## 第二轮记录（原文保留）

结论：**BLOCKED** — 1 项阻塞（spec 文本含已为假的断言，且无法由 sync 自动收敛）

## 本轮实际运行的命令

| 命令 | 结果 |
| --- | --- |
| `openspec list` | 4 个活跃 change，本 change ✓ Complete |
| `openspec status --change public-api-catalog-admin-display --json` | proposal / specs×2 / design / tasks 的 `existingOutputPaths` 均非空，非 blocked |
| `openspec validate --all` | **15 passed, 0 failed**；`✓ change/public-api-catalog-admin-display` |
| `cargo test` | **500 passed; 0 failed; 0 ignored** |
| Requirement→Scenario 扫描脚本 | `requirements_without_scenario = 0` |
| `git status --short` | 无 `.env` / `credentials.json` / `.codegraph/`；见文末 |

## 三维核验

### Completeness — PASS（1 项勾了但单测不存在）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 核心工件 | PASS | 齐全；specs 含 `public-api-catalog` 与 `admin-public-api-view` |
| tasks | PASS | 29/29 完成，`- [ ]` 计数为 0 |
| Requirement→Scenario | PASS | 9 Requirements / 23 Scenarios，无空 Requirement（脚本确认） |
| evidence: Verification | PASS（已失效，见发现 3） | `verification.md` |
| evidence: Compliance | PASS | `spec-compliance-report.md`（WARN，2 项已修） |
| **evidence: Bridge** | **缺失** | 见发现 2 |
| **evidence: Completion** | **缺失** | 见发现 2 |

本轮核到的落地物：`src/public_api/{mod,catalog,dto,routes_test}.rs`；路由 `src/admin/router.rs:50`；handler `src/admin/handlers.rs:356-361`；service `src/admin/service.rs:1225-1241`；启动日志 `src/main.rs:220-224`；前端 `admin-ui/src/types/api.ts:199-234`、`admin-ui/src/api/public-api.ts`、`admin-ui/src/components/public-api-panel.tsx`、入口 `dashboard.tsx:594` + `:848`。

**勾了但找不到落地物：tasks 3.4 的「未认证 401」单测。** 全仓无任何测试构造 `AdminState` 或 `create_admin_router`（`src/admin/` 下仅 `error.rs:74`、`service.rs:1506`、`types.rs:550` 三个 test 模块，均为 service/type 级）。鉴权在代码层成立（`router.rs:50` 的 `/public-api` 位于 `:80-83` 的 `admin_auth_middleware` layer 之内），`verification.md:74-75` 也有 401 的 curl，且本轮实测复现（无 key → HTTP 401，`{"type":"authentication_error"}`）。同项另两条有测试：`service.rs:1773`、`dto.rs:179`。

### Correctness — PASS（机制成立；evidence 已被后续 change 覆盖成失效快照）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| Scenario 意图满足 | PASS | 23 项均有实现或测试对应 |
| 防漂移契约有效性 | PASS（破坏验证已不可复现） | 双向断言在 `routes_test.rs:44`（live 非 404/405）、`:64`（planned 必须 404）、`:80`（别名 404）、`:98`（live 必须 401） |
| catalog 完备性 | PASS | `catalog.rs:228-365` 共 11 项测试；DTO 8 项 |
| 密钥约束 | PASS | `service.rs:1773-1791` 正则断言全文无完整 key；掩码链路 `get_public_api` → `get_auth_settings` → `mask_api_key`（`service.rs:1117-1126`），永不出明文 |
| 本轮实测 | PASS | `GET /api/admin/public-api` 带 adminApiKey → 200，families/endpoints 完整，`apiKeyMask` 为 `sk-k***3456` 形式 |
| 非目标未被越界 | PASS | aliases 全空且有测试锁定（`catalog.rs:265`）；`suggestedBaseUrl` 恒为 `None`（`service.rs:1239`）；未改 `/settings/endpoint` 语义 |

### Coherence — **FAIL（1 项阻塞）**

| 项 | 状态 | 说明 |
| --- | --- | --- |
| **spec ↔ 代码现状** | **FAIL** | 见发现 1 |
| proposal ↔ 实际改动 | PASS | Impact 已含 dev-dependency 声明 |
| design ↔ 实现 | WARN | 掩码规则口径不符，见发现 4 |
| 命名纪律（§1） | PASS | 全部用 `publicApi`/`public-api`，无裸 `endpoint` |
| README 同步 | PASS | `README.md:175`、`:580-586`，TOC `:70` |
| 依赖登记 | PASS | `tower` 已登记 `docs/tooling-sources.md:17-26`；`Cargo.toml:40-41` 固定 `0.5.2 features=["util"]`；`git show HEAD:Cargo.lock` 确认 tower 原本已在（无新 crate 引入） |
| 面板文案 ↔ spec | PASS | planned 标「未启用」（`public-api-panel.tsx:26`、`:201-205`）、上游/对外区分（`:97-100`、`:264-268`）、`OPENAI_BASE_URL` 带 `/v1`（`:253-257`）、Models 需鉴权（`:275-277`）、`/cc/v1` 缓冲差异（`:270-273`）、Base URL 只影响展示（`:41-42`、`:83-87`） |

## 发现项

### 1. 【阻塞】spec 含已为假的断言，且无法由 `openspec-sync-specs` 自动收敛

`specs/public-api-catalog/spec.md:64-76` 的 ADDED Requirement「注册表登记的初始端点集合」：

- `:66` 正文：「MUST include the planned OpenAI-compatible endpoints with status `planned`」
- `:73-76` Scenario「OpenAI 端点登记为 planned」：「`POST /v1/chat/completions` 与 `POST /v1/responses` MUST 存在且 status 为 planned」

**当前代码两者均为 Live**（`src/public_api/catalog.rs:151` chat、`:174` responses），且均已挂载（`src/openai/mod.rs:29-30`，`main.rs:174` merge）。测试 `catalog.rs:294 test_chat_completions_live`、`:303 test_responses_live_retrieve_still_planned` 正面锁定 Live。design.md §3 canonical 表（`:54-55`）同样标 Planned。

前一轮报告（发现于「归档前必须处理」条）判断可由 `openspec-sync-specs` 在 B/C 归档后收敛。**本轮复核推翻该判断**：

```
public-api-catalog-admin-display/specs/  →  admin-public-api-view  public-api-catalog
openai-chat-completions-compat/specs/    →  openai-chat-completions
openai-responses-api-compat/specs/       →  admin-runtime-settings  openai-responses  openai-responses-websearch
```

B 与 C 的 specs 目录里**都没有 `public-api-catalog` 能力文件**，它们的状态翻转写在各自的能力内（`openai-chat-completions/spec.md:215-232`、`openai-responses/spec.md:223-235`）。因此没有任何后续 change 会覆盖 `public-api-catalog` 这条 Scenario——原样归档会在 `openspec/specs/public-api-catalog/spec.md` 留下一条与代码永久相反的断言。

处置选项（需维护者决定，本次未擅自改动 spec）：

1. 归档前直接修正本 change 的 `spec.md:66` 与 `:73-76`，改为反映终态（chat/responses 为 live，`/v1/responses/{id}` 仍 planned）。最简，但改的是 Phase A 的历史陈述。
2. 删掉 `:73-76` 这条 Scenario，把 OpenAI 端点状态完全交给 B/C 的能力表达；`:66` 正文同步去掉 planned 子句。
3. 在 B 或 C 补一个 `public-api-catalog` 的 MODIFIED 能力文件覆盖该 Requirement，再按 A → B → C 顺序归档。最合规，但需新增工件。

推荐选项 2：Phase A 建立的是「注册表机制」，具体端点状态属于 B/C 的职责，本就不该在 A 的 spec 里硬编码 OpenAI 端点的瞬时状态。

同类问题在 `openai-chat-completions-compat` 也存在一条（其 spec `:224-227` 断言 responses 仍 planned），见该 change 的 verify 报告发现 1。

### 2. `bridge-plan.md` 与 `verification-before-completion.md` 缺失

AGENTS.md Skills 门禁要求实现前跑 `openspec-superpowers-bridge`、归档前跑 `verification-before-completion`。本批次四个 change 均缺两者。

- **bridge 不事后补写**：它是「实现前检查点」，补写会伪造流程时序，违反「只报告本会话真实运行过的命令」。对本 change 影响最小：全新模块 + Admin 只读接口，无既有调用链改动。
- **completion 可补**：本轮查证 `openspec/changes/archive/` 下 **6 个已归档 change 全部有**该文件（`2026-07-21-improve-credential-ingest` 等），4 个待归档 change 全部没有。相对惯例是明确缺口，且现在跑仍在正确时序内。

### 3. `verification.md` 是 Phase A 时点快照，多处已失效

不是造假，但按可复现口径必须点出：

| evidence 位置 | 记录内容 | 当前现状 |
| --- | --- | --- |
| `:32`、`:49` | 引用测试 `test_openai_endpoints_planned` | **源码中已不存在**（全仓 grep 0 命中），被替换为 `catalog.rs:294`/`:303` |
| `:38` | 「21 passed」 | `public_api` 现有 11+8+4 = 23 个测试 |
| `:94-96`、`:110-111` | `/v1/chat/completions`、`/v1/responses` 为 planned / 404 | 两者均为 Live 且已挂载 |
| `:41-55` | 破坏验证（改 chat.completions 为 Live → 5 项转红） | 按现状不可复现 |
| — | `routes_test.rs:18-24` | 已被后续 change 改为需 merge `crate::openai::create_openai_routes`；Phase A 时 `src/openai` 不存在 |

`spec-compliance-report.md:61-67` 的发现 3（`.github/workflows/_runs.json`）亦已失效：该文件当前不在 `.github/workflows/` 下，也不在 `git status` 中，无需处置。

若归档要求 evidence 可复现，需重跑或在文首加注时点说明。

### 4. design ↔ 实现的掩码口径不符（WARN，功能不受影响）

design §5（`:111`）与 tasks 3.3 写「掩码规则沿用 `src/main.rs:212` 现有写法（前半 + `***`）」，实现实际复用 `AdminService::mask_api_key`（前 4 + `***` + 后 4，`service.rs:1123-1125`）；`main.rs:217` 的内联掩码仍是各自一套。功能上仍满足 spec「只回掩码、不回明文」，仅与设计文本口径不同。

### 5. CodeGraph 影响面分析未执行（批次级 WARN）

详见 `openai-responses-api-compat` 的 verify 报告。本 change 以新增模块为主，漏判风险低。

## 证据路径

- `openspec/changes/public-api-catalog-admin-display/evidence/verification.md`（Phase A 快照，见发现 3）
- `openspec/changes/public-api-catalog-admin-display/evidence/spec-compliance-report.md`
- 本文件

## 归档前必须处理

1. **发现 1（阻塞）**：修正 `specs/public-api-catalog/spec.md:66` 与 `:73-76`，否则 `openspec/specs/` 会留下永久假陈述。推荐选项 2。
2. 发现 2：`verification-before-completion` 门禁（或明确以本报告为等价产出）。
3. 发现 3：为 `verification.md` 加时点说明，或重跑关键验证。

归档次序：本 change 是 B/C 的机制前序，仍应先归档，但**必须先解决发现 1**。

## 本轮未验证项

- `pnpm build` 本轮运行过（用于部署二进制，成功），但未针对本 change 做产物比对。
- Playwright 渲染结论、admin-ui 浏览器行为未复现。
- `verification.md` 中的历史 curl 输出与启动日志未逐条复现；本轮仅复现了 `/api/admin/public-api` 的 401（无 key）与 200（带 adminApiKey）两次。

## git status 检查

无 `config.json`、`credentials.json`、`.env`、`.codegraph/` 进入变更集。未跟踪项为本批次四个 change 目录、`src/openai/`、`src/public_api/`、两个 admin-ui 新文件与 `docs/multi-protocol-api-design.md`，均为预期产物。
