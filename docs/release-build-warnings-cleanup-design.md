> 修订记录：
> - 2026-07-31 第二轮实测复核。修正 D 类修复写法（原写法编译失败）、A 类调用点计数与传递依赖、C 类成因表述、第 3 步风险评估，并补充 CI 门禁缺口。
> - 2026-08-03 审查修订：将过期的 `AGENTS.md` 行号引用改为小节标题引用，避免本 change 自身插入「零新增编译告警」后行号漂移。
> - 2026-08-04 审查修复：端点目录计数由误记的 8 `Live` 更正为实测 7 `Live` + 1 `Planned`（共 8 条）。
> - 2026-08-04 bridge 复验：`get_context_window_size` 生产调用点由误记的 8 处更正为 rg 实测 5 处。

# Release 构建告警清零优化方案

分析日期：2026-07-31
分析基线：`dev` @ `e8c5eda`（工作树干净）
环境：Windows 11 / bash / Rust 1.94.1

## 项目目标：零新增编译告警

**任何代码实现都不得引入新的编译告警，与是否走 OpenSpec 流程无关。**

这条是长期项目目标，不限于本次清理：

- 适用范围包括走 OpenSpec change 的高风险变更，也包括 `AGENTS.md`「OpenSpec 条件」中可豁免的拼写/注释/单行修复
- 判定命令为 `cargo check --release --all-targets`，本项目以它为唯一准绳（理由见「成功标准」）
- 消除手段限于修正真实问题：移动导入、删除死代码、收敛 `#[cfg(test)]`。**不得用 `#![allow(dead_code)]` 或构造假数据充数**
- 确有正当保留理由时，用最小范围 `#[allow(...)]` 并在紧邻位置注明理由，不得放在 crate 或 module 级

已同步至 `AGENTS.md`、`spec/requirements.md`、`spec/design.md`、`openspec/project.md`、`.codex/skills/verification-before-completion/SKILL.md`。

## 本方案目标与约束

目标：让 `cargo check --release --all-targets` 对项目自身产生**零告警**，作为上述长期目标的起点基线。

硬约束：**不改变任何运行时行为**。本方案只做三类动作——移动导入、删除无调用点代码、把测试专用符号收敛到 `#[cfg(test)]`。不重构逻辑，不调整函数签名，不改变对外契约。

## 现状实测

本轮实际运行并确认：

```bash
cargo build --release                    # 退出码 0，12 warnings
cargo check --release --all-targets      # 14 项唯一告警（多出 chat_with_rewrite）
cargo check --release                    # 12 项（非测试目标）
cargo check --release --tests            # 8 项（测试目标）
```

告警全部来自项目源码，与第三方依赖、release 优化、LTO 均无关。

根因是项目只有 binary target（无 `src/lib.rs`，`Cargo.toml` 未声明 `[lib]`），因此 `pub` 不构成"对外 API"这一豁免理由——所有 `pub` 项都必须在 crate 内有实际调用点，否则即为死代码。同时测试辅助入口与旧兼容包装混在生产编译单元里，普通 release 构建不编译 `#[cfg(test)]` 调用点，于是被判定未使用。

## 关键区分：`--tests` 差集

单看 `cargo build --release` 无法区分"真死代码"与"测试专用符号"，两者修复方式完全不同。对照非测试与测试两组 check 的差集可以精确分类：

**A 类 — 测试目标下告警消失**：符号确实只服务测试，收敛到 `#[cfg(test)]`。

**B 类 — 测试目标下告警仍在**：任何编译配置下都无调用点，属真死代码，直接删除。

`src/anthropic/middleware.rs:60` 这条告警横跨两类：非测试下报三个方法，测试下只剩 `with_kiro_provider`。所以它必须拆开处理，不能整块归类。

## 完整告警清单

### A 类：测试专用符号（6 项）→ `#[cfg(test)]`

