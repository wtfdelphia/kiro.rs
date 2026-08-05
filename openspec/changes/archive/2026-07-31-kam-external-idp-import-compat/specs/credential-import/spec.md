## MODIFIED Requirements

### Requirement: KAM/Admin 导入接收 provider 与 profileArn

Credential import via Admin API and KAM JSON MUST accept optional provider and profileArn fields and MUST NOT discard a provided profileArn on add.

Provider backfill MUST be scoped to IdC accounts only. An `external_idp` account MUST NOT receive a backfilled provider, because the source export deliberately leaves `provider` unset for external accounts and a backfilled IdC provider would misroute later profile decisions.

#### Scenario: KAM 平铺 JSON 含 profileArn

- GIVEN 导入 JSON 条目包含 refreshToken、clientId、clientSecret、profileArn、provider
- WHEN 通过 Admin/KAM 导入成功
- THEN 持久化后的凭据保留 profileArn 与 provider

#### Scenario: KAM IdC 缺省 provider

- GIVEN 导入条目有 clientId 与 clientSecret 但未给 provider
- AND 该条目未被判定为 external_idp
- WHEN 判定为 idc 并添加
- THEN provider 默认为 BuilderId（或与 Kiro-Go 等价的默认），以便固定 ARN 路径生效

#### Scenario: external 条目不回填 provider

- **GIVEN** 导入条目被判定为 `external_idp` 且未提供 provider
- **WHEN** 导入成功
- **THEN** 持久化后的凭据 provider MUST 仍为空，MUST NOT 被填为 BuilderId

### Requirement: 导入后触发 profile 解析与可观测状态

After a successful credential add, the system MUST attempt profile resolution (and MAY fetch usage) and surface hasProfileArn (or equivalent) in Admin credential status.

Import success MUST also surface identity fields when present (userId/email/nickname) without exposing secrets.

Credential status MAY surface whether external endpoint metadata is configured, but MUST NOT return the stored client secret, refresh token, or account password in any field.

#### Scenario: 导入后状态

- GIVEN BuilderId 凭据导入且无初始 profileArn
- WHEN 添加流程完成 resolve
- THEN Admin 列表/详情显示 hasProfileArn=true，或明确显示解析失败原因且不含密钥明文

#### Scenario: 导入后身份可见

- **WHEN** ingest stored userId and email after import
- **THEN** Admin credential status MUST show email and userId for operator identification

#### Scenario: external 元数据只暴露配置状态

- **WHEN** Admin 列出一条 external_idp 凭据
- **THEN** 响应 MAY 指示 endpoint 元数据是否已配置
- **AND** MUST NOT 包含 clientSecret、refreshToken 或源文件中的账号 password

### Requirement: 导入验活区分余额与对话前置条件

Import verification MUST NOT treat usage-limits success alone as proof that chat will succeed when profileArn remains unresolved for a profile-required account type.

Batch/import results MUST still allow operators to distinguish full readiness vs profile warning, consistent with existing verified vs verified_warn semantics where UI applies them.

#### Scenario: 仅余额成功

- GIVEN resolve 失败但 getUsageLimits 成功
- WHEN 导入验活结束
- THEN 结果提示 profile 未解析风险（可配置是否回滚），而不是仅显示余额成功为完全健康

#### Scenario: batch 条目级警告

- **WHEN** a batch item persists but profile remains unresolved
- **THEN** the item result MUST be distinguishable from a fully healthy import (warning status or message) without failing the entire batch by default

### Requirement: KAM/Admin 导入接收身份字段

Credential import via Admin API and KAM JSON MUST accept optional userId, nickname, and startUrl (in addition to existing provider/profileArn) and MUST pass them into ingest without discarding non-empty values.

Identity mapping MUST be applied uniformly across all supported container shapes. A field mapped in the flat shape but skipped in the nested shape is a defect: the same account imported in two shapes MUST yield the same identity fields.

#### Scenario: KAM 含 userId 与 nickname

- **WHEN** KAM JSON includes userId and nickname (nested or flat form)
- **THEN** successful import MUST persist those fields on the credential

#### Scenario: 平铺与嵌套格式均映射身份字段

- **WHEN** import input uses KAM flat format with top-level userId/nickname/startUrl
- **THEN** normalization MUST map them equivalently to nested credentials form before ingest

#### Scenario: label 在两种形态都映射为 nickname

- **GIVEN** 同一账号分别以平铺形态与旧版嵌套形态提交，两者均带 `label`
- **WHEN** 导入执行
- **THEN** 两种形态 MUST 都把 `label` 映射为 nickname
- **AND** 嵌套形态 MUST NOT 因原样透传而丢弃 `label`

### Requirement: 导入默认冲突策略利于重导

Import-oriented entry points (KAM/batch/import API) MUST default onConflict to upsert when a stable userId is present, while preserving hash reject when userId is absent.

#### Scenario: 同 userId 重导更新

- **WHEN** the same KAM account (same userId) is imported again with a new refreshToken
- **THEN** the system MUST update the existing credential id rather than creating a duplicate row when upsert applies

### Requirement: 批量导入主路径服务端化

Admin Batch and KAM import UIs MUST be able to complete multi-item import via the server batch import API as the primary path, without requiring per-item client-side disable+delete rollback as the only recovery mechanism.

Container parsing and authentication classification MUST be performed server-side. The client MUST NOT re-derive an item's authentication method from field shapes, because a client-side rule that disagrees with the server's rule produces per-entry-point divergence for the same file.

#### Scenario: UI 调用 batch

