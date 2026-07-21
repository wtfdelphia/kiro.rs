# Capability: credential-ingest

## Purpose

Define the unified credential ingest pipeline for Admin add/import: identity fields, refresh gate, user-info enrichment, conflict/upsert rules, and single/batch import APIs.

## Requirements

### Requirement: 统一 ingest 管道为唯一入库路径

The system MUST route Admin credential creation and import through a single internal ingest pipeline that validates shape, refreshes OAuth credentials when applicable, resolves identity, applies conflict rules, and persists only on success.

#### Scenario: OAuth refresh 失败不落盘

- **WHEN** an OAuth credential is submitted with a refreshToken that fails network refresh
- **THEN** the system MUST NOT append or update any entry in credentials storage
- **AND** the API MUST return an invalid_credential (or equivalent client error) without writing secrets to logs

#### Scenario: API Key 跳过 refresh

- **WHEN** a credential is submitted as API Key (kiroApiKey present or authMethod api_key)
- **THEN** the system MUST NOT require refreshToken refresh
- **AND** MUST still validate non-empty kiroApiKey and apply API Key hash dedup rules

### Requirement: 凭据身份元数据字段

Credential model and Admin status MUST support optional userId, nickname, and startUrl fields without breaking load of legacy credentials.json that omit them.

#### Scenario: 旧文件缺字段可加载

- **WHEN** credentials.json entries lack userId, nickname, and startUrl
- **THEN** the system MUST load them successfully with those fields absent/None

#### Scenario: 列表暴露身份字段

- **WHEN** a stored credential has userId and/or nickname
- **THEN** GET credentials status MUST include those fields for Admin UI display (omit when empty)

### Requirement: OAuth 用户信息 enrich（best-effort）

After a successful OAuth refresh during ingest, the system MUST attempt to fetch user info (email and userId) using the new access token.

#### Scenario: GetUserInfo 成功写回

- **WHEN** GetUserInfo succeeds
- **THEN** the persisted credential MUST store returned email and userId unless the request already provided non-empty values (request body takes priority)

#### Scenario: GetUserInfo 失败不阻断入库

- **WHEN** GetUserInfo fails after successful refresh
- **THEN** ingest MUST still persist the credential if other checks pass
- **AND** MUST log a warning without secret material

### Requirement: 冲突与 upsert 策略

Ingest MUST apply conflict rules in this order of identity strength: API Key hash; OAuth userId; OAuth refreshToken hash.

#### Scenario: 默认 reject 重复 refreshToken hash

- **WHEN** onConflict is reject (default for POST /credentials) and an OAuth refreshToken hash matches an existing entry
- **THEN** the system MUST reject the add with a duplicate/already-exists style error

#### Scenario: userId upsert 更新并复活

- **WHEN** onConflict is upsert and the resolved userId matches an existing credential
- **THEN** the system MUST keep the same credential id
- **AND** MUST update tokens and related auth fields from the ingest result
- **AND** MUST clear disabled state / disabled_reason so the credential is usable again

#### Scenario: 无 userId 禁止 silent upsert

- **WHEN** onConflict is upsert but no userId is available after enrich
- **THEN** the system MUST NOT merge by guessing
- **AND** MUST fall back to refreshToken hash reject/insert rules without cross-account merge

#### Scenario: API Key 重复默认不覆盖

- **WHEN** an API Key hash matches an existing entry
- **THEN** the system MUST reject as duplicate by default

### Requirement: POST /credentials 兼容扩展

Existing POST /api/admin/credentials MUST remain the supported single-add entry, running through ingest, and MAY accept optional userId, nickname, startUrl, and onConflict.

#### Scenario: 旧客户端最小 body

- **WHEN** a client posts only refreshToken and authMethod as today
- **THEN** behavior MUST remain create-or-reject-by-hash (default reject) with forced refresh for OAuth

#### Scenario: 响应附加 action

- **WHEN** ingest creates a new credential
- **THEN** response MUST include success and credentialId
- **AND** SHOULD include action=created when the extended response is enabled
- **WHEN** ingest upserts an existing credential
- **THEN** response SHOULD include action=updated and the same credentialId

### Requirement: 单条 import API

The system MUST provide POST /api/admin/credentials/import for import-oriented single ingest with forced OAuth refresh and user-info enrich semantics aligned to Kiro-Go import.

#### Scenario: import 成功返回身份

- **WHEN** import receives a valid refreshToken and refresh succeeds
- **THEN** response MUST report success with credentialId
- **AND** SHOULD include email and userId when available

### Requirement: 批量 import API

The system MUST provide POST /api/admin/credentials/import/batch that ingests multiple items and returns per-item results without requiring the client to orchestrate N independent create calls as the primary path.

#### Scenario: 混合结果汇总

- **WHEN** a batch contains create, update, duplicate, and failed items
- **THEN** the response MUST include per-item status and a summary of counts
- **AND** successful items MUST remain persisted even if later items fail (unless stopOnError is true and documented short-circuit applies only to subsequent items)

#### Scenario: 默认串行

- **WHEN** batch options omit concurrency
- **THEN** the server MUST process with concurrency 1 by default to reduce upstream rate-limit risk

#### Scenario: refresh 失败条目不写盘

- **WHEN** one batch item fails refresh
- **THEN** that item MUST be marked failed
- **AND** MUST NOT create a partial OAuth entry for that item

### Requirement: 密钥与日志安全

Ingest and import APIs MUST NOT return full refreshToken, clientSecret, or kiroApiKey in JSON responses, and MUST NOT write those secrets in clear text to logs.

#### Scenario: 状态接口继续脱敏

- **WHEN** Admin lists credentials after import
- **THEN** secrets remain absent or hashed/masked as in existing status fields
