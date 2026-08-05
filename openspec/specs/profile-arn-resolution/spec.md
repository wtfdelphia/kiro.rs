# Capability: profile-arn-resolution

## Purpose

Resolve and cache Kiro `profileArn` before upstream generateAssistantResponse / MCP / usage-limits requests when a **trusted** ARN is available, so credentials without a pre-stored ARN can still chat. Known IDE/short-circuit **placeholder** ARNs MUST NOT be treated as successful resolution or unconditionally persisted, because upstream often rejects them with `User is not authorized` while the same token succeeds without profileArn.

## Requirements

### Requirement: 请求前必须解析 profileArn

The system MUST attempt to resolve a profile ARN for supported credential types before calling Kiro generateAssistantResponse (and MCP when applicable), instead of only injecting a pre-existing profile_arn field. Resolution MUST prefer trusted cache, then dynamic list / refresh-derived ARNs. When no trusted ARN is available, the system MAY proceed without profileArn rather than inventing or forcing a known placeholder.

Resolution MUST NOT perform an upstream call whose outcome cannot affect the result. Specifically, when the credential's account type routes token refresh to an endpoint that does not return `profileArn`, a refresh MUST NOT be issued for the purpose of obtaining one: the account-type soft-unavailable decision MUST be evaluated **before** the refresh fallback, not after it. Ordering these the other way makes the soft-unavailable branch unreachable for such credentials and adds a guaranteed-useless round trip to every request that resolves a profile ARN.

The same prohibition MUST extend to **repeating** a resolution attempt that has already been observed to fail. When a credential whose refresh endpoint *may* return a profileArn has just completed a full resolution attempt without obtaining a trusted ARN, that outcome is evidence that further attempts are useless for as long as the credential is unchanged. Repeating the attempt on every request is therefore the same defect as the ordering defect above, only reached by a different path: measured at one `ListAvailableProfiles` round trip plus one forced token refresh (0.87–1.23 s) per request, with the refresh additionally rotating the refreshToken and rewriting credentials storage. The system MUST suppress such repeats for a bounded cooldown window, and the suppression MUST cover **both** the list call and the refresh, because the list call is issued unconditionally before the decision is made and costs a full TLS handshake plus up to three retries.

Suppression MUST NOT alter what the current request returns when resolution fails hard: the cooldown constrains **subsequent** requests' round trips, not this request's error.

#### Scenario: 可信缓存命中

- GIVEN 凭据已持久化非空 profileArn 且该 ARN 不是已知固定占位值
- WHEN 发起上游对话请求
- THEN 使用缓存 ARN 注入请求体且不强制调用 ListAvailableProfiles
- AND MUST NOT 触发 token 强制刷新

#### Scenario: 固定占位 ARN 不可信

- GIVEN 凭据 profileArn 等于已知 BuilderId/Social 固定占位 ARN
- WHEN resolve / ensure profile 执行
- THEN 不得将该占位 ARN 视为已成功解析；SHOULD 清除持久化占位值；不得仅因存在占位 ARN 而跳过后续真实解析或无 ARN 路径

#### Scenario: BuilderId 无可信缓存

- GIVEN authMethod 为 idc 且 provider 为 BuilderId，profileArn 为空或仅为固定占位
- WHEN resolve_profile_arn 执行且无法从 ListAvailableProfiles 得到可信 ARN
- THEN MUST NOT 无条件写回固定 BuilderId 占位 ARN；允许以无 profileArn 继续（或返回 soft-unavailable），以便上游在无 ARN 时仍可能成功

#### Scenario: IdC 账号不得为取 ARN 而强制刷新 token

- GIVEN 凭据为 IdC 形态（authMethod ∈ {idc, builder-id, iam}，或同时具备 clientId 与 clientSecret 而被推断为 BuilderId），且持有 refreshToken
- AND 该账号类型的 token 刷新走 AWS SSO OIDC token 端点，其响应不含 profileArn
- WHEN ListAvailableProfiles 失败、返回空列表或仅返回已知占位 ARN
- THEN 系统 MUST 直接判定为 soft-unavailable 并以无 profileArn 继续
- AND MUST NOT 为获取 profileArn 而调用强制刷新
- AND 该判定 MUST 先于 refresh fallback 求值，使其对该类凭据可达

