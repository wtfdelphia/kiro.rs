## Why

Kiro 侧 `claude-opus-5` 已上线（由维护者确认可用），但本项目的 Claude 归一逻辑没有该分支：
实测 `normalize_claude_model("claude-opus-5")` 返回 `None`，因此客户端请求 opus-5 会被解析
管线判为 unmapped 并拒绝，不会发起上游 generate。同时 1M 上下文白名单、adaptive thinking
判定和 `/v1/models` 静态 fallback 均未覆盖该模型。

上游 `hank9999/kiro.rs@5ca5703` 已实现同一能力，但其代码结构与本项目已分叉，无法直接
cherry-pick（见 design.md 的差异分析）。本 change 按语义重新实现，与既有 Sonnet 5 / Opus 4.8
支持的处理方式保持一致。

## What Changes

- 在 `normalize_claude_model` 的 opus 分支新增 `opus-5` 判定，置于所有 `4-x` 判定之前，使
  `claude-opus-5` 与 `claude-opus-5-thinking` 归一为上游 id `claude-opus-5`。
- 将 `claude-opus-5` 纳入 `get_context_window_size` 的 1M 上下文白名单。
- 将 `opus-5` 纳入 `override_thinking_from_model_name` 的 adaptive thinking 集合，与 Sonnet 5
  行为一致。
- 在 `/v1/models` 的静态 fallback 列表补充 `claude-opus-5` 与 `claude-opus-5-thinking`；动态
  catalog 路径无需改动，因为它由上游返回的 catalog 驱动。
- README 模型映射表补充一行。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `model-resolution`: 归一规则新增 opus-5，并明确「更具体的版本判定先于更宽松的版本判定」这一
  既有隐含约束，使后续新增 opus 版本不会被 `4-x` 分支提前截获。

## Impact

- 代码：`src/anthropic/converter.rs`（归一分支、上下文窗口、单测）、`src/anthropic/handlers.rs`
  （adaptive thinking、静态 fallback、单测）。
- 文档：`README.md` 模型映射表。
- 规格：`model-resolution` delta；归档后同步到 `openspec/specs/model-resolution/`。
- 不影响：凭据、认证、Admin API、Docker/发布、配置 schema、admin-ui、`model-catalog` 的缓存与
  刷新机制。

## Assumptions

- Kiro 侧 `claude-opus-5` 这一上游 model id 已可用（维护者已确认）。若实际不可用，加入映射会
  让请求打到不存在的上游模型，需回滚本 change。
- opus-5 与 Sonnet 5 同为 1M 上下文、同走 adaptive thinking。该假设沿用上游实现与既有 Sonnet 5
  处理方式，未经本地真实凭据探活验证。
- 静态 fallback 的 `created` 时间戳沿用 Sonnet 5 的 `1782777600`（Jun 30, 2026）；该字段仅用于
  列表展示，不参与解析或路由决策。

## Success Criteria

- `claude-opus-5` 与 `claude-opus-5-thinking` 归一为 `claude-opus-5`。
- `claude-opus-4-5-20251101` 仍归一为 `claude-opus-4.5`，不被 opus-5 分支误截获。
- `get_context_window_size("claude-opus-5")` 返回 1_000_000。
- opus-5 请求的 thinking 类型为 adaptive，并附带 high effort output config。
- `/v1/models` 静态 fallback 含两个 opus-5 条目。
- `cargo check --release --all-targets` 零新增告警；`cargo test` 相关模块通过。
