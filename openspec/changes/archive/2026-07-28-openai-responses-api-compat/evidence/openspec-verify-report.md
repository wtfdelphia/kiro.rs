# OpenSpec Verify Report: openai-responses-api-compat

核验日期：2026-07-28（第四轮：completion 门禁已补齐、次序依赖已满足）
核验范围：本 change（同批次另三个 change 各有独立报告）
结论：**PASS** — 可归档

## 第四轮复核（2026-07-28）

| 项 | 结果 |
| --- | --- |
| `openspec validate openai-responses-api-compat --strict` | **change is valid** |
| `openspec validate --all` | 15 passed, 0 failed |
| `cargo test` | **500 passed, 0 failed** |
| tasks | 65/65，`[ ]` = 0、`[~]` = 0 |
| Requirement→Scenario | 20 req / 67 scen，`requirements_without_scenario = 0` |
| `git check-ignore` / 敏感文件 | 三者均被忽略；候选提交无敏感文件 |
| catalog 实测 | `/v1/responses` = live 且 200；`/v1/responses/{id}` = planned 且 404 |

**归档次序依赖已满足**：前两个 change 的 spec 阻塞项（断言 OpenAI 端点为 planned）已在第三轮解除，本 change 可作为最后一个归档。

**本轮修正的工件表述**：

- `proposal.md` 的「待前序 change 归档后由 `openspec-sync-specs` 收敛」补充说明：该设想**不成立**（三个 change 的 `specs/` 互不包含对方能力文件，sync 无可覆盖目标）。本 change 的 spec 断言与代码一致，无需后续收敛。
- `tasks.md:22`（3.1）加命名注记：实现未抽出 `build_responses_object`，改为 `handlers.rs:1097` 内联构造
- `tasks.md:24`（3.3）加覆盖方式注记：非流式构造路径无直接单测，由序列化形状测试 + 流式侧测试 + 真实凭据 curl 覆盖

**本 change 的 spec ↔ 代码一致性是四个中唯一完全通过的**：`openai-responses/spec.md:223-235` 断言 responses = live、retrieve 仍 planned，两者均与代码及实测吻合（`catalog.rs:174` Live / `:192` Planned）。

**配置注释交叉核对**：`config.rs:122-123` 的注释称判定「含 `web_search_20250305` 等形状」，已核对 `src/openai/websearch.rs:38` 的 name 白名单确实含该值，注释与实现一致。

**completion 门禁已补齐**：`evidence/verification-before-completion.md` 已写入，内含 Admin 开关六步端到端实测（401 / 掩码 / 落盘 / 热更新生效 / 不影响 Anthropic / 可逆）、11 个语义事件序列、D2 400、D11 双判定、`config.example.json` 可加载性验证与 6 项剩余风险。

---

## 第二轮记录（原文保留）

结论：**WARN** — 本 change 自身无阻塞项；但归档次序依赖前两个 change 的阻塞项先解决

## 本轮实际运行的命令

| 命令 | 结果 |
| --- | --- |
| `openspec list` | 4 个活跃 change，本 change ✓ Complete |
| `openspec status --change openai-responses-api-compat --json` | proposal / specs×3 / design / tasks 的 `existingOutputPaths` 均非空，非 blocked |
| `openspec validate --all` | **15 passed, 0 failed**；`✓ change/openai-responses-api-compat` |
| `cargo test` | **500 passed; 0 failed; 0 ignored** |
| `cargo build --release` | Finished（0 error） |
| Requirement→Scenario 扫描脚本 | `requirements_without_scenario = 0` |
| 实测 `POST /v1/responses` | **HTTP 200**，`resp_` id、`status: completed`、`output[0].content[0].type == "output_text"`、usage 三字段齐 |

规模：65 项 tasks 全部完成，20 Requirements / 67 Scenarios（`openai-responses` 12/39、`openai-responses-websearch` 6/18、`admin-runtime-settings` 2/10）。

## 三维核验

