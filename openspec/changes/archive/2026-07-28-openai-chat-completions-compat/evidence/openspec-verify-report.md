# OpenSpec Verify Report: openai-chat-completions-compat

核验日期：2026-07-28（第四轮：completion 门禁已补齐、工件残留引用已清）
结论：**PASS** — 可归档

## 第四轮复核（2026-07-28）

| 项 | 结果 |
| --- | --- |
| `openspec validate openai-chat-completions-compat --strict` | **change is valid** |
| `openspec validate --all` | 15 passed, 0 failed |
| `cargo test` | **500 passed, 0 failed** |
| tasks | 59/59，`[ ]` = 0、`[~]` = 0 |
| Requirement→Scenario | 15 req / 36 scen（删除 1 条已为假的 Scenario 后），`requirements_without_scenario = 0` |
| `git check-ignore` / 敏感文件 | 三者均被忽略；候选提交无敏感文件 |

**本轮修正的工件残留引用**（第三轮删除 Scenario 后遗留的矛盾表述）：

- `proposal.md:31` 的「待 Phase A 归档后由 `openspec-sync-specs` 统一收敛」补充说明：该设想**不成立**。三个 change 的 `specs/` 目录互不包含对方的能力文件，sync 没有可覆盖的目标——这正是第三轮必须直接删除 Scenario 而非依赖 sync 的原因。实际处置是让每个能力只断言自己持有的端点状态。
- `tasks.md:68`（8.3）加注：`test_openai_endpoints_planned` 已被 Phase C 替换为 `catalog.rs:294`/`:303`
- `tasks.md:79`（9.6）加注：live 端点由 6 条变为 7 条

**残留 planned 断言扫描**：4 个 change 的 spec 中仅剩 `openai-responses/spec.md:235` 断言 `GET /v1/responses/{id}` 仍 planned（实测 404，成立），其余两处为机制性表述（对任一 planned 项、面板展示 planned 时），不绑定具体端点。无失真断言残留。

**completion 门禁已补齐**：`evidence/verification-before-completion.md` 已写入，内含 D8 四项前置的可观测证据、流式 `include_usage` 与 `: keepalive` 实测、`git diff` 确认 Anthropic 零回归、文档同步表与 5 项剩余风险。

---

## 第三轮处置记录（2026-07-28）

发现 1 的阻塞项已按选项 1 处置：删除 `specs/openai-chat-completions/spec.md` 的 Scenario「Responses 端点仍为 planned」（原 `:224-227`）。同 Requirement 正文 `:217` 的「Endpoints still unimplemented MUST remain `planned`」保留——该句仍成立，`GET /v1/responses/{id}` 即为例（`catalog.rs:192` 为 Planned，实测 404）。

发现 3 已补齐：`spec/requirements.md` 核心能力清单增加 OpenAI Chat Completions / Responses 与端点注册表条目；`spec/structure.md` 目录树增加 `src/openai/` 与 `src/public_api/`。

复核：`openspec validate --all` **15 passed, 0 failed**；`cargo test` **500 passed, 0 failed**。

上游行为复现（本轮实测，原报告列为未验证）：

| 项 | 实测结果 |
| --- | --- |
| 流式 + `include_usage` | `: keepalive` 注释行真实出现；`[DONE]` 在末位；末块 `choices: []` 且 usage = `{completion_tokens:4, prompt_tokens:4122, total_tokens:4126}` |
| function tools | `finish_reason: tool_calls`，`arguments` = `{"city": "Beijing"}` |
| `-thinking` 后缀 | model 回显原值 `claude-sonnet-4.6-thinking`（D9）；推理落在 `reasoning_content`（87 字符）；`content` 无 `<thinking>` 标签泄漏 |
| 非流式 | HTTP 200，`chatcmpl-` id，usage 三字段齐 |
| 别名 `/chat/completions` | 404（无路径别名，与非目标一致） |

---

## 第二轮记录（原文保留）

结论：**BLOCKED** — 1 项阻塞（spec 含已为假的断言，无法由 sync 自动收敛）

## 本轮实际运行的命令