#### Scenario: 每请求不得重复无效强刷

- GIVEN 某 IdC 凭据始终无法获得可信 profileArn
- WHEN 连续多个请求（对话、MCP、余额查询、模型缓存刷新或 Admin 连通性测试）先后执行 profileArn 解析
- THEN 其中 MUST NOT 有任何一次为获取 profileArn 而发起 token 强制刷新
- AND 每个请求 MUST 各自以无 profileArn 正常继续

#### Scenario: Enterprise 动态列表

- GIVEN 支持动态 profile 的账号类型且无可信缓存、无固定表项
- WHEN resolve_profile_arn 执行且 ListAvailableProfiles 返回至少一个非占位 arn
- THEN 采用首个非空可信 arn 缓存并注入请求

#### Scenario: refresh fallback

- GIVEN ListAvailableProfiles 失败或空列表
- AND 凭据不属于「刷新端点不返回 profileArn」的账号类型（即 Social 等刷新走 Kiro 自有 refreshToken 端点者），且持有 refreshToken
- WHEN resolve_profile_arn 执行且 refresh 响应包含非占位 profileArn
- THEN 使用 refresh 返回的可信 profileArn 缓存并注入请求

#### Scenario: Social 首次强刷行为不得回归

- GIVEN 凭据为 Social 形态且持有 refreshToken，无可信缓存
- AND 该凭据在本进程内**无未过期的 ARN 解析冷却记录**（首次尝试，或冷却已到期，或凭据自记录后已变更）
- WHEN ListAvailableProfiles 失败、返回空列表或仅返回占位 ARN
- THEN 系统 MUST 执行强制刷新以尝试取得 profileArn
- AND 刷新成功但仍无可信 ARN 时返回 soft-unavailable
- AND 刷新失败时 MUST 保留同时包含 list 与 refresh 两处失败原因的错误信息

### Requirement: profileArn 解析决策必须可在离线条件下验证

The profile ARN resolution decision — which of cache hit, list result, account-type soft-unavailable, refresh fallback, or hard failure applies — MUST be expressible and testable without performing network I/O.

Embedding the ListAvailableProfiles HTTP call inline in the resolution function makes the decision order unassertable in tests: a change that reorders or removes a branch cannot be caught by any offline test. The decision MUST therefore be separable from its side effects, with the list stage's outcome supplied as data.

Public resolution entry points MUST keep their existing signatures so that call sites are unaffected.

#### Scenario: 决策与副作用分离

- WHEN 实现 profileArn 解析
- THEN list 阶段的结果 MUST 可作为数据（成功并带 ARN / 空列表 / 占位 / 失败）传入决策逻辑
- AND 决策逻辑 MUST 可在无网络访问、无真实凭据的条件下被单元测试直接断言
- AND `resolve_profile_arn` 与 `ensure_profile_arn_for_request` 的公开签名 MUST 保持不变

#### Scenario: 账号类型与 list 结果的组合可断言

- GIVEN 一组凭据形态（api_key / 不支持 profile / IdC / BuilderId / Social）与一组 list 结果（带可信 ARN / 空 / 占位 / 失败）
- WHEN 对其组合执行决策逻辑
- THEN 每个组合 MUST 产出确定且可断言的动作（使用该 ARN / 不支持 / soft-unavailable / 强刷 / 失败）
- AND 「IdC × list 未得可信 ARN」的全部组合 MUST 断言为 soft-unavailable 而非强刷

### Requirement: profileArn 解析失败必须在冷却窗口内被抑制

For credential types whose refresh endpoint may return a profileArn (Social and external_idp — i.e. those not covered by the account-type soft-unavailable decision), the system MUST record the outcome of a completed resolution attempt in memory per credential, and MUST skip further resolution attempts for that credential while the recorded cooldown is in effect.

