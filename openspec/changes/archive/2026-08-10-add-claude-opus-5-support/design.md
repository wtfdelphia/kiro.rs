## Context

Claude 模型归一集中在 `src/anthropic/converter.rs` 的 `normalize_claude_model`。该函数先剥离
`THINKING_SUFFIX`，再按 sonnet / opus / haiku 分支用 `contains` 匹配版本号。`map_model` 现在只是
它的薄包装（converter.rs:222），供旧调用方使用。

`get_context_window_size` 复用 `map_model` 的结果，用归一后的 id 做 1M 白名单判定。
`override_thinking_from_model_name`（handlers.rs:840）用原始 model 字符串判断是否 adaptive thinking。
`/v1/models` 优先走 `models_from_catalog`（由上游 catalog 驱动），仅在 catalog 为空时回落到
`static_fallback_models`（handlers.rs:222）。

实测当前行为：

| 输入 | 当前归一结果 |
| --- | --- |
| `claude-opus-5` | `None`（被判 unmapped 并拒绝） |
| `claude-opus-5-thinking` | `None` |
| `claude-opus-4-5-20251101` | `claude-opus-4.5` |
| `claude-opus-4-8` | `claude-opus-4.8` |

### 为何不 cherry-pick 上游实现

上游 `hank9999/kiro.rs@5ca5703`（父提交 `606c6bc`，该父提交在本仓库历史中）实现了同一能力。
本地试算 `git cherry-pick --no-commit 5ca5703` 的结果：`README.md` 与 `handlers.rs` 自动合并，
`converter.rs` **冲突**。三处结构性分叉：

1. **归一函数已重构。** 上游修改 `map_model` 内部的 if-else 链；本项目该逻辑已移入
   `normalize_claude_model`，`map_model` 仅为薄包装。冲突正落在此处。
2. **thinking 后缀处理方式不同。** 本项目在函数开头 `strip_suffix(THINKING_SUFFIX)` 后用 `base`
   匹配；上游全程用 `model_lower` 直接 `contains`。因此上游 commit message 所述「把 opus-5 判断
   置于 4-5 之前以避免 opus-4-5 误匹配」在本项目中并非必要前提：本项目 opus 分支顺序为
   4-8 → 4-7 → 4-6 → 4-5，`opus-4-5` 会在 4-5 分支命中，不会走到 opus-5 判定。
3. **`/v1/models` 不再是静态列表。** 上游往纯静态 `vec![]` 硬加两个 `Model{}`；本项目该端点优先
   由动态 catalog 驱动，静态表仅为 fallback，且静态表 id 使用连字符风格（`claude-opus-4-8`）。

因此采用「按语义重新实现」，而非搬运 diff。

## Goals / Non-Goals

**Goals:**

- 让 `claude-opus-5` 走通解析、上下文窗口、thinking 策略与公开模型列表四个面。
- 保持与既有 Sonnet 5 / Opus 4.8 支持一致的处理方式，不引入新的模型处理范式。
- 固定「更具体的版本判定先于更宽松的版本判定」这一顺序约束，避免后续版本被提前截获。

**Non-Goals:**

- 不改动 `model-catalog` 的缓存、刷新或预热机制。
- 不引入模型能力的配置化声明（上下文窗口、thinking 策略仍为代码内常量判定）。
- 不重构 `normalize_claude_model` 的 `contains` 匹配方式为正则或结构化版本解析。
- 不追加 opus-5 的真实凭据探活测试。

## Decisions

### D1. opus-5 判定置于 opus 分支最前

尽管本项目已剥离 thinking 后缀、`opus-4-5` 不会误匹配 opus-5，仍将 `opus-5` 判定放在所有
`4-x` 判定之前。

理由：`contains("opus-5")` 与 `contains("4-5")` 对形如 `claude-opus-5-4-5` 之类的畸形输入存在
顺序敏感性；更重要的是，未来若出现 `opus-5-1`，把具体版本前置可避免它被宽松分支截获。这与
sonnet 分支已有的 `sonnet-5` 前置写法一致（converter.rs:188）。

### D2. 上下文窗口与 thinking 策略沿用 Sonnet 5 的处理

`claude-opus-5` 纳入 1M 白名单，`opus-5` 纳入 adaptive thinking 集合。两者均与 Sonnet 5 相同。

这是基于上游实现与既有惯例的假设，本地无凭据探活。若 Kiro 侧 opus-5 实际为 200K 上下文或
非 adaptive thinking，需单独修正——该风险记入 proposal 的 Assumptions。

### D3. 只补静态 fallback，不动 catalog 路径

`/v1/models` 的动态路径由上游返回的 catalog 驱动，Kiro 上线 opus-5 后会自然出现，无需代码改动。
静态 fallback 是 catalog 为空时的兜底，需要显式补两个条目（基座与 thinking 变体），与既有
Sonnet 5 条目对齐。

`created` 沿用 Sonnet 5 的 `1782777600`：该字段仅用于列表展示，不参与解析或路由，且 opus-5 的
真实发布日期无法核实。

## Risks / Trade-offs

- [Kiro 侧 opus-5 实际不可用] → 维护者已确认可用；若有误，回滚本 change 的四处改动即可，无数据
  或配置迁移。
- [上下文窗口或 thinking 策略假设有误] → 仅影响 opus-5 单个模型的请求参数，不影响其他模型；
  修正成本为改动白名单一行。
- [`contains` 匹配对畸形 id 的顺序敏感性] → 由 D1 的前置顺序与回归单测约束；彻底解决需结构化
  版本解析，属独立重构，不在本 change 范围。

## Migration Plan

1. 修改 `normalize_claude_model` 的 opus 分支，新增单测覆盖 opus-5 与 opus-4-5 不误匹配。
2. 扩展 1M 上下文白名单与 adaptive thinking 集合。
3. 补静态 fallback 的两个条目，扩展既有 `static_fallback_models_has_core_ids` 断言。
4. 同步 README 模型映射表。
5. 运行 `cargo check --release --all-targets`（零新增告警）与相关模块测试。

回滚：四处改动彼此独立，可分别回退；无持久化状态、无配置 schema 变化。

## Open Questions

无。opus-5 的可用性已由维护者确认；上下文窗口与 thinking 策略的假设已记入 Assumptions 与 Risks。
