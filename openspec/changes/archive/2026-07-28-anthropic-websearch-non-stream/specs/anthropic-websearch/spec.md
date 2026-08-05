## ADDED Requirements

### Requirement: web_search 请求遵循客户端的流式选择

When an Anthropic endpoint request is routed to the server-side web search path, the response form MUST follow the request's `stream` field. A streaming request MUST receive a server-sent event stream. A non-streaming request MUST receive a single JSON message object with a JSON content type. The service MUST NOT return an event stream to a request that did not ask for streaming.

#### Scenario: 非流式请求返回 JSON

- **WHEN** 请求携带 web_search 工具且 `stream` 为 false（或缺省）
- **THEN** 响应的 content type MUST 表明为 JSON，响应体 MUST 为单个 message 对象，MUST NOT 为事件流

#### Scenario: 流式请求返回事件流

- **WHEN** 请求携带 web_search 工具且 `stream` 为 true
- **THEN** 响应 MUST 为服务端事件流，其事件序列 MUST 与本能力引入前一致

#### Scenario: 两个 Anthropic 端点行为一致

- **WHEN** 相同的非流式 web_search 请求分别发往标准端点与 Claude Code 兼容端点
- **THEN** 两者 MUST 返回结构相同的 JSON message 对象（该端点对的缓冲差异只作用于上游生成流，与本路径无关）

### Requirement: 两种模式产出相同的内容块

For a given query and search result set, the streaming and non-streaming paths MUST produce the same ordered content blocks: a text block stating the search being performed, a server-tool-use block naming the search tool and carrying the query, a search-result block carrying the results, and a text block carrying the readable summary. Neither path may omit or reorder blocks relative to the other.

#### Scenario: 非流式内容块顺序

- **WHEN** 非流式 web_search 请求成功
- **THEN** `content` MUST 依次为：text（搜索说明）、server_tool_use、web_search_tool_result、text（结果摘要）

#### Scenario: 块内容跨模式一致

- **WHEN** 同一查询与同一搜索结果分别经流式与非流式路径产出
- **THEN** 对应块的字段值 MUST 逐一相等

#### Scenario: server_tool_use 携带查询

- **WHEN** 任一模式下产出 server-tool-use 块
- **THEN** 其输入 MUST 含实际执行的查询字符串，其名称 MUST 为 web 搜索工具名

### Requirement: 非流式 usage 与停止原因

The non-streaming response MUST report token usage and MUST record that a server-side search was performed, consistent with what the streaming path reports in its final usage update. The stop reason MUST indicate normal completion.

#### Scenario: usage 含服务端搜索计数

- **WHEN** 非流式 web_search 请求成功
- **THEN** `usage` MUST 含输入与输出 token 数，且 MUST 记录本次服务端搜索的次数

#### Scenario: stop_reason 为正常结束

- **WHEN** 非流式 web_search 请求成功
- **THEN** `stop_reason` MUST 表示正常结束，MUST NOT 为 tool_use 或长度超限

#### Scenario: model 回显原值

- **WHEN** 任一模式下响应携带模型名
- **THEN** 该值 MUST 为客户端请求中的模型名

### Requirement: 无结果与失败的明确表达

When the search backend returns no results or the search call fails, the response MUST still be well-formed and MUST state that no results were found. A response that looks successful but conveys nothing is not acceptable.

#### Scenario: 无结果时摘要明确

- **WHEN** 搜索未返回任何结果
- **THEN** 摘要文本 MUST 明确说明未找到结果，且搜索结果块的结果列表 MUST 为空

#### Scenario: 搜索调用失败仍返回良构响应

- **WHEN** 搜索后端调用失败
- **THEN** 响应 MUST 仍为良构的 message 对象（流式为完整事件流），MUST NOT 为截断的流或空响应体

#### Scenario: 无法提取查询

- **WHEN** 无法从消息中提取搜索查询
- **THEN** 两种模式 MUST 均返回 400 且使用 Anthropic 错误信封，MUST NOT 用空查询调用搜索后端
