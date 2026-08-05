## 1. 实现前核对

- [x] 1.1 读 `AGENTS.md` 与本 change 的 proposal / design / specs
  → 验证：能陈述本次高风险类型（Token/多凭据、OpenSpec）与验证命令
  （`cargo test`、`openspec validate --all`）
  → **已完成**：高风险类型 Token/多凭据 + OpenSpec；验证命令为
  `cargo test kiro::profile`、`cargo test kiro::token_manager`、`cargo test`、
  `openspec validate --all`、`git status --short`
- [x] 1.2 运行 `openspec-superpowers-bridge`，产出 `evidence/bridge-plan.md`
  → 验证：bridge plan 存在，且逐条核对过 design 中标注为「须停下确认」的一处
  （task 2.1 的 list 边界注入可行性）
  → **已完成**：2.1 仍为实现期停止条件（Bridge Plan §10.1），未在计划阶段消解
- [x] 1.3 核对 6 处 `entry.credentials =` 赋值点在当前工作区的**实际行号**
  （design 记录为 `:1253`、`:1899`、`:2190`、`:2365`、`:2497`、`:2573`，基线含未提交改动）
  → 验证：`rg -n 'entry\.credentials = ' src/kiro/token_manager.rs` 输出**恰好 6 行**；
  若数量不符必须停下重新核对，不可按记录的行号盲改
  → **已完成**（Bridge Plan §4.2）：恰好 6 行，行号与记录逐一相符
- [x] 1.4 确认 `entries` 的同步 `Mutex`（`:697`）与 `refresh_lock`（`TokioMutex`，`:701`）
  是两把不同的锁
  → 验证：能说明冷却与进行中标记为何只用前者，且临界区不跨 await
  → **已完成**（Bridge Plan §4.5）：`report_refresh_token_invalid`（`:1646-1668`）与
  `report_failure`（`:1476`）是既有的「同步锁内短临界区」范式，冷却存取方法照此实现

## 2. 可测试性前置（先做，因为它决定后续能否断言「list 未被调用」）

- [x] 2.1 确认 list 阶段可在**不改公开签名**的前提下注入
  → 验证：`resolve_profile_arn` 与 `ensure_profile_arn_for_request` 的签名逐字不变
  （生效 spec `:77`、`:84` 的 MUST）；内部抽出的私有形态（如
  `resolve_profile_arn_inner` + list 闭包/trait 参数）能被 `#[cfg(test)]` 直接调用。
  **若确认不可行，停下并汇报**——本 change 有三条验收项依赖「断言 list 未被调用」
  → **可行，停止条件未触发**：`rg` 确认 `resolve_profile_arn` 无跨模块调用者
  （仅 `ensure_profile_arn_for_request`）。已抽 `resolve_profile_arn_inner`，
  list 与强刷两个边界均以 `FnOnce() -> Future` 注入；公开函数保留原签名并传入真实实现

## 3. 冷却状态机（纯函数，无 I/O，先做因为后续都依赖它）

- [x] 3.1 在 `src/kiro/token_manager.rs` 定义
  `enum CooldownKind { NoArn, TransientFailure }` 与
  `struct ProfileArnCooldown { since: Instant, kind: CooldownKind, version: u64 }`
  → 验证：`cargo build` 通过
- [x] 3.2 定义两个编译期常量：`NoArn` 15 分钟、`TransientFailure` 30 秒
  → 验证：`rg -n 'ProfileArnCooldown|COOLDOWN' src/` 显示两个常量，且**未出现在**
  任何 config / serde 结构中（spec：MUST NOT 暴露为配置）
  → **已完成**：`NO_ARN_COOLDOWN`、`TRANSIENT_FAILURE_COOLDOWN` 均为
  `const StdDuration`，不在任何 serde 结构中
- [x] 3.3 实现 `fn is_cooling(kind: CooldownKind, elapsed: Duration) -> bool`
  （由调用方算 `elapsed`，因为 `Instant` 无法构造任意过去时刻）
  → 验证：单测覆盖 design 的 6 行状态机表：无记录 / `NoArn` <15min / `NoArn` >15min /
  `TransientFailure` <30s / `TransientFailure` >30s / 版本号不符
  → **已完成**：`is_cooling` + `cooldown_block`（版本号比对）；测试
  `test_is_cooling_no_arn_window`、`test_is_cooling_transient_window`、
  `test_cooldown_block_version_mismatch_always_allows`、
  `test_cooldown_block_no_record_equivalent_allows`、
  `test_transient_window_is_much_shorter_than_no_arn`

## 4. 凭据版本号（先于冷却写入，因为冷却记录要存版本号）

