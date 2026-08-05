## ADDED Requirements

### Requirement: 统一模型解析管线

The system MUST resolve client- or admin-supplied model strings through a single resolution pipeline before chat conversion, credential test, public model listing decisions, and model-aware credential routing. The pipeline MUST strip a configured thinking suffix when present, apply compatibility aliases, normalize Claude version spellings, and then apply catalog/policy decisions.

#### Scenario: thinking 后缀不改变基座映射

- **WHEN** 输入模型为可识别 Claude id 且带 thinking 后缀（如 claude-sonnet-4.6-thinking）
- **THEN** 解析结果的上游 modelId 为去掉后缀后的可识别 Claude id，并标记 thinking 已请求（若调用方需要）

#### Scenario: 未知且不可透传则拒绝

- **WHEN** 输入既不匹配别名/归一规则，也不满足 catalog 透传条件
- **THEN** 解析失败并返回可诊断的 unmapped/not-available 原因，且 MUST NOT 发起上游 generate

### Requirement: 兼容别名与 auto

The system MUST map documented compatibility aliases (including common OpenAI-style ids such as gpt-4o and gpt-4) to explicit Claude upstream model ids. The model id `auto` MUST resolve to a configured default chat model (default MAY be claude-sonnet-4.6) rather than failing as unmapped.

#### Scenario: gpt-4o 别名

- **WHEN** 输入 model 为 gpt-4o（大小写不敏感）
- **THEN** resolve 为明确的 Claude 上游 id（与项目别名表一致，如 claude-sonnet-4.5）

#### Scenario: auto 映射默认模型

- **WHEN** 输入 model 为 auto
- **THEN** resolve 为 defaultChatModel 配置值（缺省为文档化的默认 Claude id），不得返回 unmapped

### Requirement: Catalog 透传策略

When catalog passthrough is enabled, an input model id that exactly matches (case-insensitive) an id present in the global or per-credential model catalog MUST be accepted as a passthrough upstream model id without rewriting it to a Claude alias. When passthrough is disabled or the id is not present in catalog, non-Claude unknown ids MUST be rejected.

#### Scenario: catalog 命中透传

- **WHEN** allowCatalogPassthrough 为 true，且 catalog 含 gpt-5.6-sol，输入为 gpt-5.6-sol
- **THEN** resolve 为 passthrough 上游 id gpt-5.6-sol

#### Scenario: catalog 未命中拒绝

- **WHEN** 输入为不在别名表且不在 catalog 中的陌生 id
- **THEN** 解析失败，不得静默映射到 Claude

### Requirement: 解析结果可供调用方使用

Resolution MUST expose at least the resolved upstream model id and a resolve kind (mapped alias/normalize vs passthrough vs reject reason) so Admin test responses and diagnostics can report what will be sent upstream.

#### Scenario: 调用方读取 kind

- **WHEN** 成功解析别名或透传模型
- **THEN** 调用方可获得 resolvedModelId 与 resolveKind（或等价字段）