| 位置 | 符号 | 测试调用点 | 生产替代入口 |
|---|---|---|---|
| `src/admin/service.rs:52` | `AdminService::new` | `service.rs` 内 16 处 | `new_with_runtime`（`main.rs:206`） |
| `src/anthropic/converter.rs:227` | `default_resolution_policy` | `converter.rs` 内 7 处 + `convert_request` | 运行时策略经参数传入 |
| `src/anthropic/converter.rs:466` | `convert_request` | `converter.rs` 内 6 处 | `convert_request_with_policy` |
| `src/anthropic/middleware.rs:70` | `AppState::set_auth` | `middleware.rs` 内 2 处 | `with_auth_runtime` |
| `src/anthropic/middleware.rs:78` | `AppState::auth_snapshot` | `middleware.rs` 内 4 处 | 中间件直接读 `self.auth` |
| `src/kiro/token_manager.rs:2254` | `add_credential` | `token_manager.rs` 内 6 处 | Admin 走统一 `ingest_credential` |

**`default_resolution_policy` 与 `convert_request` 必须同时收敛，不能拆成两步。** 前者的 8 个调用点中有 7 处在测试内，第 8 处是 `converter.rs:467`——位于 `convert_request` 体内，而 `convert_request` 自身也是 A 类待收敛项。只给 `default_resolution_policy` 加 `#[cfg(test)]` 会让生产编译的 `convert_request` 找不到它，直接 E0425。

注意 `add_credential` 名称在仓库里有两个不同符号：`MultiTokenManager::add_credential`（本条，未使用）与 `AdminService::add_credential`（`service.rs:317`，生产在用，经 `admin/router.rs:31` 挂载，内部转调 `ingest_from_request`）。只处理前者。

### B 类：真死代码（6 项）→ 删除

| 位置 | 符号 | 判定依据 |
|---|---|---|
| `src/anthropic/middleware.rs:60` | `AppState::with_kiro_provider` | 全仓零调用点；已由 `with_kiro_provider_arc`（`router.rs:47` 在用）取代 |
| `src/anthropic/router.rs:23` | `create_router_with_provider` | 全仓零调用点；仅转调 `create_router_with_provider_and_auth`，后者才是被导出的入口 |
| `src/kiro/kam_adapter.rs:112` | `AdaptedDocument::credentials` | 全仓零 `.credentials()` 调用；业务直接遍历 `records` 逐条处理以保留失败原因 |
| `src/kiro/model/credentials.rs:276` | `CredentialsConfig::load` | 全仓零调用点；`main.rs:49` 与全部测试已切到 `load_detailed` |
| `src/kiro/provider.rs:133` | `KiroProvider::default_endpoint` | 全仓零调用点；Admin 从运行时配置读（`service.rs:84`） |
| `src/openai/responses.rs:394` | `chat_with_rewrite` | 测试内 helper，自身无调用点。仅 `--all-targets` 可见 |

删除安全性已核实：以上符号**均未被任何 `mod.rs` 重导出**。`anthropic/mod.rs:35` 导出的是 `create_router_with_provider_and_auth`（保留项），不是待删的 `create_router_with_provider`。因此删除是自包含的，无需同步修改导出清单。

`KiroProvider::default_endpoint` 无连带影响：同名字段 `default_endpoint`（`provider.rs:46`）在 `endpoint_for`（`provider.rs:102`）中被读取，配套 setter `set_default_endpoint`（`provider.rs:119`）在 `service.rs:1250` 生产使用，删 getter 不会产生"只写不读字段"的新告警。

`AdaptedDocument::credentials` 无连带影响：同 `impl` 块的 `has_failures`（`kam_adapter.rs:117`）在 `service.rs:641` 有生产调用点，删 `credentials` 不会让整个 `impl` 变成死代码。

### C 类：机械修复（1 项）

`src/openai/handlers.rs:1207` — `mut ctx: ResponsesStreamContext` 参数上的 `mut` 多余。

