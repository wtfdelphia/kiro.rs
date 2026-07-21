# Capability: credential-import

## Purpose

Delta: extend KAM/Admin import to pass identity fields through ingest and keep provider/profileArn guarantees while aligning verification with unified ingest.

## ADDED Requirements

### Requirement: KAM/Admin 导入接收身份字段

Credential import via Admin API and KAM JSON MUST accept optional userId, nickname, and startUrl (in addition to existing provider/profileArn) and MUST pass them into ingest without discarding non-empty values.

#### Scenario: KAM 含 userId 与 nickname

- **WHEN** KAM JSON includes userId and nickname (nested or flat form)
- **THEN** successful import MUST persist those fields on the credential

#### Scenario: 平铺与嵌套格式均映射身份字段

- **WHEN** import input uses KAM flat format with top-level userId/nickname/startUrl
- **THEN** normalization MUST map them equivalently to nested credentials form before ingest

### Requirement: 导入默认冲突策略利于重导

Import-oriented entry points (KAM/batch/import API) MUST default onConflict to upsert when a stable userId is present, while preserving hash reject when userId is absent.

#### Scenario: 同 userId 重导更新

- **WHEN** the same KAM account (same userId) is imported again with a new refreshToken
- **THEN** the system MUST update the existing credential id rather than creating a duplicate row when upsert applies

### Requirement: 批量导入主路径服务端化

Admin Batch and KAM import UIs MUST be able to complete multi-item import via the server batch import API as the primary path, without requiring per-item client-side disable+delete rollback as the only recovery mechanism.

#### Scenario: UI 调用 batch

- **WHEN** user submits a multi-item JSON import in Admin UI after this change
- **THEN** the client MUST call the batch import endpoint (optionally chunked)
- **AND** MUST render per-item created/updated/duplicate/failed statuses from the server response

## MODIFIED Requirements

### Requirement: 导入后触发 profile 解析与可观测状态

After a successful credential add, the system MUST attempt profile resolution (and MAY fetch usage) and surface hasProfileArn (or equivalent) in Admin credential status.

Import success MUST also surface identity fields when present (userId/email/nickname) without exposing secrets.

#### Scenario: 导入后状态

- **GIVEN** BuilderId 凭据导入且无初始 profileArn
- **WHEN** 添加流程完成 resolve
- **THEN** Admin 列表/详情显示 hasProfileArn=true，或明确显示解析失败原因且不含密钥明文

#### Scenario: 导入后身份可见

- **WHEN** ingest stored userId and email after import
- **THEN** Admin credential status MUST show email and userId for operator identification

### Requirement: 导入验活区分余额与对话前置条件

Import verification MUST NOT treat usage-limits success alone as proof that chat will succeed when profileArn remains unresolved for a profile-required account type.

Batch/import results MUST still allow operators to distinguish full readiness vs profile warning, consistent with existing verified vs verified_warn semantics where UI applies them.

#### Scenario: 仅余额成功

- **GIVEN** resolve 失败但 getUsageLimits 成功
- **WHEN** 导入验活结束
- **THEN** 结果提示 profile 未解析风险（可配置是否回滚），而不是仅显示余额成功为完全健康

#### Scenario: batch 条目级警告

- **WHEN** a batch item persists but profile remains unresolved
- **THEN** the item result MUST be distinguishable from a fully healthy import (warning status or message) without failing the entire batch by default
