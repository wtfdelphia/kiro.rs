## MODIFIED Requirements

### Requirement: 统一 ingest 管道为唯一入库路径

The system MUST route Admin credential creation and import through a single internal ingest pipeline that validates shape, refreshes OAuth credentials when applicable, resolves identity, applies conflict rules, and persists only on success.

Shape validation MUST be authentication-family aware. Required fields per family:

| family | required | optional |
| --- | --- | --- |
| `social` | `refreshToken` | `profileArn`, `machineId` |
| `idc` | `refreshToken`, `clientId`, `clientSecret` | `startUrl` (expected for Enterprise) |
| `external_idp` | `refreshToken`, `clientId`, and one of `tokenEndpoint` / `issuerUrl` | `clientSecret`, `scopes` |
| `api_key` | non-empty API key | — |

`external_idp` MUST NOT require `clientSecret`, because public clients legitimately have none. Requiring it forces operators to fabricate a secret, which both fails upstream and pollutes stored credentials.

#### Scenario: OAuth refresh 失败不落盘

- **WHEN** an OAuth credential is submitted with a refreshToken that fails network refresh
- **THEN** the system MUST NOT append or update any entry in credentials storage
- **AND** the API MUST return an invalid_credential (or equivalent client error) without writing secrets to logs

#### Scenario: API Key 跳过 refresh

- **WHEN** a credential is submitted as API Key (kiroApiKey present or authMethod api_key)
- **THEN** the system MUST NOT require refreshToken refresh
- **AND** MUST still validate non-empty kiroApiKey and apply API Key hash dedup rules

#### Scenario: external 公共客户端通过校验

- **GIVEN** 一条 external_idp 提交带 `refreshToken`、`clientId` 与合法 `tokenEndpoint`，无 `clientSecret`
- **WHEN** ingest 执行形状校验
- **THEN** 校验 MUST 通过
- **AND** MUST NOT 因缺少 `clientSecret` 而拒绝

#### Scenario: external 缺 endpoint 与 issuer 被拒

- **GIVEN** 一条 external_idp 提交既无 `tokenEndpoint` 也无 `issuerUrl`
- **WHEN** ingest 执行形状校验
- **THEN** MUST 拒绝并说明 external 需要其中之一

#### Scenario: external 刷新前必须校验 endpoint

- **WHEN** ingest 对 external_idp 凭据执行强制刷新
- **THEN** endpoint MUST 先通过白名单校验
- **AND** 校验失败 MUST NOT 发起任何出站请求

### Requirement: 凭据身份元数据字段

Credential model and Admin status MUST support optional userId, nickname, and startUrl fields without breaking load of legacy credentials.json that omit them.

The same backward-compatibility guarantee MUST hold for external endpoint metadata fields (`tokenEndpoint`, `issuerUrl`, `scopes`): adding them MUST NOT break loading of credential files that omit them.

#### Scenario: 旧文件缺字段可加载

- **WHEN** credentials.json entries lack userId, nickname, and startUrl
- **THEN** the system MUST load them successfully with those fields absent/None

#### Scenario: 列表暴露身份字段

- **WHEN** a stored credential has userId and/or nickname
- **THEN** GET credentials status MUST include those fields for Admin UI display (omit when empty)

#### Scenario: 旧文件缺 external 字段可加载

- **WHEN** credentials.json entries lack tokenEndpoint, issuerUrl, and scopes
- **THEN** 加载 MUST 成功，三字段为空
- **AND** 既有 social/idc/api_key 凭据的行为 MUST 不变

### Requirement: 冲突与 upsert 策略

Ingest MUST apply conflict rules in this order of identity strength: API Key hash; OAuth userId; OAuth refreshToken hash.

When an upsert updates an existing credential, the field overlay MUST carry external endpoint metadata. Omitting these fields from the overlay would silently strip a working external credential's refresh configuration on re-import.

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

#### Scenario: upsert 保留 external endpoint 元数据

- **GIVEN** 一条已存在的 external_idp 凭据
- **WHEN** 同一 userId 再次导入并触发 upsert
- **THEN** 更新后的凭据 MUST 保留或按新值更新 tokenEndpoint / issuerUrl / scopes
- **AND** MUST NOT 因 overlay 遗漏而将三字段清空

