# 验证记录（Anthropic web_search 非流式修复）

日期：2026-07-27。以下均为本会话真实运行的命令与输出摘录。
**本 change 的所有路径都能端到端验证**——web_search 不经上游 generate，
示例凭据下也能完整跑通。

## 1. OpenSpec

```
$ openspec validate --all
✓ change/anthropic-websearch-non-stream
Totals: 15 passed, 0 failed
```

## 2. 测试

```
$ cargo test
test result: ok. 498 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

新增 11 项（Phase C 后 487 → 498），全部在 `anthropic::websearch`：

| 断言 | 说明 |
| --- | --- |
| `test_dispatch_follows_client_stream_field` | 分派跟随客户端 `stream` |
| `test_stream_field_defaults_to_false` | 缺省 `stream` 视为非流式 |
| `test_non_stream_returns_json_not_sse` | content-type 为 JSON |
| `test_non_stream_block_order_and_types` | 四个块类型与顺序 |
| `test_non_stream_message_envelope` | type/role/model/stop_reason/id 前缀 |
| `test_non_stream_usage_fields` | usage 含 `server_tool_use.web_search_requests` |
| `test_non_stream_server_tool_use_carries_query` | 查询进入 server_tool_use |
| `test_non_stream_no_results_is_explicit` | 无结果时结果列表空 + 摘要明确 |
| `test_blocks_identical_across_modes` | 两模式块内容逐字段相等 |
| `test_stream_usage_matches_non_stream` | 两模式 output_tokens 口径一致 |
| `test_stream_event_sequence_unchanged` | 流式事件序列回归保护 |

## 3. 门禁有效性验证

### 3.1 第一次尝试失败（记录过程）

最初的测试直接调用 `build_websearch_non_stream_response`，**绕过了分派逻辑**。
把 `if !payload.stream` 改成 `if false`（即恢复原缺陷）后，17 项测试照样全绿——
这个门禁是假的。

### 3.2 修正后

把分派抽成独立函数 `wants_stream(payload)` 并直接断言它。再次破坏验证：

```
# 把 wants_stream 改成恒返回 true（即修复前的行为）
test anthropic::websearch::tests::test_dispatch_follows_client_stream_field ... FAILED
test anthropic::websearch::tests::test_stream_field_defaults_to_false ... FAILED
  stream:false 必须走非流式路径

test result: FAILED. 17 passed; 2 failed
```

已恢复，19 项全绿。

## 4. 修复前后对比（真实服务）

### 4.1 非流式（本次修复的核心）

```
$ curl -X POST -H "x-api-key: <client>" .../v1/messages \
    -d '{"model":"claude-sonnet-4.5","max_tokens":1024,"stream":false,
         "messages":[{"role":"user","content":"rust 2026"}],
         "tools":[{"type":"web_search_20250305","name":"web_search","max_uses":8}]}'

content-type: application/json          <- 修复前是 text/event-stream

type/role/model: message assistant claude-sonnet-4.5
stop_reason: end_turn
blocks: ['text', 'server_tool_use', 'web_search_tool_result', 'text']
usage: {"cache_creation_input_tokens":0,"cache_read_input_tokens":0,
        "input_tokens":6,"output_tokens":39,
        "server_tool_use":{"web_search_requests":1}}
query: rust 2026
```

### 4.2 流式（回归，行为必须不变）

```
$ curl -N -X POST ... -d '{...,"stream":true,...}'
content-type: text/event-stream

$ grep "^event:" s.sse | uniq -c
      1 message_start
      1 content_block_start   1 content_block_delta   1 content_block_stop   # text index 0
      1 content_block_start                           1 content_block_stop   # server_tool_use
      1 content_block_start                           1 content_block_stop   # tool_result
      1 content_block_start   2 content_block_delta   1 content_block_stop   # text index 3
      1 message_delta
      1 message_stop
```

11 段事件序列与修复前一致（抽取共享块构造未改变流式行为）。

### 4.3 `/cc/v1` 一致性

```
$ curl -X POST ... .../cc/v1/messages -d '{...}'      # 缺省 stream
content-type: application/json
blocks: ['text','server_tool_use','web_search_tool_result','text'] | stop: end_turn
```

两个 Anthropic 端点行为一致（缓冲差异只作用于上游生成流，与本路径无关）。

## 5. 回归

```
# 无 web_search 的普通请求：Anthropic 错误方言未变
$ curl -X POST .../v1/messages -d '{"model":"nope",...}'
{"error":{"type":"invalid_request_error","message":"模型不支持: nope"}}

# 混合工具不走 websearch（判定口径未改）→ 转发上游
$ curl -X POST .../v1/messages -d '{...,"tools":[{"name":"web_search",...},{"name":"other",...}]}'
{"error":{"type":"api_error","message":"上游 API 调用失败: 所有凭据均已禁用（0/1）"}}

# OpenAI 端点未受影响
POST /v1/chat/completions              -> 503（无凭据，预期）
POST /v1/responses + web_search        -> output: ['web_search_call','message']
```

`has_web_search_tool` 的判定口径未改（`tools.len() == 1 && name == "web_search"`）。

## 6. 安全与卫生

```
$ git status --short
（无 config.json / credentials.json / .codegraph）
```

临时配置与凭据副本已删除。

## 7. 未执行项

无。本 change 的全部功能路径均已端到端验证——web_search 不依赖上游 generate，
不受示例凭据限制（这也是选它作为 Phase D 首项的原因）。

`pnpm build` 未重跑：前端无改动。
