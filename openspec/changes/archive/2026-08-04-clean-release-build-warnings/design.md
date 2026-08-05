## Context

基线：`dev` @ `e8c5eda`，工作树干净，Rust 1.94.1 / Windows 11 / bash。

完整分析产物：`docs/release-build-warnings-cleanup-design.md`（含破坏性实验原始记录）。本文件只保留实现所需的决策与验证策略，不复述分析过程。

## 当前实现

`cargo check --release --all-targets` 输出的 14 项唯一告警：

```
src\openai\handlers.rs:1207:5      variable does not need to be mutable
src\anthropic\middleware.rs:60:12  method `with_kiro_provider` is never used
src\anthropic\router.rs:23:8       function `create_router_with_provider` is never used
src\kiro\kam_adapter.rs:112:12     method `credentials` is never used
src\kiro\model\credentials.rs:276  associated function `load` is never used
src\kiro\provider.rs:133:12        method `default_endpoint` is never used
src\openai\responses.rs:394:8      function `chat_with_rewrite` is never used
src\public_api\catalog.rs:13:5     variant `Beta` is never constructed
src\anthropic\handlers.rs:25:68    unused import: `map_model`
src\admin\service.rs:52:12         associated function `new` is never used
src\anthropic\converter.rs:227:8   function `default_resolution_policy` is never used
src\anthropic\converter.rs:466:8   function `convert_request` is never used
src\anthropic\middleware.rs:60:12  methods `with_kiro_provider`, `set_auth`, `auth_snapshot` are never used
src\kiro\token_manager.rs:2254:18  method `add_credential` is never used
```

前 8 条来自 test target，后 6 条来自 bin target，`middleware.rs:60` 在两处以不同符号集出现。

## 目标设计

### A 类：收敛到 `#[cfg(test)]`（6 项）

| 位置 | 符号 | 测试调用点 | 生产替代入口 |
|---|---|---|---|
| `src/admin/service.rs:52` | `AdminService::new` | `service.rs` 内 16 处 | `new_with_runtime`（`main.rs:206`） |
| `src/anthropic/converter.rs:227` | `default_resolution_policy` | `converter.rs` 内 7 处 + `convert_request` 体内 1 处 | 运行时策略经参数传入 |
| `src/anthropic/converter.rs:466` | `convert_request` | `converter.rs` 内 6 处 | `convert_request_with_policy` |
| `src/anthropic/middleware.rs:70` | `AppState::set_auth` | `middleware.rs` 内 2 处 | `with_auth_runtime` |
| `src/anthropic/middleware.rs:78` | `AppState::auth_snapshot` | `middleware.rs` 内 4 处 | 中间件直接读 `self.auth`（`:85`） |
| `src/kiro/token_manager.rs:2254` | `MultiTokenManager::add_credential` | `token_manager.rs` 内 6 处 | Admin 走统一 `ingest_credential` |

**`default_resolution_policy` 与 `convert_request` 必须同一次提交。** 前者 8 个调用点中 7 处在测试内，第 8 处是 `converter.rs:467`——位于 `convert_request` 体内：

```rust
pub fn convert_request(req: &MessagesRequest) -> Result<ConversionResult, ConversionError> {
    convert_request_with_policy(req, &default_resolution_policy(), None)
}
```

只给 `default_resolution_policy` 加 `#[cfg(test)]`，生产编译的 `convert_request` 会报 E0425。反向单独加也不行（`default_resolution_policy` 会变成无调用点）。两者是一个原子单元。

**同名符号消歧。** `add_credential` 在仓库里有两个不同符号：

- `MultiTokenManager::add_credential`（`token_manager.rs:2254`）— 本条，无生产调用点
- `AdminService::add_credential`（`service.rs:317`）— 生产在用，经 `admin/router.rs:31` 挂载，内部转调 `ingest_from_request`

只处理前者。

### B 类：删除（6 项）

| 位置 | 符号 | 判定依据 |
|---|---|---|
| `src/anthropic/middleware.rs:60` | `AppState::with_kiro_provider` | 全仓零调用点；已由 `with_kiro_provider_arc`（`router.rs:47` 在用）取代 |
| `src/anthropic/router.rs:23` | `create_router_with_provider` | 全仓零调用点；仅转调 `create_router_with_provider_and_auth`，后者才是被导出的入口 |
| `src/kiro/kam_adapter.rs:112` | `AdaptedDocument::credentials` | 全仓零 `.credentials()` 调用；业务直接遍历 `records` 逐条处理以保留失败原因 |
| `src/kiro/model/credentials.rs:276` | `CredentialsConfig::load` | 全仓零调用点；`main.rs:49` 与全部测试已切到 `load_detailed` |
| `src/kiro/provider.rs:133` | `KiroProvider::default_endpoint` | 全仓零调用点；Admin 从运行时配置读（`service.rs:84`） |
| `src/openai/responses.rs:394` | `chat_with_rewrite` | 测试内 helper，自身无调用点。仅 `--all-targets` 可见 |

