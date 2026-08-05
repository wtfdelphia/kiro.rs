# 端到端验证（tasks 10.4）

- 日期：2026-07-29
- 客户端：Codex Desktop，`cli_version 0.146.0-alpha.3.1`
- 模型：`gpt-5.6-sol`（主对话轮）/ `gpt-5.6-luna`（辅助请求）
- 服务：本次构建的 kiro-rs，日志级别 info
- 场景：要求执行 `git status`

## 结论

**通过。** 工具透传与响应侧还原在真实客户端上闭环，工具调用链未断。

## 日志证据（原文摘录，按时间序）

```
06:56:35 INFO  Received POST /v1/responses  model=gpt-5.6-luna stream=true
06:56:35 WARN  请求携带 text.format（结构化输出），但上游无对应能力，该字段被忽略
06:56:35 INFO  工具已送达上游 count=3 names=exec,wait,request_user_input

06:56:35 INFO  Received POST /v1/responses  model=gpt-5.6-sol stream=true
06:56:35 WARN  请求携带 text.format（结构化输出），但上游无对应能力，该字段被忽略
06:56:35 INFO  工具已送达上游 count=9 names=exec,wait,request_user_input,
               collaboration__followup_task,collaboration__interrupt_agent,
               collaboration__list_agents,collaboration__send_message,
               collaboration__spawn_agent,collaboration__wait_agent

06:56:42 INFO  工具调用已分派（流式） upstream_name=exec
               item_type="custom_tool_call" namespace="-"

06:56:43 INFO  Received POST /v1/responses  model=gpt-5.6-sol stream=true
06:56:43 INFO  工具已送达上游 count=9 names=exec,wait,request_user_input,
               collaboration__followup_task,...（同上）
```

## 逐项核对

| 验证点 | 证据 | 结论 |
| --- | --- | --- |
| `additional_tools` 提取（D4） | 主请求 `count=9`；改动前该形状下上游工具数为 0 | 通过 |
| `namespace` 展平（D6） | 6 个 `collaboration__*` 作为独立工具出现，无 `collaboration` 空壳 | 通过 |
| `custom` 降级（D5） | `exec` 出现在上游工具清单中（改动前 schema 为空） | 通过 |
| 响应侧 freeform 还原（D9.1/D9.2） | `item_type="custom_tool_call"`，非 `function_call` | 通过 |
| **工具链未断（D8）** | 06:56:42 分派 `exec` → **06:56:43 客户端发起下一轮**，工具仍为 9 个 | **通过** |
| `text.format` 可观测（D10） | warn 在真实流量上输出；改动前被 serde 静默忽略 | 通过 |

## 「工具链未断」为何是决定性证据

单测无法覆盖「客户端是否接受还原后的 item 形状」这一段。

Codex 的 `exec` handler 只接受 `ToolPayload::Custom{input}`
（`codex-rs/core/src/tools/code_mode/execute_handler.rs:124-130`），
收到 `function_call` 一律回 `"exec expects raw JavaScript source text"`。
若还原形状有误，客户端会拒绝自己的工具调用，**不会产生下一轮请求**。

06:56:42 → 06:56:43 的一秒间隔内客户端完成了：接受 item → 执行 `exec` →
回传结果 → 发起新一轮。这证明 design D9 的还原形状判断正确。

## 未被本次验证覆盖

| 项 | 说明 |
| --- | --- |
| `collaboration__*` 的实际调用与回程还原 | 本次模型只调了 `exec`，展平工具的 `namespace` 还原路径未被真实触发（有单测覆盖，见 tasks 7.7 / 8.5） |
| 超长工具名的两级映射 | 实测工具名最长 30 字符（`collaboration__interrupt_agent`），未触发缩短（有锁定测试覆盖） |
| lark 文法降级后的输出合规性 | 模型本次生成的 `exec` 输入被客户端接受，但样本量为 1，不足以判断降级描述的长期有效性 |
| `sequence_number` | kiro.rs 不发该字段，客户端正常工作 → 印证 design D9.2 的推测（Codex 不依赖它） |

## 噪音说明

日志中的 `正在刷新 IdC Token...` / `凭据 #1 Token 已强制刷新` 来自
Admin API 手动刷新端点（`src/admin/handlers.rs:144`），不在 `/v1/responses`
代理路径上，与本次验证无关。
