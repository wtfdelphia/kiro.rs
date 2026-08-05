# Spec Compliance Report: openai-responses-api-compat

审查日期：2026-07-27

## 总体状态：**PASS**

初审为 WARN（上游对话路径未真实验证）。2026-07-28 用真实凭据补验通过，
见 `evidence/live-upstream-verification.md`，状态升为 PASS。

## 六维审查

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | PASS | specs 含 3 个能力（含 `admin-runtime-settings` 的 MODIFIED+ADDED，2026-07-28 核验补漏）。新增 `src/openai/{responses,responses_types,responses_stream,websearch}.rs`；修改 `openai/{mod,handlers}.rs`、`anthropic/mod.rs`（导出）、`anthropic/websearch.rs`（**本 change 仅改 4 处可见性**；该文件当前工作区另有非流式分支等逻辑改动，归属同批次的 `anthropic-websearch-non-stream` change，非本 change 越界）、`admin/{router,handlers,service,types}.rs`（开关）、`model/config.rs`（字段）、`admin-ui`（开关 UI）、`catalog.rs`（status）。均在 Impact 内 |
| Design | PASS | 复用 Phase B 的 `prepare`（不重抄 D8 四项）；无状态返 400（D2）；仅挂 Responses（D10）；宽判定 + Anthropic 侧不改（D11）；usage 位置（D12） |
| Scenarios | PASS | 20 Requirements / 67 Scenarios 全部有实现或测试对应 |
| Project Rules | PASS | 协议/SSE + Admin API + 配置 schema 三类高风险均已走 OpenSpec；无凭据入库 |
| Verification | PASS | `cargo test` 498 passed；`pnpm build` 通过；web_search 两种模式端到端跑通（含真实 MCP 结果）；上游对话非流式与流式语义事件均已用真实凭据验证 |
| README/AGENTS Sync | PASS | README 新增 Responses 端点、无状态说明、web_search 说明、Admin 开关；TOC 已同步 |

## Scenario 覆盖抽样

### openai-responses（39 Scenarios）

| Scenario | 证据 |
| --- | --- |
| 未鉴权请求被拒绝 | `mod.rs:test_responses_auth_required` |
| 字符串输入 / 单对象等价 / 空输入被拒 | `responses.rs:test_string_input` / `test_single_object_equals_one_element_array` / `test_empty_input_rejected` |
| message item / 无 type 用 role | `responses.rs:test_message_item` / `test_message_item_without_type_uses_role` |
| function_call_output 配对 / call_id 兼容 / 回落 content | `test_function_call_output_pairing` / `test_function_call_output_accepts_tool_call_id` / `test_function_call_output_falls_back_to_content` |
| **连续 function_call 合并** | `test_consecutive_function_calls_merge_into_one_assistant`（断言仅一条 assistant 且含两个 call） |
| **裸文本与图片归集** | `test_bare_text_and_image_items_collapse_into_user` |
| 归集在遇到带 role 的 item 时结束 | `test_pending_flushed_before_roled_item` |
| output_text item 归为 assistant | `test_output_text_becomes_assistant` |
| instructions 生效 / 缺省不产生空指令 | `test_instructions_becomes_leading_system` / `test_blank_instructions_ignored`；`handlers.rs:test_responses_instructions_reaches_upstream` 验证到达上游 |
| **previous_response_id 被拒绝** | `test_previous_response_id_rejected` + `mod.rs:test_responses_previous_response_id_rejected_over_http`（HTTP 层 400）；evidence §4 实测文案 |
| 不静默丢历史 | 同上（返回 400 而非忽略字段） |
| store 被忽略但不报错 | `test_store_does_not_fail_request` |
| model 缺省 / 别名回显 | `responses_types.rs:test_model_default_when_absent` / `test_model_echoed_as_given` |
| 文本输出结构 / 工具调用输出结构 | `responses_stream.rs:test_text_deltas_and_final_text` / `test_function_call_event_sequence` |
| 工具名还原 | `responses_stream.rs:test_tool_name_restored`；`handlers.rs:test_responses_tool_name_map_captured` |
| input_tokens 优先用上游信号 | `responses_stream.rs:test_usage_prefers_context_signal` |
| metadata 回显 | `responses_stream.rs:test_metadata_and_instructions_echoed` |
| **事件为命名 SSE 事件** | `responses_stream.rs:test_sse_format_has_event_line`；evidence §3.2 实测 `event:` 行 |
| **文本路径事件顺序** | `test_text_only_event_sequence`（逐名断言 9 个事件） |
| **文本后接工具调用时先关闭文本 item** | `test_text_then_tool_closes_message_item_first` |
| **output 索引不重复** | `test_output_index_increments_per_item`（断言 [0,1,2]） |
| 上游失败且已开始输出 | `test_failed_event` + `test_started_false_before_output` |
| 流以 DONE 结束 / 保活不破坏协议 | `test_done_and_keepalive_format` |
| 非法 JSON / 模型无法解析 | `mod.rs:test_responses_invalid_json_openai_shape` |
| thinking 后缀注入指令 | `handlers.rs:test_responses_thinking_suffix_reaches_upstream` |
| Responses 登记为 live 且可命中 / retrieve 仍 planned | `catalog.rs:test_responses_live_retrieve_still_planned` + `mod.rs:test_responses_retrieve_not_mounted` |
| 既有端点行为不变 | 全量 498 passed；evidence §7 实测三种方言分家 |

