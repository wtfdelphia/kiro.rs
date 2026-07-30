# Spec Compliance Report: codex-responses-lite-tool-passthrough

日期：2026-07-29
审查类型：实现后 / 提交前合规（`spec-compliance-check`，tasks 10.7）
总体状态：**PASS**

> 说明：本次工作区同时存在三个 active change。三者文件归属互不重叠，本报告只审查
> `src/openai/**` 与 `docs/codex-responses-lite-wire-analysis.md`；
> `src/kiro/**` 与 `admin-ui/**` 的改动归属另两个 change，不计入本 change 的越界。

## 六维表

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | **PASS** | 改动落在 proposal Impact 声明的 7 个文件 + 新增 `responses_tools.rs`。proposal「不改」声明全部核实成立：`src/anthropic/` **零改动**（`git diff --stat` 空，1.1/1.2 的可见性提升已完整撤回）；`src/openai/converter.rs` 仅加 warn + 2 项测试，过滤谓词语义与原 `!t.name.is_empty()` 等价，无行为变化；`websearch.rs` 零改动；`Cargo.toml`/`Cargo.lock` 零改动。 |
| Design | **PASS** | D3.1 / D4.1-4.2 / D5 / D6 / D7 / D8 / D9.1 / D9.2 / D10 逐点在代码中落实，详见下方决策点对照。design 的「修正记录」（`design.md:56-58`）对撤回项留痕充分。 |
| Scenarios | **PASS** | 37 个 Scenario（MODIFIED 11 + ADDED 26）全部有实现与测试对应，详见覆盖表。 |
| Project Rules | **PASS** | 属 AGENTS.md OpenSpec 条件「Anthropic/Kiro 协议、SSE 流式、转换逻辑」，已建 change，`evidence/bridge-plan.md`（184 行）齐备。高风险矩阵「协议 / SSE」要求的 `cargo test` 已实跑，并有真实客户端端到端证据。 |
| Verification | **PASS** | tasks 各组声称的测试计数与本会话实跑逐一核对（见验证记录），除 5.5 一处笔误（WARN-1）外全部吻合。`evidence/end-to-end-verification.md` 的日志格式与代码中 `tracing::info!` 字段结构可交叉验证为真实产物，且诚实列出 4 项未被真实流量覆盖的路径。 |
| README/AGENTS Sync | **PASS** | 不改启动、构建、部署、测试入口与 API 端点（`/v1/responses` 已在 README `:35`/`:539` 列明）。`spec/requirements.md:12` 的概括性描述仍成立。 |

## Design 决策点对照

| 决策点 | 实现 | 一致? |
| --- | --- | --- |
| D3.1 `ToolRewriteMap` key 用展平名/原名，不预测缩短 | 归一层零引用 `shorten_tool_name`/`TOOL_NAME_MAX_LEN`（已 grep `src/openai/` 确认）；锁定测试 `test_custom_freeform_key_is_original_name_not_shortened`、`test_namespace_reverse_map_key_is_flattened_name_not_shortened` | ✅ |
| D4.1/D4.2 `additional_tools` 走 `Value` 保真路径 | `responses.rs:146-152` `tools.extend(list.iter().cloned())`，不经 `OpenAiTool`（其手写 Deserialize 只留 4 字段）；缺 `tools` 数组时 warn | ✅ |
| D5 `custom` 降级为 `{input: string}` + 前插调用约定 + 追加 lark 文法 | `responses_tools.rs` freeform 分支；测试 `test_custom_downgraded_to_input_string_schema`、`test_custom_description_starts_with_invocation_note`、`test_custom_grammar_definition_preserved` | ✅ |
| D6 namespace 展平 `<ns>__<name>` + 冲突 400 不自动改名 | `responses_tools.rs:173-190` 两类冲突各返回 `OpenAiError::InvalidRequest`（→ 400，`error.rs:43`），信息含双方名字与 `rename` 建议；同一工具重复去重不报错（`:188-189`） | ✅ |
| D7 递归剔除 `encrypted` | `responses_tools.rs:68-83` 覆盖 Object 与 Array；作用域仅归一层（全仓 grep 无外部调用），未触碰 `/v1/messages` 共享的 `normalize_json_schema` | ✅ |
| D8 历史 item 与 `tool_choice` 改写 | `custom_tool_call`/`_output` 归并到既有 function 分支；`function_call` 带 `namespace` 无条件拼展平名；`tool_choice` 三形状改写 | ✅ |
| D9.1 非流式还原按 `freeform` 集合分派 | `handlers.rs` `build_tool_call_item` + `with_namespace`；`ResponsesContext`（`handlers.rs:993`）承载映射，文件内私有未扩散 | ✅ |
| D9.2 流式吞增量、stop 时一次性发出 | `responses_stream.rs:393-398` `if !is_freeform` 才转发增量；added 时 `input:""` 不带 `arguments`（`:349-358`） | ✅ |
| D10 可观测性 + `text.format` 不模拟 | `responses.rs:31-36` 入口 warn；无任何 system/instruction 注入（已 grep 确认） | ✅ |

