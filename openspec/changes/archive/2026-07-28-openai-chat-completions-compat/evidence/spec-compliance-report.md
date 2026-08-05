# Spec Compliance Report: openai-chat-completions-compat

审查日期：2026-07-27

## 总体状态：**PASS**

初审为 WARN（真实上游对话未验证）。2026-07-28 用真实凭据补验通过，
见 `evidence/live-upstream-verification.md`，状态升为 PASS。

## 六维审查

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | PASS | 新增 `src/openai/{mod,types,converter,stream,error,handlers}.rs`；修改 `main.rs`（mod + merge）、`anthropic/mod.rs`（仅加 `pub(crate)` 导出）、`anthropic/{router,handlers}.rs`（仅改可见性）、`public_api/catalog.rs`（status）。**未改任何 Anthropic 实现逻辑** |
| Design | PASS | 走适配层（D1）不写直达转换器；四项前置各自捞（D8）；model 回显原值（D9）；`include_usage` 已实现（D12）；三个 layer 补齐（§9） |
| Scenarios | PASS | 15 Requirements / 37 Scenarios 全部有实现或测试对应 |
| Project Rules | PASS | 协议/SSE 变更已走 OpenSpec；无真实凭据入库；`git status` 干净 |
| Verification | PASS | `cargo test` 498 passed；四项门禁破坏验证有效；真实上游对话、流式 + include_usage、function tools、thinking 后缀均已用真实凭据验证 |
| README/AGENTS Sync | PASS | README 新增「OpenAI 兼容端点」小节与接入注意事项；TOC 已同步 |

## Scenario 覆盖抽样

| Scenario | 证据 |
| --- | --- |
| 未鉴权请求被拒绝 | `mod.rs:test_auth_required_no_key_rejected`；破坏 auth layer 后转红 |
| 大请求体不被默认限制拦截 | `mod.rs:test_body_over_default_limit_not_rejected`；破坏 body limit 后转红 |
| 缺少 user 消息 / messages 为空 | `converter.rs:test_no_user_message_rejected` / `test_empty_messages_rejected` |
| temperature 被接受但不生效 | `converter.rs:test_temperature_and_top_p_not_forwarded`（`MessagesRequest` 无该字段，编译期即保证不透传） |
| 嵌套形状 / 顶层形状 | `types.rs:test_tool_nested_shape` / `test_tool_top_level_shape` / `test_tool_both_shapes_equivalent` |
| assistant 工具调用映射 | `converter.rs:test_tool_calls_to_tool_use` / `test_malformed_arguments_degrades_to_empty_object` |
| tool 角色配对与归集 | `converter.rs:test_consecutive_tool_messages_collapse_into_one_user` |
| 图片 part / 远程 URL 跳过 | `converter.rs:test_image_data_url_converted` / `test_remote_image_url_skipped` |
| **thinking 后缀注入指令** | `handlers.rs:test_thinking_suffix_reaches_upstream`；注释掉 `override_thinking_from_model_name` 后转红，失败输出显示降级后的上游请求 |
| **别名模型回显** | `handlers.rs:test_echo_model_is_original_request_value` |
| **超长工具名往返** | `handlers.rs:test_tool_name_map_captured_for_long_names`；置空 map 后转红 |
| 工具调用的 finish_reason / 上下文超限 | `stream.rs:test_finish_reason_tool_calls` / `test_finish_reason_length_on_context_full` |
| prompt_tokens 优先用上游信号 | `stream.rs:test_usage_prefers_context_signal` / `handlers.rs:test_non_stream_prompt_tokens_prefers_context_signal` |
| 非流式 / 流式 思考分离 | `handlers.rs:test_non_stream_thinking_separated_from_content` / `stream.rs:test_thinking_routed_to_reasoning_content` |
| 首块只带 role | `stream.rs:test_first_chunk_carries_only_role` |
| 工具调用增量 index 稳定 | `stream.rs:test_multiple_tools_have_stable_distinct_index` |
| 保活不破坏协议 | `stream.rs:test_keepalive_is_sse_comment_not_chunk` |
| 请求 / 未请求 usage | `stream.rs:test_include_usage_emits_usage_chunk_before_done` / `test_no_usage_chunk_when_not_requested` |
| 非法 JSON / 模型无法解析 | `mod.rs:test_invalid_json_returns_openai_error_shape` / `handlers.rs:test_unresolvable_model_rejected_with_openai_shape`；实测两端点方言分家 |
| **web_search 作为普通工具** | `converter.rs:test_web_search_named_tool_stays_ordinary` + `handlers.rs:test_web_search_tool_not_intercepted` |
| Chat Completions 登记为 live 且可命中 | `catalog.rs:test_chat_completions_live` + `routes_test.rs`（router 已 merge OpenAI 路由） |
| Anthropic 端点行为不变 | `anthropic/mod.rs` 仅加导出；`router.rs`/`handlers.rs` 仅改可见性（`git diff` 核实）；全量 498 passed |

## 发现项

### 1. 真实上游对话未验证（已解除）

示例凭据的 refreshToken 为 20 字符占位值，会被判定为已截断，无法发起真实
generate。因此流式 chunk 序列、`include_usage` 末块、function tools 的**端到端**
行为仅由单测覆盖（22 项 stream + 15 项 handler）。

已在 tasks 9.1–9.4 标记 `[~]` 并在 evidence §5/§10 说明。curl 实测确认请求
已走通到 provider 层（返回「所有凭据均已禁用」而非更早的错误）。

**2026-07-28 已解除**：release 构建配真实凭据验证通过——非流式对话
（`prompt_tokens: 4123` 来自上游 context-usage 反算）、流式 chunk 序列
（含真实出现的 `: keepalive` 注释行保活）、`include_usage` 末块、
function tools 往返（`finish_reason: tool_calls`）、`-thinking` 后缀
（`reasoning_content` 有值且 `content` 无标签泄漏）。详见
`evidence/live-upstream-verification.md`。

### 2. cors layer 缺失未做转红验证（WARN，已明示）

三个 layer 中 auth 与 body limit 已破坏验证有效；cors 需浏览器环境才能观测，
仅由代码审查确认已挂（`mod.rs:create_openai_routes`）。

## 证据路径

- `openspec/changes/openai-chat-completions-compat/evidence/verification.md`

## 剩余风险

- `tool_choice` 接受但不映射，客户端若依赖强制工具调用会得到非预期结果（已在
  proposal 非目标与 catalog client_hints 中声明）
