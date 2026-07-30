# OpenSpec Verify Report: codex-responses-lite-tool-passthrough

日期：2026-07-29
审查类型：归档前工件与证据核验（`openspec-verify-change`）
总体状态：**PASS**

> 本次工作区同时存在三个 active change，三者文件归属互不重叠。本报告只核验本 change
> 的工件、tasks 与 evidence；`src/kiro/**` 与 `admin-ui/**` 归属另两个 change。

## 三维结论

| 维度 | 状态 | 结论 |
| --- | --- | --- |
| Completeness | **PASS** | `openspec status --json` 四类工件全部 `status: "done"`，`isComplete: true`。tasks **60/60 勾选，0 未勾**。evidence 三件齐备：`bridge-plan.md`（184 行）、`end-to-end-verification.md`（71 行）、`spec-compliance-report.md`。8 个 Requirement 各有 Scenario（共 37 个），无空 Requirement。 |
| Correctness | **PASS** | 37 个 Scenario 逐条有实现与测试对应（已在 compliance 报告建表，本轮复核测试名与实跑一致）。9 个 design 决策点全部落实。成功标准（工具送达上游、`custom_tool_call` 还原、工具链未断）有真实客户端证据。 |
| Coherence | **PASS** | proposal / design / tasks / spec 四者已消除本轮前的三处不一致（见「本轮修正」）。README 与 `spec/` 无需同步，理由成立。 |

## 工件完整性（openspec status）

```
$ openspec status --change codex-responses-lite-tool-passthrough --json
"isComplete": true
artifacts: proposal(done) / design(done) / specs(done) / tasks(done)
specs 实存：specs/openai-responses/spec.md
nextSteps: All planning artifacts are complete
```

```
$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)
  ✓ change/codex-responses-lite-tool-passthrough
```

## tasks 勾选项的证据支撑

抽查覆盖全部 10 个任务组，重点核验「声称有验证结果」的条目：

| 任务组 | 声称 | 本会话实跑核验 | 支撑 |
| --- | --- | --- | --- |
| 1（可见性与类型） | 1.1/1.2/2.4 **已撤回** | `git diff --stat -- src/anthropic/` 为**空** | ✅ 撤回完整 |
| 2（归一层骨架） | `responses_tools` 通过 | 27 passed | ✅（计数见修正 1） |
| 3（additional_tools 提取） | `openai::responses::` 34 passed | 现 46 passed（含任务组 6 的 12 项） | ✅ 累进一致 |
| 4（custom 降级） | 10 passed | 归入 `responses_tools` 27 项 | ✅ 断言逐项 `--list` 核实存在 |
| 5（namespace 展平） | 29 passed | **27 passed** | ⚠ 修正 1 |
| 6（历史 item / tool_choice） | `openai::responses::` 46 passed | 46 passed | ✅ 吻合 |
| 7（非流式还原） | `openai::handlers` 31 passed | 31 passed | ✅ 吻合 |
| 8（流式还原） | `openai::responses_stream` 27 passed | 27 passed | ✅ 吻合 |
| 9（可观测性） | `openai::` 239 passed | 未单独复跑该前缀；全量 570 passed 覆盖 | ✅ |
| 10.1（全量） | 563 passed | **570 passed** | ✅ 差额为另两 change 同期新增测试 |
| 10.2（零回归） | `anthropic::` 103 / `converter` 20 | 103 / 20 | ✅ 吻合 |
| 10.2（警告基线） | 10 条与改前一致 | `git stash` 取基线对比：改前改后同为 10 条 | ✅ 独立复核 |
| 10.3（validate） | 18 passed | **20 passed** | ✅ 差额为另两 change 各增一项 |
| 10.4（端到端） | 通过，见 evidence | 日志字段与代码 `tracing::info!` 结构交叉吻合 | ✅ |
| 10.5（探针清理） | 有遗留待用户处理 | `kiro_release/` 与 `wire-capture/` **均已不存在**；`src/` 探针字样零命中 | ✅ 遗留已消解 |
| 10.6（git status） | 无凭据、无 `.codegraph/` | `git check-ignore -v` 确认三项均被忽略 | ✅ |

