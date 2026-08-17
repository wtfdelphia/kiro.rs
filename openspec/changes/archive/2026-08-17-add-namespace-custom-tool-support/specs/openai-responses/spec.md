## MODIFIED Requirements

### Requirement: namespace 工具展平与命名冲突

A `namespace` tool carries its real tools nested in an inner list. The system MUST flatten each inner `function` tool into an independent top-level tool named `<namespace>__<name>`, MUST NOT collapse the group into a single empty tool, and MUST retain a reverse mapping so responses can be restored.

An inner `custom` tool MUST be treated the same way as a top-level `custom` tool, but registered under the flattened name: it MUST become a function tool named `<namespace>__<name>` whose schema is the freeform degraded schema, whose description carries the invocation-convention note, the original description, and any grammar definition, and whose flattened name is recorded both as a freeform tool and in the namespace reverse mapping. Inner tools of any other non-function shape MUST still be dropped with a warning.

When flattening produces a name collision the system MUST return 400 with both colliding names in the message, and MUST NOT disambiguate automatically. Automatic renaming would give the same tool different names across turns, which the client cannot predict.

#### Scenario: 内层工具独立展平

- **WHEN** 请求含 `{"type":"namespace","name":"collaboration","tools":[...6 个 function...]}`
- **THEN** 到达上游的工具列表 MUST 含 6 个独立工具，名字分别为 `collaboration__<原名>`
- **AND** MUST NOT 出现一个名为 `collaboration` 的空壳工具

#### Scenario: 内层 custom 工具展平并降级

- **WHEN** 请求含 `{"type":"namespace","name":"functions","tools":[{"type":"custom","name":"apply_patch","description":"...","format":{"definition":"..."}}]}`
- **THEN** 到达上游的工具列表 MUST 含名为 `functions__apply_patch` 的工具
- **AND** 其 schema MUST 为含单个必填字符串属性 `input` 的对象
- **AND** 其 description MUST 以调用约定说明开头，保留原 description，并包含文法定义
- **AND** MUST NOT 输出「内层工具形状非 function」一类丢弃告警

#### Scenario: 内层 custom 工具缺少名字时留痕丢弃

- **WHEN** namespace 的某个内层 `custom` 工具缺少 name
- **THEN** MUST 输出一条含该 namespace 的 warning
- **AND** 该工具 MUST 被丢弃，其余内层工具 MUST 不受影响

#### Scenario: 展平名与顶层工具冲突

- **WHEN** 展平结果与某个顶层工具同名
- **THEN** 响应 MUST 为 400，错误信息 MUST 含冲突双方的名字

#### Scenario: 两个 namespace 展平到同名

- **WHEN** 两个 namespace 的内层工具展平后得到同一个名字
- **THEN** 响应 MUST 为 400，错误信息 MUST 含冲突双方的名字

#### Scenario: 内层 custom 展平名参与冲突检查

- **WHEN** 内层 `custom` 工具的展平名与顶层工具或其他展平名相同
- **THEN** 响应 MUST 为 400，冲突判定与内层 `function` 工具一致

#### Scenario: 展平名超长时的截断保持确定性

- **WHEN** 展平名超过上游工具名长度上限
- **THEN** 缩短结果 MUST 与既有超长工具名机制一致，同一输入多次调用 MUST 得到同一名字

### Requirement: 客户端方言工具的响应侧还原

A downgraded tool MUST be reported back in the shape the client registered it under, otherwise the client rejects its own tool call and the model retries indefinitely.

Calls to a tool that was downgraded from `custom` MUST be emitted as a custom-tool-call item carrying an `input` field rather than a function-call item carrying `arguments`. Calls to a flattened namespace tool MUST be emitted with the original tool name plus the originating `namespace` field, because the client matches tools by the `(namespace, name)` pair.

These two restorations MUST compose. A call to a tool that is both downgraded from an inner `custom` tool and flattened from a namespace MUST be emitted as a custom-tool-call item whose name is the original inner name and whose `namespace` field is the originating namespace.

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

#### Scenario: namespace 内 freeform 调用的两级还原

- **WHEN** 上游返回对某个由 namespace 内层 `custom` 工具降级而来的展平名的调用
- **THEN** 输出 item 的类型 MUST 为 custom-tool-call，MUST 带 `input` 字段
- **AND** name MUST 为内层原名，且 MUST 带对应的 `namespace` 字段
- **AND** MUST NOT 为 function-call item

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
