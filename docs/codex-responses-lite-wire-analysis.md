# Codex responses-lite 工具承载形状：wire 实测分析

> 性质：事实记录（wire-level 实测 + 权威源码核对）。方案与任务见
> `openspec/changes/codex-responses-lite-tool-passthrough/`
> 日期：2026-07-29
> 取证手段：kiro.rs 入口落盘原始请求体（临时探针，已回滚）4 份；
> openai/codex 本地 checkout 源码核对；Kiro IDE 官方扩展反编译对照

## 0. 结论先行

`/v1/responses` 对 Codex 客户端丢失全部工具，根因在 kiro.rs：
**responses-lite 模式的工具不在 `tools` 字段里，而在 `input[0]` 的
`additional_tools` item 里，该 item 被 kiro.rs 静默丢弃。**

## 1. 抓包样本

探针位置：`src/openai/handlers.rs` 的 `post_responses` 入口，落盘 `body` 原文。

| 文件 | 字节 | model | 用途 |
| --- | --- | --- | --- |
| `...T042537.400` | 59680 | `gpt-5.6-luna` | 辅助请求：生成任务标题 |
| `...T044750.862513` | 115442 | `gpt-5.6-sol` | 主对话轮 |
| `...T044820.718388` | 59680 | `gpt-5.6-luna` | 辅助请求 |
| `...T044820.737572` | 82875 | `gpt-5.6-sol` | 主对话轮 |

4 份的顶层结构完全一致：

```
顶层字段: client_metadata, include, input, model, parallel_tool_calls,
          prompt_cache_key, reasoning, store, stream, text, tool_choice
has tools 字段  : False
has instructions: False
```

## 2. 工具承载形状

### 2.1 lite 模式把工具搬进 input

`codex-rs/core/src/client.rs:855-879`：

```rust
let (instructions, tools) = if model_info.use_responses_lite {
    let tools = create_tools_json_for_responses_api(&prompt.tools)?;
    let mut prefix = vec![ResponseItem::AdditionalTools {
        id: None, role: "developer".to_string(), tools,
    }];
    // base_instructions 也作为 developer message 进 prefix
    input.splice(0..0, prefix);
    (String::new(), None)   // instructions 空串、tools 为 None
```

`instructions` 因 `#[serde(skip_serializing_if = "String::is_empty")]`
（`codex-rs/codex-api/src/common.rs:254`）而不出现；`tools` 为 `Option::None` 同样不出现。

官方对照测试：`codex-rs/core/tests/suite/responses_lite.rs:111-131`
断言 `body.get("tools").is_none()` 与 `input[0]["type"] == "additional_tools"`。

命中该路径的模型（`codex-rs/models-manager/models.json`）：

| slug | `use_responses_lite` | `tool_mode` |
| --- | --- | --- |
| `gpt-5.6-sol` / `gpt-5.6-terra` / `gpt-5.6-luna` | true | `code_mode_only` |
| `gpt-5.5` / `gpt-5.4` / `gpt-5.4-mini` / `gpt-5.2` | false | 无 |

### 2.2 实测工具集

主请求（`gpt-5.6-sol`）4 个工具：

| wire `type` | 名称 | 载荷字段 |
| --- | --- | --- |
| `custom` | `exec` | `format{type:"grammar", syntax:"lark", definition}` |
| `function` | `wait` | `parameters` / `strict` |
| `function` | `request_user_input` | `parameters` / `strict` |
| `namespace` | `collaboration` | `tools[]`（6 个内层 function） |

`collaboration` 内层：`followup_task` / `interrupt_agent` / `list_agents` /
`send_message` / `spawn_agent` / `wait_agent`，均为
`{type, name, description, strict, parameters}`。来源是 `multi_agent_version: v2`。

辅助请求（`gpt-5.6-luna`）少 `collaboration`，其余相同。

### 2.3 `code_mode_only` 把执行能力折叠进 exec

`codex-rs/core/src/tools/spec_plan.rs:435-445` 的 `is_hidden_by_code_mode_only`：
`tool_mode == CodeModeOnly` 时，除 `exec` 与 `wait` 外的嵌套工具
（`is_code_mode_nested_tool`，`code-mode-protocol/src/description.rs:248-250`）
全部从顶层工具列表剔除，改为写进 `exec` 的 description。

实测印证：`exec.description` 长 10558 字符，含 `git` / `shell` / `apply_patch`
的用法说明。所以该模型**没有独立的 shell / apply_patch 工具**，`exec` 是唯一执行入口。

### 2.4 exec 的输入是裸 JS，不是 JSON

`exec.description` 原文：

