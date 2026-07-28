# OpenSpec Verify Report: anthropic-websearch-non-stream

核验日期：2026-07-28（第三轮：completion 门禁已补齐）
结论：**PASS** — 可归档；1 项不补写的流程缺口 + 2 项验证物薄弱（均已记录）

## 第三轮复核（2026-07-28）

| 项 | 结果 |
| --- | --- |
| `openspec validate anthropic-websearch-non-stream --strict` | **change is valid** |
| `openspec validate --all` | 15 passed, 0 failed |
| `cargo test` | **500 passed, 0 failed** |
| tasks | 23/23，`[ ]` = 0、`[~]` = 0 |
| Requirement→Scenario | 4 req / 12 scen，`requirements_without_scenario = 0` |
| `git check-ignore config.json credentials.json .codegraph/` | 三者均被忽略 |
| 候选提交敏感文件扫描 | 无 |

**发现 2 已解除**：`evidence/verification-before-completion.md` 已写入（本轮补齐，时序正确——归档前动作）。内含全量命令表、Live Smoke（非流式/流式/开关隔离/五个 Anthropic 端点）、文档同步表与剩余风险。

本轮未发现新的不一致。本 change 的 spec 不断言任何 OpenAI 端点状态，未受另两个 change 的 spec 修正波及。

---

## 第二轮记录（原文保留）

核验日期：2026-07-28（第二轮复核，含 U+FFFD 附带查证）
结论：**WARN** — 无阻塞项，无功能剩余风险；2 项流程缺口 + 1 项验证物薄弱

## 本轮实际运行的命令

| 命令 | 结果 |
| --- | --- |
| `openspec list` | 4 个活跃 change，本 change ✓ Complete |
| `openspec status --change anthropic-websearch-non-stream --json` | proposal / specs / design / tasks 的 `existingOutputPaths` 均非空，非 blocked |
| `openspec validate --all` | **15 passed, 0 failed**；`✓ change/anthropic-websearch-non-stream` |
| `cargo test` | **500 passed; 0 failed; 0 ignored** |
| `cargo test websearch` | 47 passed; 0 failed（453 filtered out） |
| `cargo build --release` | Finished（10 条既有 dead-code warning，0 error） |
| Requirement→Scenario 扫描脚本 | `requirements_without_scenario = 0` |

## 三维核验

### Completeness — PASS

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 核心工件 | PASS | 齐全；specs 含 `anthropic-websearch` |
| tasks | PASS | 23/23 完成，`- [ ]` 计数为 0 |
| Requirement→Scenario | PASS | 4 Requirements / 12 Scenarios，无空 Requirement（脚本逐项扫描确认） |
| evidence: Verification | PASS | `verification.md` |
| evidence: Compliance | PASS | `spec-compliance-report.md` |
| **evidence: Bridge** | **缺失** | 见发现 1（批次级，已决定不补写） |
| **evidence: Completion** | **缺失** | 见发现 2 |

本轮逐项核到的落地物：

| 任务 | 落地物 |
| --- | --- |
| 1.1 抽取共享构造 | `src/anthropic/websearch.rs:240` `build_websearch_blocks` |
| 1.2 流式复用共享构造 | `websearch.rs:305`；`blocks[1]`/`blocks[2]` 入事件于 `:360`、`:374` |
| 2.1 按 stream 字段分派 | `websearch.rs:500`、`:525` `wants_stream` |
| 2.2/2.3 非流式 message 对象 | `websearch.rs:532`；`server_tool_use.web_search_requests` 在 `:554` |
| 3.x 单测 | `websearch.rs:830/857/873/895/911/924/949/1001/1057/1070/1081` |
| 4.6 README | `README.md:564-578`，TOC `:69` |
| 4.7 设计文档 | `docs/multi-protocol-api-design.md:652-655` 标为已修复 |

evidence 引用的 11 个新测试函数名**逐一在源码中存在**，无虚构测试名。

### Correctness — PASS

| 项 | 状态 | 说明 |
| --- | --- | --- |
| Scenario 意图满足 | PASS | 12 项均有实现；10 项有单测或实测，2 项见发现 3/4 |
| 门禁有效性（两轮） | PASS | 首轮门禁失效（测试绕过分派，改 `if false` 后仍全绿）；抽出 `wants_stream` 后重测，恢复原缺陷时 2 项立即转红 |
| 流式行为未变 | PASS | 源码级对照 `git show HEAD:`：两版均 13 个 `SseEvent::new`、序列相同；搬迁字段（`encrypted_content`/`page_age`/`%B %-d, %Y`）逐项在位于 `:248-268` |
| 跨模式一致性 | PASS | `test_blocks_identical_across_modes`（`:949`）、`test_stream_usage_matches_non_stream`（`:1001`），本轮 `cargo test` 通过 |
| 端到端验证 | PASS | 本轮实测非流式返 JSON（HTTP 200，四块结构正确）、流式返 SSE，`/cc/v1` 同构见首轮 evidence |
| 非目标未被越界 | PASS | `has_web_search_tool`（`:108-112`）与 HEAD 零 diff；MCP 调用/结果解析/摘要生成逻辑体未改（仅 `pub` → `pub(crate)`） |

**附带查证：搜索结果 title 中的 U+FFFD（本轮新增，结论为非缺陷）**

