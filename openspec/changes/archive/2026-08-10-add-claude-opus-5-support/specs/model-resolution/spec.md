## ADDED Requirements

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
