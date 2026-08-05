# Spec Compliance Report: anthropic-websearch-non-stream

审查日期：2026-07-27

## 总体状态：**PASS**

无 CRITICAL、无 WARN。本 change 的全部功能路径均已端到端验证。

## 六维审查

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | PASS | 仅改 `src/anthropic/websearch.rs`。调用点 `handlers.rs:405/924` 未改（已传入完整 payload）。未触碰判定口径、MCP 调用、结果解析、摘要生成 |
| Design | PASS | 抽取 `build_websearch_blocks` 两路径共用；`wants_stream` 分派；非流式构造四块 message 对象。流式事件序列经对照确认未变 |
| Scenarios | PASS | 4 Requirements / 12 Scenarios 全部有实现与端到端证据 |
| Project Rules | PASS | 协议变更已走 OpenSpec；无凭据入库；`git status` 干净 |
| Verification | PASS | `cargo test` 498 passed；分派门禁破坏验证有效；非流式/流式/`cc/v1` 三路径 curl 实测 |
| README/AGENTS Sync | PASS | README 新增「WebSearch 工具」小节说明两种模式；TOC 已同步；`docs/multi-protocol-api-design.md` 的该项待办已标记为已修复 |

## 流式行为未变的核实方法

不只依赖测试，另做了源码级对照（`git show HEAD:src/anthropic/websearch.rs`）：

```
修复前: 13 个 SseEvent::new -> [message_start, content_block_start, content_block_delta,
        content_block_stop, content_block_start, content_block_stop, content_block_start,
        content_block_stop, content_block_start, content_block_delta, content_block_stop,
        message_delta, message_stop]
修复后: 13 个 SseEvent::new -> （完全相同）
```

块字段搬入 `build_websearch_blocks` 后逐项核实仍在：`encrypted_content`、
`page_age`、`from_timestamp_millis`、`%B %-d, %Y` 格式化、`web_search_result`
类型标记均保留，语义未改。

## Scenario 覆盖

| Scenario | 证据 |
| --- | --- |
| **非流式请求返回 JSON** | `test_non_stream_returns_json_not_sse`；evidence §4.1 实测 `content-type: application/json` |
| **流式请求返回事件流** | `test_stream_event_sequence_unchanged`；evidence §4.2 实测 13 段序列 |
| 两个 Anthropic 端点行为一致 | evidence §4.3 实测 `/cc/v1` 同样返回 JSON 与四块结构 |
| 非流式内容块顺序 | `test_non_stream_block_order_and_types` |
| **块内容跨模式一致** | `test_blocks_identical_across_modes`（server_tool_use 与 tool_result 整块比对；两个 text 块拼回 delta 后比对） |
| server_tool_use 携带查询 | `test_non_stream_server_tool_use_carries_query`；evidence §4.1 实测 `query: rust 2026` |
| usage 含服务端搜索计数 | `test_non_stream_usage_fields`；`test_stream_usage_matches_non_stream` 保证两模式口径一致 |
| stop_reason 为正常结束 | `test_non_stream_message_envelope` |
| model 回显原值 | 同上（断言 `claude-sonnet-4.5`） |
| **无结果时摘要明确** | `test_non_stream_no_results_is_explicit`（断言结果列表为空 + 摘要含 "No results"） |
| 搜索调用失败仍返回良构响应 | MCP 失败时 `search_results` 为 `None`，走同一构造路径；evidence §4.1 即为该场景（示例凭据下 MCP 无结果） |
| 无法提取查询 | 现有 400 分支未改，位于 stream 分派之前，两模式共用 |

## 发现项

### 1. 首次编写的门禁是假的（已修复，记录备查）

最初的测试直接调用 `build_websearch_non_stream_response`，**绕过了分派逻辑**。
把 `if !payload.stream` 改成 `if false`（即恢复原缺陷）后，17 项测试照样全绿。

**处置**：把分派抽成独立函数 `wants_stream(payload)` 并直接断言。再次破坏验证时
`test_dispatch_follows_client_stream_field` 与 `test_stream_field_defaults_to_false`
立即转红。

**教训（对后续 change 有效）**：「测了构造函数」不等于「测了行为」。断言必须能覆盖
从入口到产出的分派路径，否则门禁形同虚设。本批次其它三个 change 的门禁均从真实
入口进入（HTTP 请求或 `prepare`），已核实有效。

## 证据路径

- `openspec/changes/anthropic-websearch-non-stream/evidence/verification.md`

## 剩余风险

无。web_search 不依赖上游 generate，全部路径已在示例凭据环境下端到端验证。

唯一的行为变更（`stream:false` 从 SSE 变为 JSON）是修复而非破坏：原行为下没有
客户端能正确消费该响应。