4 条锁定测试经 `--list` 逐一确认存在并通过：
`test_freeform_upstream_arguments_delta_not_forwarded`、
`test_long_freeform_tool_name_roundtrip_still_custom_tool_call`、
`test_two_level_mapping_long_flattened_name`、
`test_namespace_reverse_map_key_is_flattened_name_not_shortened`。

## Requirement / Scenario 完整性

| Requirement | Scenario 数 | 实现/测试对应 |
| --- | --- | --- |
| input item 类型分派（MODIFIED） | 11 | 全覆盖 |
| responses-lite 的 additional_tools 工具承载 | 4 | 全覆盖 |
| 非 function 形状的工具降级 | 3 | 全覆盖 |
| namespace 工具展平与命名冲突 | 4 | 全覆盖 |
| 客户端方言工具的响应侧还原 | 8 | 全覆盖 |
| 流式还原时参数缓冲到完成才发出 | 3 | 全覆盖 |
| 工具丢弃与不支持能力的可观测性 | 3 | 全覆盖 |
| 多工具客户端无法使用 web_search 代执行 | 1 | 既有行为规格化（`websearch.rs:50` 前提真实存在，文件零改动） |

无空 Requirement，无未覆盖 Scenario。逐条对照表见 `spec-compliance-report.md`。

## proposal / design 与实际改动的一致性

三条「不改」声明经 diff 核实全部成立：

| 声明 | 核实 |
| --- | --- |
| 不改 `src/anthropic/converter.rs` 的 `normalize_json_schema` | `git diff --stat -- src/anthropic/` 为空 |
| 不改 Chat Completions 转换语义 | `converter.rs` 改动为纯 warn + 2 测试，过滤谓词语义与原 `!t.name.is_empty()` 等价 |
| `strip_encrypted` 作用域仅限新增归一层 | 全仓 grep 无 `responses_tools.rs` 外的调用 |

风险声明与实际一致：proposal Risks 的「lark 文法降级后模型可能生成不合文法输入」在
`end-to-end-verification.md` 中如实记录为样本量 1、长期有效性未知。
「未纳入 `tool_search` proxy（4 份抓包零出现）」与代码零实现一致。

## evidence 核验

| 证据 | 状态 | 核验 |
| --- | --- | --- |
| `bridge-plan.md` | ✅ 184 行 | Skills 门禁「开始实现前」产物 |
| `end-to-end-verification.md` | ✅ 71 行 | 真实客户端（Codex Desktop `0.146.0-alpha.3.1`）日志摘录；字段结构与 `handlers.rs:130-136`、`:1400-1405`、`responses_stream.rs:332-342` 的 `tracing::info!` 逐字段吻合，可交叉验证为真实产物；诚实列出 4 项未被真实流量覆盖的路径 |
| `spec-compliance-report.md` | ✅ | 本轮上游门禁，状态 PASS |
| 敏感数据 | ✅ 零命中 | 三份 evidence + `docs/codex-responses-lite-wire-analysis.md`（254 行）按 `eyJ*` / `Bearer *` / `arn:aws:*:<account>` / `Cookie` / `sk-*` / `refreshToken":"...` 扫描 |

只记录真实命令：evidence 中的所有命令输出均可在本会话复现（见「验证记录」）。

## 本轮修正（消除工件间不一致）

### 修正 1：tasks 5.5 / 10.1 / 10.3 计数校正

- `tasks.md:57` 原写 `responses_tools` → 29 passed，实为 **27**（`--list` 逐项核对 27 个测试名）。
  5.5 注记「同批完成 `extract_custom_input` 7 项 + `rewrite_tool_choice` 3 项」的累加出现笔误。
  **声称的每条断言都真实存在，无测试缺失**，仅计数不准。
