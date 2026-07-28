# 验证记录（Phase B）

日期：2026-07-27。以下均为本会话真实运行的命令与输出摘录。

## 1. OpenSpec

```
$ openspec validate --all
✓ change/openai-chat-completions-compat
✓ change/public-api-catalog-admin-display
Totals: 13 passed, 0 failed
```

## 2. 测试

```
$ cargo test
test result: ok. 386 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

新增 80 项（Phase A 后 306 → 386）：

| 模块 | 项数 | 覆盖 |
| --- | --- | --- |
| `openai::types` | 11 | tool 双形状、max_tokens 优先级、include_usage 默认、content 三态 |
| `openai::converter` | 18 | system 合并、tool_calls/tool_result 配对与归集、image parts、参数不透传 |
| `openai::error` | 6 | shape 非 Anthropic、状态码映射、provider 错误分流 |
| `openai::stream` | 22 | 首块仅 role、tool_calls 增量与 index 稳定、thinking 跨 chunk、usage 开关、UTF-8 边界 |
| `openai::handlers` | 15 | D8 四项、D9 回显、D10 不劫持、finish_reason 优先级、非流式 shape |
| `openai::tests`（路由） | 7 | auth 矩阵、body limit、错误 shape、405 |
| `public_api` 增量 | 1 | chat completions live / responses 仍 planned |

## 3. 门禁有效性验证（关键）

逐个破坏后确认测试转红，再恢复。

### 3.1 D8 第一项：thinking 后缀

注释掉 `override_thinking_from_model_name(&mut msg_req)`：

```
test openai::handlers::tests::test_thinking_suffix_reaches_upstream ... FAILED
test openai::handlers::tests::test_thinking_enabled_flag_set_from_suffix ... FAILED

带 -thinking 后缀的请求必须把 thinking 指令带到上游，实际:
{"conversationState":{...,"currentMessage":{"userInputMessage":{"content":"hi",
 "modelId":"claude-sonnet-4.5","origin":"AI_EDITOR","userInputMessageContext":{}}}}}
```

失败输出直接展示了降级后的上游请求——system 里没有 `<thinking_mode>`。这正是
四份设计稿误述该逻辑时会产生的静默 bug。

### 3.2 D8 第二项：tool_name_map

把 `tool_name_map: conversion.tool_name_map` 改为 `HashMap::new()`：

```
test openai::handlers::tests::test_tool_name_map_captured_for_long_names ... FAILED
```

### 3.3 auth layer（安全）

从 `create_openai_routes` 去掉 `auth_middleware` layer：

```
test openai::tests::test_auth_required_no_key_rejected ... FAILED
test openai::tests::test_auth_required_wrong_key_rejected ... FAILED
```

证明「`merge` 不传播 layer」这个陷阱已被测试覆盖。

### 3.4 body limit

去掉 `DefaultBodyLimit::max(MAX_BODY_SIZE)`：

```
test openai::tests::test_body_over_default_limit_not_rejected ... FAILED
assertion `left != right` failed: 4MB 请求不应被默认 2MB 限制拦截
```

四项破坏均已恢复，`cargo test openai` 重新全绿（80 passed）。

## 4. 启动日志（catalog 驱动）

```
可用 API:
  GET  /v1/models
  POST /v1/messages
  POST /v1/messages/count_tokens
  POST /cc/v1/messages
  POST /cc/v1/messages/count_tokens
  POST /v1/chat/completions      <- 新增，未改日志代码
```

只改了 catalog 中的 status，日志自动多出一条。Phase A 建立的单一事实源生效。

## 5. 鉴权与错误方言（真实服务）

```
$ curl -X POST .../v1/chat/completions -d '{...}'          # 无 key
401

$ curl -X POST -H "x-api-key: <client>" ... -d '{bad'
{"error":{"message":"Invalid JSON: key must be a string at line 1 column 2",
          "type":"invalid_request_error","code":null}}

$ ... -d '{"model":"claude-sonnet-4.5","messages":[{"role":"system","content":"s"}]}'
{"error":{"message":"messages must contain at least one non-system message",
          "type":"invalid_request_error","code":null}}

$ ... -d '{"model":"nope-xyz","messages":[{"role":"user","content":"hi"}]}'
{"error":{"message":"模型不支持: nope-xyz","type":"invalid_request_error","code":null}}

$ ... -d '{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"hi"}]}'
{"error":{"message":"上游 API 调用失败: 所有凭据均已禁用（0/1）",
          "type":"server_error","code":null}}
```

最后一条说明请求已走通到 provider 层。**未能完成真实上游对话验证**：
本地只有示例凭据（`credentials.example.social.json` 的 refreshToken 是占位值，
长度 20 字符会被判定为已截断），无法发起真实 generate 调用。因此
`stream:true` 的 chunk 序列、`include_usage` 末块、function tools 主路径
的**端到端**行为未经真实上游验证，仅由 22 项 stream 单测（给定 Event 序列
断言 chunk 序列）与 15 项 handler 单测覆盖。

## 6. 错误方言分家（回归）

```
# Anthropic 端点：{error:{type,message}}，无 code
$ curl -X POST .../v1/messages -d '{"model":"nope-xyz","max_tokens":10,...}'
{"error":{"type":"invalid_request_error","message":"模型不支持: nope-xyz"}}

# OpenAI 端点：{error:{message,type,code}}
{"error":{"message":"模型不支持: nope-xyz","type":"invalid_request_error","code":null}}
```

两种方言严格分家，共用同一份错误文案。

## 7. 回归与边界

```
GET  /v1/models  带 key        -> 200
POST /v1/responses             -> 404    # 仍为 planned
POST /chat/completions         -> 404    # 首版无别名（D5）
GET  /v1/chat/completions      -> 405    # 方法不匹配（单测）
```

## 8. Admin 面板

```
$ curl -H "x-api-key: <admin>" .../api/admin/public-api

  GET   /v1/models                    live
  POST  /v1/messages                  live
  POST  /v1/messages/count_tokens     live
  POST  /cc/v1/messages               live
  POST  /cc/v1/messages/count_tokens  live
  POST  /v1/chat/completions          live      <- 自动变更
  POST  /v1/responses                 planned
  GET   /v1/responses/{id}            planned

chat hints:
  · OPENAI_BASE_URL 需带 /v1 后缀
  · 响应回显的 model 为客户端请求的原始名（如 gpt-4o），并非实际执行的 Claude 模型…
  · usage 需客户端传 stream_options.include_usage 才在流式末尾返回
  · 不支持服务端 web_search 工具，该能力仅在 /v1/responses 提供
```

Admin UI 未改一行代码，面板自动反映新状态。`pnpm build` 未重跑
（前端无改动，dist 与 Phase A 一致）。

## 9. 安全与卫生

```
$ git status --short
（无 config.json / credentials.json / .codegraph）
```

临时配置、凭据副本、破坏性验证的备份文件均已删除。

## 10. 未执行项

- 真实上游对话（流式 / 非流式 / tools）：无有效凭据，见 §5
- `pnpm build`：前端无改动，未重跑
- cors layer 缺失的转红验证：需要浏览器环境，仅由代码审查保证（layer 已挂）
