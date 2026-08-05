## MODIFIED Requirements

### Requirement: input item 类型分派

Array items MUST be dispatched by their `type` (falling back to `role` when `type` is absent). Message items MUST become messages of the stated role. Function-call items MUST become assistant tool calls. Function-call-output items MUST become tool messages carrying the originating call id. Bare text and image items MUST be accumulated into a user message.

Client-dialect tool items MUST be rewritten rather than dropped: `custom_tool_call` MUST become an assistant tool call whose arguments wrap the raw input, and `custom_tool_call_output` MUST become a tool message. A `function_call` item carrying a `namespace` field MUST have its name rewritten to the flattened form so that the name matches what was sent in the request's tool list.

#### Scenario: message item

- **WHEN** item 为 `{"type":"message","role":"user","content":"hi"}`
- **THEN** 归一出一条 user 消息

#### Scenario: function_call_output 配对

- **WHEN** item 为 `{"type":"function_call_output","call_id":"c1","output":"result"}`
- **THEN** 归一出一条 tool 消息，其 call id MUST 为 `c1`，内容 MUST 为 `result`

#### Scenario: call_id 与 tool_call_id 兼容

- **WHEN** function-call-output item 只提供 `tool_call_id` 而非 `call_id`
- **THEN** 该值 MUST 被用作配对 id

#### Scenario: 连续 function_call 合并进同一条 assistant 消息

- **WHEN** input 数组含两个相邻的 `function_call` item
- **THEN** 它们 MUST 归入同一条 assistant 消息的工具调用列表，MUST NOT 拆成两条 assistant 消息（并行工具调用必须留在同一轮，否则上游要求的工具调用与结果配对会被破坏）

#### Scenario: 裸文本与图片 item 归集为 user 消息

- **WHEN** input 数组含不带 role 的 `input_text` 与 `input_image` item
- **THEN** 它们 MUST 被归集进同一条 user 消息，MUST NOT 被丢弃

#### Scenario: 归集在遇到带 role 的 item 时结束

- **WHEN** 裸 `input_text` item 之后紧随一个带 role 的 message item
- **THEN** 先前累积的内容 MUST 先成为一条独立 user 消息，顺序 MUST 保持

#### Scenario: output_text item 归为 assistant

- **WHEN** item 为 `{"type":"output_text","text":"prev answer"}`
- **THEN** 归一出一条 assistant 消息

#### Scenario: custom_tool_call 归一为工具调用

- **WHEN** item 为 `{"type":"custom_tool_call","call_id":"c1","name":"exec","input":"const x = 1;"}`
- **THEN** 归一出一条 assistant 工具调用，其 arguments MUST 为 `{"input":"const x = 1;"}`，call id MUST 为 `c1`
- **AND** MUST NOT 被丢弃（丢弃会使上一轮的调用与结果双双消失，模型将重复执行同一操作）

#### Scenario: custom_tool_call_output 归一为工具结果

- **WHEN** item 为 `{"type":"custom_tool_call_output","call_id":"c1","output":"done"}`
- **THEN** 归一出一条 tool 消息，其 call id MUST 为 `c1`

#### Scenario: custom_tool_call_output 的非字符串 output

- **WHEN** `output` 为对象或数组
- **THEN** 它 MUST 被 JSON 字符串化；为 null 时 MUST 为空字符串

#### Scenario: 带 namespace 的 function_call 拼成展平名

- **WHEN** item 为 `{"type":"function_call","namespace":"collaboration","name":"spawn_agent",...}`
- **THEN** 归一出的工具调用名 MUST 为 `collaboration__spawn_agent`
- **AND** 该改写 MUST NOT 取决于该 namespace 是否出现在本轮的工具列表中（历史与当前工具集未必一致，但同一工具在两处必须同名）

## ADDED Requirements

### Requirement: responses-lite 的 additional_tools 工具承载

Some clients send tool definitions inside the input stream rather than in the top-level `tools` field: the first input item is `{"type":"additional_tools","role":"developer","tools":[...]}` and the request carries no `tools` field at all. The system MUST extract those tool definitions and treat them as the request's tool list. The `additional_tools` item itself MUST NOT produce any conversation message, and MUST NOT be silently discarded.

#### Scenario: 工具从 additional_tools 提取

- **WHEN** 请求无顶层 `tools` 字段，且 `input[0]` 为 `additional_tools` item 内含 4 个工具定义
- **THEN** 到达上游的工具列表 MUST 包含这 4 个工具，且各自的 schema MUST NOT 为空对象

#### Scenario: additional_tools 不产生对话消息

