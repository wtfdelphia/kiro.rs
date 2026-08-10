# Capability: model-resolution

## Purpose

Resolve client- or admin-supplied model strings through a single pipeline (thinking-suffix strip, compatibility aliases, Claude version normalization, catalog/policy passthrough) before chat conversion, credential test, public model listing, and model-aware routing, exposing a resolved upstream id and resolve kind (or diagnosable rejection reason).

## Requirements

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

### Requirement: Claude 版本归一必须先判具体版本再判宽松版本

Claude 版本归一 MUST 按「更具体的版本标识先于更宽松的版本标识」的顺序判定。当一个版本标识是
另一个的子串或前缀时，更具体者 MUST 先被判定，使宽松分支不会提前截获具体版本。

归一 MUST 覆盖 `claude-opus-5`：输入含 `opus-5` 的模型标识 MUST 归一为上游 id `claude-opus-5`，
且该判定 MUST 位于所有 `opus-4-x` 判定之前。带 thinking 后缀的输入 MUST 归一到同一基座 id。

#### Scenario: opus-5 归一为上游 id

- **WHEN** 输入模型为 `claude-opus-5`
- **THEN** 归一结果为 `claude-opus-5`

#### Scenario: opus-5 的 thinking 变体归一到同一基座

- **WHEN** 输入模型为 `claude-opus-5-thinking`
- **THEN** 归一结果为 `claude-opus-5`

#### Scenario: 带日期后缀的 opus-4-5 不被 opus-5 截获

- **WHEN** 输入模型为 `claude-opus-4-5-20251101`
- **THEN** 归一结果为 `claude-opus-4.5`，MUST NOT 为 `claude-opus-5`

### Requirement: Opus 5 必须具备与 Sonnet 5 一致的上下文与 thinking 处理

`claude-opus-5` MUST 被纳入 1M 上下文模型集合。含 `opus-5` 的模型请求 MUST 使用 adaptive
thinking 类型，并 MUST 附带与其他 adaptive thinking 模型一致的 high effort output config。

#### Scenario: opus-5 使用 1M 上下文

- **WHEN** 查询 `claude-opus-5` 的上下文窗口大小
- **THEN** 返回 1000000

#### Scenario: opus-5 使用 adaptive thinking

- **WHEN** 客户端请求含 `opus-5` 的模型且启用 thinking
- **THEN** thinking 类型为 adaptive
- **AND** 请求 MUST 附带 high effort 的 output config

### Requirement: 静态模型 fallback 必须包含 Opus 5

当全局模型 catalog 为空而 `GET /v1/models` 回落到静态列表时，该静态列表 MUST 包含
`claude-opus-5` 与其 thinking 变体。动态 catalog 路径 MUST NOT 因本要求而改变行为：它由上游
返回的 catalog 驱动。

#### Scenario: 空 catalog 时静态列表含 opus-5

- **WHEN** 全局模型 catalog 为空且请求 `GET /v1/models`
- **THEN** 返回的静态 fallback 列表包含 `claude-opus-5` 与 `claude-opus-5-thinking`

#### Scenario: 非空 catalog 时不受静态列表影响

- **WHEN** 全局模型 catalog 非空且请求 `GET /v1/models`
- **THEN** 返回结果由 catalog 驱动，MUST NOT 因静态 fallback 含 opus-5 而额外注入该条目