## Requirement / Scenario 对照

### openai-responses（MODIFIED）— Requirement: input item 类型分派

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| message item | `test_message_item` / `test_message_item_without_type_uses_role` | PASS |
| function_call_output 配对 | `test_function_call_output_pairing` | PASS |
| call_id 与 tool_call_id 兼容 | `test_function_call_output_accepts_tool_call_id` | PASS |
| 连续 function_call 合并进同一条 assistant | `test_consecutive_function_calls_merge_into_one_assistant`（+ `test_function_call_after_text_assistant_not_merged` 反向锁定） | PASS |
| 裸文本与图片归集为 user | `test_bare_text_and_image_items_collapse_into_user` | PASS |
| 归集在遇到带 role 的 item 时结束 | `test_pending_flushed_before_roled_item` | PASS |
| output_text 归为 assistant | `test_output_text_becomes_assistant` | PASS |
| custom_tool_call 归一为工具调用 | `test_custom_tool_call_becomes_function_call_with_input_wrapper` | PASS |
| custom_tool_call_output 归一为工具结果 | `test_custom_tool_call_output_becomes_tool_message` | PASS |
| 非字符串 output / null | `test_custom_tool_call_output_stringifies_non_string`、`test_custom_tool_call_output_null_becomes_empty` | PASS |
| 带 namespace 的 function_call 拼展平名 | `test_function_call_with_namespace_uses_flattened_name`、`test_custom_tool_call_with_namespace_flattened`、`test_function_call_without_namespace_keeps_bare_name` | PASS |

### Requirement: responses-lite 的 additional_tools 工具承载（ADDED）

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| 工具从 additional_tools 提取 | `test_additional_tools_reach_upstream`（真实抓包形状，5 工具）；端到端 `count=9` | PASS |
| additional_tools 不产生对话消息 | `test_additional_tools_item_produces_no_message` | PASS |
| 与顶层 tools 共存时合并（顶层在前） | `test_top_level_tools_merged_before_additional` | PASS |
| 工具定义保真（`custom.format` / `namespace.tools[]`） | `test_additional_tools_schemas_not_empty` + D4.1 的 `Value` 路径 | PASS |

### Requirement: 非 function 形状的工具降级（ADDED）

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| custom 降级后 schema 非空 | `test_custom_downgraded_to_input_string_schema` | PASS |
| 调用约定说明置于描述最前且原说明保留 | `test_custom_description_starts_with_invocation_note` | PASS |
| 语法定义不丢失 | `test_custom_grammar_definition_preserved`（+ `test_custom_without_format_still_downgrades`） | PASS |

### Requirement: namespace 工具展平与命名冲突（ADDED）

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| 内层工具独立展平（无空壳） | `test_namespace_flattened_into_independent_tools`、`test_flatten_namespace_name_uses_double_underscore`、`test_namespace_children_field_falls_back_to_children` | PASS |
| 展平名与顶层工具冲突 → 400 含双方名字 | `test_flattened_name_conflicts_with_top_level_tool`（断言含 `rename`） | PASS |
| 两个 namespace 展平到同名 → 400 | `test_two_namespaces_flatten_to_same_name` | PASS |
| 展平名超长时截断保持确定性 | 复用既有 `shorten_tool_name`（`src/anthropic/converter.rs:806`，`TOOL_NAME_MAX_LEN=63`，哈希后缀确定性），归一层不写第二套规则；`test_tool_name_map_captured_for_long_names` | PASS |