本轮实测发现 `web_search_tool_result` 的部分 `title` 含替换字符（如 `L'\u{FFFD}quipe`）。经临时插桩定位：

- MCP 原始响应字节整体 `utf8=VALID`，`content-type: application/json`
- 在原始字节上直接统计 `EF BF BD` 序列，与最终发给客户端的 U+FFFD 计数**精确相等**：34→34、0→0、1→1

结论：替换字符是上游 Kiro MCP 抓取非 UTF-8 页面（Latin-1 法语/意大利语站点）时自己解码坏后写入 JSON 的合法内容，本 change 的 `response.text()` 透传路径无损。在本侧「修复」只能靠猜测性还原（把 `L'\u{FFFD}quipe` 猜成 `L'équipe`），属启发式改写上游数据且超出规格范围，故不修改代码。诊断插桩已全部撤除（`grep -c "TEMP-DIAG" src/anthropic/websearch.rs` = 0）。

### Coherence — PASS

| 项 | 状态 | 说明 |
| --- | --- | --- |
| proposal ↔ 实际改动 | PASS（行号有偏差） | 仅改 `src/anthropic/websearch.rs`；调用点未改，与 Impact 声明一致。行号见下 |
| design ↔ 实现 | PASS | 共享块构造、`wants_stream` 分派、四块 message 对象逐项落地 |
| Modified Capabilities 声明 | PASS | `openspec/specs/` 下确无 `anthropic-websearch`，本 change 建立首个契约 |
| README 同步 | PASS | `README.md:564-578` + TOC `:69` |
| 设计定稿同步 | PASS | `docs/multi-protocol-api-design.md:652-655` |
| 与其余 3 个 change 无 spec 冲突 | PASS | 本 change 的 spec 不声明任何 OpenAI 端点状态，无归档次序依赖 |
| 行为变更性质 | PASS | `stream:false` 从 SSE 变 JSON 是修复而非破坏——原行为下无客户端能正确消费 |

行号引用偏差（不影响结论）：proposal 写「调用点 `handlers.rs:405`/`:924`」，实际 `post_messages` 在 `handlers.rs:374`、`post_messages_cc` 在 `:897`，两处判定在 `:405`/`:929`，`handle_websearch_request` 调用在 `:416`/`:940`。design §1/§2 的行号同样有几行偏移。结论（`handlers.rs` 的 diff 只有两处 `pub(crate)` 可见性变更）正确。

## 发现项

### 1. `bridge-plan.md` 缺失（批次级，已决定不补写）

见 `public-api-catalog-admin-display` 的 verify 报告发现 1。对本 change 影响最小：改动范围单文件、目标明确，实现前已通过 `git show HEAD:` 对照与源码精读定位全部调用点。

### 2. `verification-before-completion.md` 缺失（归档前必须处理）

本轮查证：`openspec/changes/archive/` 下 **6 个已归档 change 全部有**该文件，4 个待归档 change **全部没有**。相对历史惯例是明确缺口。本报告「本轮实际运行的命令」一节可作等价产出，但文件名与惯例不一致。

### 3. Scenario 3.7「查询无法提取时两种模式均 400」缺少验证物

生产代码在 `websearch.rs:469-481`（位于分派之前，两模式共用），结构上成立。但既无单测覆盖 `extract_search_query` 返回 `None` 的分支，`verification.md` §5 的回归 curl 也只覆盖了「模型不支持」与「混合工具」，未覆盖缺失查询。`spec-compliance-report.md:51` 以「分支未改、位于分派之前」为证据，属结构性论证而非验证物。

### 4. Scenario 3.2「stream:true 仍为 SSE」的单测支撑为间接

`:1063` 断言 `wants_stream(true)==true`、`:1081` 断言事件名序列，但**无单测直接断言流式分支的 content-type 为 `text/event-stream`**（`:851` 是非流式方向的反向断言）。`verification.md` §4.2 的 curl 与本轮实测覆盖了这一点，故不算无证据，但门禁强度弱于报告表述给人的印象。

### 5. 首轮门禁失效的过程记录（已修复，价值在于教训）

`spec-compliance-report.md` 发现 1 已详载。对其余三个 change 的反查结论：门禁均从真实入口进入（HTTP 请求或 `prepare` 函数），无同类问题。

**教训**：「测了构造函数」不等于「测了行为」。断言必须覆盖从入口到产出的分派路径。

### 6. 术语不一致（非缺陷）

design/verification 称「11 段事件」，compliance 报告称「13 段 / 13 个 `SseEvent::new`」。实际为 13 个构造点，示例数据下运行时 14 个事件（摘要分 2 个 delta）。「11」源自旧代码注释的 11 个编号步骤。

## 证据路径

- `openspec/changes/anthropic-websearch-non-stream/evidence/verification.md`
- `openspec/changes/anthropic-websearch-non-stream/evidence/spec-compliance-report.md`
- 本文件

## 归档前必须处理

- 发现 2：`verification-before-completion` 门禁（或明确以本报告为等价产出）

本 change 与其余三个无归档次序依赖（建立独立能力，不涉及 catalog status），可独立归档。

## 本轮未验证项

首轮 evidence 中的 curl 输出与启动日志属历史会话记录，未逐条复现；本轮仅复现了 web_search 非流式与流式各一次实测请求（均 HTTP 200）。`pnpm build` 本轮运行过（用于部署二进制）但与本 change 无关。
