# Capability: profile-arn-resolution

## ADDED Requirements

### Requirement: 请求前必须解析 profileArn

The system MUST resolve a profile ARN for supported credential types before calling Kiro generateAssistantResponse (and MCP when applicable), instead of only injecting a pre-existing profile_arn field.

#### Scenario: 缓存命中

- GIVEN 凭据已持久化非空 profileArn
- WHEN 发起上游对话请求
- THEN 使用缓存 ARN 注入请求体且不强制调用 ListAvailableProfiles

#### Scenario: BuilderId 无缓存

- GIVEN authMethod 为 idc 且 provider 为 BuilderId，profileArn 为空
- WHEN resolve_profile_arn 执行
- THEN 使用固定 BuilderId profile ARN 写回凭据并用于后续请求，且不调用 ListAvailableProfiles

#### Scenario: Enterprise 动态列表

- GIVEN 支持动态 profile 的账号类型且无缓存、无固定表项
- WHEN resolve_profile_arn 执行且 ListAvailableProfiles 返回至少一个 arn
- THEN 采用首个非空 arn 缓存并注入请求

#### Scenario: refresh fallback

- GIVEN ListAvailableProfiles 失败或空列表，但 refresh 响应包含 profileArn
- WHEN resolve_profile_arn 执行
- THEN 使用 refresh 返回的 profileArn 缓存并注入请求

### Requirement: usage limits 与对话共享解析语义

getUsageLimits MUST attempt the same resolve path before attaching profileArn query when the credential supports profiles.

#### Scenario: 导入后查余额

- GIVEN 新导入支持型凭据尚无 profileArn 缓存
- WHEN 查询 usage/balance
- THEN 系统先 resolve；成功则 query 带 profileArn；Unsupported 类型允许不带 profileArn 继续

### Requirement: 错误分类不得误杀 refreshToken

When upstream returns bearer-token-invalid and the request lacked a resolved profileArn, the system MUST attempt profile resolution and retry before treating the refresh token as permanently invalid.

#### Scenario: 缺 profile 导致 403

- GIVEN 当前 access token 有效但请求未带 profileArn 且上游返回 bearer invalid
- WHEN provider 处理错误
- THEN 先 resolve profileArn 并重试；仅在已有 profile 或 resolve 后仍失败时按既有 auth 失败策略处理，且不得仅因该 403 标记 InvalidRefreshToken

### Requirement: provider 元数据可持久化

Credentials MUST support an optional provider field used by fixed-ARN and supports_profiles decisions, persisted in multi-credential JSON.

#### Scenario: 序列化 roundtrip

- GIVEN 凭据含 provider=BuilderId
- WHEN 写入再加载 credentials 文件
- THEN provider 与 profileArn 保持不变