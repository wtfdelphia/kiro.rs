# Capability: credential-online-auth

## Purpose

Admin online authorization flows for adding Kiro accounts (BuilderId device code, IAM SSO, SSO bearer token) that complete into the unified ingest pipeline.

## Requirements

### Requirement: BuilderId 设备码登录

The system MUST support Admin-authenticated BuilderId device authorization start and poll endpoints that create a credential via ingest upon completion.

#### Scenario: start 返回会话与用户码

- **WHEN** Admin calls POST /api/admin/auth/builderid/start with optional region
- **THEN** the system MUST return sessionId, userCode, verificationUri (or equivalent), and polling interval guidance
- **AND** MUST NOT persist a credential yet

#### Scenario: poll pending

- **WHEN** poll is called before the user finishes authorization
- **THEN** the system MUST return completed=false with a pending/slow_down style status
- **AND** MUST NOT create a credential

#### Scenario: poll 完成入库

- **WHEN** authorization completes successfully
- **THEN** the system MUST ingest the resulting tokens (authMethod idc, provider BuilderId unless overridden)
- **AND** MUST return completed=true with credential identity fields (id/email when available)
- **AND** MUST invalidate the session

### Requirement: IAM SSO 登录

The system MUST support Admin-authenticated IAM SSO start and complete endpoints that ingest credentials on success.

#### Scenario: start 需要 startUrl

- **WHEN** start is called without startUrl
- **THEN** the system MUST return invalid_request

#### Scenario: complete 成功入库

- **WHEN** complete receives a valid sessionId and callbackUrl and exchange succeeds
- **THEN** the system MUST ingest tokens via the unified pipeline
- **AND** SHOULD store startUrl on the credential when available

### Requirement: SSO Token 导入

The system MUST support POST /api/admin/auth/sso-token that accepts one or more bearer tokens (newline-separated) and imports accounts through SSO exchange + ingest.

#### Scenario: 多行部分成功

- **WHEN** multiple tokens are submitted and some fail exchange
- **THEN** the response MUST include successful accounts and error list for failures
- **AND** successful imports MUST be persisted

#### Scenario: 全失败

- **WHEN** all tokens fail
- **THEN** the system MUST return a non-success error outcome and MUST NOT claim success

### Requirement: 在线授权会话安全

Online auth sessions MUST be admin-authenticated, short-lived, and removed after completion or expiry.

#### Scenario: 会话过期

- **WHEN** poll/complete is called after TTL expiry
- **THEN** the system MUST reject with not_found or invalid session error
- **AND** MUST NOT leave usable tokens in session storage

#### Scenario: 非 Admin 拒绝

- **WHEN** start/poll/complete/sso-token is called without valid Admin credentials
- **THEN** the system MUST reject with authentication error

### Requirement: 在线授权复用 ingest

Successful online auth MUST NOT bypass conflict/refresh/user-info rules of credential-ingest.

#### Scenario: 完成后走 ingest

- **WHEN** any online auth flow completes with tokens
- **THEN** credential persistence MUST go through ingest_credential (or equivalent shared path)
- **AND** OAuth refresh/userId rules of credential-ingest apply