**删除的自包含性已核实。** 6 项均未被任何 `mod.rs` 重导出。`anthropic/mod.rs:35` 导出的是 `create_router_with_provider_and_auth`（保留项），不是待删的 `create_router_with_provider`。无需同步修改导出清单。

**两处连带风险已排除：**

- `KiroProvider::default_endpoint`（getter）：同名字段 `default_endpoint`（`provider.rs:46`）在 `endpoint_for`（`:102`）中被读取，配套 setter `set_default_endpoint`（`:119`）在 `service.rs:1250` 生产使用。删 getter 不会产生「只写不读字段」新告警。
- `AdaptedDocument::credentials`：同 `impl` 块的 `has_failures`（`kam_adapter.rs:117`）在 `service.rs:641` 有生产调用点。删 `credentials` 不会让整个 `impl` 变成死代码。

### C 类：删多余 `mut`（1 项）

`src/openai/handlers.rs:1207` 的 `mut ctx: ResponsesStreamContext`。

参数绑定本身从不需要可变：`ctx.initial_events()`（`responses_stream.rs:148`）取的是 `&self`，随后 `ctx` 被整体 move 进 `unfold` 的初始状态元组。真正需要 `mut` 的是闭包签名里解构出的 `mut ctx`，那是另一个绑定，不受影响。

### D 类：移动导入（1 项）

`src/anthropic/handlers.rs:25` 的 `map_model`。

**这不是「函数只被测试使用」。** `map_model` 是生产热路径符号：`converter.rs:352` 的 `get_context_window_size` 调用它，后者有 5 个生产调用点（`anthropic/handlers.rs:740`、`anthropic/stream.rs:640`、`openai/handlers.rs:378`、`openai/stream.rs:150`、`openai/responses_stream.rs:168`）。

真实原因是 `handlers.rs` 自身的导入冗余：该文件生产区（1–1161 行）一次都没用到 `map_model`，唯一使用点在 `:1183`、`:1210`、`:1211`，均位于 `#[cfg(test)]`（起于 `:1162`）内。

**必须移动，不能删除。** 已实测：删掉顶层导入后 `cargo check --release --all-targets` 报 3 处 `error[E0425]: cannot find function map_model in this scope`。`handlers.rs:1164` 的 `use super::*` 无法兜底——glob 只能带入 `handlers` 模块作用域里当前可见的符号，顶层导入一删，作用域里就没有 `map_model` 可带。

修复为两处编辑：

```rust
// handlers.rs:25 —— 从导入列表移除 map_model
use super::converter::{
    ConversionError, THINKING_SUFFIX, convert_request_with_policy, resolve_model,
};

// #[cfg(test)] mod tests 内新增
use super::super::converter::map_model;
```

**路径必须两层 `super`。** 测试模块内的 `super` 指向 `handlers` 模块而非 `anthropic` 模块，写成 `use super::converter::map_model;` 会报：

```
error[E0432]: unresolved import `super::converter`: could not find `converter` in `super`
```

两层写法已实测通过且 `handlers.rs` 告警清零，与仓库既有风格一致（`converter.rs:1270`、`:1482` 等处同款 `use super::super::types::...`）。

### E 类：保留 + 窄范围 `#[allow]`（1 项）

`src/public_api/catalog.rs:13` 的 `EndpointStatus::Beta` 从未被构造。当前目录共 8 个端点：7 个为 `Live`、1 个为 `Planned`。

**删除会违反生效 spec。** `openspec/specs/public-api-catalog/spec.md:39` 是无条件 MUST：

> Each entry MUST declare a status of `live`, `beta`, or `planned`.

`Beta` 不是孤立枚举值：`catalog.rs:23` 的 `as_str` 为它保留 `"beta"` 分支，`as_str` 经 `dto.rs:118` 用于 DTO 序列化。删 variant 需同步删 `as_str` 分支，等于收窄已发布的 status 取值域。

因此保留是规格约束下的唯一合法解，不是权衡取舍。**禁止通过构造虚假 Beta 端点数据来消除告警**——那会同时违反 `public-api-catalog` 的 live/planned 一致性 MUST。

## 数据流与影响面

无数据流变化。所有改动都在编译期可见性层面：

- A 类：符号在非测试编译中不再存在，测试编译中不变
- B 类：符号完全消失，无调用点故无影响
- C 类：绑定可变性标注，无语义
- D 类：导入位置，符号本体在 `converter.rs` 未动
- E 类：仅加 lint 属性

**零签名变更、零契约变更。** 保留项（`new_with_runtime`、`convert_request_with_policy`、`with_auth_runtime`、`with_kiro_provider_arc`、`create_router_with_provider_and_auth`、`load_detailed`、`ingest_credential`、`set_default_endpoint`、`has_failures`）逐字不变。

Admin API、Anthropic/OpenAI 端点、SSE 流、token 刷新链路、凭据入库管道的行为均不受影响。