### Requirement: POST /credentials 兼容扩展

Existing POST /api/admin/credentials MUST remain the supported single-add entry, running through ingest, and MAY accept optional userId, nickname, startUrl, and onConflict. It MAY additionally accept optional tokenEndpoint, issuerUrl, and scopes.

The `authMethod` field MUST be validated against the canonical alias set. It is currently an unvalidated free-form string, which lets an unsupported value persist and then silently route to the wrong refresh endpoint at runtime. Validation MUST reject unknown values at the API boundary.

#### Scenario: 旧客户端最小 body

- **WHEN** a client posts only refreshToken and authMethod as today
- **THEN** behavior MUST remain create-or-reject-by-hash (default reject) with forced refresh for OAuth

#### Scenario: 响应附加 action

- **WHEN** ingest creates a new credential
- **THEN** response MUST include success and credentialId
- **AND** SHOULD include action=created when the extended response is enabled
- **WHEN** ingest upserts an existing credential
- **THEN** response SHOULD include action=updated and the same credentialId

#### Scenario: authMethod 缺省不变

- **WHEN** a client omits authMethod
- **THEN** the default MUST remain `social`, unchanged from before this change

#### Scenario: 未知 authMethod 在 API 边界被拒

- **WHEN** a client posts an authMethod outside the canonical alias set
- **THEN** the API MUST return a client error listing the accepted values
- **AND** MUST NOT persist the credential
- **AND** MUST NOT defer the failure to refresh time

### Requirement: 密钥与日志安全

Ingest and import APIs MUST NOT return full refreshToken, clientSecret, or kiroApiKey in JSON responses, and MUST NOT write those secrets in clear text to logs.

This MUST extend to material introduced by import-tool formats: the account password and proxy password present in export files MUST never be persisted, returned, or logged. It MUST also extend to the external OAuth2 refresh exchange: the request form and the upstream error body MUST NOT be logged verbatim.

#### Scenario: 状态接口继续脱敏

- **WHEN** Admin lists credentials after import
- **THEN** secrets remain absent or hashed/masked as in existing status fields

#### Scenario: 导入文件中的密码不入库

- **WHEN** 导入文档包含账号 password 或代理 password
- **THEN** 系统 MUST NOT 持久化、返回或记录这些值

#### Scenario: external 刷新不记录 form 与响应原文

- **WHEN** external OAuth2 刷新请求失败
- **THEN** 日志与 API 错误体 MUST NOT 包含请求 form 原文、refresh token、
  access token 或完整 client secret

## ADDED Requirements

### Requirement: region 解析链不得因导入能力而改变

Existing region resolution MUST remain unchanged by this capability. Effective auth-region resolution MUST keep falling back to the credential-level general region, and effective api-region resolution MUST keep falling back only to global configuration, not to the credential-level general region.

The two chains differ intentionally: auth region selects the token refresh endpoint, api region selects the data-plane endpoint, and a deployment may legitimately authenticate in one region while calling another. Unifying them would remove that capability.

Import MUST therefore write a source region into the credential's general region field and rely on the existing auth-region fallback, rather than altering either resolver.

#### Scenario: auth region 回退到凭据级 region

- **GIVEN** 一条凭据设置了通用 region，未设置 auth region 覆盖
- **WHEN** 解析有效 auth region
- **THEN** 结果 MUST 为凭据级 region

#### Scenario: api region 不回退到凭据级 region

- **GIVEN** 一条凭据设置了通用 region，未设置 api region 覆盖
- **WHEN** 解析有效 api region
- **THEN** 结果 MUST 为全局配置值，MUST NOT 为凭据级 region
- **AND** 该行为 MUST 与本变更前完全一致

#### Scenario: api region 覆盖仍优先

- **GIVEN** 一条凭据同时设置了 api region 与通用 region
- **WHEN** 解析有效 api region
- **THEN** 结果 MUST 为 api region 覆盖值

#### Scenario: 导入账号刷新使用源 region

- **GIVEN** 一条导入记录带 region
- **WHEN** 该凭据入库后触发 token 刷新
- **THEN** 刷新端点 MUST 使用该 region
- **AND** 该效果 MUST 由既有 auth region 回退链产生，MUST NOT 依赖对解析函数的修改
