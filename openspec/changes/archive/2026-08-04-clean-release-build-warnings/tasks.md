## 1. 实现前核对

- [x] 1.1 读 `AGENTS.md` 与本 change 的 proposal / design / specs
  → 验证：能陈述本次高风险类型（Anthropic 转换逻辑、Token/多凭据、认证中间件、Admin/凭据管理、跨模块变更、OpenSpec）与验证命令
- [x] 1.2 运行 `openspec-superpowers-bridge`，产出 `evidence/bridge-plan.md`
  → 验证：bridge plan 存在，且逐条核对过 design 标注的两处硬约束（A 类传递依赖、D 类双层 `super`）
- [x] 1.3 记录改动前告警基线
  → 验证：`cargo check --release --all-targets` 唯一告警数 **恰好 14**；若不符必须停下重新核对（工具链版本或工作树状态已变），不可按记录的行号盲改
- [x] 1.4 核对 14 项告警的**实际行号**与 design 记录一致
  → 验证：逐项比对；任一项行号漂移则停下重新定位，禁止按旧行号编辑

## 2. C 类 + D 类：机械修复（先做，风险最低且独立）

> 编号说明：本章节号与任务号对齐（§2/§3/§4/§5）。design 中的“实施顺序 1–4”对应这里的 §2–§5。

- [x] 2.1 删 `src/openai/handlers.rs:1207` 参数上多余的 `mut`
  → 验证：`cargo check --release --all-targets` 不再报该行；`initial_events` 取 `&self` 故无编译影响
- [x] 2.2 从 `src/anthropic/handlers.rs:25` 的导入列表移除 `map_model`（**只移除这一个符号**，`ConversionError`、`THINKING_SUFFIX`、`convert_request_with_policy`、`resolve_model` 保留）
  → 验证：此时应出现 3 处 E0425（`:1183`、`:1210`、`:1211`），这是预期中间态
- [x] 2.3 在 `src/anthropic/handlers.rs` 的 `#[cfg(test)] mod tests`（起于 `:1162`）内补 `use super::super::converter::map_model;`
  → 验证：**必须两层 `super`**。单层 `use super::converter::map_model;` 会报 E0432（测试模块内 `super` 指向 `handlers` 而非 `anthropic`）。补后 3 处 E0425 消失
  → 停止条件：若此处仍报错，停下汇报，不可改为删除测试代码或加 `allow`
- [x] 2.4 确认 `map_model` 本体未被触碰
  → 验证：`src/anthropic/converter.rs:222` 的 `pub fn map_model` 逐字未变；`get_context_window_size`（`:352`）仍调用它
- [x] 2.5 第 2 步（C+D）整体门槛
  → 验证：`cargo check --release --all-targets` 唯一告警数 **14 → 12**

## 3. B 类：删除 6 个无调用点符号

- [x] 3.1 删 `src/anthropic/middleware.rs:60` 的 `AppState::with_kiro_provider`
  → 验证：`with_kiro_provider_arc`（`:65`）保留且 `router.rs:47` 仍调用它
- [x] 3.2 删 `src/anthropic/router.rs:23` 的 `create_router_with_provider`
  → 验证：`create_router_with_provider_and_auth`（`:38`）保留；`anthropic/mod.rs:35` 的导出行未改
- [x] 3.3 删 `src/kiro/kam_adapter.rs:112` 的 `AdaptedDocument::credentials`
  → 验证：同 `impl` 的 `has_failures`（`:117`）保留；`service.rs:641` 仍调用它，故 `impl` 不会整体变死代码
- [x] 3.4 删 `src/kiro/model/credentials.rs:276` 的 `CredentialsConfig::load`
  → 验证：`load_detailed`（`:286`）保留；`main.rs:49` 与全部测试仍走 `load_detailed`
- [x] 3.5 删 `src/kiro/provider.rs:133` 的 `KiroProvider::default_endpoint`（getter）
  → 验证：字段 `default_endpoint`（`:46`）与 setter `set_default_endpoint`（`:119`）均保留；`endpoint_for`（`:102`）仍读该字段，故不产生「只写不读字段」新告警
- [x] 3.6 删 `src/openai/responses.rs:394` 测试内的 `chat_with_rewrite`
  → 验证：同模块的 `chat`（`:389`）与其他 helper 保留且仍被测试使用
- [x] 3.7 确认无一项需要同步修改 `mod.rs` 导出
  → 验证：`git diff --stat` 中不含任何 `mod.rs`
- [x] 3.8 第 3 步（B）整体门槛
  → 验证：`cargo check --release --all-targets` 唯一告警数 **12 → 6**，且无新增告警类型（尤其无「只写不读字段」）
  → 停止条件：若出现未预期的调用点导致编译失败，停下重新 grep，禁止强行删除或加 `allow` 掩盖

## 4. A 类：6 项收敛 `#[cfg(test)]`

- [x] 4.1 `src/admin/service.rs:52` 的 `AdminService::new` 加 `#[cfg(test)]`
  → 验证：`new_with_runtime`（`:59`）不变；`main.rs:206` 仍调用后者；16 处测试调用点均在 `#[cfg(test)]`（起于 `:1656`）内
