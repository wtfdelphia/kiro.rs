# 验证记录（Phase C）

日期：2026-07-27。以下均为本会话真实运行的命令与输出摘录。

## 1. OpenSpec

```
$ openspec validate --all
✓ change/openai-responses-api-compat
Totals: 14 passed, 0 failed
```

## 2. 测试

```
$ cargo test
test result: ok. 487 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

新增 101 项（Phase B 后 386 → 487）：

| 模块 | 项数 | 覆盖 |
| --- | --- | --- |
| `openai::responses_types` | 13 | model 缺省/回显、wants_stateful、三种 item 序列化形状 |
| `openai::responses` | 29 | input 三形状、item 类型分派、并行 function_call 合并、pending parts 归集、instructions 位置、D2 报错 |
| `openai::responses_stream` | 20 | 事件顺序、output_index 管理、message item 关闭时机、failed、thinking 排除、SSE 格式 |
| `openai::websearch` | 21 | 宽判定矩阵、开关、查询提取、两种输出形态、分片无损 |
| `openai::handlers` 增量 | 7 | Responses 的 D8 四项、function_call 历史、开关门禁 |
| `openai::tests` 增量 | 7 | Responses 的 auth/body limit/405/retrieve 404/D2 over HTTP |
| `admin::service` 增量 | 3 | 开关默认值、切换、落盘 |
| `public_api` 增量 | 1 | responses live / retrieve planned / hints 完备 |

## 3. web_search 代执行（**唯一端到端跑通的路径**）

该路径不经上游 generate，因此在只有示例凭据的环境下也能完整验证。

### 3.1 非流式

```
$ curl -X POST -H "x-api-key: <client>" .../v1/responses \
    -d '{"model":"claude-sonnet-4.5","input":"rust 2026 news","tools":[{"type":"web_search"}]}'

{"id":"resp_bd938b8b...","object":"response","created_at":1785205132,
 "status":"completed","model":"claude-sonnet-4.5",
 "output":[
   {"id":"ws_b66f689b...","type":"web_search_call","status":"completed",
    "action":{"query":"rust 2026 news","type":"search"}},
   {"id":"msg_ad297b30...","type":"message","role":"assistant","status":"completed",
    "content":[{"type":"output_text",
      "text":"Here are the search results for \"rust 2026 news\":\n\nNo results found.\n\n..."}]}]}
```

输出结构正确（`web_search_call` + `message`）。MCP 因示例凭据无结果，摘要**明确写了
"No results found"** —— 不是看起来正常但内容为空的成功响应（spec 要求）。

### 3.2 流式

```
$ curl -N -X POST ... -d '{...,"stream":true,"tools":[{"type":"web_search"}]}'

$ grep "^event:" ws.sse | uniq -c
      1 response.created
      1 response.in_progress
      1 response.output_item.added        <- web_search_call
      1 response.output_item.done
      1 response.output_item.added        <- message
      1 response.content_part.added
      2 response.output_text.delta
      1 response.content_part.done
      1 response.output_item.done
      1 response.completed
$ tail -2 ws.sse
data: [DONE]
```

事件序列与 design §8.4 完全一致，SSE 带 `event:` 行。

### 3.3 运行时开关（热更新）

```
$ curl -H "x-api-key: <admin>" .../api/admin/settings/websearch
{"webSearchEmulation":true}                          # 默认启用

$ curl -X PUT ... -d '{"webSearchEmulation":false}'
{"success":true,"message":"web_search 代执行已关闭（仅影响 /v1/responses 端点）"}

# 关闭后同一请求不再代执行，改走正常 tools 路径去调上游
$ curl -X POST ... -d '{...,"tools":[{"type":"web_search"}]}'
{"error":{"message":"上游 API 调用失败: 所有凭据均已禁用（0/1）",
          "type":"server_error","code":null}}

$ curl -X PUT ... -d '{"webSearchEmulation":true}'    # 恢复
{"success":true,"message":"web_search 代执行已启用（仅影响 /v1/responses 端点）"}

