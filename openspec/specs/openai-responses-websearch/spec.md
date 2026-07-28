# Capability: openai-responses-websearch

## Purpose

Emulate server-side web search for the Responses endpoint: when a request declares exactly one web-search-shaped tool, the proxy performs the search via Kiro MCP and returns a `web_search_call` output item alongside the answer. Detection is deliberately wide (type prefix, `google_search`, known names). Behavior is governed by a runtime switch owned by `admin-runtime-settings`; the Anthropic endpoints are unaffected.

## Requirements

### Requirement: Responses 端点的 server-side web_search

The Responses endpoint MUST support a server-side web search tool: when the request declares exactly one web-search tool, the service MUST perform the search itself and return the results as response output, rather than asking the model to emit a tool call. This capability MUST be available only on the Responses endpoint, because only that protocol has a faithful representation for a proxy-executed search.

#### Scenario: 单个 web_search 工具触发代执行

- **WHEN** 请求恰好声明一个 web-search 工具且该能力已启用
- **THEN** 服务 MUST 自行执行搜索并把结果作为响应输出返回，MUST NOT 把该工具转发给上游模型

#### Scenario: 混合工具不触发代执行

- **WHEN** 请求同时声明 web-search 工具与其它工具
- **THEN** MUST NOT 触发代执行，所有工具 MUST 走正常工具路径

#### Scenario: Chat Completions 端点不提供该能力

- **WHEN** 相同的 web-search 工具声明发往 Chat Completions 端点
- **THEN** MUST NOT 触发代执行

### Requirement: web_search 工具判定口径

Tool detection MUST accept the shapes real clients send: a tool whose type begins with the web-search prefix, a tool whose type denotes a search provider, or a tool whose name matches one of the documented web-search tool names (including the dated official name). Detection MUST be case-insensitive on the type.

#### Scenario: 按 type 前缀命中

- **WHEN** 工具的 type 以 web-search 前缀开头（含带日期或带 preview 后缀的变体）
- **THEN** 该工具 MUST 被识别为 web-search 工具

#### Scenario: 按 name 命中带日期的官方名

- **WHEN** 工具的 name 为带日期的官方 web_search 名称
- **THEN** 该工具 MUST 被识别为 web-search 工具

#### Scenario: 普通 function 工具不被误判

- **WHEN** 工具是名称与 web 搜索无关的普通 function 工具
- **THEN** MUST NOT 被识别为 web-search 工具

### Requirement: Anthropic 端点判定口径不变

This change MUST NOT alter the web-search tool detection used by the Anthropic endpoints. Broadening that detection would turn requests currently forwarded upstream into proxy-executed searches, which is a behavior reversal outside this change's scope.

#### Scenario: Anthropic 侧行为不变

- **WHEN** 本能力实现完成
- **THEN** Anthropic 端点对 web-search 工具的判定结果 MUST 与实现前完全一致

#### Scenario: 两端点差异被记录

- **WHEN** 同一个带日期官方名的 web-search 工具分别发往 Anthropic 端点与 Responses 端点
- **THEN** 二者行为不同这一事实 MUST 被记录在对外端点目录的接入提示中，以便用户预期一致

### Requirement: web_search 运行时开关

Because detection is intentionally broad, the capability MUST be switchable at runtime without restarting the service. It MUST default to enabled for compatibility with existing deployments. When disabled, a web-search tool declaration MUST be treated as an ordinary tool rather than rejected.

#### Scenario: 默认启用

- **WHEN** 配置未显式设置该开关
- **THEN** 该能力 MUST 为启用状态

#### Scenario: 关闭后不拦截

- **WHEN** 该开关被关闭且请求声明了 web-search 工具
- **THEN** MUST NOT 触发代执行，该工具 MUST 走正常工具路径，且请求 MUST NOT 因此失败

#### Scenario: 开关变更立即影响本端点

- **WHEN** 该开关在运行期被改变
- **THEN** 本端点后续请求 MUST 立即按新状态处理，MUST NOT 需要重启进程

（该开关的读写接口、鉴权与持久化契约属 `admin-runtime-settings` 能力，
见本 change 的 `specs/admin-runtime-settings/spec.md`。）

### Requirement: 搜索查询提取

The search query MUST be taken from the most recent user text in the normalized input. The Anthropic-side extraction, which strips a Claude Code specific prompt prefix, MUST NOT be reused here because OpenAI clients do not send that prefix.

#### Scenario: 取最后一条 user 文本

- **WHEN** 归一后的消息含多条 user 消息
- **THEN** 查询 MUST 取自最后一条 user 消息的文本

#### Scenario: 无可用查询

- **WHEN** 归一后不存在可作为查询的 user 文本
- **THEN** 响应 MUST 为 400 并说明无法提取搜索查询，MUST NOT 用空查询调用搜索后端

#### Scenario: 不剥离 Anthropic 专用前缀

- **WHEN** 用户文本恰好以 Claude Code 的搜索提示前缀开头
- **THEN** 该文本 MUST 被原样用作查询（本端点不做该前缀处理）

### Requirement: 搜索结果的 Responses 输出映射

Search results MUST be reported using Responses output items: a web-search-call item describing the executed search, followed by an assistant message item carrying a readable summary of the results. Both streaming and non-streaming forms MUST be supported.

#### Scenario: 非流式输出结构

- **WHEN** 非流式请求触发了代执行搜索
- **THEN** `output` MUST 依次含一个 web-search-call item（状态为已完成，携带查询）与一个 message item（含 `output_text` 摘要）

#### Scenario: 流式事件序列

- **WHEN** 流式请求触发了代执行搜索
- **THEN** MUST 依次发出 web-search-call item 的添加与完成事件、message item 的添加事件、content part 添加事件、文本增量事件、content part 完成与 item 完成事件、完成事件，并以 `[DONE]` 结束

#### Scenario: 搜索后端失败不返回空成功

- **WHEN** 搜索后端调用失败或返回无结果
- **THEN** 响应 MUST 明确表达该情况（错误响应，或摘要文本明确说明未找到结果），MUST NOT 返回一个看起来正常但内容为空的成功响应

#### Scenario: usage 不伪造上游信号

- **WHEN** 请求走了代执行搜索路径（未调用上游模型）
- **THEN** usage 的输入 tokens MUST 使用本地估算值，MUST NOT 声称来自上游用量信号
