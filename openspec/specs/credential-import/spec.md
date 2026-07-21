# Capability: credential-import

## Purpose

Admin API and KAM JSON import accept optional provider/profileArn, attempt profile resolution after add, and distinguish balance-only success from full chat readiness when profileArn remains unresolved.

## Requirements

### Requirement: KAM/Admin 导入接收 provider 与 profileArn

Credential import via Admin API and KAM JSON MUST accept optional provider and profileArn fields and MUST NOT discard a provided profileArn on add.

#### Scenario: KAM 平铺 JSON 含 profileArn

- GIVEN 导入 JSON 条目包含 refreshToken、clientId、clientSecret、profileArn、provider
- WHEN 通过 Admin/KAM 导入成功
- THEN 持久化后的凭据保留 profileArn 与 provider

#### Scenario: KAM IdC 缺省 provider

- GIVEN 导入条目有 clientId 与 clientSecret 但未给 provider
- WHEN 判定为 idc 并添加
- THEN provider 默认为 BuilderId（或与 Kiro-Go 等价的默认），以便固定 ARN 路径生效

### Requirement: 导入后触发 profile 解析与可观测状态

After a successful credential add, the system MUST attempt profile resolution (and MAY fetch usage) and surface hasProfileArn (or equivalent) in Admin credential status.

#### Scenario: 导入后状态

- GIVEN BuilderId 凭据导入且无初始 profileArn
- WHEN 添加流程完成 resolve
- THEN Admin 列表/详情显示 hasProfileArn=true，或明确显示解析失败原因且不含密钥明文

### Requirement: 导入验活区分余额与对话前置条件

Import verification MUST NOT treat usage-limits success alone as proof that chat will succeed when profileArn remains unresolved for a profile-required account type.

#### Scenario: 仅余额成功

- GIVEN resolve 失败但 getUsageLimits 成功
- WHEN 导入验活结束
- THEN 结果提示 profile 未解析风险（可配置是否回滚），而不是仅显示余额成功为完全健康