> - Runs raw JavaScript -- no Node, no file system, no network access, no console.
> - Accepts raw JavaScript source text, not JSON, quoted strings, or markdown code fences.
> - All nested tools are available on the global `tools` object, for example `await tools.exec_command(...)`.

`format.definition` 为 lark 文法（`start: pragma_source | plain_source`，
`SOURCE: /[\s\S]+/`），与 `codex-rs/core/src/tools/code_mode/execute_spec.rs:14-22`
的 `CODE_MODE_FREEFORM_GRAMMAR` 逐字一致。

**降级为 function 时必须处理这个矛盾**：模型被 schema 要求回 JSON `arguments`，
却在同一个工具的 description 里读到「不要用 JSON」。

## 3. kiro.rs 的丢弃链路

`src/openai/responses.rs` 的 `convert_items`，item type 为 `additional_tools`：

1. 不匹配 `message` / `function_call` / `function_call_output` / `input_text` 等已知分支
2. 落到 `_ =>` 兜底（`:190`）
3. `role = "developer"` 非空 → 走 `build_message`
4. `build_message`（`:217-272`）依次找 `content` / `text`，两者都无 → 返回 `None`
5. item 被跳过，**且不打日志**（warn 只在 `role` 为空的 else 分支，`:198`）

同时 `req.tools` 为 `None`（顶层无 `tools` 字段），`to_chat_request_json:59-73`
的 `if let Some(tools)` 不成立，`chat["tools"]` 不写。

上游 `userInputMessageContext.tools` 为空数组。模型答「未提供可用的终端或文件读取工具」
是对其所见事实的准确陈述。

同类问题：历史回传里的 `custom_tool_call` 与 `custom_tool_call_output`
同样落 `_ =>` 分支（这两个有 warn），导致第二轮里工具调用与结果双双消失。

## 4. 上游能力边界

### 4.1 Kiro 只认 toolSpecification

反编译 `%LOCALAPPDATA%/Programs/Kiro/resources/app/extensions/kiro.kiro-agent/dist/extension.js`：

`extension.js:445514` 的 `Tool2.visit` 只有两个成员：

```js
if (value.toolSpecification !== void 0) return visitor.toolSpecification(...);
if (value.cachePoint       !== void 0) return visitor.cachePoint(...);
```

`extension.js:445508` 的归一函数 `Tt4` 接受四种输入形状，输出恒为
`{toolSpecification: {name, description, inputSchema: {json: W19(parameters)}}}`。
MCP 工具同样降级，并由 `formatToolName`（`:593349`）改名为 `mcp_<server>_<tool>`。

所以「Kiro IDE 里所有工具都能用」不是因为上游支持多种工具协议，
而是官方客户端在发送前把每种工具都降级成了普通 function + JSON Schema。
**`custom` 降级为 JSON Schema 是官方标准做法，不是妥协。**

kiro.rs 侧对应：`src/kiro/model/requests/conversation.rs:146-153` 的
`UserInputMessageContext` 只有 `toolResults` 与 `tools` 两个字段。

### 4.2 没有结构化输出字段

辅助请求带 `text.format = {type:"json_schema", strict:true, schema:{...}}`
（Codex 用它约束任务标题为 `{"title": "..."}`）。

`ResponsesRequest`（`src/openai/responses_types.rs:20-50`）没有 `text` 字段，
serde 静默忽略。而上游 `userInputMessageContext` 也没有任何 response format 概念，
**无处透传**。只能声明不支持并打 warn；在 prompt 层模拟会把 `strict: true`
降格为「尽力」，属假装支持。

## 5. 响应侧的强约束

### 5.1 freeform 工具必须回 custom_tool_call

`codex-rs/core/src/tools/code_mode/execute_handler.rs:124-130`：

```rust
match payload {
    ToolPayload::Custom { input } if is_exec_tool_name(&tool_name) => ...,
    _ => Err(FunctionCallError::RespondToModel(format!(
        "{PUBLIC_TOOL_NAME} expects raw JavaScript source text"))),
}
```

`build_tool_call`（`codex-rs/core/src/tools/router.rs:127-175`）把
`ResponseItem::FunctionCall` 映射成 `ToolPayload::Function{arguments}`，
`CustomToolCall` 映射成 `ToolPayload::Custom{input}`。

因此响应侧回 `function_call` 时，`exec` 会被客户端自己拒掉，模型陷入重试。
**这是功能性阻断，不是降级质量问题。**

### 5.2 namespace 工具按 (namespace, name) 匹配

`ResponseItem::FunctionCall` 与 `CustomToolCall` 都带独立的
`namespace: Option<String>` 字段（`codex-rs/protocol/src/models.rs:880` / `:934`）。
`router.rs:137` 用 `ToolName::new(namespace, name)` 查注册表。