$ curl -X PUT .../api/admin/settings/websearch        # 未认证
401
```

开关行为符合 spec：关闭后不拦截、不报错、走正常工具路径。

## 4. 无状态语义（D2）

```
$ curl -X POST ... -d '{"model":"claude-sonnet-4.5","input":"hi","previous_response_id":"resp_1"}'
{"error":{"message":"previous_response_id is not supported: this service does not enable
 stateful continuation. Send the full conversation in `input` instead.",
 "type":"invalid_request_error","code":null}}

$ curl -X POST ... -d '{"model":"claude-sonnet-4.5","input":""}'
{"error":{"message":"input must contain at least one message",
          "type":"invalid_request_error","code":null}}
```

**实现时修正的一处顺序问题**：归一校验最初放在 provider 检查之后，导致无 provider
环境下 `previous_response_id` 请求返回 503 而非 400（被单测 `test_responses_
previous_response_id_rejected_over_http` 抓到）。已把归一前置——请求本身不合法时
应回 400，不该被 503 掩盖。

## 5. 启动日志（catalog 驱动）

```
可用 API:
  GET  /v1/models
  POST /v1/messages
  POST /v1/messages/count_tokens
  POST /cc/v1/messages
  POST /cc/v1/messages/count_tokens
  POST /v1/chat/completions
  POST /v1/responses            <- 新增，未改日志代码
```

## 6. Admin 面板

```
  GET   /v1/models                    live
  POST  /v1/messages                  live
  POST  /v1/messages/count_tokens     live
  POST  /cc/v1/messages               live
  POST  /cc/v1/messages/count_tokens  live
  POST  /v1/chat/completions          live
  POST  /v1/responses                 live      <- 自动变更
  GET   /v1/responses/{id}            planned
```

## 7. 回归

```
# Anthropic 方言未变（无 code 字段）
$ curl -X POST .../v1/messages -d '{"model":"nope",...}'
{"error":{"type":"invalid_request_error","message":"模型不支持: nope"}}

# Chat 端点方言未变（有 code 字段）
$ curl -X POST .../v1/chat/completions -d '{"model":"nope",...}'
{"error":{"message":"模型不支持: nope","type":"invalid_request_error","code":null}}

$ curl .../v1/responses/resp_123        -> 404    # retrieve 仍 planned
```

三种协议的错误方言各自独立，Anthropic 侧 `has_web_search_tool` 未改（D11）。

## 8. admin-ui

```
$ pnpm build
$ tsc -b && vite build
✓ 1777 modules transformed.
dist/assets/index-D1BdKajv.js   469.58 kB │ gzip: 148.85 kB
✓ built in 48.88s
```

设置面板新增「Web 搜索代执行」分组（Switch + 说明文案）。**未做浏览器渲染验证**
（Phase A 已验证该面板的 dialog 与暗色配色，本次仅在既有分组后追加同构组件）。

## 9. 安全与卫生

```
$ git status --short
（无 config.json / credentials.json / .codegraph）
```

临时配置与凭据副本已删除。

## 10. 未执行项

**真实上游对话未验证**：与 Phase B 同因——示例凭据的 refreshToken 是 20 字符占位值，
会被判定为已截断。因此以下路径**仅由单测覆盖**（给定 Event 序列断言事件序列），
未经真实上游端到端验证：

- `/v1/responses` 非流式基础对话（tasks 9.1）
- `/v1/responses` 流式语义事件（tasks 9.2）—— 但 web_search 路径的流式已真实跑通（§3.2），
  两者共用同一套 `ResponsesSseEvent` 序列化与 SSE 格式
- function tools 的上游往返

已真实跑通的：web_search 代执行两种模式、D2 报错、input 校验、鉴权矩阵、开关热更新、
启动日志、Admin 面板、错误方言分家。

其它未跑项：

- 浏览器渲染验证（见 §8）
- Anthropic 侧 websearch 无条件返回 SSE 的既有问题未修（D11，单列 change）