Suppression MUST cover the entire resolution attempt — both `ListAvailableProfiles` and the forced refresh. Suppressing only the refresh is insufficient: the list call is issued unconditionally before the decision is made, so a refresh-only suppression still leaves one full TLS handshake (plus up to three retries with backoff) on every request.

Cooldown state MUST NOT be persisted to credentials storage. It is a runtime optimization, not a fact about the credential; persisting it would introduce an invalidation problem with no corresponding benefit, since re-attempting once per process start is acceptable.

Cooldown windows MUST be distinguished by failure kind, because a transient network fault and a confirmed absence of any usable ARN recover on entirely different timescales. Window lengths MUST be compile-time constants and MUST NOT be exposed as configuration.

#### Scenario: Social 冷却窗口内不再解析

- GIVEN 某 Social 凭据刚完成一次解析：list 未得可信 ARN，强刷成功但仍无可信 ARN
- AND 该凭据未发生任何变更
- WHEN 后续请求（对话、MCP、余额查询、模型缓存刷新或 Admin 连通性测试）在冷却窗口内执行 profileArn 解析
- THEN MUST NOT 为获取 profileArn 而发起强制刷新
- AND MUST NOT 调用 ListAvailableProfiles
- AND 每个请求 MUST 各自以无 profileArn 正常继续

#### Scenario: 冷却到期后允许再次尝试

- GIVEN 某 Social 凭据存在「已确认无可用 ARN」的冷却记录
- WHEN 冷却窗口已过期后再次执行 profileArn 解析
- THEN MUST 允许完整解析（list 与必要时的强刷）
- AND 结果 MUST 按同样规则重新写入冷却记录

#### Scenario: 瞬时故障与确认无 ARN 的窗口不同

- GIVEN 一次解析因网络错误、429 或 5xx 而未能完成
- WHEN 记录冷却
- THEN 该记录 MUST 使用明显短于「确认无可用 ARN」的窗口
- AND 一次网络抖动 MUST NOT 导致长时间抑制解析

#### Scenario: 冷却状态不落盘

- GIVEN 系统已为若干凭据写入 ARN 解析冷却记录
- WHEN 检查 credentials 持久化文件
- THEN 文件中 MUST NOT 出现任何冷却相关字段
- AND 进程重启后 MUST 允许每个凭据重新尝试一次解析

#### Scenario: 冷却不阻止使用已取得的 ARN

- GIVEN 某凭据存在未过期的冷却记录
- AND 该凭据随后获得了可信 profileArn（本地缓存已存在，或 list 本次返回）
- WHEN 执行 profileArn 解析
- THEN MUST 使用该可信 ARN
- AND MUST 清除该凭据的冷却记录

#### Scenario: IdC 与 API Key 不写冷却

- GIVEN 凭据为 IdC 形态或 API Key 形态
- WHEN profileArn 解析执行且未得可信 ARN
- THEN 判定 MUST 与冷却机制引入前逐位相同（soft-unavailable / unsupported）
- AND MUST NOT 写入任何冷却记录（该类凭据本就不走强刷，冷却对其无意义）

### Requirement: 冷却必须随凭据变更失效

Cooldown records MUST be invalidated when the credential itself changes, so that re-import, Admin upsert, expiry-driven refresh, and Admin manual force-refresh all allow an immediate new resolution attempt.

Invalidation MUST NOT be derived from the refreshToken value. The Kiro-owned refresh endpoint rotates the refreshToken as part of the very refresh that the cooldown is meant to suppress, so a refreshToken-derived fingerprint would differ on the next request and defeat the cooldown on the most common path — silently, since the resulting behavior is indistinguishable from having no cooldown at all. Invalidation MUST instead be based on a change counter asserted by whoever writes the credential, so that "the credential changed" is a declared fact rather than something inferred from token values (which cannot distinguish an external edit from the refresh just performed).

#### Scenario: 强刷自身的 refreshToken 轮换不得使冷却失效