### Completeness — PASS（1 处命名不符、1 处覆盖方式偏差）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 核心工件 | PASS | proposal / design / tasks / specs（3 个能力）齐全 |
| tasks | PASS | 65/65 完成，`- [ ]` 计数为 0 |
| Requirement→Scenario | PASS | 20 Requirements 全部至少 1 个 Scenario，共 67 个（脚本确认） |
| evidence: Verification | PASS | `verification.md` + `live-upstream-verification.md` |
| evidence: Compliance | PASS | `spec-compliance-report.md`（PASS，但见发现 5） |
| specs 能力数 | PASS | 3 个：`openai-responses`、`openai-responses-websearch`、`admin-runtime-settings`（MODIFIED + ADDED） |
| **evidence: Bridge** | **缺失** | 见发现 3（不补写） |
| **evidence: Completion** | **缺失** | 见发现 4（归档前可补） |

代码落地齐全：`responses.rs`(622)、`responses_types.rs`(356)、`responses_stream.rs`(893)、`websearch.rs`(485)；`post_responses` 在 `handlers.rs:816`；路由 `mod.rs:30`。

Admin 侧齐全：`service.rs:1197` `get_websearch_settings` / `:1203` `update_websearch_settings`（含 `save_config` 落盘 `:1215`）、`handlers.rs:342`/`:346`、`router.rs:72-75`、`types.rs:641-648`、`config.rs:120-124`（`web_search_emulation`，默认 true 经 `default_web_search_emulation()`）。admin-ui 开关在 `settings-panel.tsx` + `api/settings.ts` + `types/api.ts:189`。本轮实测 `config.json` 的 `webSearchEmulation: true` 生效。

evidence 引用的 **58 个测试函数名逐个核对全部存在于源码**。

偏差项（非功能缺口）：

- tasks 3.1 写 `build_responses_object`，源码中无该名字的函数；实际在 `handlers.rs:1097 handle_responses_non_stream` 内联构造（message item + function_call items，`:1145-1157`）。功能在位，仅任务里的函数名与实现不符。
- tasks 3.3「单测：output 结构、工具名、usage 优先级」：非流式 `ResponsesObject` 的构造路径**无直接单测**（grep 未找到任何测试调用 `handle_responses_non_stream`）。相关断言都在流式侧（`responses_stream.rs`）与 `responses_types.rs` 的序列化形状测试上；真实凭据 curl（`live-upstream-verification.md` §1）与本轮实测覆盖了它。属覆盖方式与任务描述的偏差。

### Correctness — PASS

| 项 | 状态 | 说明 |
| --- | --- | --- |
| Scenario 意图满足 | PASS | 关键项（连续 function_call 合并、pending parts 归集、output_index 管理、D2 报 400、web_search 宽判定与开关）均有专项测试 |
| D2 无状态 | PASS | `responses.rs:16-23` 返 `InvalidRequest`(400)；`handlers.rs:830` 把归一放在 provider 检查**之前**——正是 evidence §4 记录的顺序修正，代码与记录一致；HTTP 层测试 `mod.rs:188` |
| D11 宽判定 | PASS | `websearch.rs:29-39`（type 前缀 / `google_search` / name 三值，大小写不敏感）；拦截条件恰好一个 tool（`:45-53`） |
| Anthropic 侧判定零改动 | PASS | `src/anthropic/websearch.rs:108-112` 仍是 `tools.len() == 1 && name == "web_search"`，`git diff` 中该函数只在新增测试里被引用 |
| 运行时开关立即生效 | PASS | `handlers.rs:845-850` 每请求读 `p.token_manager().config().web_search_emulation`（`provider.rs:138`、`token_manager.rs:783`），无需重启成立 |
| 语义 SSE 带 `event:` 行 | PASS | `responses_stream.rs:39`；`[DONE]` `:40`；保活注释行 `:41`；`output_index` 管理在 `ResponsesStreamContext`（`:68`），测试 `:713` |
| usage 不伪造上游信号 | PASS | web_search 路径用 `estimate_chars`（`handlers.rs:908`/`:966`），不读 `context_input_tokens` |
| `admin-runtime-settings` MODIFIED 表达合法 | PASS | 已归档 spec 的 Requirement「设置变更安全与校验」标题逐字一致（`openspec/specs/admin-runtime-settings/spec.md:81` vs change spec `:3`），可被 sync 正确匹配 |
| 成功标准达成 | PASS | 真实凭据下非流式与流式语义事件均验证通过；本轮另实测非流式 HTTP 200 |
| 非目标未被越界 | PASS | 未实现 retrieve / 持久化（`catalog.rs:192` 仍 Planned）；未改 Anthropic 侧判定 |