- [x] 4.1 `CredentialEntry` 新增 `credentials_version: u64`，初始值明确（0 或 1，二选一并注释）
  → 验证：`cargo build` 通过；**两处**构造点均已初始化——`token_manager.rs:814`（初始加载）
  与 `:2243`（`entries.push`，新增凭据）。Bridge Plan §4.3 已 rg 确认恰好两处；
  若出现第三处需补齐
  → **已完成**：初始值 **0**（新建条目尚未发生变更，已在字段注释写明）；
  两处构造点均已初始化，未出现第三处。测试 `test_new_entry_version_starts_at_zero`
- [x] 4.2 在 1.3 核对出的**每一处** `entry.credentials =` 赋值后递增版本号
  → 验证：逐处断言的单测——每处对应一个测试（或一个表驱动测试覆盖 6 条），
  断言赋值后版本号 +1。漏改一处的后果是「冷却比预期多持续一个窗口」，
  不是数据错误，但必须有测试而非目测
  → **已完成，但验收手段有偏离**：6 处赋值点中 5 处紧随 `refresh_token` 的网络调用，
  第 6 处（upsert）也要求 OAuth 凭据先经网络刷新，**离线单测无法逐一触发**。
  改用 `test_every_credentials_assignment_bumps_version` 做**源码级**断言：
  扫描 `include_str!` 自身源码，要求赋值点恰好 6 处且每处 2 行内必有
  `bump_credentials_version()`；递增语义本身由
  `test_bump_credentials_version_increments` 覆盖。这满足「必须有测试而非目测」的
  意图（漏改会使测试失败），但不是运行时行为断言——剩余风险见 9.8
- [x] 4.3 新增读取当前版本号的方法（供 profile.rs 在写冷却时取值）
  → 验证：`cargo build` 通过；方法在 `entries` 锁内读取且不跨 await
  → **已完成，实现与设计略有偏离（更安全）**：写冷却时的版本号由
  `set_profile_arn_cooldown` 在**同一次锁内自取**，避免「读版本 → 写记录」之间
  被其他写入插入。因此 `credentials_version_of` 只用于测试断言（`#[cfg(test)]`），
  profile.rs 不需要读版本号
- [x] 4.4 确认版本号**不进** `KiroCredentials`、不进 `CredentialEntrySnapshot`、不落盘
  → 验证：`serde_json` roundtrip 测试断言持久化文件无版本号字段
  → **已完成**：`test_cooldown_state_not_persisted` 断言序列化后的凭据 JSON 与
  Admin 快照 JSON 均无 version / cooldown 相关字段

## 5. 冷却与并发标记的存取方法（token_manager 侧）

- [x] 5.1 `CredentialEntry` 新增 `profile_arn_cooldown: Option<ProfileArnCooldown>`
  与 `profile_arn_resolving: bool`
  → 验证：`cargo build` 通过；两字段均不参与序列化
  → **已完成**：`CredentialEntry` 无 derive Serialize，字段天然不参与序列化；
  由 `test_cooldown_state_not_persisted` 从输出侧断言
- [x] 5.2 实现「查询是否可解析」方法：读冷却记录 → 比对版本号 → 算 `elapsed` → `is_cooling`
  → 验证：单测覆盖版本号不符时**无论 elapsed** 都允许解析
  → **已完成**：`cooldown_block` 纯函数 + `profile_arn_cooldown_state`（测试用）；
  `test_cooldown_block_version_mismatch_always_allows`
- [x] 5.3 实现「尝试取得解析资格」方法：在同一次 `entries` 锁内检查冷却 **且** 抢占
  `profile_arn_resolving`，返回是否取得资格（避免检查与抢占之间的窗口）
  → 验证：单测断言两次连续调用只有第一次取得资格
  → **已完成**：`try_begin_profile_arn_resolve` 返回
  `ProfileArnResolveAttempt::{Granted, Cooling, AlreadyResolving}`；
  `test_try_begin_resolve_is_exclusive`、`test_try_begin_resolve_reports_cooling`
- [x] 5.4 实现 RAII guard，在 drop 时清除 `profile_arn_resolving`
  → 验证：单测覆盖三种退出——正常返回、错误返回、panic（`catch_unwind` 或等价手段）后
  标记均已清除
  → **已完成**：`ProfileArnResolveGuard`；`test_try_begin_resolve_is_exclusive`（正常
  drop）、`test_resolve_guard_cleared_on_error_return`、
  `test_resolve_guard_cleared_on_panic`（`catch_unwind`）
