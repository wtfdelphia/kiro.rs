## Why

`resolve_profile_arn` 的两个 fallback 分支顺序写颠倒了，导致对 IdC/BuilderId 凭据
**每个请求都会额外发起一次注定无效的 AWS OIDC token 往返**。

### 缺陷本体：死分支

`src/kiro/profile.rs:207-215` 有一个明确针对这类账号的软失败分支，注释写着
「List failed / empty: for IdC/BuilderId-style accounts, generate often works without ARN」：

```rust
// profile.rs:186  ← 强刷分支
if credentials.refresh_token.is_some() {
    match token_manager.force_refresh_token_for(credential_id).await { ... }
    // 两条路径都 return
}

// profile.rs:208  ← 账号类型软放行分支（不可达）
if looks_like_idc(credentials) || provider == BuilderId {
    return Err(anyhow!(ProfileArnUnavailable));
}
```

`looks_like_idc`（`profile.rs:54`）要求 `client_id` 与 `client_secret` 同时存在，而
`validate_refresh_token`（`token_manager.rs:72`）说明这类凭据必然带 `refresh_token`
才能工作。因此凡是能命中 208 行判定的凭据，**一定**先在 186 行返回，208 行对 IdC
永久不可达。作者想表达的「这类账号本来就没有 ARN，直接放行」被上游的强刷抢先了。

### 为什么这次强刷是确定无效的

`force_refresh_token_for`（`token_manager.rs:2210`）按 `auth_method` 分流
（`token_manager.rs:136-143`）。IdC 走 AWS SSO OIDC `oidc.{region}.amazonaws.com/token`
（`token_manager.rs:249`），该端点的标准响应不含 `profileArn`，
`IdcRefreshResponse.profile_arn`（`src/kiro/model/token_refresh.rs:45`）对 IdC 基本恒为 `None`。

于是 `profile.rs:196` 返回的 `ProfileArnUnavailable` 与 208 行返回值完全相同，
中间那次 OIDC 往返纯属白做。这不是「可能无效」，是路径上确定无收益。

Social 凭据不同：走 Kiro 自家 `prod.{region}.auth.desktop.kiro.dev/refreshToken`
（`token_manager.rs:158`），`RefreshResponse.profile_arn`（`token_refresh.rs:18`）
可能真的返回值。**Social 的强刷有意义，本 change 不动它。**

### 触发面：五个入口，不止对话

同一条链有五个调用点，每个都会走到 `profile.rs:186`：

| 调用点 | 场景 |
| --- | --- |
| `src/kiro/provider.rs:393` | generateAssistantResponse（每次对话） |
| `src/kiro/provider.rs:178` | MCP / WebSearch |
| `src/kiro/token_manager.rs:1808` | 查余额前 |
| `src/kiro/token_manager.rs:2469` | 刷新模型缓存前 |
| `src/admin/service.rs:734` | Admin 凭据连通性测试 |

另有 2 处在 bearer-invalid 错误恢复分支内二次调用（`provider.rs:298`、`provider.rs:556`），
合计 7 处。后两者由真实 403 触发、每凭据每请求限一次，前移后行为不变。

admin-ui 的余额自动刷新默认间隔 120s（`admin-ui/src/components/dashboard.tsx:32`），
批量强刷 token 按钮亦在此列，因此该缺陷在无对话流量时也会持续触发。

### 可观测症状

日志中每请求稳定出现 `凭据 #N Token 已强制刷新`（`token_manager.rs:2242`，
`force_refresh_token_for` 独占）。注意 `正在刷新 IdC Token...`（`token_manager.rs:235`）
**不能**作为判据——普通过期刷新路径（`try_ensure_token`，`token_manager.rs:1124`）
同样打印它。

### 代价

- 每请求额外一次 OIDC 往返，直接进入请求延迟
- `force_refresh_token_for` 持 `refresh_lock`（`token_manager.rs:2221`），
  该锁与 `try_ensure_token` 共用（`token_manager.rs:1108`），并发请求在此串行化
- 每次强刷成功都 `persist_credentials()`（`token_manager.rs:2238`），
  且 IdC/Social 响应带 `refreshToken` 时会轮换它（`token_manager.rs:310-312`），
  高频下反复重写 `credentials.json`
- 无谓的 OIDC 调用量，有触发上游限流风险（429 分支见 `token_manager.rs:298`）

## What Changes

