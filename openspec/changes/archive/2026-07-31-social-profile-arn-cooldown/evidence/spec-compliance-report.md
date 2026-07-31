# Spec Compliance Report — social-profile-arn-cooldown

> 产出时间：2026-07-31
> 分支：`dev`（主分支 `master`）
> 基线：`857c76f`（`kam-external-idp-import-compat` 已提交，工作区 diff 仅含本 change）
> 审查范围：`git diff` = `src/kiro/profile.rs`、`src/kiro/token_manager.rs`

## 六维结论

| 维度 | 状态 | 依据 |
| --- | --- | --- |
| Scope | PASS | 改动文件恰为 proposal Scope 列出的两个；无第三个文件被触碰 |
| Design | PASS | 冷却检查在 list 之前、版本号而非哈希、未抢到者不等待——三项关键设计决策逐条落实 |
| Scenarios | PASS | 25 个 Scenario 均有实现或测试证据（见下表） |
| Project Rules | PASS | OpenSpec 门禁齐备；`openspec validate --all` 20/20；无真实凭据入库 |
| Verification | PASS | 本会话真实运行 5 条命令，结果如实记录；9.5 明示为 SKIPPED |
| README/AGENTS Sync | PASS | 判定无需同步，理由见 §5 |

**总体状态：PASS**（含 2 项 WARN 级剩余风险，均非阻塞）

## 1. Scope

`git diff --name-only`：

```
src/kiro/profile.rs
src/kiro/token_manager.rs
```

与 proposal Scope 完全一致。非目标逐条复核：

| Non-Goal | 复核结果 |
| --- | --- |
| 不改 `decide_profile_action` | `git diff` 不含该函数任何行；10 个既有测试未修改 |
| 不改 `force_refresh_token_for` 语义与签名 | 仅多出 `bump_credentials_version()` 一行 + 8.1 的留痕注释；无清冷却逻辑 |
| 不改 IdC / API Key 决策 | decide 未改；`test_idc_soft_unavailable_writes_no_cooldown`、`test_api_key_unsupported_writes_no_cooldown` 断言逐位不变 |
| 不改硬失败错误内容与传播路径 | 两处 `bail!` 文本逐字保留；测试断言错误字符串形状 |
| 不引入配置项 | 两个窗口为 `const StdDuration`，不在任何 serde 结构中 |
| 不落盘冷却状态 | `test_cooldown_state_not_persisted` 断言凭据 JSON 与 Admin 快照均无相关字段 |
| 不改 `refresh_lock` 粒度 | 冷却存取全部在 `entries` 同步锁内，未触碰 `refresh_lock` |
| 不改 list 的重试与退避 | `list_available_profiles_with_retry` 函数体未改 |
| 不修取锁前克隆凭据的既有缺陷 | 仅加注释留痕，逻辑一字未动 |
| 不重定义 `refresh_routes_to_idc` | `git diff` 不含该函数任何行 |

**七个调用点零改动**：`git diff --name-only` 不含 `provider.rs`、`admin/service.rs`；
`token_manager.rs` 中 `ensure_profile_arn_for_request` 的两个调用点在 diff 中无任何行。

**公开签名逐字不变**：`resolve_profile_arn` 与 `ensure_profile_arn_for_request` 的
参数与返回类型未变（生效 spec `:77`、`:84` 的 MUST）。可测性通过私有
`resolve_profile_arn_inner`（list 与强刷以 `FnOnce() -> Future` 注入）实现。

## 2. Design 一致性

| design 决策 | 实现位置 | 一致 |
| --- | --- | --- |
| 冷却检查在 `ListAvailableProfiles` **之前** | `profile.rs` 冷却分支位于 api_key/supports 判定之后、`list_stage()` 之前 | ✓ |
| 检查与抢占在同一次锁内（避免窗口） | `try_begin_profile_arn_resolve` 单次 `entries.lock()` 内完成两件事 | ✓ |
| 指纹用版本号，不用 refreshToken 哈希 | `credentials_version: u64`，6 处赋值点递增 | ✓ |
| 冷却写入在强刷**之后**，记录新版本号 | `set_profile_arn_cooldown` 在 `refresh_stage().await` 之后调用，且版本号在**同一次锁内**自取 | ✓ |
| 未抢到者直接软放行，不等待 | `AlreadyResolving` → `Err(ProfileArnUnavailable)`，无同步原语 | ✓ |
| RAII guard 覆盖全部退出路径 | `ProfileArnResolveGuard` 以 `let _resolve_guard` 绑定至函数末尾 | ✓ |
| `Instant` + 调用方算 elapsed | `is_cooling(kind, elapsed)`；`Instant` 不序列化 | ✓ |
| 两把锁必须区分，临界区不跨 await | 五个冷却方法全部同步、无 `.await` | ✓ |
| 四条日志 | `rg -c 'tracing::' src/kiro/profile.rs` → 4（原为 0） | ✓ |

