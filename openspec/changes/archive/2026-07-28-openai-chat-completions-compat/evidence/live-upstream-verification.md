# 真实上游验证补录（Phase B）

日期：2026-07-28
环境：release 构建部署至本机 `kiro_release` 目录，配合**真实有效凭据**运行
（Token 刷新成功、19 个模型缓存已拉取）

本文补齐 `verification.md` §10 与 tasks 9.1–9.4 中因缺少有效凭据而标记为
`[~]` 的项。密钥值均以占位符表示。

## 1. 非流式基础对话（tasks 9.1）

```
$ curl -X POST -H "x-api-key: <client>" .../v1/chat/completions \
    -d '{"model":"claude-sonnet-4.5",
         "messages":[{"role":"user","content":"回答一个词：Rust 的所有权机制叫什么？"}],
         "max_tokens":100}'

{
  "id": "chatcmpl-8b3c5a6321f94db7a9b4bb09f4ae2ff0",
  "object": "chat.completion",
  "created": 1785208473,
  "model": "claude-sonnet-4.5",
  "choices": [{"index":0,
    "message":{"role":"assistant","content":"Ownership"},
    "finish_reason":"stop"}],
  "usage": {"prompt_tokens":4123,"completion_tokens":3,"total_tokens":4126}
}
```

`prompt_tokens: 4123` 来自 `contextUsageEvent` 反算而非本地估算——**D12 的 usage
优先级在真实数据下生效**（本地估算对这条短请求会得到远小于 4123 的值）。

## 2. 流式 + include_usage（tasks 9.2、9.3）

```
$ curl -N -X POST ... -d '{...,"stream":true,"stream_options":{"include_usage":true}}'
```

原始输出（逐行）：

```
0: data: {"choices":[{"delta":{"role":"assistant"},"finish_reason":null,"index":0}],...}
2: data: {"choices":[{"delta":{"content":"1"},...}],...}
4: : keepalive                                    <- SSE 注释行保活
6: data: {"choices":[{"delta":{"content":"\n2"},...}],...}
8: data: {"choices":[{"delta":{"content":"\n3"},...}],...}
10: data: {"choices":[{"delta":{},"finish_reason":"stop","index":0}],...}
12: data: {"choices":[],...,"usage":{...}}         <- choices 为空数组
14: data: [DONE]
```

逐条对照 design §6：

| 设计要求 | 实测 |
| --- | --- |
| 首块只带 role，不带 content | 第 0 行 ✓ |
| 文本增量 | 第 2/6/8 行，拼接为 `1\n2\n3` ✓ |
| **保活为 SSE 注释行，不是伪 chunk** | 第 4 行 `: keepalive` ✓（真实流中确实出现了） |
| 末块带 finish_reason、delta 为空 | 第 10 行 ✓ |
| usage chunk 的 choices 为空数组 | 第 12 行 ✓ |
| 以 `[DONE]` 结束 | 第 14 行 ✓ |

保活行出现在流中间，验证了「OpenAI 协议无 ping 事件，用注释行」这个选择在真实
时序下有效——若发伪 chunk，SDK 会把它当成一次空增量。

## 3. function tools 真实往返（tasks 9.4）

```
$ curl -X POST ... -d '{"model":"claude-sonnet-4.5",
    "messages":[{"role":"user","content":"北京天气怎么样？用工具查"}],
    "max_tokens":300,
    "tools":[{"type":"function","function":{"name":"get_weather",
      "description":"查询城市天气",
      "parameters":{"type":"object","properties":{"city":{"type":"string"}},
                    "required":["city"]}}}]}'

finish_reason: tool_calls
tool_call: get_weather {"city": "北京"}
```

工具定义（Chat 嵌套形状）→ 上游 → 工具调用回传，全链路正确。
`finish_reason` 为 `tool_calls` 符合 spec。

## 4. thinking 后缀（D8 第一项，真实链路）

```
$ curl -X POST ... -d '{"model":"claude-sonnet-4.5-thinking",
    "messages":[{"role":"user","content":"17 乘 23 等于多少？"}],"max_tokens":600}'

reasoning_content 存在: True
  思考片段: 'The user is asking me to calculate 17 multiplied by 23 in Chinese...'
content: '17 × 23 = 391'
content 是否含 thinking 标签: False
```

这条最关键：**四份设计稿都误述了 thinking 的生效点**（写成「由下游 `resolve_model`
处理」），实际靠 handler 层显式调用 `override_thinking_from_model_name`。
现在有真实证据表明该链路完整工作：

- 后缀被识别，thinking 指令到达上游
- 思考内容进 `reasoning_content`
- `content` 干净，无 `<thinking>` 标签泄漏

## 5. Anthropic 端点回归（真实上游）

```
$ curl -X POST ... .../v1/messages -d '{"model":"claude-sonnet-4.5","max_tokens":50,
    "messages":[{"role":"user","content":"只回复 OK"}]}'

content: OK | stop: end_turn | usage: {'input_tokens': 4106, 'output_tokens': 1}
```

## 6. 状态更新

tasks 9.1–9.4 的 `[~]` 现可转为完成。`verification.md` §5/§10 中「真实上游对话
未验证」的限制已解除。

仍未做：cors layer 缺失的转红验证（需浏览器环境，见 spec-compliance-report 发现 2）。