## 异常路径

本 change 无运行时异常路径（无新增逻辑分支）。实现期的失败模式与处置：

| 失败 | 表现 | 处置 |
|---|---|---|
| D 类路径写成单层 `super` | E0432 unresolved import | 改两层 `super::super::converter::map_model` |
| D 类误删而非移动 | 3 处 E0425 | 恢复符号可见性，在测试模块补导入 |
| A 类拆步提交 | E0425（`convert_request` 找不到 `default_resolution_policy`） | 两者同批加属性 |
| B 类删除后编译失败 | 未预期的调用点 | 停下重新 grep，不可强行删除或加 `allow` 掩盖 |
| 告警数不降反升 | 出现「只写不读字段」或 impl 级告警 | 回退该项，重新核对连带引用 |
| `cargo test` 失败 | A 类收敛导致测试链接问题 | 回退该项；不可用 `allow` 绕过 |

## 回滚

每类独立可回滚，粒度为一次 `git revert`：

- 分步提交（1: C+D，2: B，3: A，4: E），任一步失败只回退该步
- 无数据迁移、无配置变更、无持久化状态，回滚即恢复原状
- B 类删除的 6 个符号若将来需要，从 git 历史恢复即可（均为薄包装，无独立逻辑）

## 验证策略

### 分步门槛

```text
1. C + D 类  → cargo check --release --all-targets：14 → 12
2. B 类      → 同上：12 → 6
3. A 类      → 同上：6 → 1，且 cargo test 全绿
4. E 类      → 同上：1 → 0
```

**第 3 步的风险是编译期可见性，不是断言语义。** A 类 6 项只加属性、不改函数体、不改测试的调用目标，因此断言语义不会变。已知的真实风险是符号间传递依赖（`default_resolution_policy` → `convert_request`）。

该步仍须跑 `cargo test`：check 只覆盖编译，不覆盖 `#[cfg(test)]` 收敛后测试是否仍能链接到全部依赖符号。

### 告警计数口径

以**唯一告警行数**计，不以 cargo 汇总行数字计。`grep -c "warning:"` 会把两条汇总行也算进去（当前得 16，实际 14）。推荐计数命令：

```bash
cargo check --release --all-targets --message-format short 2>&1 \
  | grep "warning:" | grep -v "generated .* warning" | sort -u | wc -l
```

### 最终验证

```bash
cargo check --release --all-targets   # 0 告警（准绳）
cargo build --release                 # 0 warnings
cargo test                            # 全绿
openspec validate --all               # 通过
git status --short                    # 无敏感文件
```

**以 `cargo check --release --all-targets` 为准绳，而非 `cargo build --release`。** 后者漏掉 `chat_with_rewrite`（它在 `responses.rs` 的 `#[cfg(test)] mod tests` 内，只有测试目标可见），是更松的门槛。

### 不采用的做法

- **不加 crate/module 级 `#![allow(dead_code)]`**：会掩盖后续真实废弃代码，把一次性清理变成永久失明
- **不跑 `cargo fix`**：cargo 自报每个 target 只能 apply 1 suggestion，覆盖率极低；且它对 `map_model` 的自动修复是删顶层导入，会破坏测试编译
- **不构造假数据**：特指不新增虚假 Beta 端点

## 后续项：CI 缺少告警门禁

当前 CI（`.github/workflows/build.yaml:131`、`build-dev-release.yaml:123`）只跑 `cargo build --release`，无 `-D warnings`，也无 clippy 步骤。清零之后没有任何机制防止回归。

新 capability `build-warning-hygiene` 在规格层承载了门禁要求，但**规格不等于自动执行**。要让「零新增」有机器强制力，需在 CI 增加一步 `cargo check --release --all-targets` 并对告警失败。

这属于 CI/发布脚本变更，命中 `AGENTS.md`「OpenSpec 条件」中的 Docker / 发布 / CI 部署脚本强制项，且超出本 change「不改运行时行为」的范围。**作为独立 change 处理，不在本次顺手做。** 本次先把基线降到零，门禁随后补。

## 本轮已实测（分析阶段）

- `cargo build --release`（退出码 0）、`cargo check --release --all-targets`、`cargo check --release`、`cargo check --release --tests`，四者均成功
- D 类破坏性实验三轮：删顶层导入 → 复现 3 处 E0425；补单层 `use super::converter::map_model` → 复现 E0432；改用 `use super::super::converter::map_model` → 编译通过且 `handlers.rs` 告警清零。实验后工作树已还原
- 调用点统计经 grep 全仓核对；`#[cfg(test)]` 边界经行号比对确认
- `openspec/specs/` 全量 grep：12 个待处理符号均未被任何生效 spec 提及；`public-api-catalog/spec.md:39` 确认为 E 类的规格依据

未运行 `cargo test`。因此「A 类收敛后测试仍通过」在实现阶段才能确认，属剩余风险。