### Requirement: 客户端方言工具的响应侧还原（ADDED）

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| freeform 工具调用回 custom_tool_call | `test_freeform_tool_becomes_custom_tool_call`、`test_custom_tool_call_item_serialization_shape`；端到端 `item_type="custom_tool_call"` | PASS |
| 超长 freeform 工具名往返 | **锁定测试** `test_long_freeform_tool_name_roundtrip_still_custom_tool_call` | PASS |
| 展平名还原为 namespace 与原名 | `test_flattened_tool_restored_to_namespace_and_original_name` | PASS |
| 两级映射叠加 | **锁定测试** `test_two_level_mapping_long_flattened_name` | PASS |
| 原始输入的提取（含 `input` 键） | `test_extract_input_with_input_key` | PASS |
| 模型直接返回裸输入（非法 JSON） | `test_extract_input_invalid_json_returns_raw`、`test_extract_input_no_input_key_non_empty_returns_raw`、`test_freeform_tool_raw_source_passthrough` | PASS |
| 空输入（空白 / `{}`） | `test_extract_input_blank`、`test_extract_input_empty_object` | PASS |
| 含换行与引号无损 | `test_extract_input_preserves_newlines_and_quotes`、`test_freeform_input_preserves_newlines_and_quotes` | PASS |

### Requirement: 流式还原时参数缓冲到完成才发出（ADDED）

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| 上游参数增量不透传 | **锁定测试** `test_freeform_upstream_arguments_delta_not_forwarded`；零回归 `test_plain_tool_still_forwards_arguments_delta` | PASS |
| 输入事件在参数完成后发出且只一次 | `test_freeform_input_events_emitted_once_after_stop`（+ `test_freeform_unfinished_tool_closed_on_finish` 收尾） | PASS |
| item 事件的类型改写（added `input:""` / done 完整） | `test_freeform_item_shape_added_and_done`、`test_stream_namespace_restored_in_items` | PASS |

### Requirement: 工具丢弃与不支持能力的可观测性（ADDED）

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| 丢弃工具时留痕（name + type），其余不受影响 | `converter.rs:290-297` warn 带 `tool_type`；`test_unnamed_tool_dropped_others_kept`、`test_all_tools_unnamed_yields_none`、`test_unnamed_tool_dropped_others_unaffected`（归一层 5 处 warn） | PASS |
| text.format 不被静默接受且请求仍处理 | `responses.rs:31-36` warn；`test_text_format_does_not_reject_request` | PASS |
| 不做 prompt 层模拟 | `responses.rs` 无 system/instruction 注入（grep 零命中）；`text` 字段仅读取不参与转换（`responses_types.rs` 注释已固化理由） | PASS |

### Requirement: 多工具客户端无法使用 web_search 代执行（ADDED）

| Scenario | 实现或测试 | 状态 |
| --- | --- | --- |
| 多工具请求不走代执行 | `src/openai/websearch.rs:50` `list.len() == 1` 前提真实存在（文件零改动，属既有行为的规格化）；丢弃时 warn 见上一 Requirement | PASS |

## 发现项

### WARN-1：tasks 5.5 的测试计数与实际不符（LOW）

- **事实**：`tasks.md:57` 写「`cargo test --bin kiro-rs openai::responses_tools` → 29 passed」，
  本会话实跑为 **27 passed**（`--list` 逐项核对：27 个测试名）。tasks 5.5 的注记说「同批完成
  `extract_custom_input` 测试（7 项）与 `rewrite_tool_choice` 测试（3 项）」——按 4.6 的 10 项 +
  namespace 8 项 + 这 10 项 ≈ 28，计数在增补过程中出现累加笔误。
- **影响**：仅记录准确性问题。27 项全部通过，且 5.4/5.5 声称的每一项断言（含两条锁定测试）
  都已在 `--list` 输出中逐一核实存在，无测试缺失。
- **建议**：归档前把 5.5 的计数改为 27，或统一引用本报告的验证记录。

### INFO-1：临时探针与 debug 构建残留已消解（原 tasks 10.5 遗留项）

- **事实**：tasks 10.5 记录「`kiro_release/kiro-rs.exe` 仍是 debug 探针版，原版备份在
  `kiro-rs.exe.bak-wire`（待用户处理）」。本会话核实 **`kiro_release/` 目录与 `wire-capture/`
  均已不存在**。`grep` 在 `src/` 中对 `wire capture` / `dbg!` / `eprintln!` / `println!` /
  `TODO` / `FIXME` 零命中（含新增文件 `responses_tools.rs`）。