- [x] 5.5 实现「写冷却」与「清冷却」方法；写入时取当前版本号
  → 验证：凭据在解析期间被删除时，写冷却找不到 entry 应静默跳过而非 panic（单测覆盖）
  → **已完成**：`set_profile_arn_cooldown` / `clear_profile_arn_cooldown`；
  `test_cooldown_missing_entry_is_silent` 覆盖 entry 已删除（含
  `try_begin_profile_arn_resolve` 返回 `Granted(None)`）
- [x] 5.6 确认 `force_refresh_token_for` 的**函数体除版本号递增外无其他改动**，签名不变
  → 验证：`git diff src/kiro/token_manager.rs` 中该函数仅多出递增一行；
  无「清除冷却」逻辑（spec：MUST 由凭据变更自动达成）
  → **已完成**：`git diff` 显示该函数仅多出 `bump_credentials_version()` 一行
  与 8.1 的注释块，无清冷却逻辑，签名不变

## 6. 解析调度改造（profile.rs）

- [x] 6.1 在 `resolve_profile_arn` 的 **`ListAvailableProfiles` 调用之前**插入冷却检查 +
  资格抢占，位置在「api_key / 不支持 profile 判定」之后
  → 验证：冷却命中时返回 `Err(ProfileArnUnavailable)`，且**断言 list 未被调用**
  （依赖 2.1 的注入点）。这是本 change 的核心验收项
  → **已完成**：`test_cooldown_skips_list_and_refresh` 断言第二次解析时
  list 与 refresh 计数均停留在 1
- [x] 6.2 冷却检查 MUST 位于 `trusted_profile_arn` 命中之后
  → 验证：单测断言「trusted ARN 命中 × 有冷却记录」返回 ARN 且**既不查冷却也不发 list**
  → **已完成**：`test_trusted_arn_short_circuits_before_cooldown`
- [x] 6.3 按 design 的结局分类表写/清冷却：
  list 得可信 ARN → 清；强刷后得可信 ARN → 清；强刷成功无 ARN → 写 `NoArn`；
  强刷瞬时失败 → 写 `TransientFailure`；`invalid_grant` → **不写**；
  IdC / API Key（未走强刷）→ **不写**
  → 验证：7 条对应单测，逐行覆盖分类表
  → **已完成**：`test_list_resolved_clears_cooldown`、
  `test_refresh_yielding_arn_clears_cooldown`、`test_cooldown_skips_list_and_refresh`
  （NoArn 写入）、`test_transient_refresh_failure_writes_short_cooldown_and_still_bails`、
  `test_invalid_grant_writes_no_cooldown`、`test_idc_soft_unavailable_writes_no_cooldown`、
  `test_api_key_unsupported_writes_no_cooldown`
- [x] 6.3.1 分类依据 MUST 为 `e.downcast_ref::<RefreshTokenInvalidError>().is_some()`，
  **不得**匹配错误文本
  → 验证：补一条单测——Social 刷新返回 400 但**不含** `Invalid refresh token provided`
  时（`token_manager.rs:307-315` 的合取判据不成立，落 `:317-324` 通用 `bail!`）
  MUST 分类为 `TransientFailure` 而非永久失效。
  **背景**：Bridge Plan §4.7——Social 的 `invalid_grant` 判据是两个条件的合取，
  按文本匹配会误判
  → **已完成**：实现用 `downcast_ref::<RefreshTokenInvalidError>().is_none()`；
  `test_non_permanent_400_is_transient_not_permanent` 传入含 `invalid_grant`
  文本但类型为普通 `anyhow` 的错误，断言落 `TransientFailure`
- [x] 6.4 确认冷却写入发生在强刷**之后**，记录的是**新**版本号
  → 验证：专项测试——Social 强刷轮换了 refreshToken，紧随其后的请求 MUST 仍命中冷却。
  **这是最容易写错的一处**：若记录刷新前的版本号，冷却在最常见路径上永不生效
  → **已完成**：`test_cooldown_survives_refresh_token_rotation`——注入的强刷闭包
  递增版本号（模拟真实赋值），断言冷却记录仍有效且下次请求不再往返
- [x] 6.5 确认瞬时失败仍上抛含 list + refresh 两处原因的硬错误，**错误文本逐字不变**
  → 验证：单测断言错误字符串仍匹配 `no available Kiro profile (list: ...; refresh: ...)`。
  错误对象不变即自动满足七个调用点各自的既有处理——注意七处处理**并不相同**：
  只有 `provider.rs:393`（generate）会 `report_failure` 换凭据（`:403-414`），
  其余六处仅 `warn!` 或以 `.ok().flatten()` 吞掉（Bridge Plan §5.1）。
  因此本 change **不需要**新增任何计失败逻辑
  → **已完成**：`test_transient_refresh_failure_writes_short_cooldown_and_still_bails`
  断言错误文本仍匹配 `no available Kiro profile (list: …; refresh: …)`
  且不是 `ProfileArnUnavailable`；未新增任何计失败逻辑