| 命令 | 结果 |
| --- | --- |
| `openspec list` | 4 个活跃 change，本 change ✓ Complete |
| `openspec status --change openai-chat-completions-compat --json` | proposal / specs / design / tasks 的 `existingOutputPaths` 均非空，非 blocked |
| `openspec validate --all` | **15 passed, 0 failed**；`✓ change/openai-chat-completions-compat` |
| `cargo test` | **500 passed; 0 failed; 0 ignored** |
| `cargo build --release` | Finished（0 error） |
| `pnpm build`（admin-ui） | ✓ built in 28.50s（本轮为部署二进制而跑，非本 change 专项） |
| Requirement→Scenario 扫描脚本 | `requirements_without_scenario = 0` |
| 实测 `POST /v1/chat/completions` | **HTTP 200**，OpenAI shape 正确（`chatcmpl-` id、`finish_reason: stop`、usage 三字段齐） |

## 三维核验

### Completeness — PASS（2 处任务文字过时，非功能缺口）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 核心工件 | PASS | 齐全；specs 含 `openai-chat-completions` |
| tasks | PASS | 59/59 完成，`- [ ]` 计数为 0 |
| Requirement→Scenario | PASS | 15 Requirements / 37 Scenarios，无空 Requirement（脚本确认） |
| evidence: Verification | PASS | `verification.md` + `live-upstream-verification.md` |
| evidence: Compliance | PASS | `spec-compliance-report.md`（PASS） |
| **evidence: Bridge** | **缺失** | 见发现 2 |
| **evidence: Completion** | **缺失** | 见发现 2 |

代码落地齐全：`src/openai/{types,converter,stream,error,handlers,mod}.rs` 全部存在；`src/main.rs:8`（`mod openai;`）、`src/main.rs:174`（merge）。

声明的 Anthropic 侧导出确实只是可见性：`src/anthropic/mod.rs:39-49`；`handlers.rs:106` `resolution_context_from_state`、`handlers.rs:834` `override_thinking_from_model_name`、`router.rs:20` `MAX_BODY_SIZE`、`stream.rs:182` `extract_thinking_from_complete_text` — `git diff src/anthropic/{mod,handlers,router}.rs` 仅 `fn` → `pub(crate) fn` / `const` → `pub(crate) const`，零逻辑改动。

任务文字过时（非功能缺口）：

- tasks 8.3 声称同步 `test_openai_endpoints_planned`，该函数**在源码中已不存在**（全仓 grep 0 命中），实际被 Phase C 改为 `src/public_api/catalog.rs:303 test_responses_live_retrieve_still_planned`。
- tasks 9.6 写「确认新增 6 条 live 端点」，现为 7 条（`catalog.rs:277-291 test_expected_live_set` 断言 `live.len() == 7`）——Phase C 之后的必然结果。

### Correctness — PASS