### 1. 账号类型判定前移到强刷之前

把 `profile.rs:207-215` 的软放行判定移到 `profile.rs:186` 强刷分支**之前**，
并把判定谓词换为「刷新是否走 OIDC」（见 Impact 中的说明）。调整后的决策顺序：

```
trusted cache 命中          → 返回 ARN
        ↓ 未命中
ListAvailableProfiles 成功  → 缓存并返回 ARN
        ↓ 失败/空/占位
刷新走 OIDC 的账号           → 返回 ProfileArnUnavailable（软放行，不强刷）   ← 前移
        ↓ 刷新走 Social
有 refreshToken             → 强刷（Social 可能返回 ARN）
        ↓ 无
按既有规则 bail
```

选择前移判定而非加负缓存：负缓存需给 `CredentialEntry`（`token_manager.rs:405`）
引入新的可变状态，并回答「是否落盘、冷却多久、凭据变更时如何失效」三个问题，
而 IdC 这条路径的收益恒为零，根本不需要「记住失败」——它不该被尝试。
Social 的重试问题是另一回事，见 Non-Goals。

### 2. 抽出可注入的 list 边界（为了能验证第 1 点）

`resolve_profile_arn` 目前把 `ListAvailableProfiles` 的 HTTP 调用直接写在函数体内
（`profile.rs:172` → `list_available_profiles_with_retry` → `profile.rs:289` 发真实请求）。
`profile.rs` 现有 12 个测试全是纯函数（provider 推断、占位 ARN 识别、body 解析、
瞬态错误分类），没有一个覆盖 `resolve_profile_arn` 的决策顺序。

不重构就无法断言「IdC 账号在 list 失败后不得强刷」——这恰是本 change 的核心主张。
故把 list 调用抽为可注入的依赖（函数指针或窄 trait，二选一见 design），
公开 API `resolve_profile_arn` / `ensure_profile_arn_for_request` 签名保持不变，
生产调用点零改动。

## Non-Goals

- **不改 Social 凭据的强刷行为。** Social refresh 可能返回真实 `profileArn`，
  强刷是有效尝试。其「反复重试」问题需要负缓存/冷却，属独立 change。
- **不引入负缓存或冷却计时器**，不给 `CredentialEntry` 增加可变状态或持久化字段。
- **不改 `force_refresh_token_for` 自身语义**（Admin 强制刷新入口依赖其无条件行为，
  `src/admin/service.rs:576`）。
- **不改 `refresh_token` 的分流结果。** 提取 `refresh_routes_to_idc` 是纯重构，
  分流去向逐位不变，不调整 `auth_method` 推断规则。
- **不改 `refresh_lock` 的粒度。** 锁竞争在本 change 后会因调用量下降而缓解，
  但拆分锁属独立的并发优化。
- **不改 `persist_credentials` 的落盘策略**（写放大随强刷减少而自然缓解）。
- **不改另外四个调用点的代码**；它们通过共用 `ensure_profile_arn_for_request` 自动受益。
- **不改 `provider.rs:543` 的 bearer-invalid 错误恢复分支。** 那是另一条强刷路径，
  由真实 403 触发、每凭据每请求最多一次（`force_refreshed` HashSet 去重），行为正确。
- 不改固定占位 ARN 表与占位识别逻辑（`profile.rs:105`）。

## Assumptions

- **AWS SSO OIDC token 端点不返回 `profileArn`。** 依据：AWS SSO OIDC
  `CreateToken` 是标准 OAuth2 端点；`IdcRefreshResponse.profile_arn` 字段标了
  `#[serde(default)]`，属防御性可选。**本 change 不依赖该假设成立于 100% 情形**——
  即使某 IdC 变体返回 ARN，前移后的行为退化为「不带 ARN 继续」，
  与现状 `profile.rs:196` 的最终结果一致，不构成回归。
- 上游 generate 对 IdC/BuilderId 账号在无 `profileArn` 时可成功。这是既有
  `profile-arn-resolution` spec 已确立的前提（「BuilderId 无可信缓存」场景），
  非本 change 新增假设。
- `refresh_token` 现有的 `auth_method` 分流规则（`token_manager.rs:128-143`）正确，
  本 change 只提取不修改。**不再假设** `looks_like_idc || provider==BuilderId`
  等价于「刷新走 OIDC」——该假设已被 Bridge Plan 证伪，见 Impact。