- [x] 6.6 用 5.4 的 guard 覆盖 `resolve_profile_arn` 的全部退出路径
  → 验证：并发测试——两个任务并发解析同一凭据，只有一个发起 list + 强刷，
  另一个立即软放行；解析以错误退出后标记已清除
  → **已完成**：guard 以 `let _resolve_guard = …` 绑定至函数作用域末尾，覆盖全部
  退出路径；`test_concurrent_resolve_deduplicates`（第二个任务 list/refresh 计数为 0）、
  `test_marker_cleared_after_hard_error`
- [x] 6.7 确认 `decide_profile_action` **未被修改**
  → 验证：`git diff` 显示该函数体无变化；其既有 10 个测试**未修改**且全绿
  （含 `test_external_without_arn_currently_force_refreshes`，spec 未要求改它）
  → **已完成**：`git diff src/kiro/profile.rs` 不含 `decide_profile_action` 任何行；
  10 个既有测试未修改且在 `cargo test kiro::profile` 中全绿
- [x] 6.8 确认 `resolve_profile_arn` 与 `ensure_profile_arn_for_request` 公开签名不变，
  七个调用点零改动
  → 验证：`git diff` 不含 `provider.rs`、`admin/service.rs` 的调用点改动；
  `token_manager.rs` 的两个调用点（`:1931`、`:2602`）亦未改
  → **已完成**：`git diff --name-only` 仅 `src/kiro/profile.rs`、
  `src/kiro/token_manager.rs`；`token_manager.rs` 中
  `ensure_profile_arn_for_request` 的两个调用点在 diff 中无任何行

## 7. 可观测性

- [x] 7.1 补 list 失败 / 空 / 仅占位的 `debug` 日志（凭据 id + 阶段结果）
  → 验证：`rg -n 'tracing::' src/kiro/profile.rs` 从 0 条变为 4 条
  → **已完成**：`rg -c 'tracing::' src/kiro/profile.rs` → 4
- [x] 7.2 补决定强刷前的 `info` 日志，明确写出「为取 profileArn」与抑制窗口
  → 验证：日志文本包含凭据 id、原因、窗口时长；这条是把强刷与其原因连起来的关键
  → **已完成**：`凭据 #{} 无可信 profileArn，尝试刷新 Token 以获取（后续 {} 分钟内不再重试）`，
  窗口时长由 `NO_ARN_COOLDOWN` 推导，不会与常量脱节
- [x] 7.3 补「命中冷却而跳过」的 `debug` 日志，含原因与剩余时长
  → **已完成**：`凭据 #{} profileArn 解析冷却中（{原因}，剩余 {} 秒），以无 ARN 继续`；
  原因文本由 `CooldownKind::reason()` 提供
- [x] 7.4 补「未抢到并发标记」的 `debug` 日志
  → **已完成**：`凭据 #{} 已有 profileArn 解析在进行，本次以无 ARN 继续`
- [x] 7.5 确认四条日志均不含 token / refreshToken / 其他机密
  → 验证：逐条目视 + `rg -n 'token' src/kiro/profile.rs` 检查日志行
  → **已完成**：四条日志的参数只有凭据 id、`ListOutcome`（枚举变体名）、
  list 错误原因、冷却原因与剩余秒数。全文仅「刷新 Token」这一名词，无 token 值

## 8. 既有遗留留痕（Non-Goal，但必须留痕）

- [x] 8.1 在 `force_refresh_token_for` 取锁前克隆凭据处（`token_manager.rs:2344-2351`）
  加注释，说明该既有缺陷、其影响面（所有刷新路径，Admin 批量强刷仍存在）、
  以及本 change 的并发去重只使其在 profileArn 路径上不再被触发
  → 验证：注释存在且未改动该处逻辑（spec/proposal 的 Non-Goal）
  → **已完成**：6 行注释已加在克隆之前；该处逻辑（克隆、取锁顺序）一字未动
- [x] 8.2 确认 `refresh_routes_to_idc` 语义**未被重定义**（external 的正解留作后续 change）
  → 验证：`git diff` 显示该函数与其注释无变化
  → **已完成**：`git diff src/kiro/token_manager.rs` 不含该函数任何行