- **WHEN** `input[0]` 为 `additional_tools` item
- **THEN** 归一结果 MUST NOT 因该 item 产生任何 user 或 assistant 消息

#### Scenario: 与顶层 tools 共存时合并

- **WHEN** 请求同时有顶层 `tools` 与 `additional_tools` item
- **THEN** 两者 MUST 合并，顶层 `tools` 在前

#### Scenario: additional_tools 的工具定义保真

- **WHEN** `additional_tools` 内含 `custom` 工具（带 `format`）或 `namespace` 工具（带内层 `tools[]`）
- **THEN** 这些形状特有的字段 MUST 完整到达降级逻辑，MUST NOT 在中间层被丢弃
- **AND** 该保真要求只覆盖 `additional_tools` 路径：顶层 `tools` 字段沿用既有的解析结构，
  其 `custom` / `namespace` 特有字段不保证保真（该形状未在真实客户端上观察到）

### Requirement: 非 function 形状的工具降级

The upstream tool model accepts only plain function tools with a JSON Schema. Tool shapes that the upstream does not understand MUST be downgraded rather than passed through or dropped.

A `custom` tool MUST become a function tool whose schema is a single required string property `input`. Its original description MUST be preserved, prefixed with a note stating that the tool is invoked as a JSON function call with the raw input placed in the `input` field, so that the note overrides any instruction in the original description telling the model not to use JSON. Any grammar definition carried by the tool MUST be appended to the description so the syntax constraint remains visible to the model.

#### Scenario: custom 工具降级后 schema 非空

- **WHEN** 请求含 `{"type":"custom","name":"exec","format":{"type":"grammar","syntax":"lark","definition":"..."}}`
- **THEN** 到达上游的工具 schema MUST 为含单个必填字符串属性 `input` 的对象
- **AND** MUST NOT 为空对象

#### Scenario: 调用约定说明置于描述最前

- **WHEN** custom 工具的原始 description 含「不要使用 JSON」一类的输入格式说明
- **THEN** 降级后的 description MUST 以调用约定说明开头，明确 `input` 字段承载原始输入
- **AND** 原始说明 MUST 被保留

#### Scenario: 语法定义不丢失

- **WHEN** custom 工具带 `format.definition`
- **THEN** 该定义 MUST 出现在降级后的 description 中

### Requirement: namespace 工具展平与命名冲突

A `namespace` tool carries its real tools nested in an inner list. The system MUST flatten each inner function into an independent top-level tool named `<namespace>__<name>`, MUST NOT collapse the group into a single empty tool, and MUST retain a reverse mapping so responses can be restored.

When flattening produces a name collision the system MUST return 400 with both colliding names in the message, and MUST NOT disambiguate automatically. Automatic renaming would give the same tool different names across turns, which the client cannot predict.

#### Scenario: 内层工具独立展平

- **WHEN** 请求含 `{"type":"namespace","name":"collaboration","tools":[...6 个 function...]}`
- **THEN** 到达上游的工具列表 MUST 含 6 个独立工具，名字分别为 `collaboration__<原名>`
- **AND** MUST NOT 出现一个名为 `collaboration` 的空壳工具

#### Scenario: 展平名与顶层工具冲突

- **WHEN** 展平结果与某个顶层工具同名
- **THEN** 响应 MUST 为 400，错误信息 MUST 含冲突双方的名字

#### Scenario: 两个 namespace 展平到同名

- **WHEN** 两个 namespace 的内层工具展平后得到同一个名字
- **THEN** 响应 MUST 为 400，错误信息 MUST 含冲突双方的名字

#### Scenario: 展平名超长时的截断保持确定性

- **WHEN** 展平名超过上游工具名长度上限
- **THEN** 缩短结果 MUST 与既有超长工具名机制一致，同一输入多次调用 MUST 得到同一名字

### Requirement: 客户端方言工具的响应侧还原

A downgraded tool MUST be reported back in the shape the client registered it under, otherwise the client rejects its own tool call and the model retries indefinitely.

Calls to a tool that was downgraded from `custom` MUST be emitted as a custom-tool-call item carrying an `input` field rather than a function-call item carrying `arguments`. Calls to a flattened namespace tool MUST be emitted with the original tool name plus the originating `namespace` field, because the client matches tools by the `(namespace, name)` pair.

Long tool names are shortened before reaching the upstream and restored on the way back by the existing name mapping. The restoration lookup MUST therefore be keyed on the name as it exists after that restoration, not on the shortened form. Both levels of mapping MUST be applied: getting either one wrong breaks the tool-call chain silently, with the only symptom being the client rejecting its own tool call.

#### Scenario: freeform 工具调用回 custom_tool_call