### Coherence — PASS（1 项报告表述需加注）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| proposal ↔ 实际改动 | PASS | Modified Capabilities 已声明 `admin-runtime-settings`（发现 1 修复后） |
| 能力边界 | PASS | 开关的读写/鉴权/持久化契约归 `admin-runtime-settings`；「开关变更立即影响本端点」留在 websearch 能力（发现 2 修复后） |
| design ↔ 实现 | PASS | D2/D10/D11/D12 逐条落地；实现中发现的顺序问题已修并记录 |
| catalog ↔ 文档 ↔ 实现 | PASS | `catalog.rs:167-184` Live + hints 含 previous_response_id / web_search / `event:`（测试 `:322` 锁定）；`retrieve` = Planned（`:192`），三处一致 |
| README / 设计定稿同步 | PASS | `README.md:536`、`:548-557`、`:578`；`docs/multi-protocol-api-design.md:47`（P2 已解决）、`:266`（Live）、`:764`（Phase C 已完成）、`:651-655`（记录 Anthropic websearch 非流式问题由第三个 change 修复） |
| 与 chat change 无实现冲突 | PASS | 共用 `prepare`（`handlers.rs:103`）而非各写一套；共用 `OpenAiTool` 双形状；**都未注册 `/v1/models`**（grep 确认只在 `anthropic/router.rs`）；model resolution 走同一 `resolution_context_from_state` + `convert_request_with_policy`，无分叉 |
| **spec ↔ 代码现状** | PASS | 本 change 的三个 spec 中**无已为假的断言**：`openai-responses/spec.md:223-235` 声明 responses = live、retrieve 仍 planned，两者均与代码一致（`catalog.rs:174` Live、`:192` Planned） |
| compliance 报告表述 | WARN | 见发现 5 |

**本 change 是四个中 spec ↔ 代码一致性唯一完全通过的**：前两个 change 的 spec 各含一条已被本 change 推翻的 planned 断言（见各自报告发现 1）。

## 发现项

### 1. `admin-runtime-settings` 未声明为 Modified Capability（已修复）

本 change 新增 `GET/PUT /api/admin/settings/websearch`（`src/admin/router.rs`、`handlers.rs`、`service.rs`、`types.rs`）与配置字段 `webSearchEmulation`（`src/model/config.rs`）。

`admin-runtime-settings` **已归档**在 `openspec/specs/`，其现有 5 个 Requirement 覆盖 proxy / endpoint / auth / 设置安全 / client-identity，**不含 websearch**。原 proposal 声明「Modified Capabilities: 无」，理由对未归档的两个能力成立，但漏掉了已归档的 `admin-runtime-settings`。

**处置（方案 A，改动限于工件 + 补测试，未动功能代码）**：

1. 新增 `specs/admin-runtime-settings/spec.md`：`## MODIFIED Requirements`（「设置变更安全与校验」扩展到新增设置组）+ `## ADDED Requirements`（「Admin 可读写 web 搜索代执行开关」，7 个 Scenario）
2. 更新 proposal 的 Modified Capabilities，保留「为何另两个能力不作为 Modified」的说明
3. 补两条测试锁定原本仅靠结构保证的 Scenario：`service.rs:test_websearch_response_has_no_unrelated_secrets`、`websearch.rs:test_detection_independent_of_websearch_emulation_switch`
4. tasks 追加 6.7 / 6.8 记录该补漏