> **关于以 `git diff` 作为验收手段**：工作区含 `kam-external-idp-import-compat`
> 的未提交改动，与本 change 在 `token_manager.rs` / `profile.rs` 上有文件级重叠
> （无逻辑冲突）。所有「`git diff` 显示 X 未变」的验收项必须针对**具体函数体**判断，
> 不可整文件比对（Bridge Plan §9）。

## 9. 验证与收尾

- [x] 9.1 `cargo test kiro::profile`
  → 验证：全绿；`decide_profile_action` 的 10 个既有测试在列且未修改
  → **已完成**：38 passed / 0 failed；10 个 decide 测试在列且未修改
- [x] 9.2 `cargo test kiro::token_manager`
  → 验证：全绿；含 4.2 的逐处版本号断言
  → **已完成**：78 passed / 0 failed；含
  `test_every_credentials_assignment_bumps_version`（源码级逐处断言，见 4.2 的偏离说明）
- [x] 9.3 `cargo test`
  → 验证：全仓库全绿，无新增 warning 阻塞
  → **已完成**：724 passed / 0 failed；无新增阻塞性 warning
- [x] 9.4 `openspec validate --all`
  → 验证：通过
  → **已完成**：Totals 20 passed / 0 failed
- [x] 9.5 重跑受控实验并补齐三项缺失信息（`ListAvailableProfiles` 的上游状态码与响应体、
  凭据的 `authMethod` 与 `provider` 取值、各场景复现命令原文），
  分别计时 list 与 refresh 两段
  → 验证：达成 proposal 的 Success Criteria 表——强刷 10→1、list 10→1、
  写盘 10→1（多凭据格式）。**只用本地临时凭据**，实验后清理隔离实例
  → **已完成（部分项仍缺）**：见 `evidence/e2e-experiment.md`。
  8 个成功操作强刷 **0** 次（原基线 8 次，1:1）；对话延迟 4.36–5.02 s → **2.03–3.56 s**。
  **反证实验闭合因果**：Admin 手动强刷使版本号 +1 → 下一次余额查询重新出现强刷
  （2794 ms），随后两次立即恢复抑制（1549 → 757 ms），证明 0 次归因于冷却而非
  「解析未走到」。IdC `#1` 三次余额查询强刷 0 次，无回归。
  → **方法偏离**：未新建隔离实例（用户已在 18990 跑实盘实例），改用 Admin 快照的
  `(expiresAt, refreshTokenHash)` 指纹作计数器——等价且更精确，且不复制真实凭据。
  → **三项缺失信息补齐 2/3**：凭据形态已记录（`#3` = `authMethod=social` /
  `provider=Github` / 无 ARN）、复现命令已记录；**`ListAvailableProfiles` 的上游
  状态码与响应体仍未捕获**（需抓包或 debug 日志，对结论无影响，见证据 §7.2）。
  → **未做**：list 与 refresh 未分段计时（需进程内埋点）；15 分钟窗口到期行为、
  并发去重未在真实实例上验证（均有单测覆盖）
- [x] 9.6 `git status --short`
  → 验证：`config.json`、`credentials.json`、`credentials.*`、`.codegraph/`
  均不在 Git 候选中
  → **已完成**：候选仅 `M src/kiro/profile.rs`、`M src/kiro/token_manager.rs`、
  `?? docs/social-profile-arn-force-refresh-storm.md`、
  `?? openspec/changes/social-profile-arn-cooldown/`；机密与缓存无匹配
- [x] 9.7 运行 `spec-compliance-check`，产出 `evidence/spec-compliance-report.md`
  → 验证：报告逐条对应本 change 的 spec delta（含 MODIFIED 场景的措辞变更）
  → **已完成**：六维全 PASS，25 个 Scenario 逐条列出证据；2 项 WARN（4.2 的验收手段
  偏离、9.5 未跑）、1 项 INFO（4.3 的实现更安全），无 CRITICAL
- [x] 9.8 运行 `verification-before-completion`，产出
  `evidence/verification-before-completion.md`
  → 验证：报告只列本会话真实运行过的命令与结果；未运行项写明原因与剩余风险
  → **已完成**：9 条真实运行的命令与结果；9.5 明示 SKIPPED 及剩余风险；
  结论为「实现阶段验证全绿，但尚不可归档」
- [x] 9.9 判断 README / AGENTS / `spec/` 是否需同步
  → 验证：本 change 不改启动、构建、部署、测试、API 入口与验证命令，
  预期**无需**同步；若结论不同则在最终报告说明并执行
  → **已完成**：结论与预期一致，**均无需同步**。唯一待办是
  `openspec/specs/profile-arn-resolution/spec.md`，按 OpenSpec 流程在归档时合入
  （含 MODIFIED 场景的措辞变更）