- GIVEN 某 Social 凭据的强刷成功并返回了新的 refreshToken，但未返回 profileArn
- WHEN 该次解析完成并写入冷却记录
- THEN 记录 MUST 对应刷新**之后**的凭据状态
- AND 紧随其后的请求 MUST 命中冷却而不再发起 list 或强刷

#### Scenario: 凭据变更使冷却失效

- GIVEN 某凭据存在未过期的冷却记录
- WHEN 该凭据因重新导入、Admin upsert、过期刷新或 Admin 手动强制刷新而被写入新内容
- THEN 下一次 profileArn 解析 MUST 允许完整尝试

#### Scenario: Admin 强制刷新语义不变

- WHEN Admin 调用强制刷新接口
- THEN 该接口 MUST 保持无条件刷新并持久化的既有语义与签名
- AND 冷却失效 MUST 由凭据变更自动达成，而不依赖该接口内的显式清除逻辑

### Requirement: 并发解析同一凭据必须去重

When multiple requests concurrently resolve a profile ARN for the same credential, at most one MUST perform the resolution attempt. The others MUST proceed without a profileArn.

Without this, the cooldown check and the resulting action are not atomic: N concurrent requests can all read "not cooling", all issue a list plus a forced refresh, and serialize on the refresh lock — making the worst case after the change identical to before it. This path is additionally hazardous because the forced-refresh implementation clones the credential before acquiring the refresh lock, so queued tasks carry a refreshToken that the preceding refresh has already rotated away.

The in-progress marker MUST be cleared on every exit path, including error returns and unwinding. A leaked marker would permanently stop ARN resolution for that credential within the process, which is worse than the defect being fixed.

#### Scenario: 并发解析只有一个发起往返

- GIVEN 两个或更多请求同时为同一条无可信 ARN 的凭据解析 profileArn
- WHEN 解析执行
- THEN 其中 MUST 只有一个发起 ListAvailableProfiles 与强制刷新
- AND 其余 MUST 立即以无 profileArn 继续，不等待、不发起任何上游往返

#### Scenario: 解析异常退出不泄漏标记

- GIVEN 一个请求已取得某凭据的解析资格
- WHEN 该解析以错误返回、提前 return 或 panic 结束
- THEN 进行中标记 MUST 已被清除
- AND 后续请求 MUST 能正常取得解析资格

### Requirement: 冷却不得改变刷新失败的错误语义

Recording a cooldown and deciding what the current request returns MUST remain independent. A resolution attempt that fails hard MUST return the same error object it returns today, so that every call site's existing handling — whether it counts a failure and switches credentials, or merely logs and proceeds — is reached unchanged.

Softening a hard refresh failure into a soft-unavailable would silently change that handling: on the conversation path, an expired refreshToken would stop being counted as failed and stop leading to a disable, turning a hard fault into a silent one. Permanent authentication failures MUST additionally be excluded from the cooldown entirely: such credentials are disabled immediately and are no longer selected, so a cooldown is meaningless, and if an operator re-enables one it MUST be retried at once rather than after the cooldown window.

Classification of a refresh failure MUST be based on the error's type, not on matching its message text, so that a failure which is not a permanent authentication failure cannot be mistaken for one (or vice versa) as upstream wording changes.

#### Scenario: 瞬时失败仍上抛硬错误

- GIVEN profileArn 解析的强刷因网络错误、429 或 5xx 失败
- WHEN 该次解析结束
- THEN 系统 MUST 上抛同时包含 list 与 refresh 两处失败原因的错误（与冷却机制引入前逐字相同）
- AND 各调用点对该错误的既有处理 MUST 保持不变（计失败换凭据者仍计失败，仅记日志者仍仅记日志）
- AND MUST 同时写入短窗口冷却记录以抑制后续请求的重复往返

#### Scenario: 非永久失败不得被误判为永久失败

- GIVEN 强刷失败但该错误不属于永久认证失效类型（例如 400 但不含永久失效判据）
- WHEN 系统对该失败分类以决定是否写冷却
- THEN 分类 MUST 依据错误类型而非错误文本匹配
- AND 该失败 MUST 写入短窗口冷却记录，而非被当作永久认证失效处理

