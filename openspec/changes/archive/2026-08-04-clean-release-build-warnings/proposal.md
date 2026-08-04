## Why

`cargo check --release --all-targets` 在 `dev` @ `e8c5eda` 上产生 **14 项唯一告警**，全部来自项目源码，与第三方依赖、release 优化、LTO 均无关。

完整分析、分类依据与破坏性实验记录见 `docs/release-build-warnings-cleanup-design.md`。

### 实测基线

```bash
cargo build --release                    # 退出码 0，12 warnings
cargo check --release --all-targets      # 14 项唯一告警（多出 chat_with_rewrite）
cargo check --release                    # 12 项（非测试目标）
cargo check --release --tests            # 8 项（测试目标）
```

### 根因

项目只有 binary target（无 `src/lib.rs`，`Cargo.toml` 未声明 `[lib]`），因此 `pub` **不构成**「对外 API」这一豁免理由——所有 `pub` 项都必须在 crate 内有实际调用点，否则即为死代码。同时测试辅助入口与旧兼容包装混在生产编译单元里，普通 release 构建不编译 `#[cfg(test)]` 调用点，于是被判定未使用。

### 为什么现在做，以及为什么必须同时立规

告警数只会单调增长。本次清理若不配套「零新增」门禁，下一批告警会以同样方式积累，清理沦为周期性劳动。

因此本 change 有**两个不可分割的交付物**：

1. 把当前基线降到零
2. 在规格层确立「任何代码实现不得引入新告警，与是否走 OpenSpec 流程无关」

第 2 项目前只写在 `AGENTS.md` / `spec/` / `openspec/project.md`（本 change 前置已同步），但缺少 openspec capability 承载，无法被 `openspec validate` 覆盖，也无法在后续 change 的 spec-compliance 阶段被引用。本 change 补这一块。

### `--tests` 差集是分类的唯一可靠依据

单看 `cargo build --release` 无法区分「真死代码」与「测试专用符号」，两者修复方式完全相反。对照非测试与测试两组 check 的差集才能精确分类：

- **测试目标下告警消失** → 符号只服务测试，收敛到 `#[cfg(test)]`
- **测试目标下告警仍在** → 任何编译配置下都无调用点，属真死代码，删除

`src/anthropic/middleware.rs:60` 横跨两类：非测试下报三个方法，测试下只剩 `with_kiro_provider`。必须拆开处理。

## What Changes

按修复动作分五类，共 14 项：

- **A 类（6 项）→ `#[cfg(test)]`**：`AdminService::new`、`default_resolution_policy`、`convert_request`、`AppState::set_auth`、`AppState::auth_snapshot`、`MultiTokenManager::add_credential`
- **B 类（6 项）→ 删除**：`AppState::with_kiro_provider`、`create_router_with_provider`、`AdaptedDocument::credentials`、`CredentialsConfig::load`、`KiroProvider::default_endpoint`、`chat_with_rewrite`
- **C 类（1 项）→ 删多余 `mut`**：`openai/handlers.rs:1207`
- **D 类（1 项）→ 移动导入**：`anthropic/handlers.rs:25` 的 `map_model` 移入测试模块
- **E 类（1 项）→ 保留 + 窄范围 `#[allow]`**：`EndpointStatus::Beta`
- **新增 capability `build-warning-hygiene`**：把「零新增告警」及其手段约束写入规格层

代码层**不改任何函数签名、不改任何对外契约、不改任何运行时行为**。

## 与生效 spec 的关系

### E 类受生效 spec 保护，不能删

`openspec/specs/public-api-catalog/spec.md:39` 是无条件 MUST：

> Each entry MUST declare a status of `live`, `beta`, or `planned`.

删除 `EndpointStatus::Beta` variant 需同步删 `catalog.rs:23` 的 `as_str` 中 `"beta"` 分支，而 `as_str` 经 `dto.rs:118` 用于 DTO 序列化。这等于收窄已发布的 status 取值域，**直接违反上述 MUST**。

所以 E 类的保留不是权衡取舍，是规格约束下的唯一合法解。本 change 因此**不提供** `public-api-catalog` 的 delta——既有 spec 无需修改，只需被遵守。

### A/B 类不与任何生效 spec 冲突

已逐符号 grep `openspec/specs/`：待删除与待收敛的 12 个符号**均未在任何生效 spec 中被提及**。这些符号是实现细节，不是规格约束的对象。

## Scope

代码：

- `src/admin/service.rs`：`AdminService::new` 加 `#[cfg(test)]`
- `src/anthropic/converter.rs`：`default_resolution_policy` 与 `convert_request` 加 `#[cfg(test)]`（必须同批，见 design）
- `src/anthropic/middleware.rs`：删 `with_kiro_provider`；`set_auth`、`auth_snapshot` 加 `#[cfg(test)]`
- `src/anthropic/router.rs`：删 `create_router_with_provider`
- `src/anthropic/handlers.rs`：`map_model` 从顶层导入移入测试模块
- `src/kiro/token_manager.rs`：`MultiTokenManager::add_credential` 加 `#[cfg(test)]`
- `src/kiro/kam_adapter.rs`：删 `AdaptedDocument::credentials`
- `src/kiro/model/credentials.rs`：删 `CredentialsConfig::load`
- `src/kiro/provider.rs`：删 `KiroProvider::default_endpoint`
- `src/openai/handlers.rs`：删参数上多余 `mut`
- `src/openai/responses.rs`：删测试内 `chat_with_rewrite`
- `src/public_api/catalog.rs`：`Beta` variant 加窄范围 `#[allow(dead_code)]` 并注明理由

规格：

- `openspec/changes/clean-release-build-warnings/specs/build-warning-hygiene/spec.md`：新 capability