- **判定**：该 FAIL 级风险已消解，不再是遗留项。`git status --untracked-files=all` 无相关条目。

### INFO-2：新增诊断日志无泄露风险（原 tasks 10.4 连带项）

- **事实**：两条长期保留的 info 日志经逐字段核实——`handlers.rs:130-136` 打 `count` 与工具名列表；
  `handlers.rs:1400-1405` 与 `responses_stream.rs:332-342` 打 `upstream_name` / `item_type` / `namespace`。
  **不含工具参数值、schema 内容、token 或凭据**。
- **判定**：级别（info）与内容均恰当，proposal Impact 已记录该决定。工具名属客户端自定义标识，
  在 info 级留痕是本 change 的核心诊断价值（「静默丢弃会让『模型说没有某能力』无从诊断」）。

### INFO-3：proposal 未回填 `shorten_tool_name` 的撤回修正（LOW）

- **事实**：`proposal.md:41` 仍写「超长名截断**复用现有 `shorten_tool_name`**（`src/anthropic/converter.rs:806-818`，
  已接入 `tool_name_map`），归一层不新写第二套哈希规则」。而 `design.md:56-58` 已记录撤回可见性提升，
  归一层实际零引用该函数。
- **判定**：**不构成矛盾**。proposal 的语义（截断由既有机制承担、归一层不写第二套规则）成立且已实现——
  截断发生在 anthropic 层的 `tool_name_map`，归一层只是无需**预测**其结果。措辞「复用」易被读作
  直接调用，与 tasks 1.1/1.2 的撤回标记合读才清楚。
- **建议**：归档前把 `:41` 改为「超长名截断沿用既有 `shorten_tool_name` + `tool_name_map` 机制；
  归一层不预测缩短结果（见 design D3.1 修正记录）」。

### INFO-4：`docs/` 下的抓包分析文档（LOW，判定为合规）

- **事实**：`docs/codex-responses-lite-wire-analysis.md`（254 行）放在 `docs/` 而非
  `openspec/changes/<name>/`。AGENTS.md 要求「单次变更过程只写在 `openspec/changes/<name>/`」。
- **判定**：**不违反**。该文档是 wire-level 协议逆向分析，记录的是 Codex 客户端的**长期协议事实**
  （`use_responses_lite` 的请求形状、四种工具 wire type、`additional_tools` 容器结构），
  不是本次变更的过程记录（过程记录在 `tasks.md` 与 `evidence/`）。它对后续所有 Responses 相关变更都是
  可复用的参考资料，与 `docs/tooling-sources.md` 同类。proposal `:5` 已引用它作为根因依据。
- **安全**：已按 `eyJ*` / `Bearer *` / `arn:aws:*:<account>` / `Cookie` / `sk-*` /
  `refreshToken":"...` 模式扫描，**零命中**，无真实 token、账号或凭据。

## CRITICAL

无。

## 安全核查

- `docs/codex-responses-lite-wire-analysis.md`、`evidence/end-to-end-verification.md`
  与全部 diff 均通过敏感模式扫描（零命中）。端到端日志摘录只含模型名、工具名、item 类型与时间戳。
- 新增测试用真实抓包**形状**但工具名为公开标识（`exec` / `wait` / `collaboration__*`），无凭据值。
- `git status --short` 16 条全部为预期文件，无 `config.json` / `credentials.*` / `.codegraph/`。
- 无新增 `#[allow(...)]` 掩盖警告；`types.rs` 反而移除了一处已不适用的 `#[allow(dead_code)]`。

## 验证记录（本会话真实运行）

```
$ cargo test --bin kiro-rs
test result: ok. 570 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
（tasks 10.1 声称 563，本会话 570——另两个 change 同期新增测试所致，非本 change 差异）

分模块（tasks 各组声称 vs 实跑）：
  openai::responses_tools    27 passed   （tasks 5.5 写 29 → WARN-1）
  openai::responses::        46 passed   （tasks 6.6 声称 46 ✓）
  openai::handlers           31 passed   （tasks 7.8 声称 31 ✓）
  openai::responses_stream   27 passed   （tasks 8.6 声称 27 ✓）
  openai::converter          20 passed   （tasks 10.2 声称 20 ✓，零回归）
  anthropic::               103 passed   （tasks 10.2 声称 103 ✓，零回归）
  全部 0 failed

$ cargo build
warning: `kiro-rs` (bin "kiro-rs") generated 10 warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)

$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)
（tasks 10.3 声称 18 passed；现为 20，因另两个 change 各增一项）
```