- `infer_provider` 与 `looks_like_idc` 行为不变，本 change 不修改二者。

## Impact

- **代码**：
  - `src/kiro/profile.rs`（fallback 顺序 + 决策边界提取 + 新增测试）
  - `src/kiro/token_manager.rs`（**仅**提取纯函数 `refresh_routes_to_idc`，
    供 `refresh_token` 与 `profile.rs` 共用；不改任何刷新行为，见下）
- **不改**：`src/kiro/provider.rs`、`src/admin/service.rs`
  —— 7 处调用点全部经由未变的公开签名受益。

### 为何必须触碰 token_manager.rs

Bridge Plan 核对 tasks 1.2 时发现，`looks_like_idc || provider == BuilderId`
与「刷新实际走 OIDC」**不是同一个集合**，直接用它做前移判定会引入回归：

| 凭据形态 | `looks_like_idc \|\| provider==BuilderId` | 实际刷新去向 | 后果 |
| --- | --- | --- | --- |
| `authMethod` 缺失 + clientId/Secret 齐全 | true（`infer_provider` 推出 BuilderId） | OIDC | 一致 |
| `provider: "BuilderId"` + `authMethod: "social"` | **true** | **Social** | 会被误软放行，丢掉 Social refresh 本可能返回的 ARN |

第二行是真回归：`infer_provider`（`profile.rs:67-73`）对显式 `provider` 原样返回，
而该凭据的刷新走 Kiro 自有端点，`RefreshResponse.profile_arn`（`token_refresh.rs:18`）
可能真有值。该组合虽自相矛盾，但经 Admin/KAM 导入原样接收 `provider`
（`token_manager.rs:1952-1955`）是可表达的。

因此判定谓词必须精确等于 `refresh_token` 的分流条件（`token_manager.rs:128-143`），
即把该分流提取为 `pub(crate) fn refresh_routes_to_idc(&KiroCredentials) -> bool`
并由两处共用。提取是纯重构：`refresh_token` 改为调用它，分流结果逐位不变。

选择提取而非在 `profile.rs` 复制谓词：分流规则需要单一事实源，
否则将来修改 `auth_method` 推断时容易只改一处而静默漂移。
- **spec**：MODIFIED `profile-arn-resolution`（现有「refresh fallback」场景
  未约束触发顺序，需补齐「IdC 不得强刷」的前置条件）。
- **风险类型**：Token / 多凭据（AGENTS.md 高风险矩阵）。
- **README / AGENTS**：无需同步。不涉及启动、构建、部署、测试入口或 API 端点变化。

## Success Criteria

- IdC/BuilderId 凭据在 `ListAvailableProfiles` 失败或返回空列表后，
  `resolve_profile_arn` 返回 `ProfileArnUnavailable` 且**未**调用 `force_refresh_token_for`。
- Social 凭据在同等条件下仍执行强刷（无行为回归）。
- trusted cache 命中路径不变：既不发 list 也不强刷。
- 新增测试覆盖上述三条决策分支，且在**不联网**的前提下可断言强刷未被调用。
- `profile.rs` 原有 12 个测试全部保持通过。
- `cargo test` 通过（AGENTS.md 高风险矩阵 Token/多凭据项）。
- `openspec validate --all` 通过。

## Risks

- **假设被推翻的场景已被吸收**：若某 IdC 变体确实在 refresh 响应中返回 `profileArn`，
  前移后将不再获取它，该凭据转为无 ARN 请求。影响可控——这与现状
  `profile.rs:196` 的实际结果相同（现状拿到 ARN 也只在 `profile_arn_of` 命中时才用）。
  且 `provider.rs:543` 的 bearer-invalid 分支仍是兜底：真出 403 时会重新 resolve 并强刷。
- **可测试性重构触及热路径**：`resolve_profile_arn` 经 `ensure_profile_arn_for_request`
  被 7 处调用。缓解手段是保持公开签名不变、依赖注入只作用于内部，
  并以 `cargo test` 全量回归确认。
- **测试替身与真实 HTTP 行为漂移**：注入边界后，测试断言的是决策顺序而非真实
  上游语义。`list_available_profiles` 的重试与错误分类仍由既有纯函数测试覆盖
  （`profile.rs:431` `test_transient_classification`），不因本次重构失去覆盖。
- **回滚**：改动集中在单个文件、无数据迁移、无持久化格式变更、无新增状态，
  `git revert` 即可恢复。