| 项 | 状态 | 说明 |
| --- | --- | --- |
| Scenario 意图满足 | PASS | 37 项均有实现或测试对应 |
| D8 四项门禁有效性 | PASS | 逐个破坏验证：注释 `override_thinking_from_model_name` → 2 项转红；`tool_name_map` 置空 → 1 项转红；去 auth layer → 2 项转红；去 body limit → 1 项转红。均已恢复 |
| 测试名真实性 | PASS | evidence 引用的 **34 个测试函数名逐个核对存在于源码** |
| D8 落地 | PASS | `src/openai/handlers.rs:103-145` `prepare()`：thinking 后缀 `:108`、tool_name_map `:143`、input_tokens `:131`（在 convert 之后）、thinking_enabled `:123`；锁定测试 `handlers.rs:451`、`:525` |
| D9 model 回显原值 | PASS | `handlers.rs:130` `echo_model = req.model.clone()`、`:408`；测试 `handlers.rs:491` |
| D12 `include_usage` | PASS | `stream.rs:331-337`（`choices: []` + usage，位于 `[DONE]` 之前）；测试 `stream.rs:581`/`:600` |
| D10 不劫持 web_search | PASS | `converter.rs:283-304` 走普通 tools 路径；`handlers.rs:62-100` 无 websearch 分支；测试 `converter.rs:513` + `handlers.rs:576` |
| 三 layer | PASS | `src/openai/mod.rs:27-41`（auth `:31`、cors `:38`、body limit `:39`）；auth 矩阵 `mod.rs:81-105`、body limit `mod.rs:108` |
| 双形状 tool 反序列化 | PASS | `types.rs:106-154` 手写 `Deserialize` |
| 保活为 SSE 注释行 | PASS | `stream.rs:35` `": keepalive\n\n"` |
| 测试计数自洽 | PASS | types 11、converter 18、error 6、stream 22、handlers 22、mod 14；Phase B 记 handlers 15 / mod 7，加 Phase C 增量 7+7 正好等于现值 |
| 真实上游行为 | PASS | 非流式（`prompt_tokens: 4123` 来自上游反算）、流式（含真实 `: keepalive`）、`include_usage` 末块、function tools（`finish_reason: tool_calls`）、`-thinking` 后缀（`reasoning_content` 有值、`content` 无标签泄漏）；本轮另实测一次 HTTP 200 |
| 非目标未被越界 | PASS | 无 Responses 实现代码；无路径别名（`routes_test.rs:80` 断言 `/chat/completions` 404）；未抽共享前置层 |

### Coherence — **FAIL（1 项阻塞）+ 1 项缺口**

| 项 | 状态 | 说明 |
| --- | --- | --- |
| **spec ↔ 代码现状** | **FAIL** | 见发现 1 |
| **`spec/` 长期事实同步** | **缺口** | 见发现 3 |
| proposal ↔ 实际改动 | PASS | Impact 含新增模块、main.rs merge、anthropic 导出、catalog status，与 `git diff` 一致 |
| Anthropic 零回归声明 | PASS | 逐文件 `git diff` 核实仅可见性变更 |
| design ↔ 实现 | PASS | D1、D8、D9、D12、§9 逐条落地 |
| Modified Capabilities 声明 | PASS（前提已被推翻） | 声明本身合规，但其依赖的「由 sync 收敛」前提不成立，见发现 1 |
| README / 设计文档同步 | PASS | `README.md:535-546`；`docs/multi-protocol-api-design.md:46`（P1 已解决）、`:265`（Live）、`:753`（Phase B 已完成），三处与 catalog 一致 |
| 与 responses change 无实现冲突 | PASS | 两者共用 `prepare`（`handlers.rs:103`）而非各写一套；共用 `OpenAiTool` 双形状；**都未注册 `/v1/models`**（grep 确认 `/v1/models` 只在 `anthropic/router.rs`）；model resolution 走同一 `resolution_context_from_state` + `convert_request_with_policy`，无分叉 |

## 发现项

### 1. 【阻塞】spec `:224-227` 断言 Responses 仍 planned，与代码现状相反

`specs/openai-chat-completions/spec.md:223-227` 的 Scenario「Responses 端点仍为 planned」：

> **THEN** `POST /v1/responses` MUST 仍为 planned，且请求它 MUST 返回 404

该断言在 Phase C 落地后已为假：`src/public_api/catalog.rs:174` 为 `EndpointStatus::Live`，`src/openai/mod.rs:30` 已挂载，本轮实测 `POST /v1/responses` 返回 **HTTP 200**。

前一轮报告与 proposal 均判断可由 `openspec-sync-specs` 在 Phase C 归档后收敛。**本轮复核推翻该判断**：

```
openai-chat-completions-compat/specs/  →  openai-chat-completions
openai-responses-api-compat/specs/     →  admin-runtime-settings  openai-responses  openai-responses-websearch
```

Phase C 的 specs 目录里**没有 `openai-chat-completions` 能力文件**，它的状态声明写在自有能力 `openai-responses/spec.md:223-235` 内。因此没有任何后续 change 会覆盖本 change 的这条 Scenario——原样归档会在 `openspec/specs/openai-chat-completions/spec.md` 留下一条与代码永久相反的断言。

处置选项（需维护者决定，本次未擅自改动 spec）：