### openai-responses-websearch（18 Scenarios）

| Scenario | 证据 |
| --- | --- |
| 单个 web_search 触发代执行 | `websearch.rs:test_should_emulate_single_tool`；evidence §3.1 端到端实测 |
| 混合工具不触发 | `test_should_not_emulate_mixed_tools` |
| Chat 端点不提供该能力 | Chat 侧 `test_web_search_tool_not_intercepted`；catalog hints 声明 |
| 按 type 前缀 / 按 name 带日期官方名 / 普通工具不误判 | `test_detect_by_type_prefix` / `test_dated_official_name_detected` / `test_ordinary_function_tool_not_detected` |
| **Anthropic 侧行为不变** | `git diff` 确认 `has_web_search_tool` 零改动；Anthropic 侧 8 项既有测试全绿 |
| 两端点差异被记录 | `catalog.rs:test_responses_hints_document_stateless_and_websearch` |
| 默认启用 / 关闭后不拦截 / Admin 热更新 | `service.rs:test_websearch_setting_defaults_to_enabled` / `websearch.rs:test_should_not_emulate_when_disabled` / `service.rs:test_websearch_setting_toggle`+`_persisted`；evidence §3.3 实测热更新与 401 |
| 取最后一条 user 文本 / 无可用查询 / 不剥前缀 | `test_extract_query_last_user_message` / `test_extract_query_none_when_no_user_text` / `test_anthropic_prefix_not_stripped` |
| 非流式输出结构 / 流式事件序列 | `test_output_items_structure` / `test_stream_event_sequence`；evidence §3.1/§3.2 端到端实测 |
| **搜索后端失败不返回空成功** | `test_no_results_summary_is_explicit`；evidence §3.1 实测摘要含 "No results found" |
| usage 不伪造上游信号 | `handlers.rs` web_search 路径用 `estimate_chars`，不读 `context_input_tokens`；`test_estimate_chars_never_zero` |

## 发现项

### 1. 上游对话路径未真实验证（已解除）

与 Phase B 同因（无有效凭据）。tasks 9.1/9.2 标 `[~]`，evidence §10 说明。

**2026-07-28 已解除**：真实凭据下验证通过——非流式 `ResponsesObject`
（`input_tokens: 4123` 来自上游信号）、流式语义事件序列（10 个事件逐名与设计一致，
三个 delta 拼回完整文本）、web_search 拿到真实 MCP 搜索结果。
「Kiro Event → 语义事件」这一段的实际转换行为已被真实数据检验。
详见 `evidence/live-upstream-verification.md`。

### 2. 实现过程中修正的顺序问题（已修复，记录备查）

归一校验最初位于 provider 检查之后，导致无 provider 环境下
`previous_response_id` 请求返回 503 而非 400，与 spec「MUST 为 400」冲突。
被 `test_responses_previous_response_id_rejected_over_http` 抓到并修复
（归一前置）。这说明 HTTP 层断言与纯函数断言不可互相替代。

### 3. thinking 内容在 Responses 端点被丢弃（设计选择，已声明）

Responses 协议无稳定的 reasoning part 契约，首版丢弃并 `tracing::debug` 记录。
`test_thinking_content_excluded_from_output` 锁定该行为。已在 design §6 声明。

**剩余风险**：使用 `-thinking` 模型的 Responses 客户端拿不到思考内容，且响应中
无任何提示。若后续需要，应作为独立 change 评估（OpenAI 已有 `reasoning` 字段草案）。

### 4. admin-ui 未做浏览器渲染验证（WARN，已明示）

新增的「Web 搜索代执行」分组是在既有分组后追加同构组件（Switch + 说明），
`pnpm build`（tsc + vite）通过。Phase A 已验证该面板的 dialog 与暗色配色。

## 证据路径

- `openspec/changes/openai-responses-api-compat/evidence/verification.md`

## 剩余风险

- thinking 内容静默丢弃（发现 3）
- `google_search` 判定被接受但 MCP 侧仍走 `web_search`（已在 proposal 非目标声明）