本轮复核确认修复到位：MODIFIED 的 Requirement 标题与已归档 spec `:81` 逐字一致，可被 sync 匹配；两条新测试在源码中存在（`service.rs:1773`、`anthropic/websearch.rs:1044`）且 `cargo test` 500 passed。

未采用方案 B（拆独立 change）：开关本身是运行时设置的一员，与 proxy/auth/client-identity 同类，独立成 change 会让契约更碎。

### 2. Scenario「通过 Admin 热更新」的归属偏差（已修复）

原 Scenario 描述 Admin 接口行为，实质属 `admin-runtime-settings` 契约范围。已改写为「开关变更立即影响本端点」，只描述本端点对开关状态变化的响应，并加注指向 `specs/admin-runtime-settings/spec.md`。

### 3. `bridge-plan.md` 缺失（批次级，不补写）

AGENTS.md 要求「开始实现前：openspec-superpowers-bridge」，产出 `evidence/bridge-plan.md`（对照 `archive/2026-07-27-model-resolution-identity-dark-ui/evidence/bridge-plan.md`）。本批次四个 change 均未执行。

**不补写**：bridge-plan 定位是「实现前检查点」，事后补写会伪造流程时序，违反「只报告本会话真实运行过的命令与结果」。

实际风险：bridge 的核心作用（范围/非目标核对、影响面分析、任务到验证的映射）由 design.md 与 tasks.md 实质承担。缺的是执行前的一次性交叉核对与 CodeGraph 影响面证据（见发现 6）。

### 4. `verification-before-completion.md` 缺失（归档前可补）

本轮查证：`openspec/changes/archive/` 下 **6 个已归档 change 全部有**该文件，4 个待归档 change **全部没有**。该门禁是归档前动作，现在跑仍在正确时序内。

### 5. compliance 报告的「仅改 4 处可见性」表述需加注

`spec-compliance-report.md:14` 写「`anthropic/websearch.rs`（**仅改 4 处可见性**）」。但当前 `src/anthropic/websearch.rs` **有真实逻辑改动**（`build_websearch_non_stream_response`、`wants_stream`、抽取 `build_websearch_blocks`）。

核对结论：这些逻辑改动属同批次第三个 change `anthropic-websearch-non-stream`（其 proposal/tasks 逐项对应，`docs/multi-protocol-api-design.md:651` 亦如此记录），本 change 确实只改了可见性。**但工作区是三个 change 的叠加 diff**，报告表述在写作当时成立，归档时若有人拿当前 `git diff` 复核会得出矛盾结论。

建议在该报告加一句注明 `websearch.rs` 的逻辑改动归属 `anthropic-websearch-non-stream`。

### 6. CodeGraph 影响面分析未对本仓库执行（WARN，批次级）

本批次仅在调研 sub2api 时用过 `codegraph query`，未对 kiro.rs 自身跑过 `codegraph context/impact`。替代手段为 rg/Grep 与源码精读定位调用点与影响面。`verification.md` 中的 `.codegraph` 字样仅为 gitignore 检查，不构成使用证据。

剩余风险：可能存在未被 rg 覆盖的间接调用链。考虑到改动以新增模块为主、对既有代码只做可见性调整（`git diff` 已逐项核实），漏判风险较低。

### 7. 其他已声明的剩余风险

- thinking 内容在 Responses 端点被静默丢弃（`handlers.rs:1130-1139`，仅 `tracing::debug`），已声明并有测试锁定
- `google_search` 判定接受但 MCP 仍走 `web_search`，已声明
- admin-ui 无浏览器渲染验证
- **`webSearchEmulation` 未写入 README 配置字段表**（`README.md:295-322`）与 `config.example.json`，虽有 Admin API 文档（`README.md:176`）。建议归档前补入配置表。

## 证据路径