- **WHEN** 上游返回对某个由 `custom` 降级而来的工具的调用
- **THEN** 输出 item 的类型 MUST 为 custom-tool-call，MUST 带 `input` 字段
- **AND** MUST NOT 为 function-call item

#### Scenario: 超长 freeform 工具名往返

- **WHEN** 某个 `custom` 工具的名字超过长度上限而被缩短
- **THEN** 上游以缩短名回传该调用时，输出 item 仍 MUST 为 custom-tool-call

#### Scenario: 展平名还原为 namespace 与原名

- **WHEN** 上游返回名为 `collaboration__spawn_agent` 的调用
- **THEN** 输出 item 的 name MUST 为 `spawn_agent`，且 MUST 带 `namespace` 字段值为 `collaboration`

#### Scenario: 两级映射叠加

- **WHEN** 展平名超长而被缩短，上游以缩短名回传
- **THEN** 还原 MUST 依次经过短名映射与展平逆映射，最终得到原 namespace 与原名

#### Scenario: 原始输入的提取

- **WHEN** 上游返回的 arguments 是合法 JSON 且含 `input` 键
- **THEN** `input` 的字符串值 MUST 被用作 custom-tool-call 的 input

#### Scenario: 模型直接返回裸输入

- **WHEN** 上游返回的 arguments 不是合法 JSON
- **THEN** 该字符串整体 MUST 被用作 custom-tool-call 的 input（降级后的工具描述要求原始文本，模型可能照做）

#### Scenario: 空输入

- **WHEN** 上游返回的 arguments 为空白字符串或空对象 `{}`
- **THEN** custom-tool-call 的 input MUST 为空字符串

#### Scenario: 含换行与引号的输入无损

- **WHEN** 原始输入含换行与双引号
- **THEN** 还原后的 input MUST 与原始输入逐字符相同

### Requirement: 流式还原时参数缓冲到完成才发出

The incremental payload of a custom-tool-call input event is the already-extracted raw input, which can only be extracted once the upstream argument JSON is complete. The system MUST therefore buffer upstream argument increments for downgraded tools instead of forwarding them renamed, and emit the input events only after the arguments are complete. One upstream event may therefore map to zero or two downstream events.

#### Scenario: 上游参数增量不透传

- **WHEN** 上游对某个 freeform 工具发出参数增量事件
- **THEN** 下游 MUST NOT 出现对应的转发事件

#### Scenario: 输入事件在参数完成后发出

- **WHEN** 上游的参数增量结束
- **THEN** 下游 MUST 发出 custom-tool-call 的输入增量事件（内容非空时）与完成事件
- **AND** 该输入增量事件 MUST 只出现一次

#### Scenario: item 事件的类型改写

- **WHEN** 上游对某个 freeform 工具发出 output item 的 added 与 done 事件
- **THEN** 两者的 item 类型 MUST 为 custom-tool-call，added 时 `input` MUST 为空字符串，done 时 MUST 为完整提取结果

### Requirement: 工具丢弃与不支持能力的可观测性

Silently dropping client capabilities leaves no way to diagnose why a model reports missing tools. Whenever a tool definition is dropped, the system MUST log a warning identifying the tool's name and type. Whenever the request asks for a capability the upstream cannot express, the system MUST log a warning rather than accept it silently.

Structured output (`text.format`) is such a capability: the upstream message context carries only tool definitions and tool results, with no response-format concept. The system MUST NOT simulate it through prompt injection, since that would degrade a strict schema guarantee into a best-effort hint.

#### Scenario: 丢弃工具时留痕

- **WHEN** 某个工具定义因缺少名字而被丢弃
- **THEN** MUST 输出一条含该工具 name 与 type 的 warning
- **AND** 其余工具 MUST 不受影响

#### Scenario: text.format 不被静默接受

- **WHEN** 请求携带 `text.format`
- **THEN** MUST 输出一条声明该能力不受支持的 warning
- **AND** 请求 MUST 仍被正常处理（不因该字段报错）

#### Scenario: 不做 prompt 层模拟

- **WHEN** 请求携带 `text.format` 且 `strict` 为 true
- **THEN** 系统 MUST NOT 将该 schema 注入系统指令以模拟结构化输出

### Requirement: 多工具客户端无法使用 web_search 代执行

The web_search emulation path requires the request to carry exactly one tool. Clients that send several tools in one request therefore never reach it. This is a determinate outcome for such clients, not an edge case, and MUST be stated rather than left implicit.

#### Scenario: 多工具请求不走代执行

- **WHEN** 请求含多个工具，其中一个是 web_search
- **THEN** 代执行 MUST NOT 被触发
- **AND** 该工具被丢弃时 MUST 留下 warning