- 10.1 的 563 与 10.3 的 18 是本 change 单独完成时的真值；现为 570 / 20，差额来自另两个
  active change 同期新增的测试与 change 项，非本 change 回归。
- 已在 tasks 10.7 下统一记录校正值。

### 修正 2：tasks 10.5 的遗留项已消解

原文标注「`kiro_release/kiro-rs.exe` 仍是 debug 探针版，原版备份在 `kiro-rs.exe.bak-wire`
（**待用户处理**，需停进程）」——这曾是归档的 FAIL 级阻塞项。本会话核实两个目录均已不存在，
`src/` 中 `dbg!` / `eprintln!` / `println!` / `wire capture` / `TODO` / `FIXME` 零命中。
已用删除线保留原记录并补注消解结论（保留痕迹而非抹去，便于回看）。

### 修正 3：proposal 措辞与 design D3.1 对齐

`proposal.md:41` 原写「超长名截断**复用**现有 `shorten_tool_name`」，易被读作归一层直接调用该函数；
实际归一层零引用它（`grep src/openai/` 零命中），截断由 anthropic 层既有 `tool_name_map` 承担，
归一层只是无需**预测**其结果。已改写为「沿用既有机制…也不预测缩短结果（见 design D3.1 修正记录）」，
与 design 及 tasks 1.1/1.2 的撤回标记一致。

## 失败项

无。

## 剩余风险（可接受，与 compliance 报告一致）

1. **lark 文法降级**为自然语言描述后模型可能生成不合文法输入；Codex 侧有容错重试，
   端到端样本量 1，长期有效性待观察（proposal Risks 已声明）。
2. **namespace 还原路径**未被真实流量触发（本次模型只调 `exec`）；有两条锁定测试覆盖。
3. **两级名字映射**耦合于既有 `tool_name_map` 行为；若后者改动，锁定测试会先失败（有保护）。
4. **`text.format` 不支持**是诚实边界：上游 `userInputMessageContext` 只有 `toolResults` / `tools`，
   无处透传；已明确拒绝 prompt 层模拟。

## 验证记录（本会话真实运行）

```
$ openspec status --change codex-responses-lite-tool-passthrough --json
"isComplete": true；proposal/design/specs/tasks 全部 done

$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)

$ cargo test --bin kiro-rs
test result: ok. 570 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

分模块：responses_tools 27 / responses:: 46 / handlers 31 /
        responses_stream 27 / converter 20 / anthropic:: 103，全部 0 failed

$ cargo build
warning: `kiro-rs` (bin "kiro-rs") generated 10 warnings   # 基线对比零增量

$ grep -c "^- \[x\]" tasks.md → 60 ；grep -c "^- \[ \]" tasks.md → 0
```

## 证据路径

- Bridge：`openspec/changes/codex-responses-lite-tool-passthrough/evidence/bridge-plan.md`
- Compliance：`openspec/changes/codex-responses-lite-tool-passthrough/evidence/spec-compliance-report.md`
- 端到端：`openspec/changes/codex-responses-lite-tool-passthrough/evidence/end-to-end-verification.md`
- 协议分析（长期参考）：`docs/codex-responses-lite-wire-analysis.md`
- 本报告：`openspec/changes/codex-responses-lite-tool-passthrough/evidence/openspec-verify-report.md`
- Completion：待 `verification-before-completion` 产出

## 结论

**PASS，可归档。** 四类工件完整（`isComplete: true`），tasks 60/60 且每个勾选项都有
实跑结果、diff 或 evidence 支撑，37 个 Scenario 全覆盖，validate 20/20。
本轮修正了三处工件间不一致（计数笔误、遗留项已消解、proposal 措辞），
其中 10.5 的 debug 构建残留曾是归档阻塞项，现已确认消解。
无失败项，无凭据入仓风险，警告零增量。

建议下一步：`verification-before-completion`，随后 `openspec-archive-change`。
归档时注意 `docs/codex-responses-lite-wire-analysis.md` 是长期协议参考，
**不随 change 目录一并归档**（它对后续所有 Responses 变更可复用）。
