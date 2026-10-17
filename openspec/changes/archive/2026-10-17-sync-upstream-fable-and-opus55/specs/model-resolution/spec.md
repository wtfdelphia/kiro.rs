## ADDED Requirements

### Requirement: Opus 5.5 归一必须先于 opus-5 判定

归一 MUST 覆盖 `claude-opus-5.5`：输入含 `opus-5.5` 或 `opus-5-5` 的模型标识 MUST 归一为
上游 id `claude-opus-5.5`，且该判定 MUST 位于 `opus-5` 判定之前（`opus-5` 是 `opus-5.5`
的子串，顺序颠倒会把 5.5 请求吞进 5 分支）。带 thinking 后缀与日期后缀的输入 MUST 归一到
同一基座 id。

#### Scenario: opus-5.5 的连字符与点号形式归一

- **WHEN** 输入模型为 `claude-opus-5-5` 或 `claude-opus-5.5`
- **THEN** 归一结果为 `claude-opus-5.5`

#### Scenario: opus-5.5 的 thinking 与日期变体归一到同一基座

- **WHEN** 输入模型为 `claude-opus-5-5-thinking` 或 `claude-opus-5.5-20260922`
- **THEN** 归一结果为 `claude-opus-5.5`

#### Scenario: opus-5 不被 5.5 分支截获

- **WHEN** 输入模型为 `claude-opus-5` 或 `claude-opus-5-thinking`
- **THEN** 归一结果为 `claude-opus-5`，MUST NOT 为 `claude-opus-5.5`

#### Scenario: opus-4-5 不被 5/5.5 分支截获

- **WHEN** 输入模型为 `claude-opus-4-5-20251101`
- **THEN** 归一结果为 `claude-opus-4.5`

### Requirement: Fable 归一无条件映射到 claude-fable-5.1

归一 MUST 覆盖 Fable：输入含 `fable` 的模型标识 MUST 归一为上游 id `claude-fable-5.1`。
Kiro 侧 id 为点号形式，连字符形式 `claude-fable-5-1` 与 `claude-fable-5` 实测被上游拒绝，
因此客户端拼写与上游 id 均归一到点号形式。目前 Kiro 仅上线 5.1 一个 Fable 版本，该分支
MUST 无条件映射（与 haiku 分支写法一致）。

#### Scenario: fable 连字符与点号形式归一

- **WHEN** 输入模型为 `claude-fable-5-1` 或 `claude-fable-5.1`
- **THEN** 归一结果为 `claude-fable-5.1`

#### Scenario: fable 的 thinking 变体归一到同一基座

- **WHEN** 输入模型为 `claude-fable-5-1-thinking`
- **THEN** 归一结果为 `claude-fable-5.1`，并标记 thinking 已请求

### Requirement: Opus 5.5 与 Fable 5.1 必须具备与 Sonnet 5 一致的上下文与 thinking 处理

`claude-opus-5.5` 与 `claude-fable-5.1` MUST 被纳入 1M 上下文模型集合。含 `fable` 的模型
请求 MUST 使用 adaptive thinking 类型，并 MUST 附带与其他 adaptive thinking 模型一致的
high effort output config。含 `opus-5.5` 的请求经由既有 `opus-5` 包含匹配已落入 adaptive
thinking，行为 MUST 保持。

#### Scenario: opus-5.5 与 fable 使用 1M 上下文

- **WHEN** 查询 `claude-opus-5.5` 或 `claude-fable-5.1` 的上下文窗口大小
- **THEN** 返回 1000000

#### Scenario: fable 使用 adaptive thinking

- **WHEN** 客户端请求含 `fable` 的模型且启用 thinking
- **THEN** thinking 类型为 adaptive
- **AND** 请求 MUST 附带 high effort 的 output config

### Requirement: 静态模型 fallback 必须包含 Opus 5.5 与 Fable 5.1

当全局模型 catalog 为空而 `GET /v1/models` 回落到静态列表时，该静态列表 MUST 包含
`claude-opus-5.5`、`claude-fable-5.1` 与各自的 thinking 变体，且条目 MUST 可被归一管线
解析回自身。动态 catalog 路径 MUST NOT 因本要求而改变行为：它由上游返回的 catalog 驱动。

#### Scenario: 空 catalog 时静态列表含新条目

- **WHEN** 全局模型 catalog 为空且请求 `GET /v1/models`
- **THEN** 返回的静态 fallback 列表包含 `claude-opus-5.5`、`claude-opus-5.5-thinking`、
  `claude-fable-5.1` 与 `claude-fable-5.1-thinking`

#### Scenario: 静态条目均可被归一管线解析

- **WHEN** 对静态 fallback 中任一新条目的 id 执行归一
- **THEN** 归一结果 MUST 等于该条目代表的基座上游 id（如 `claude-opus-5-5` 归一为
  `claude-opus-5.5`），MUST NOT 返回 unmapped