警告零增量已用 `git stash push --include-untracked` 取基线对比确认：改动前后同为 10 条，
均为既有 dead_code / unused_import。基线检查后工作区完整还原（`git stash list` 为空）。

范围核实命令：

```
$ git diff --stat -- src/anthropic/
（空 —— tasks 1.1/1.2 的可见性提升已完整撤回）

$ grep -rn "shorten_tool_name|TOOL_NAME_MAX_LEN" src/openai/
（空 —— 归一层不预测缩短，D3.1 落实）

$ grep -rn "strip_encrypted" src/ | grep -v responses_tools.rs
（空 —— 作用域收敛在归一层）

$ git diff -U0 -- src/ | grep -E "^\+.*(dbg!|eprintln!|println!|wire capture|TODO|FIXME)"
（空 —— 无调试残留）
```

## 未被真实流量覆盖（沿用 end-to-end-verification.md，诚实保留）

| 项 | 替代覆盖 |
| --- | --- |
| `collaboration__*` 的实际调用与回程 namespace 还原 | 本次模型只调 `exec`；单测 `test_flattened_tool_restored_to_namespace_and_original_name`、`test_stream_namespace_restored_in_items` |
| 超长工具名的两级映射 | 实测最长 30 字符未触发缩短；锁定测试 `test_long_freeform_tool_name_roundtrip_still_custom_tool_call`、`test_two_level_mapping_long_flattened_name` |
| lark 文法降级后的输出合规性 | 样本量 1（本次被客户端接受），长期有效性未知 —— proposal Risks 已声明 |
| `sequence_number` | kiro.rs 不发该字段，客户端正常工作，印证 D9.2 推测 |

## 证据路径

- Bridge：`openspec/changes/codex-responses-lite-tool-passthrough/evidence/bridge-plan.md`
- 端到端：`openspec/changes/codex-responses-lite-tool-passthrough/evidence/end-to-end-verification.md`
- 协议分析（长期参考）：`docs/codex-responses-lite-wire-analysis.md`
- 本报告：`openspec/changes/codex-responses-lite-tool-passthrough/evidence/spec-compliance-report.md`
- OpenSpec 工件：`proposal.md` / `design.md` / `tasks.md` / `specs/openai-responses/spec.md`
  （tasks 59/60，未勾选项即本门禁 10.7）

## 剩余风险（可接受）

1. **lark 文法降级为自然语言描述**后模型可能生成不合文法的输入；Codex 侧有容错与重试，
   端到端样本量 1，长期有效性待观察（proposal Risks 已声明）。
2. **namespace 还原路径**未被真实流量触发（有锁定测试覆盖）。
3. **`text.format` 不支持**是诚实边界而非缺陷：上游 `userInputMessageContext` 只有
   `toolResults` / `tools`，无处透传；已明确拒绝 prompt 层模拟。
4. 归一层与 anthropic 层的两级名字映射耦合于既有 `tool_name_map` 行为；若后者改动，
   两条锁定测试会先失败（有保护）。

## 结论

**PASS。** 37 个 Scenario 全部有实现与测试对应，9 个 design 决策点逐点落实，
4 条锁定测试（两级映射叠加、超长名往返、上游增量不透传、freeform key 不缩短）覆盖了
「静默失败」类风险。proposal 的三条「不改」声明经 diff 核实全部成立，
`src/anthropic/` 零改动，`converter.rs` 改动为纯 warn。
真实客户端端到端验证闭环（工具 9 个送达、`item_type="custom_tool_call"`、
1 秒内客户端发起下一轮证明工具链未断），且诚实列出 4 项未覆盖路径。
探针与 debug 构建残留已消解，无凭据入仓风险，警告零增量。
两处发现项（tasks 5.5 计数笔误、proposal 措辞未回填修正）均为文档准确性，不影响实现正确性。

建议下一步：勾选 tasks 10.7 的 `spec-compliance-check` →
`openspec-verify-change` → `verification-before-completion`；
归档前修正 WARN-1 计数与 INFO-3 措辞。
