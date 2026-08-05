## Why

`/v1/responses` 对 OpenAI Codex 客户端**丢失全部工具定义**，模型因此声明自己没有终端与文件能力，无法执行任何操作。

根因经 wire-level 实测确认（4 份原始请求体抓包，见 `docs/codex-responses-lite-wire-analysis.md`）：
`gpt-5.6-sol` / `gpt-5.6-luna` / `gpt-5.6-terra` 的 `use_responses_lite = true`，
请求体**不含 `tools` 字段、不含 `instructions` 字段**，工具改由
`input[0] = {"type":"additional_tools","role":"developer","tools":[...]}` 承载
（`codex-rs/core/src/client.rs:855-879`）。

kiro.rs 的 `convert_items`（`src/openai/responses.rs:190-200`）对该 item 落到 `_ =>` 兜底分支：
因 `role="developer"` 非空而走 `build_message`，该 item 既无 `content` 也无 `text`，
返回 `None` → **整条被丢弃，且不打日志**（warn 只在 role 为空的分支）。
同时 `req.tools` 为 `None`，`to_chat_request_json` 不写 `tools`。上游收到零工具。

实测工具集（主请求，`gpt-5.6-sol`，`tool_mode: code_mode_only` + `multi_agent_version: v2`）：

| wire `type` | 名称 | 说明 |
| --- | --- | --- |
| `custom` | `exec` | lark 文法，输入为裸 JavaScript 源码，**该模型唯一的执行入口** |
| `function` | `wait` | 等待 exec cell 产出 |
| `function` | `request_user_input` | 向用户提问 |
| `namespace` | `collaboration` | 内含 6 个子 agent 工具（`spawn_agent` / `wait_agent` 等） |

三种形状（`custom` / `namespace` / lite 的 `additional_tools` 容器）Kiro 上游都不认。
Kiro IDE 官方扩展的做法是**一律降级为 `toolSpecification`**（反编译 `extension.js:445508` 的归一函数 `Tt4`，
上游工具模型只有 `toolSpecification` 与 `cachePoint` 两个成员，`extension.js:445514`），
本 change 沿用该方向，不引入上游不存在的协议概念。

## What Changes

### 请求侧

- **提取 `additional_tools`**：归一时把 `input[0]` 的 `tools[]` 并入顶层 `tools`（与 `req.tools` 合并），
  原 item 丢弃（它是 `tools` 字段的搬家，不是对话内容）
- **`custom` → function 降级**：schema 用 `{input: string}`；在 description 前插入调用约定覆盖说明，
  化解 `exec` 原文「Accepts raw JavaScript source text, not JSON」与 JSON `arguments` 的直接冲突
- **`namespace` 展平**：内层 function 提升为顶层独立工具，名字为 `<namespace>__<name>`
  （对齐 Codex 自身的 `code_mode_name_for_tool_name`，`codex-rs/tools/src/code_mode.rs:155-162`）；
  同时产出逆映射供响应侧还原。展平名与顶层工具重名、或两个 namespace 展平到同名时返回 400，**不自动改名**
- **超长名截断沿用既有机制**（`shorten_tool_name` + `tool_name_map`，
  `src/anthropic/converter.rs:806-818`），归一层不新写第二套哈希规则，
  也**不预测**缩短结果——`ToolRewriteMap` 的 key 用展平名/原名，因为还原发生在响应侧分派之前
  （见 design D3.1 修正记录）
- **剔除 `encrypted`**：递归删除 schema 中的 `encrypted` 字段（Codex 的 Responses-only 标记，
  `codex-rs/tools/src/json_schema.rs:49-50`），作用域仅限本 change 新增的归一层
- **历史 item 改写**（与降级同批，缺则第二轮工具调用与结果双双丢失、模型重复执行）：
  `custom_tool_call` / `custom_tool_call_output` 转为 function 形状；
  `function_call` 带 `namespace` 时无条件拼成展平名；`tool_choice` 同步改写
- **不再静默**：丢弃工具时 `warn` 带 name 与 type

### 响应侧

- **`function_call` → `custom_tool_call` 还原**，流式与非流式都做。
  这不是降级质量问题而是功能性阻断：Codex 的 `exec` handler 只接受 `ToolPayload::Custom{input}`
  （`codex-rs/core/src/tools/code_mode/execute_handler.rs:124-130`），收到 `function_call` 一律回
  "exec expects raw JavaScript source text"，模型陷入重试
- **namespace 还原**：上游回展平名时，输出 item 还原为 `name` = 原名、`namespace` = 原 namespace。
  Codex 按 `ToolName::new(namespace, name)` 查注册表（`codex-rs/core/src/tools/router.rs:137`），
  只回展平名会查不到工具
- **流式参数缓冲**：`custom_tool_call_input.delta` 的载荷是已提取的 `input`，
  而 `input` 只有在参数 JSON 完整后才能提取，因此上游 `function_call_arguments.delta` 必须吞掉、
  缓冲到 `.done` 再一次性发出

### 诚实边界

- **`text.format` 不支持**：Kiro 上游 `userInputMessageContext` 只有 `toolResults` 与 `tools`
  两个字段（`src/kiro/model/requests/conversation.rs:146-153`），没有任何结构化输出概念，无处透传。
  本 change 只补 warn，不做 prompt 层模拟——模拟会把 `strict: true` 变成「尽力」，属假装支持

## Capabilities

### Modified Capabilities

- `openai-responses`：新增 responses-lite 工具承载形状的归一、`custom` / `namespace` 降级与响应侧还原、
  历史 item 与 `tool_choice` 改写、工具丢弃可观测性、`text.format` 的不支持声明

## Impact

- **代码**：新增 `src/openai/responses_tools.rs`（归一层）；
  `src/openai/responses.rs`（`additional_tools` 提取、历史 item 改写、`tool_choice` 改写）；
  `src/openai/responses_types.rs`（`ResponseOutputItem` 增 `input` / `namespace` 字段与构造）；
  `src/openai/responses_stream.rs`（freeform 缓冲状态机、item 类型改写、namespace 还原）；
  `src/openai/handlers.rs`（新增 `ResponsesContext` 承载还原映射；非流式分派；上游工具清单与分派结果的 info 日志）；
  `src/openai/converter.rs`（丢弃工具补 warn，唯一跨端点改动，无行为变化）；
  `src/openai/types.rs`（`tool_type` 去掉 `#[allow(dead_code)]`，已有生产使用）
- **不改**：`src/anthropic/converter.rs` 的 `normalize_json_schema`（与官方 `W19` 的差异缺本地故障证据，
  且它在 `/v1/messages` 共享路径上）；Chat Completions 端点的转换语义
  （`custom` / `namespace` / `additional_tools` 都是 Responses 协议概念，Chat 客户端不会发）
- **风险**：`exec` 的 lark 文法降级为自然语言描述后，模型可能生成不合文法的输入；
  Codex 侧对格式错误有容错与重试，风险可接受但须在端到端验证中观察
- **未纳入**：`tool_search` proxy（4 份抓包零出现，无真实驱动）
