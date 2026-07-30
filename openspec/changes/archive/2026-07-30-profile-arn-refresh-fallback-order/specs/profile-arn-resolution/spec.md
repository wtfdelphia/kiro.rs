## MODIFIED Requirements

### Requirement: 请求前必须解析 profileArn

The system MUST attempt to resolve a profile ARN for supported credential types before calling Kiro generateAssistantResponse (and MCP when applicable), instead of only injecting a pre-existing profile_arn field. Resolution MUST prefer trusted cache, then dynamic list / refresh-derived ARNs. When no trusted ARN is available, the system MAY proceed without profileArn rather than inventing or forcing a known placeholder.

Resolution MUST NOT perform an upstream call whose outcome cannot affect the result. Specifically, when the credential's account type routes token refresh to an endpoint that does not return `profileArn`, a refresh MUST NOT be issued for the purpose of obtaining one: the account-type soft-unavailable decision MUST be evaluated **before** the refresh fallback, not after it. Ordering these the other way makes the soft-unavailable branch unreachable for such credentials and adds a guaranteed-useless round trip to every request that resolves a profile ARN.

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

#### Scenario: Social 强刷行为不得回归

- GIVEN 凭据为 Social 形态且持有 refreshToken，无可信缓存
- WHEN ListAvailableProfiles 失败、返回空列表或仅返回占位 ARN
- THEN 系统 MUST 仍执行强制刷新以尝试取得 profileArn
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
