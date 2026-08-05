## ADDED Requirements

### Requirement: Admin 可读写客户端标识（Kiro/System/Node 版本）

Admin API MUST allow authenticated operators to read and update client identity fields used in upstream request fingerprinting: kiroVersion, systemVersion, and nodeVersion. Updates MUST be validated, persisted to the server config file, and applied to subsequent upstream requests without requiring a process restart. Admin API authentication via adminApiKey remains required.

#### Scenario: 读取客户端标识

- **WHEN** GET /api/admin/settings/client-identity（或等价）且 Admin 已认证
- **THEN** 返回当前 kiroVersion、systemVersion、nodeVersion 字符串

#### Scenario: 更新合法版本字段

- **WHEN** PUT 非空且长度合法的 kiroVersion/systemVersion/nodeVersion
- **THEN** 配置落盘成功，后续需要 UA/客户端标识的上游请求使用新值，无需重启进程

#### Scenario: 拒绝空值

- **WHEN** PUT 任一版本字段为空或仅空白
- **THEN** 返回 400，且 MUST NOT 修改内存与落盘中的客户端标识

#### Scenario: 未认证拒绝

- **WHEN** 无有效 adminApiKey 访问 client-identity 读或写接口
- **THEN** 返回 401/403 且配置不变

#### Scenario: 不回传无关密钥

- **WHEN** 读取 client-identity
- **THEN** 响应仅包含版本类字段，MUST NOT 附带 apiKey/adminApiKey/proxy 密码明文