- **WHEN** user submits a multi-item JSON import in Admin UI after this change
- **THEN** the client MUST call the batch import endpoint (optionally chunked)
- **AND** MUST render per-item created/updated/duplicate/failed statuses from the server response

#### Scenario: 客户端不再重算认证类型

- **WHEN** the Admin UI submits an import document containing an explicit `authMethod`
- **THEN** the client MUST forward that value rather than recomputing it from `clientId`/`clientSecret` presence
- **AND** the persisted credential's authentication method MUST reflect the server-side classifier's result

#### Scenario: 公共客户端不得被前端拒收

- **GIVEN** 一条导入记录有 `clientId` 但没有 `clientSecret`，且为 external_idp
- **WHEN** 用户提交导入
- **THEN** 客户端 MUST NOT 以「需要同时提供 clientId 和 clientSecret」为由本地判失败
- **AND** 该记录 MUST 到达服务端并按 external_idp 规则处理

## ADDED Requirements

### Requirement: 导入容器格式支持范围明确且逐条可诊断

Import MUST accept these container shapes through the Admin import path: a flat single account object, a flat account array, a `{ version, accounts: [...] }` wrapper, and legacy nested `{ credentials: {...} }` objects (single or in an array).

Container detection MUST be order-sensitive: the wrapper shape MUST be detected before the flat single-object shape, so that a malformed object carrying both an accounts array and a top-level refresh token is not silently misread.

Because the source export omits `skip_serializing_if` on nearly all optional fields, every optional field may arrive as an explicit `null`. Detection and normalization MUST treat an explicit `null` as absent, and MUST NOT infer presence from key existence alone.

#### Scenario: 四种容器均可导入

- **WHEN** 用户提交平铺单对象、平铺数组、`{ version, accounts }` 包装或旧版嵌套形态
- **THEN** 导入 MUST 正确识别记录数并逐条处理

#### Scenario: 包装判定先于平铺单条

- **GIVEN** 一个同时含 `accounts` 数组与顶层 `refreshToken` 的畸形对象
- **WHEN** 容器判别执行
- **THEN** MUST 按包装格式处理 `accounts`，MUST NOT 误判为单条凭据

#### Scenario: 显式 null 视为缺失

- **GIVEN** 导入记录的可选字段大量为显式 `null`
- **WHEN** 规范化执行
- **THEN** 这些字段 MUST 被视为缺失
- **AND** 认证类型推断 MUST NOT 因 key 存在而误判字段有值

#### Scenario: 无法识别的容器整体拒绝

- **WHEN** 提交的文档不匹配任何受支持容器
- **THEN** 请求 MUST 整体失败并给出可定位错误
- **AND** MUST NOT 产生部分导入

### Requirement: 导入必须逐条报告失败原因

Every rejected record MUST produce a per-item failure result carrying a reason. Records MUST NOT be dropped silently at any layer, including client-side pre-filtering.

Reporting only when *all* records are invalid is insufficient: a partially valid batch currently drops the invalid remainder without operator-visible feedback.

#### Scenario: 部分记录无效时逐条可见

- **GIVEN** 一个批次中部分记录缺少 refreshToken
- **WHEN** 导入执行
- **THEN** 每条无效记录 MUST 产生带原因的失败结果并对操作者可见
- **AND** MUST NOT 仅通过浏览器控制台告警丢弃

#### Scenario: 未知认证类型逐条失败

- **GIVEN** 某条记录的 `authMethod` 为不受支持的值
- **WHEN** 导入执行
- **THEN** 该条 MUST 失败并列出合法取值
- **AND** 同批次其他有效记录 MUST 照常处理

#### Scenario: 非法 endpoint 逐条失败

- **GIVEN** 某条 external 记录的 token endpoint 未通过白名单校验
- **WHEN** 导入执行
- **THEN** 该条 MUST 失败并说明 endpoint 被拒
- **AND** 错误信息 MUST NOT 包含 token 材料

#### Scenario: 预览不得展示敏感字段

- **WHEN** 导入前展示记录预览
- **THEN** 预览 MAY 展示识别出的认证类型、provider 与字段完整性
- **AND** MUST NOT 展示 refreshToken、accessToken、clientSecret、账号 password
  或代理密码

### Requirement: 导入必须映射启用状态与区域字段

Import normalization MUST translate source fields whose semantics differ from the target model, rather than dropping them.

The source export uses an `enabled` flag defaulting to true, while this system stores a `disabled` flag defaulting to false. Dropping the field silently re-enables accounts the operator had disabled upstream.

Region MUST be written to the credential's general `region` field rather than only to the auth-specific field, so that the existing auth-region fallback chain derives it without storing a duplicate value that can later drift. This MUST NOT be accompanied by any change to region resolution itself.

#### Scenario: enabled 取反映射为 disabled

- **GIVEN** 导入记录的 `enabled` 为 false
- **WHEN** 导入成功
- **THEN** 持久化凭据 MUST 为禁用状态
- **WHEN** `enabled` 为 true 或字段缺失
- **THEN** 持久化凭据 MUST 为启用状态

#### Scenario: region 写入通用字段

- **GIVEN** 导入记录带 `region`
- **WHEN** 导入成功
- **THEN** 该值 MUST 写入凭据的通用 region 字段
- **AND** 凭据的 auth-specific region 字段 MUST 保持为空
- **AND** 有效 auth region MUST 通过既有回退链取到该值

#### Scenario: 导入不改变 region 解析行为

- **WHEN** 本能力实现后解析任一凭据的有效 auth region 或有效 api region
- **THEN** 两者的回退链 MUST 与本变更前完全一致