成因是参数绑定本身从不需要可变：`ctx.initial_events()`（`responses_stream.rs:148`）取的是 `&self`，随后 `ctx` 被整体 move 进 `unfold` 的初始状态元组。真正需要 `mut` 的是闭包签名里解构出的 `mut ctx`，那是另一个绑定。删除参数上的 `mut`，无行为变化。

### D 类：导入位置（1 项）

`src/anthropic/handlers.rs:25` — `map_model` 未使用。

**这不是"函数只被测试使用"**。`map_model` 是生产热路径符号：`converter.rs:352` 的 `get_context_window_size` 调用它，而后者有 5 个生产调用点（`anthropic/handlers.rs:740`、`anthropic/stream.rs:640`、`openai/handlers.rs:378`、`openai/stream.rs:150`、`openai/responses_stream.rs:168`）。

真实原因是 `handlers.rs` 自身的导入冗余：该文件生产区（1–1161 行）一次都没用到 `map_model`，唯一使用点在 `handlers.rs:1183`、`1210`、`1211`，均位于 `#[cfg(test)]` 内。

**必须移动，不能删除。** 实测确认：直接删掉 `handlers.rs:25` 的顶层导入后，`cargo check --release --all-targets` 报 3 处 `error[E0425]: cannot find function map_model in this scope`。`handlers.rs:1164` 的 `use super::*` 无法兜底——glob 只能带入 `handlers` 模块作用域里当前可见的符号，顶层导入一删，作用域里就没有 `map_model` 可带。

正确修复是两处编辑：

1. 从 `handlers.rs:25` 的导入列表移除 `map_model`
2. 在 `#[cfg(test)] mod tests` 内补一行

```rust
use super::super::converter::map_model;
```

**注意路径要两层 `super`。** 测试模块内的 `super` 指向 `handlers` 模块而非 `anthropic` 模块，写成 `use super::converter::map_model;` 会报 `error[E0432]: unresolved import super::converter: could not find converter in super`。两层写法已实测通过，且与仓库既有风格一致（`converter.rs:1270`、`1482` 等处同款）。

### E 类：需规格决策（1 项）

`src/public_api/catalog.rs:13` — `EndpointStatus::Beta` 从未被构造。当前目录共 8 个端点：7 个为 `Live`、1 个为 `Planned`，无 `Beta`。

**建议保留 + 窄范围 `#[allow(dead_code)]` 并注明理由。** `Beta` 不是孤立枚举值：`catalog.rs:23` 的 `as_str` 为它保留了 `"beta"` 分支，而 `as_str` 被 `dto.rs:118` 用于 DTO 序列化，属对外契约的一部分。删除 variant 需同步删 `as_str` 分支，等于收窄已发布的 status 取值域，超出"不改变行为"的约束。

这是「零新增告警」目标里允许的例外形态：范围收到单个 variant，理由写在紧邻位置。禁止通过构造虚假 Beta 端点数据来消除告警。

## 实施顺序

按风险从低到高，每步独立可验证：

```text
1. C + D 类机械修复           → verify: cargo check --release --all-targets（告警 14 → 12）
2. B 类删除 6 个无调用点符号   → verify: 同上（12 → 6）
3. A 类 6 项收敛 cfg(test)     → verify: cargo check --release --all-targets（6 → 1）+ cargo test 全绿
4. E 类 Beta 规格决策          → verify: 告警清零
```

第 3 步的风险是**编译期可见性**，不是断言语义。A 类 6 项只加 `#[cfg(test)]` 属性、不改函数体，也不改测试的调用目标，所以断言语义不会变。真正会出问题的是符号间的传递依赖——已知一处是 `default_resolution_policy` → `convert_request`（见 A 类说明），两者必须同批提交。`#[cfg(test)]` 边界已逐项行号比对确认成立：`converter.rs:1159`、`middleware.rs:125`、`service.rs:1656`、`token_manager.rs:3053`、`handlers.rs:1162`。

该步仍须跑 `cargo test`，因为 check 只覆盖编译，不覆盖 `#[cfg(test)]` 收敛后测试是否仍能链接到全部依赖符号。