- `openspec/changes/openai-responses-api-compat/evidence/verification.md`
- `openspec/changes/openai-responses-api-compat/evidence/live-upstream-verification.md`
- `openspec/changes/openai-responses-api-compat/evidence/spec-compliance-report.md`
- 本文件

## 复核记录

2026-07-28 第一轮：修复发现 1、2 后由 FAIL 降为 WARN。

2026-07-28 第二轮（本次）：`openspec validate --all` 15 passed；`cargo test` **500 passed; 0 failed**；`POST /v1/responses` 实测 HTTP 200。新增发现 5、7 与 tasks 3.1/3.3 的偏差记录。结论维持 WARN。

## 归档前必须处理

1. 发现 4：`verification-before-completion` 门禁（或明确以本报告为等价产出）
2. 发现 5：为 `spec-compliance-report.md:14` 加注 websearch.rs 逻辑改动的归属
3. 发现 7：补 `webSearchEmulation` 到 README 配置字段表与 `config.example.json`
4. 归档次序：本 change 应**最后**归档。前提是 `public-api-catalog-admin-display` 与 `openai-chat-completions-compat` 各自的阻塞项（spec 含已为假的 planned 断言）先解决——本 change 的 specs 目录不含那两个能力文件，无法通过 sync 覆盖它们。

发现 3（bridge-plan 缺失）无法事后补救，建议在下一个 change 开始前先跑 bridge。

## 第三轮补验（2026-07-28，原「未验证项」已全部补齐）

发现 5 已处置：`spec-compliance-report.md:14` 已加注 `websearch.rs` 逻辑改动归属 `anthropic-websearch-non-stream`。
发现 7 已处置：`webSearchEmulation` 已写入 `README.md` 配置字段表与 `config.example.json`；并用当前二进制实测 example 配置可正常加载启动（无 panic，端点存活）。

**Admin 开关端到端（原报告明确列为未验证，本轮四条 Scenario 全部实测成立）**：

| 步骤 | 实测结果 |
| --- | --- |
| 无 key `GET /api/admin/settings/websearch` | **401** |
| 带 adminApiKey GET | `{"webSearchEmulation":true}`，不含 client/admin key |
| `PUT {"webSearchEmulation":false}` | `{"success":true,...}`，且 `config.json` 落盘为 `false` |
| 关闭后 `/v1/responses` + web_search | output 为 `['message']`，**`web_search_call` 消失 → 无需重启立即生效** |
| 同一时刻 `/v1/messages` + web_search | 四块结构 `[text, server_tool_use, web_search_tool_result, text]` 完好 → **不影响 Anthropic 端点** |
| `PUT true` 恢复 | output 恢复为 `['web_search_call', 'message']`，开关可逆 |

**Responses 流式语义事件（原未逐条复现）**：11 个事件，序列完整且带 `event:` 行——`response.created → in_progress → output_item.added → content_part.added → output_text.delta ×4 → content_part.done → output_item.done → completed`，`[DONE]` 存在。

**D2 无状态**：`previous_response_id` 实测 **HTTP 400**，错误信息为 `previous_response_id is not supported: ...`。

**D11 宽判定**：`{"type":"web_search"}` 与 `{"type":"google_search"}` 均实测 200 且触发代执行（output 含 `web_search_call`）。

**retrieve 仍 planned**：`GET /v1/responses/abc` 实测 **404**。

复核：`openspec validate --all` 15 passed；`cargo test` **500 passed, 0 failed**。

仍未验证：admin-ui 的 websearch 开关**浏览器渲染**行为（接口层已全部实测通过，仅前端交互未在浏览器中点击验证）。

## 第二轮未验证项（原文保留，已由第三轮补齐）

- `live-upstream-verification.md` 中的历史 curl 与流式语义事件序列未逐条复现；本轮仅实测一次非流式 HTTP 200。
- admin-ui 的 websearch 开关浏览器行为未验证。
- 未通过 Admin API 实际执行一次 `PUT /api/admin/settings/websearch` 的落盘验证（本轮只读确认了 `config.json` 中该字段存在且为 true）。