- [x] 4.2 **同一次编辑**给 `src/anthropic/converter.rs:227` 的 `default_resolution_policy` 与 `:466` 的 `convert_request` 加 `#[cfg(test)]`
  → 验证：**两者必须同批，不可拆步**。`default_resolution_policy` 的第 8 个调用点在 `convert_request` 体内（`:467`），单独加任一项都会报 E0425
  → 验证：`convert_request_with_policy`（`:471`）逐字不变
- [x] 4.3 `src/anthropic/middleware.rs` 的 `set_auth`（`:70`）与 `auth_snapshot`（`:78`）加 `#[cfg(test)]`
  → 验证：`with_auth_runtime`（`:52`）不变；`auth_middleware`（`:85`）直接读 `state.auth` 而非经 `auth_snapshot`；测试调用点均在 `#[cfg(test)]`（起于 `:125`）内
- [x] 4.4 `src/kiro/token_manager.rs:2254` 的 `MultiTokenManager::add_credential` 加 `#[cfg(test)]`
  → 验证：**只改这一个符号**。`AdminService::add_credential`（`service.rs:317`，经 `admin/router.rs:31` 挂载）与 `ingest_credential`（`token_manager.rs:2262`）逐字不变
- [x] 4.5 确认无任何函数签名或函数体被修改
  → 验证：`git diff` 中 A 类改动**只有新增属性行**，无其他行变更
- [x] 4.6 第 4 步（A）check 门槛
  → 验证：`cargo check --release --all-targets` 唯一告警数 **6 → 1**（仅剩 `catalog.rs:13` 的 `Beta`）
- [x] 4.7 第 4 步（A）test 门槛（**不可只看 check**）
  → 验证：`cargo test` 全绿。check 只覆盖编译，不覆盖 `#[cfg(test)]` 收敛后测试能否链接到全部依赖符号
  → 停止条件：测试失败则回退该项，禁止用 `allow` 绕过

## 5. E 类：Beta variant 规格决策

- [x] 5.1 给 `src/public_api/catalog.rs:13` 的 `Beta` variant 加最小范围 `#[allow(dead_code)]`，并在紧邻位置注明保留理由
  → 验证：注释须点明依据 `openspec/specs/public-api-catalog/spec.md:39`（status MUST 可声明为 `live` / `beta` / `planned`）与 `dto.rs:118` 的 DTO 序列化契约
  → 验证：属性附加在 variant 上，**不在 enum、module 或 crate 级**
- [x] 5.2 确认未构造任何虚假 Beta 端点
  → 验证：`catalog.rs` 的端点表条目数与 status 分布不变（7 `Live` + 1 `Planned`，共 8 条）；`as_str`（`:23`）的 `"beta"` 分支保留
- [x] 5.3 第 5 步（E）门槛
  → 验证：`cargo check --release --all-targets` 唯一告警数 **1 → 0**

## 6. 规格与文档同步

- [x] 6.1 确认「零新增告警」已同步到全部规范入口
  → 验证：`AGENTS.md`（新增「零新增编译告警」小节 + 验证纪律 + 高风险矩阵）、`spec/requirements.md`、`spec/design.md`、`openspec/project.md`、`.codex/skills/verification-before-completion/SKILL.md` 五处均含该条款
- [x] 6.2 确认本 change 不需要 `public-api-catalog` 的 spec delta
  → 验证：该 spec 无需修改，只需被遵守（E 类保留即为遵守）；本 change 的 specs/ 下只有 `build-warning-hygiene`
- [x] 6.3 判断 README 是否需要同步
  → 验证：若 README 未提及构建告警门禁则无需改动，并在最终报告说明原因
- [x] 6.4 `openspec validate --all`
  → 验证：通过，且新 capability `build-warning-hygiene` 被校验

## 7. 完成前验证

- [x] 7.1 全量验证命令
  → 验证：`cargo check --release --all-targets`（**0 告警**，准绳）、`cargo build --release`（0 warnings）、`cargo test`（全绿）、`openspec validate --all`（通过）
- [x] 7.2 确认零行为变化
  → 验证：`git diff` 逐项复核——无函数签名变更、无函数体逻辑变更、无 `mod.rs` 导出变更、无配置/依赖变更
- [x] 7.3 运行 `spec-compliance-check`，产出 `evidence/spec-compliance-report.md`
  → 验证：逐条核对 `build-warning-hygiene` 的四个 Requirement 与实际改动一致，尤其「窄范围抑制仅在删除会违反规格或契约时允许」只被 E 类一项使用
- [x] 7.4 运行 `verification-before-completion`，产出 `evidence/verification-before-completion.md`
  → 验证：含 Verification 列表（须含告警数 14 → 0）、Documentation Sync 表、Residual Risk（CI 门禁作为后续独立 change 未落地）
- [x] 7.5 `git status --short`
  → 验证：无 `config.json`、`credentials.json`、`credentials.*`、`.codegraph/` 或真实密钥进入候选提交