已在本 change 前置完成的规范同步（不在本 change 的代码 scope 内，但属交付物）：`AGENTS.md`、`spec/requirements.md`、`spec/design.md`、`openspec/project.md`、`.codex/skills/verification-before-completion/SKILL.md`。

## Non-Goals

- **不改任何运行时行为。** 只做移动导入、删除无调用点代码、收敛 `#[cfg(test)]`、删多余 `mut`、加窄范围 `allow`。
- **不重构逻辑、不调整函数签名、不改对外契约。** 保留项的签名逐字不变。
- **不删除 `EndpointStatus::Beta`。** 受 `public-api-catalog` 生效 spec 保护，见上文。
- **不加 crate 级或 module 级 `#![allow(dead_code)]`。** 会把一次性清理变成永久失明。
- **不跑 `cargo fix`。** cargo 自报每个 target 只能 apply 1 suggestion，覆盖率极低；且它对 `map_model` 的自动修复是删顶层导入，会破坏测试编译（已实测，见 design）。
- **不为消除告警构造假数据。** 特指不新增虚假 Beta 端点。
- **不改 CI。** 在 CI 增加 `-D warnings` 或 clippy 步骤命中 `AGENTS.md`「OpenSpec 条件」中的 Docker / 发布 / CI 部署脚本强制项，且超出「不改运行时行为」范围。作为后续独立 change，见 design「后续项」。
- **不清理预先存在的其他死代码。** 只处理编译器实际报出的这 14 项。
- **不改 admin-ui。** 本批告警全部来自 Rust 源码。

## Assumptions

- **告警数以「唯一告警行数」计，不以 cargo 汇总行数字计。** `cargo check --release --all-targets` 输出两个 target 的汇总行（test 8 warnings、bin 12 warnings 含 6 duplicates），去重后为 14 项。`grep -c "warning:"` 得 16 是因为含 2 条汇总行本身。本 change 全程按 14 → 0 计。
- **Rust 版本对告警集合有影响。** 基线为 Rust 1.94.1。工具链升级可能引入或消除告警，届时基线需重测。
- **A 类 6 项的测试调用点全部位于 `#[cfg(test)]` 内**，已逐项行号比对确认边界：`converter.rs:1159`、`middleware.rs:125`、`service.rs:1656`、`token_manager.rs:3053`、`handlers.rs:1162`。因此加属性不会使测试失去调用目标。

## Success Criteria

| 指标 | 当前 | 目标 |
| --- | --- | --- |
| `cargo check --release --all-targets` 唯一告警数 | **14** | **0** |
| `cargo build --release` warnings | 12 | 0 |
| `cargo test` | 未跑（本轮） | 全绿 |
| `openspec validate --all` | 19 passed | 通过（含新 capability） |
| 运行时行为变化 | — | **零**（无签名变更、无契约变更） |

分步验证门槛（每步独立可验证）：

```text
1. C + D 类  → cargo check --release --all-targets：14 → 12
2. B 类      → 同上：12 → 6
3. A 类      → 同上：6 → 1，且 cargo test 全绿
4. E 类      → 同上：1 → 0
```

## Risks

| 风险 | 后果 | 控制措施 |
| --- | --- | --- |
| **D 类导入路径写错** | 测试编译失败（E0432）。测试模块内 `super` 指向 `handlers` 而非 `anthropic`，单层 `super::converter` 不存在 | 必须写 `use super::super::converter::map_model;`，已实测通过；tasks 有专项验证。与仓库既有风格一致（`converter.rs:1270`、`:1482`） |
| **A 类传递依赖漏改** | 生产编译 E0425。`default_resolution_policy` 第 8 个调用点在 `convert_request` 体内（`converter.rs:467`），而后者也是 A 类项 | 两者必须同批加属性，不可拆步；tasks 3.2 强制同一次提交 |
| 误把 D 类当死代码删除 | 测试编译失败（3 处 E0425）。`map_model` 是生产热路径符号，经 `get_context_window_size` 有 5 个生产调用点 | proposal / design / tasks 三处均标注「必须移动，不能删除」；已实测复现该失败 |
| A 类收敛后测试链接失败 | `cargo test` 失败 | 第 3 步必须跑 `cargo test`，不能只看 check（check 只覆盖编译，不覆盖测试链接） |
| B 类符号实际被重导出 | 删除后编译失败 | 已核实全部 6 项均未被任何 `mod.rs` 重导出。`anthropic/mod.rs:35` 导出的是 `create_router_with_provider_and_auth`（保留项） |
| 删 getter 产生「只写不读字段」新告警 | 告警数不降反升 | 已核实 `provider.rs:46` 的 `default_endpoint` 字段在 `endpoint_for`（`:102`）被读，setter 在 `service.rs:1250` 生产使用 |
| 删 `credentials()` 使整个 `impl` 变死代码 | 新增 impl 级告警 | 已核实同 `impl` 的 `has_failures`（`kam_adapter.rs:117`）在 `service.rs:641` 有生产调用点 |
| 清零后无回归防护 | 告警重新积累 | 新 capability `build-warning-hygiene` 在规格层承载门禁；CI 层门禁作为后续独立 change |
| 窄范围 `allow` 被滥用为通用逃逸口 | 门禁形同虚设 | spec 中限定：仅当删除会违反生效 spec 或改变对外契约时可用，且必须紧邻注明理由 |

风险类型（`AGENTS.md` 高风险矩阵）：**Anthropic 转换逻辑 + Token/多凭据 + 认证中间件 + Admin/凭据管理 + 跨模块变更 + OpenSpec**。

对应验证：`cargo check --release --all-targets`、`cargo test`、`openspec validate --all`、`git status --short`。