1. 归档前删除 `:224-227` 这条 Scenario，`:217` 正文的「Endpoints still unimplemented MUST remain `planned`」保留（该句本身仍成立，`/v1/responses/{id}` 即为例）。**推荐**：本 change 的职责是 chat 端点自身，不该断言姊妹端点的瞬时状态。
2. 改写为「`GET /v1/responses/{id}` MUST 仍为 planned 且返回 404」——同样成立（`catalog.rs:192` 为 Planned），且与 Phase C 的 `openai-responses/spec.md:232-235` 一致。
3. 在 Phase C 补一个 `openai-chat-completions` 的 MODIFIED 能力文件覆盖该 Requirement，再按 B → C 顺序归档。最合规，但需新增工件。

同类问题在 `public-api-catalog-admin-display` 亦存在一条（其 spec `:66`、`:73-76` 断言两个 OpenAI 端点均为 planned），见该 change 的 verify 报告发现 1。两处应一并处置。

### 2. `bridge-plan.md` 与 `verification-before-completion.md` 缺失

- **bridge 不事后补写**（伪造流程时序）。对本 change 影响最大：它是四个中**唯一修改了既有文件**的（`anthropic/{mod,router,handlers}.rs`），bridge 的影响面分析本会有价值。替代手段是逐文件 `git diff` 核实「只改可见性、不改逻辑」，已在 Coherence 维度确认。
- **completion 可补**：本轮查证 `openspec/changes/archive/` 下 6 个已归档 change 全部有该文件，4 个待归档 change 全部没有。

### 3. `spec/` 长期事实未同步 OpenAI 协议兼容

`spec/requirements.md:9-16` 的核心能力清单只有 Anthropic + `/cc/v1`，**未提 OpenAI 协议兼容**；`spec/structure.md:6-12` 的目录树**缺 `src/openai/` 与 `src/public_api/`**（本轮 grep 确认两文件均 0 命中 `openai`/`public_api`）。

按 AGENTS.md「README / AGENTS / spec 同步纪律」——「影响启动、构建、部署、测试、API 入口时必须同步对应入口」——新增两个对外协议端点与两个顶层模块属于必须同步的范围。四个 change 的 tasks 均未包含此项。建议归档时补入 `spec/requirements.md` 与 `spec/structure.md`。

### 4. cors layer 缺失未做转红验证（WARN，已明示）

三个 layer 中 auth 与 body limit 已破坏验证有效；cors 需浏览器环境才能观测，仅由代码审查确认已挂（`mod.rs:38`）。若被误删无测试转红；但故障表现明确（CORS 报错），定位成本可接受。

### 5. `tool_choice` 接受但不映射（已声明，非缺陷）

客户端若依赖强制工具调用会得到非预期结果。已在 proposal 非目标与 catalog `client_hints` 声明。

### 6. CodeGraph 影响面分析未执行（批次级 WARN）

详见 `openai-responses-api-compat` 的 verify 报告。

## 证据路径

- `openspec/changes/openai-chat-completions-compat/evidence/verification.md`
- `openspec/changes/openai-chat-completions-compat/evidence/live-upstream-verification.md`
- `openspec/changes/openai-chat-completions-compat/evidence/spec-compliance-report.md`
- 本文件

## 归档前必须处理

1. **发现 1（阻塞）**：处置 `specs/openai-chat-completions/spec.md:224-227`。推荐选项 1 或 2。与 `public-api-catalog-admin-display` 发现 1 一并处理。
2. 发现 2：`verification-before-completion` 门禁（或明确以本报告为等价产出）。
3. 发现 3：补 `spec/requirements.md` 与 `spec/structure.md` 的 OpenAI 协议与新模块条目。
4. 归档次序：在 `public-api-catalog-admin-display` 之后、`openai-responses-api-compat` 之前。

## 本轮未验证项

- `live-upstream-verification.md` 中的历史 curl 输出（4123 tokens、keepalive 行等）未逐条复现；本轮仅实测一次非流式 HTTP 200。
- cors 行为、浏览器端表现未验证。