#### Scenario: invalid_grant 不进入冷却

- GIVEN profileArn 解析的强刷因 invalid_grant（refreshToken 永久失效）而失败
- WHEN 该次解析结束
- THEN MUST NOT 写入任何冷却记录
- AND MUST 继续走既有的 refreshToken 失效禁用策略

### Requirement: profileArn 解析必须可观测

The resolution path MUST emit enough log detail for an operator to connect an observed forced token refresh to profileArn resolution as its cause, and to see why the list stage failed.

Today the resolution module emits no log lines at all, so an operator sees only repeated refresh logs from the token manager with nothing linking them to profileArn resolution — this is the direct reason the behavior appears inexplicable. Logs MUST NOT include tokens or other secrets.

#### Scenario: 强刷原因可追溯

- WHEN 系统为获取 profileArn 而决定发起强制刷新
- THEN MUST 记录一条包含凭据 id、该动作原因为 profileArn 解析、以及后续抑制窗口的日志
- AND MUST NOT 包含 token、refreshToken 或其他机密

#### Scenario: 冷却与并发跳过可见

- WHEN 某次解析因命中冷却或因已有解析在进行而被跳过
- THEN MUST 记录一条日志，说明凭据 id、跳过原因，冷却情形下并说明剩余时长

#### Scenario: list 失败原因可见

- WHEN ListAvailableProfiles 失败、返回空列表或仅返回占位 ARN
- THEN MUST 记录一条包含凭据 id 与该阶段结果的日志，使冷却不掩盖持续失败

### Requirement: usage limits 与对话共享解析语义

getUsageLimits MUST attempt the same resolve path before attaching profileArn query when the credential supports profiles.

#### Scenario: 导入后查余额

- GIVEN 新导入支持型凭据尚无 profileArn 缓存
- WHEN 查询 usage/balance
- THEN 系统先 resolve；成功则 query 带 profileArn；Unsupported 类型或仅有占位/无可信 ARN 时允许不带 profileArn 继续

### Requirement: 错误分类不得误杀 refreshToken

When upstream returns bearer-token-invalid and the request lacked a resolved profileArn, the system MUST attempt profile resolution and retry before treating the refresh token as permanently invalid.

#### Scenario: 缺 profile 导致 403

- GIVEN 当前 access token 有效但请求未带 profileArn 且上游返回 bearer invalid
- WHEN provider 处理错误
- THEN 先 resolve profileArn 并重试；仅在已有可信 profile 或 resolve 后仍失败时按既有 auth 失败策略处理，且不得仅因该 403 标记 InvalidRefreshToken

### Requirement: 坏 profileArn 导致未授权时必须可恢复

When ListAvailableModels or generate returns 403 / `User is not authorized` for a request that included a profileArn (including known placeholders), the system MUST clear or skip that ARN and retry without it at least once before treating the credential as permanently unauthorized solely for that reason.

#### Scenario: ListAvailableModels 去 ARN 重试

- GIVEN 请求带 profileArn 且上游对该 ARN 返回 403 unauthorized，但对无 ARN 返回 200
- WHEN 模型目录刷新执行
- THEN 系统无 ARN 重试成功；SHOULD 清除本地坏/占位 profileArn

#### Scenario: generate/test 去 ARN 重试

- GIVEN 请求体注入了 profileArn 且上游返回 User is not authorized
- WHEN provider 或 Admin test generate 处理错误
- THEN 清除该 profileArn 后重试；在账号健康时 test /v1/messages 可成功

### Requirement: provider 元数据可持久化

Credentials MUST support an optional provider field used by supports_profiles and related decisions, persisted in multi-credential JSON.

#### Scenario: 序列化 roundtrip

- GIVEN 凭据含 provider=BuilderId
- WHEN 写入再加载 credentials 文件
- THEN provider 保持不变；若 profileArn 为已知占位值则不得再被当作可信缓存强制注入