## 成功标准

```bash
cargo build --release
cargo check --release --all-targets
cargo test
openspec validate --all
```

以 **`cargo check --release --all-targets` 零告警**为准绳，而非 `cargo build --release`——后者漏掉 `chat_with_rewrite`（它在 `responses.rs` 的 `#[cfg(test)] mod tests` 内，只有测试目标可见），是更松的门槛。这一选择同时是「零新增告警」项目目标的判定命令。

## 明确不采用的做法

- **不加 crate/module 级 `#![allow(dead_code)]`**。会掩盖后续真实废弃代码，把一次性清理变成永久失明。
- **不直接跑 `cargo fix`**。cargo 自报只能 `apply 1 suggestion`（两个目标各 1 条），覆盖率极低；且它对 `map_model` 的自动修复是删除顶层导入，会破坏测试编译（见 D 类）。
- **不为消除告警构造假数据**。特指 E 类的 Beta 端点。

## 后续项：CI 缺少告警门禁

当前 CI（`.github/workflows/build.yaml:131`、`build-dev-release.yaml:123`）只跑 `cargo build --release`，无 `-D warnings`，也无 clippy 步骤。清零之后没有任何机制防止回归。

「零新增告警」要真正落地，需要在 CI 增加一步 `cargo check --release --all-targets` 并对告警失败。但这属于 CI/发布脚本变更，命中 `AGENTS.md`「OpenSpec 条件」中的 Docker / 发布 / CI 部署脚本强制项，且超出本方案"不改变运行时行为"的范围。

**建议独立 change 处理，不在本次顺手做。** 本次清理先把基线降到零，门禁随后补。

## 流程门禁

按 `AGENTS.md` 的 OpenSpec 条件，本清理**必须先建立 OpenSpec change**。触及的强制条件：

- Anthropic 转换逻辑（`anthropic/converter.rs`）
- Token 刷新 / 多凭据（`kiro/token_manager.rs`）
- API Key / 认证中间件（`anthropic/middleware.rs`）
- Admin API 或凭据管理（`admin/service.rs`、`kiro/model/credentials.rs`）
- 跨模块变更 / 大范围重构

建议 change 名：`clean-release-build-warnings`（已确认 `openspec/changes/` 下无同名目录）。

不要合入 `social-profile-arn-cooldown`——该 change 已归档至 `openspec/changes/archive/2026-07-31-social-profile-arn-cooldown`，且两者目标、影响面与验证标准无关。

按 `AGENTS.md`「OpenSpec 条件」可豁免条款（"纯拼写、注释、单行且无行为变化的修复"），严格可豁免的只有 C 类删 `mut` 一项。D 类跨两处编辑（删顶层导入 + 在测试模块补导入），不符合"单行"字面要求。既然目标是一次性清零，建议 C、D 一并纳入同一个 change，不拆散。

实施流程：

```text
openspec-new-change / openspec-propose
→ openspec-superpowers-bridge
→ openspec-apply-change
→ spec-compliance-check
→ verification-before-completion
```

## 与 profile_arn 改动的关系

本批告警**不是** `e8c5eda`（Social 凭据 `profileArn` 解析冷却）引入的。该提交已落地，工作树干净，告警在此基线上稳定复现。

## 本轮验证边界

已实跑：

- `cargo build --release`（退出码 0）、`cargo check --release --all-targets`、`cargo check --release`、`cargo check --release --tests`，四者均成功
- D 类破坏性实验：删顶层导入 → 复现 3 处 E0425；补 `use super::converter::map_model` → 复现 E0432；改用 `use super::super::converter::map_model` → 编译通过且 `handlers.rs` 告警清零。实验后工作树已还原（`git status --short` 仅剩本文档）

调用点统计经 grep 全仓核对，`#[cfg(test)]` 边界经行号比对确认。

未运行 `cargo test`。因此"A 类收敛后测试仍通过"未经验证，属实施阶段需承担的剩余风险。

本文档为分析产物，未修改任何源码。

