# Capability: profile-arn-resolution

## Purpose

Resolve and cache Kiro `profileArn` before upstream generateAssistantResponse / MCP / usage-limits requests when a **trusted** ARN is available, so credentials without a pre-stored ARN can still chat. Known IDE/short-circuit **placeholder** ARNs MUST NOT be treated as successful resolution or unconditionally persisted, because upstream often rejects them with `User is not authorized` while the same token succeeds without profileArn.

## Requirements

### Requirement: 请求前必须解析 profileArn

The system MUST attempt to resolve a profile ARN for supported credential types before calling Kiro generateAssistantResponse (and MCP when applicable), instead of only injecting a pre-existing profile_arn field. Resolution MUST prefer trusted cache, then dynamic list / refresh-derived ARNs. When no trusted ARN is available, the system MAY proceed without profileArn rather than inventing or forcing a known placeholder.

#### Scenario: 可信缓存命中

- GIVEN 凭据已持久化非空 profileArn 且该 ARN 不是已知固定占位值
- WHEN 发起上游对话请求
- THEN 使用缓存 ARN 注入请求体且不强制调用 ListAvailableProfiles

#### Scenario: 固定占位 ARN 不可信

- GIVEN 凭据 profileArn 等于已知 BuilderId/Social 固定占位 ARN
- WHEN resolve / ensure profile 执行
- THEN 不得将该占位 ARN 视为已成功解析；SHOULD 清除持久化占位值；不得仅因存在占位 ARN 而跳过后续真实解析或无 ARN 路径

#### Scenario: BuilderId 无可信缓存

- GIVEN authMethod 为 idc 且 provider 为 BuilderId，profileArn 为空或仅为固定占位
- WHEN resolve_profile_arn 执行且无法从 ListAvailableProfiles / refresh 得到可信 ARN
- THEN MUST NOT 无条件写回固定 BuilderId 占位 ARN；允许以无 profileArn 继续（或返回 soft-unavailable），以便上游在无 ARN 时仍可能成功

#### Scenario: Enterprise 动态列表

- GIVEN 支持动态 profile 的账号类型且无可信缓存、无固定表项
- WHEN resolve_profile_arn 执行且 ListAvailableProfiles 返回至少一个非占位 arn
- THEN 采用首个非空可信 arn 缓存并注入请求

#### Scenario: refresh fallback

- GIVEN ListAvailableProfiles 失败或空列表，但 refresh 响应包含非占位 profileArn
- WHEN resolve_profile_arn 执行
- THEN 使用 refresh 返回的可信 profileArn 缓存并注入请求

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
