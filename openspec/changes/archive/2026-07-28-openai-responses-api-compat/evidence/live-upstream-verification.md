# 真实上游验证补录（Phase C）

日期：2026-07-28
环境：release 构建 + 本机真实有效凭据（Token 刷新成功、19 个模型缓存已拉取）

本文补齐 `verification.md` §10 与 tasks 9.1–9.2 中标记为 `[~]` 的项。
密钥值均以占位符表示。

## 1. 非流式对话（tasks 9.1）

```
$ curl -X POST -H "x-api-key: <client>" .../v1/responses \
    -d '{"model":"claude-sonnet-4.5",
         "input":"用一个词回答：Rust 的包管理器叫什么？","max_output_tokens":50}'

{
  "id": "resp_c8d389f3de504bdaa9a71e2aae6ed8d2",
  "object": "response",
  "created_at": 1785208598,
  "status": "completed",
  "model": "claude-sonnet-4.5",
  "output": [{
    "id": "msg_ed3926116e8c41c69e674fe5c2666461",
    "type": "message", "role": "assistant", "status": "completed",
    "content": [{"type":"output_text","text":"Cargo"}]
  }],
  "usage": {"input_tokens":4123,"output_tokens":2,"total_tokens":4125}
}
```

逐项核对 spec：`status: completed`、message item 含 `output_text` part、
`input_tokens: 4123` 来自上游 context-usage 信号（非本地估算）、model 回显原值。

## 2. 流式语义事件（tasks 9.2）

```
$ curl -N -X POST ... -d '{"model":"claude-sonnet-4.5","input":"数到三，只输出数字",
    "max_output_tokens":50,"stream":true}'

事件序列:
   response.created
   response.in_progress
   response.output_item.added
   response.content_part.added
   response.output_text.delta
   response.output_text.delta
   response.output_text.delta
   response.content_part.done
   response.output_item.done
   response.completed
   [DONE]

拼接文本: '1\n2\n3'
```

与 design §7 的文本路径事件顺序**逐名一致**，且 SSE 带 `event:` 行
（区别于 Chat Completions 的纯 `data:` 行）。三个 delta 拼回完整文本，无丢失。

这解除了 `verification.md` §10 中「未验证的是 Kiro Event → 语义事件这一段的
实际转换行为」这一限制。

## 3. web_search 代执行（真实 MCP 后端）

`verification.md` §3 已在示例凭据下验证过结构，但当时 MCP 返回空结果。
本次用真实凭据拿到了实际搜索结果：

```
$ curl -X POST ... -d '{"model":"claude-sonnet-4.5",
    "input":"Rust 1.90 release notes","tools":[{"type":"web_search"}]}'

output items: ['web_search_call', 'message']
query: Rust 1.90 release notes
摘要前 300 字:
Here are the search results for "Rust 1.90 release notes":

1. **Patch Notes 1.</a>90 — Rust Console Edition**
   In pitch black, explore the highly radiated corridors with new Night Vision
   Goggles, fight your way through with 3 new weapons - ...
```

- output items 结构正确（`web_search_call` + `message`）
- 查询**原样传递**给 MCP（未剥离、未改写）
- 真实结果被正确解析并生成摘要

注意：搜到的是游戏 Rust 而非编程语言。这是上游搜索引擎的相关性问题，**不是代理
缺陷**——代理的职责是原样传递查询并忠实呈现返回结果，两者都做到了。

顺带说明：结果标题中出现 `1.</a>90` 这样的残留 HTML 片段，来自搜索后端的原始
数据。当前实现不做 HTML 清洗（`generate_search_summary` 原样拼接 title/snippet）。
是否清洗属产品决策，不在本 change 范围。

## 4. 状态更新

tasks 9.1–9.2 的 `[~]` 现可转为完成。`verification.md` §10 的「上游对话未验证」
限制已解除。

仍未做：

- admin-ui 浏览器渲染验证（见 spec-compliance-report 发现 4）
- thinking 内容在 Responses 端点被丢弃仍是已知设计选择（发现 3），未改

---

## 附：核验补漏项的运行时验证（2026-07-28 13:47）

`openspec-verify-report` 发现 1 修复时新增的两条 Scenario，此处补运行时证据。
重新部署的 release 产物与 11:12 版本字节数相同（11820032），因两处改动均在
`#[cfg(test)]` 区内、不进入 release 产物——已用 `cargo build --release` 重建确认。

### Scenario「响应不含无关密钥」

```
$ curl -H "x-api-key: <admin>" .../api/admin/settings/websearch
{"webSearchEmulation":true}

含 client key: False
含 admin key:  False
字段数: 1
```

响应恰为单字段，与单测 `test_websearch_response_has_no_unrelated_secrets`
的断言一致。

### Scenario「不影响 Anthropic 端点」

```
$ curl -X PUT ... -d '{"webSearchEmulation":false}'
web_search 代执行已关闭（仅影响 /v1/responses 端点）

# 开关关闭后，Anthropic 端点仍按自身口径代执行
$ curl -X POST .../v1/messages -d '{...,"tools":[{"type":"web_search_20250305",
    "name":"web_search","max_uses":8}]}'
Anthropic 侧 blocks: ['text','server_tool_use','web_search_tool_result','text']
```

开关关闭状态下 Anthropic 端点行为不变，证实该开关只约束
`/v1/responses`（与单测的编译期签名约束互补：运行时确认行为，编译期防止误接）。

开关已恢复为启用。

### 四协议冒烟

| 端点 | 结果 |
| --- | --- |
| `POST /v1/messages` | `OK`，usage `{input:4106, output:1}` |
| `POST /v1/chat/completions` | `OK`，`finish_reason: stop`，usage `{4106,1,4107}` |
| `POST /v1/responses` | `OK`，`status: completed`，usage `{4106,1,4107}` |
| `GET /v1/models` | 38 个模型 |

启动日志 7 个对外端点齐全；凭据 Token 刷新成功、19 个模型缓存已拉取。