只回展平名而不带 `namespace`，客户端查不到工具。展平必须配逆映射。

### 5.3 Codex 不读 SSE 的 event: 行

`codex-rs/codex-api/src/sse/responses.rs:532` 只解析 `sse.data`，
按 JSON body 内的 `type` 字段分派。kiro.rs 同时发 `event:` 与 `data:` 行，兼容。

解析器实际 match 的事件名（`sse/responses.rs:330-466`）中，
**没有** `response.function_call_arguments.delta` 分支——function 工具的参数靠
`response.output_item.done` 一次性交付；`custom` 工具有专属的
`response.custom_tool_call_input.delta`。

`custom` 工具的权威事件序列（`codex-rs/app-server/tests/suite/v2/turn_start.rs:2906-2932`）：

```
response.output_item.added              item{type:custom_tool_call, call_id, name, input:"", status:in_progress}
response.custom_tool_call_input.delta   {item_id, call_id, delta}
response.output_item.done               item{type:custom_tool_call, ..., input:<完整文本>}
```

## 6. 易踩的坑

### 6.1 `inputSchema` / `deferLoading` 是方言，不是 wire 形状

`~/.codex/sessions/**/rollout-*.jsonl` 的 `session_meta.dynamic_tools` 里，
内层工具用 `inputSchema` 与 `deferLoading`（camelCase）。**那不是 HTTP wire 形状。**

它是 `DynamicToolSpec`（`codex-rs/protocol/src/dynamic_tools.rs:10-27`，
`#[serde(rename_all = "camelCase")]`），Desktop 经 app-server JSON-RPC 传入 Codex 的内部结构。
发往 HTTP 的是 `ResponsesApiTool`（`codex-rs/tools/src/responses_api.rs:25-37`）：
`name` / `description` / `strict` / `defer_loading` / `parameters`。

转换在 `dynamic_tool_to_responses_api_tool`，且
`codex-rs/core/src/tools/handlers/dynamic.rs:58-59` 显式把 `defer_loading` 置 `None`。

实测 4 份抓包中，内层工具字段恒为 `{type, name, description, strict, parameters}`，
`inputSchema` 与 `deferLoading` **零出现**。

推论：`dynamic_tools` 与实际请求 `tools` 无对应关系。会话记录里 Desktop 的 16 个
`codex_app` 工具（`read_thread_terminal` / `list_threads` 等）在 4 份抓包中全部不出现——
它们 `deferLoading: true` 走 deferred 通道，且被 `code_mode_only` 折叠。

### 6.2 schema 里的 `encrypted` 字段

`collaboration` 内层 3 个工具（`followup_task` / `send_message` / `spawn_agent`）的
`parameters.properties.message` 带 `"encrypted": true`。

这是 Codex 的 Responses-only 标记（`codex-rs/tools/src/json_schema.rs:49-50`
注释：`Responses-only marker for reviewed encrypted tool parameters`），Kiro 上游无此概念。

### 6.3 请求侧没有 `local_shell`

`ToolSpec`（`codex-rs/tools/src/tool_spec.rs:19-53`）只有 5 个 variant：
`Function` / `Namespace` / `ToolSearch` / `WebSearch` / `Freeform`。
**没有 `LocalShell`**——`local_shell_call` 只作为响应侧 `ResponseItem` 存在
（`codex-rs/protocol/src/models.rs:860`）。

## 7. 未验证项与剩余风险

| 项 | 状态 |
| --- | --- |
| `tool_search` 是否真被发送 | **4 份抓包零出现**。`search_tool_enabled` 要求 `model_info.supports_search_tool`，未纳入本次范围 |
| `web_search` 是否真被发送 | 零出现。lite 模式下 `hosted_model_tool_specs` 直接返回空（`spec_plan.rs:303-305`），hosted 工具不发 |
| 上游对 `$schema` / `$defs` / `oneOf` 的容忍度 | 未验证。本次实测 schema 未出现这些构造，风险低于预期 |
| lark 文法降级后模型的实际表现 | 未验证。模型看到自然语言描述的约束而非结构化文法，可能生成不合文法输入；Codex 侧 `apply-patch/src/parser.rs` 有容错与重试 |
| `sequence_number` 是否被 Codex 依赖 | 推测不依赖：`ResponsesStreamEvent` 有该字段但 `process_responses_event` 未读取。kiro.rs 当前不发 |
| `prefer_websockets` | `models.json` 中该字段存在，但 Rust 侧 `ModelInfo` **无对应字段**，代码中零引用，属未消费的元数据 |
| Codex 版本漂移 | 基于本地 checkout 与 `cli_version 0.146.0-alpha.3.1`。`ToolSpec` 曾变动（`local_shell` 已移出请求侧），升级后须重核 §2 |