**一处实现优于 design 的偏离**：design/tasks 4.3 设想「profile.rs 读版本号 → 写冷却时传入」。
实现改为 `set_profile_arn_cooldown` 在同一次锁内自取当前版本号，消除了
「读版本 → 写记录」之间被其他写入插入的窗口。`credentials_version_of` 因此降为
`#[cfg(test)]`。此偏离不改变任何外部行为，且严格更安全。

## 3. Scenario 覆盖（25 个）

### MODIFIED Requirement: 请求前必须解析 profileArn（8 个 Scenario）

| Scenario | 证据 |
| --- | --- |
| 可信缓存命中 | `test_trusted_arn_short_circuits_before_cooldown`（list/refresh 计数 0） |
| 固定占位 ARN 不可信 | `test_placeholder_arn_not_trusted`（既有，逻辑未改） |
| BuilderId 无可信缓存 | 既有行为未改（decide 未改） |
| IdC 不得为取 ARN 而强刷 | `test_idc_soft_unavailable_writes_no_cooldown`（refresh 计数 0） |
| 每请求不得重复无效强刷（IdC） | 既有 `test_decide_idc_never_force_refreshes` 未修改 |
| Enterprise 动态列表 | `test_list_resolved_clears_cooldown` |
| refresh fallback | `test_refresh_yielding_arn_clears_cooldown` |
| Social **首次**强刷行为不得回归 | `test_cooldown_skips_list_and_refresh` 的第一次解析（list 1 / refresh 1）；既有 `test_decide_social_still_force_refreshes` 未修改 |

### ADDED: 解析失败必须在冷却窗口内被抑制（6 个）

| Scenario | 证据 |
| --- | --- |
| Social 冷却窗口内不再解析 | `test_cooldown_skips_list_and_refresh`（**核心验收项**：list 与 refresh 计数均停在 1） |
| 冷却到期后允许再次尝试 | `test_cooldown_expiry_allows_new_attempt`（回拨 16 分钟 → 计数 2/2） |
| 瞬时故障与确认无 ARN 窗口不同 | `test_transient_window_is_much_shorter_than_no_arn`（30s × 4 < 15min） |
| 冷却状态不落盘 | `test_cooldown_state_not_persisted` |
| 冷却不阻止使用已取得的 ARN | `test_trusted_arn_short_circuits_before_cooldown`、`test_list_resolved_clears_cooldown` |
| IdC 与 API Key 不写冷却 | `test_idc_soft_unavailable_writes_no_cooldown`、`test_api_key_unsupported_writes_no_cooldown` |

### ADDED: 冷却必须随凭据变更失效（3 个）

| Scenario | 证据 |
| --- | --- |
| 强刷自身的 refreshToken 轮换不得使冷却失效 | `test_cooldown_survives_refresh_token_rotation`（**最易写错处**：强刷闭包递增版本号后冷却仍有效） |
| 凭据变更使冷却失效 | `test_credentials_change_invalidates_cooldown`、`test_credentials_change_invalidates_cooldown_in_resolve` |
| Admin 强制刷新语义不变 | `force_refresh_token_for` 仅多出递增一行，无清冷却逻辑（`git diff` 证据） |

### ADDED: 并发解析同一凭据必须去重（2 个）

| Scenario | 证据 |
| --- | --- |
| 并发解析只有一个发起往返 | `test_concurrent_resolve_deduplicates`（第二个任务 list/refresh 计数 0） |
| 解析异常退出不泄漏标记 | `test_resolve_guard_cleared_on_error_return`、`test_resolve_guard_cleared_on_panic`（`catch_unwind`）、`test_marker_cleared_after_hard_error` |

### ADDED: 冷却不得改变刷新失败的错误语义（3 个）

| Scenario | 证据 |
| --- | --- |
| 瞬时失败仍上抛硬错误 | `test_transient_refresh_failure_writes_short_cooldown_and_still_bails`（错误文本形状 + 非 `ProfileArnUnavailable` + 已写短窗口冷却） |
| 非永久失败不得被误判为永久失败 | `test_non_permanent_400_is_transient_not_permanent`（含 `invalid_grant` 文本但类型非 `RefreshTokenInvalidError` → `TransientFailure`） |
| invalid_grant 不进入冷却 | `test_invalid_grant_writes_no_cooldown` |

### ADDED: profileArn 解析必须可观测（3 个）

| Scenario | 证据 |
| --- | --- |
| 强刷原因可追溯 | `info` 日志含凭据 id + 「为取 profileArn」+ 窗口时长（由常量推导，不会脱节） |
| 冷却与并发跳过可见 | 两条 `debug` 日志，冷却情形含原因与剩余秒数 |
| list 失败原因可见 | `debug` 日志含凭据 id + `ListOutcome` + 原始错误 |
| 无机密 | 四条日志参数仅 id / 枚举名 / 错误原因 / 秒数；全文无 token 值 |

## 4. 验证（仅本会话真实运行）

```
cargo build                     → Finished（0 error）
cargo test kiro::profile        → 38 passed / 0 failed
cargo test kiro::token_manager  → 78 passed / 0 failed
cargo test                      → 724 passed / 0 failed
openspec validate --all         → 20 passed / 0 failed
git status --short              → 仅 2 个 M + 2 个 ??，无机密/缓存
```

**SKIPPED**：tasks 9.5（重跑受控实验，补齐三项缺失信息并分别计时 list 与 refresh）。
原因：需真实 Social 凭据与隔离实例，属会话外的手工端到端验证。
剩余风险：proposal Success Criteria 表（强刷 10→1、list 10→1、写盘 10→1）
**未经实测确认**，当前只有单测层面的等价断言（同一凭据第二次解析时两个上游边界
计数不增）。归档前应补做。

## 5. README / AGENTS / spec 同步

| 入口 | 判断 | 理由 |
| --- | --- | --- |
| `README.md` | 无需 | 不改启动、构建、部署、测试、API 入口；无新增配置项或环境变量 |
| `AGENTS.md` | 无需 | 不改 AI 纪律、高风险矩阵、验证命令 |
| `spec/design.md`、`requirements.md`、`structure.md` | 无需 | 模块边界与数据流不变；冷却是 `src/kiro/` 内部实现细节，无新增模块或文件 |
| `openspec/specs/profile-arn-resolution/spec.md` | 归档时同步 | 按 OpenSpec 流程，delta 在 `changes/.../specs/`，归档才合入主规格 |
| `credentials.example.*.json` | 无需 | 冷却状态不落盘 |
| `admin-ui/` | 无需 | `CredentialEntrySnapshot` 未新增字段（已由测试断言） |

## 6. 发现项与剩余风险

| # | 级别 | 内容 | 处置 |
| --- | --- | --- | --- |
| F1 | WARN | tasks 4.2 要求「逐处断言赋值后版本号 +1」的运行时单测，但 6 处赋值点中 5 处紧随 `refresh_token` 的网络调用、第 6 处（upsert）也要求 OAuth 凭据先经网络刷新，**离线单测无法逐一触发**。改用源码级断言 `test_every_credentials_assignment_bumps_version`（扫描自身源码，要求恰好 6 处且每处 2 行内必有递增）+ 递增语义单测 | 已在 tasks 4.2 明示偏离。该手段满足「必须有测试而非目测」的意图（漏改即测试失败），但对赋值点的**运行时**行为无覆盖。漏改后果是「冷却比预期多持续一个窗口」，非数据错误，proposal Risks 表已判为可接受 |
| F2 | WARN | tasks 9.5 端到端实验未跑（见 §4） | 归档前补做；当前 Success Criteria 无实测支撑 |
| F3 | INFO | design/tasks 4.3 设想的「profile.rs 读版本号后传入」被改为「写冷却方法在同一次锁内自取」 | 偏离更安全（消除读写之间的窗口），无外部行为变化，已在 tasks 4.3 记录 |

**无 CRITICAL，无 FAIL 级发现。** 停止条件逐条复核：

- 非授权范围被修改：否（`Cargo.toml`、`Cargo.lock`、`admin-ui/src` 均未动）
- 真实凭据或本地缓存进入提交：否（`git status --porcelain` 对
  `credentials.json|config.json|.codegraph|.env` 无匹配）
- Requirement/Scenario 与实现无法对应：否（25/25 有证据）
- validate 失败或关键验证缺失无说明：否（20/20 通过；唯一 SKIPPED 已明示）

## 证据路径

- Bridge Plan：`openspec/changes/social-profile-arn-cooldown/evidence/bridge-plan.md`
- 本报告：`openspec/changes/social-profile-arn-cooldown/evidence/spec-compliance-report.md`
- 完成前核验：`openspec/changes/social-profile-arn-cooldown/evidence/verification-before-completion.md`
- 问题分析与实验：`docs/social-profile-arn-force-refresh-storm.md`
