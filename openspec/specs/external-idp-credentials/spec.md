# Capability: external-idp-credentials

## Purpose

Support Microsoft Entra ID / Azure AD accounts (`authMethod = external_idp`) exported by external credential managers: one canonical authentication classifier shared by every entry point, strict whitelist validation of token endpoints, OAuth2 `refresh_token` refresh against the validated Microsoft endpoint, lossless persistence of external endpoint metadata, structured credential-file container detection, atomic credential-file writes, and backed-up container migration.

## Requirements

### Requirement: 认证类型规范化为单一事实源

The system MUST provide a single canonical authentication-method classifier used by all credential entry points (Admin add, KAM import, startup file load) and by refresh routing. Client-side and server-side entry points MUST NOT each implement their own inference rules.

An explicit `authMethod` value MUST take precedence over any field-shape inference. Recognized aliases MUST be matched case-insensitively:

| aliases | canonical |
| --- | --- |
| `social` | `social` |
| `idc`, `IdC`, `builder-id`, `iam` | `idc` |
| `external_idp`, `external-idp`, `externalidp`, `azure`, `azuread`, `azure_ad` | `external_idp` |
| `api_key`, `apikey` | `api_key` |

When `authMethod` is absent, inference MUST apply in this fixed order:

1. a whitelisted `tokenEndpoint` or `issuerUrl` is present → `external_idp`
2. `clientId` and `clientSecret` are both present → `idc`
3. a `refreshToken` is present → `social`

The `external_idp` inference step MUST be evaluated **before** the `idc` step, because an external account may legitimately carry both a client ID and a client secret. Ordering these the other way makes the `external_idp` branch unreachable for confidential clients.

#### Scenario: 显式 authMethod 覆盖字段猜测

- **GIVEN** 一条凭据同时具备 `clientId` 与 `clientSecret`，且 `authMethod` 显式为 `external_idp`
- **WHEN** 分类器执行
- **THEN** 结果 MUST 为 `external_idp`
- **AND** MUST NOT 因 client 字段齐全而判为 `idc`

#### Scenario: 别名大小写不敏感

- **WHEN** 输入 `authMethod` 为 `IdC`、`AZURE_AD` 或 `APIKEY`
- **THEN** 分类器 MUST 分别归一为 `idc`、`external_idp`、`api_key`

#### Scenario: 缺省时 external 推断先于 idc

- **GIVEN** 一条凭据无 `authMethod`，但同时具备 `clientId`、`clientSecret` 与白名单内的 `tokenEndpoint`
- **WHEN** 分类器执行
- **THEN** 结果 MUST 为 `external_idp`

#### Scenario: 显式未知值必须拒绝

- **GIVEN** 一条凭据的 `authMethod` 为不在别名表内的值
- **WHEN** 通过导入入口提交
- **THEN** 该记录 MUST 被报告为失败，错误 MUST 列出合法取值
- **AND** MUST NOT 静默降级为 `social` 或任何其他类型
- **WHEN** 该值出现在启动加载的凭据文件中
- **THEN** 加载 MUST 失败并指出该凭据在文件中的位置

#### Scenario: 现有落盘归一函数行为不变

- **WHEN** 引入新分类器后执行凭据落盘
- **THEN** 既有的 `authMethod` 落盘归一行为（已知别名归一、未知值原样透传）MUST 保持不变
- **AND** 历史脏值 MUST NOT 因本能力导致落盘失败

### Requirement: external token endpoint 必须通过严格白名单校验

Because credential import files are untrusted external input, any token endpoint derived from them MUST be validated before use. An unvalidated endpoint would turn credential import into an arbitrary SSRF vector that leaks refresh tokens.

Validation MUST reject unless all of the following hold:

- the value parses as a URL using a URL parser (MUST NOT be validated by string prefix/suffix matching)
- the scheme is `https`
- the URL carries no userinfo component
- the host is a domain, not an IP literal (IPv4 or IPv6) — this MUST be an explicit check, not an incidental consequence of the domain whitelist
- the lowercased domain is neither `localhost` nor a `.localhost` subdomain
- the lowercased domain exactly equals a whitelisted Microsoft login domain, or is a subdomain of one

The whitelist MUST NOT be runtime-configurable, because a configurable whitelist is a bypassable whitelist.

#### Scenario: 合法 Microsoft 登录域被接受

- **WHEN** endpoint 的 host 为 Microsoft 公有云、美国政府云或中国云的登录域，或其子域，且使用 HTTPS
- **THEN** 校验 MUST 通过

#### Scenario: 非 HTTPS 被拒绝

- **WHEN** endpoint 使用 `http` 或任何非 `https` scheme
- **THEN** 校验 MUST 失败

#### Scenario: userinfo 混淆被拒绝

- **GIVEN** endpoint 形如 `https://<whitelisted-domain>@attacker.example/token`
- **WHEN** 校验执行
- **THEN** MUST 失败，因为真实 host 是 `attacker.example`

#### Scenario: 反斜杠归一化绕过被拒绝

- **GIVEN** endpoint 在白名单域名前含反斜杠等待归一化字符，使字符串后缀匹配会误判通过
- **WHEN** 校验执行
- **THEN** MUST 失败，因为校验依据是 URL 解析器给出的 host 而非字符串片段

#### Scenario: IP 与本机地址被显式拒绝

- **WHEN** endpoint 的 host 是 IPv4 字面量、IPv6 字面量、`localhost` 或 `.localhost` 子域
- **THEN** 校验 MUST 失败
- **AND** 该拒绝 MUST 由独立的显式判断产生，使其在单元测试中可断言

#### Scenario: 后缀伪装域被拒绝

- **GIVEN** endpoint 的 host 包含白名单域名作为**非后缀**片段（例如白名单域名后再接攻击者域）
- **WHEN** 校验执行
- **THEN** MUST 失败

#### Scenario: issuerUrl 派生结果必须复检

- **GIVEN** 凭据只提供 `issuerUrl` 而未提供 `tokenEndpoint`
- **WHEN** 系统按固定规则从 issuer 派生 token endpoint
- **THEN** 派生结果 MUST 再次通过同一校验器
- **AND** 非白名单 issuer 派生出的 endpoint MUST 被拒绝

#### Scenario: 拒绝原因不得泄露凭据材料

- **WHEN** 校验失败且输入含 userinfo 或查询参数
- **THEN** 错误信息 MUST NOT 包含密码片段或任何 token 材料

### Requirement: external_idp 刷新走 Microsoft OAuth2 refresh_token grant

The system MUST route refresh for `external_idp` credentials to the validated Microsoft token endpoint using an OAuth2 `refresh_token` grant, and MUST NOT route them to the AWS SSO OIDC endpoint or the Kiro Desktop Auth endpoint.

Routing to AWS SSO OIDC sends a Microsoft client ID to AWS and yields a deterministic `400 invalid_request`; routing to Kiro Desktop Auth sends a Microsoft refresh token to an endpoint that cannot honor it.

The request MUST use `application/x-www-form-urlencoded`. `grant_type`, `client_id`, and `refresh_token` MUST always be present. `client_secret` and `scope` MUST be included only when non-empty, so that public clients need not fabricate a secret.

#### Scenario: 分派选中 external 而非 IdC

- **GIVEN** 一条 `external_idp` 凭据同时具备 `clientId` 与 `clientSecret`
- **WHEN** 刷新执行
- **THEN** 请求 MUST 发往已校验的 Microsoft token endpoint
- **AND** MUST NOT 发往 `oidc.<region>.amazonaws.com`

#### Scenario: 分派选中 external 而非 Social

- **GIVEN** 一条显式 `authMethod = external_idp` 的凭据从凭据文件加载
- **WHEN** 刷新执行
- **THEN** 请求 MUST NOT 发往 Kiro Desktop Auth 的 refreshToken 端点

#### Scenario: 公共客户端不需要 client_secret

- **GIVEN** 一条 `external_idp` 凭据只有 `clientId`，没有 `clientSecret`
- **WHEN** 构造刷新请求
- **THEN** form MUST NOT 包含 `client_secret` 键（不得为空串）
- **AND** 刷新流程 MUST NOT 因缺少 `clientSecret` 而拒绝该凭据

#### Scenario: scopes 为空时不发 scope

- **GIVEN** 一条 `external_idp` 凭据的 `scopes` 为空或缺失
- **WHEN** 构造刷新请求
- **THEN** form MUST NOT 包含 `scope` 键

#### Scenario: refresh token 轮换

- **WHEN** external 刷新响应包含新的 `refresh_token`
- **THEN** 系统 MUST 持久化轮换后的值，语义与既有 Social/IdC 轮换一致

#### Scenario: Social 与 IdC 刷新行为不得回归

- **WHEN** 刷新分派从两路扩为四路后处理 Social 或 IdC 凭据
- **THEN** 端点、请求体形态与错误分类 MUST 与本变更前逐位一致

#### Scenario: 错误响应必须脱敏

- **WHEN** external 刷新返回错误
- **THEN** 系统记录与 API 错误体 MUST NOT 包含 refresh token、access token、
  完整 client secret 或请求 form 原文

### Requirement: external_idp 凭据字段可持久化且不丢失

Credentials MUST support optional `tokenEndpoint`, `issuerUrl`, and `scopes` fields, persisted in multi-credential JSON and preserved across upsert. `scopes` MUST be a single string (space-delimited), matching the source export format, so import performs no lossy conversion.

`external_idp` credentials MUST NOT have `provider` backfilled to `BuilderId` when absent, because the source system deliberately leaves `provider` unset for external accounts.

#### Scenario: 三字段 round-trip

- **GIVEN** 一条含 `tokenEndpoint`、`issuerUrl`、`scopes` 的 external 凭据
- **WHEN** 写入凭据文件再加载
- **THEN** 三个字段 MUST 保持不变

#### Scenario: upsert 不丢 external 字段

- **GIVEN** 一条已存在的 external 凭据
- **WHEN** 同一账号再次导入并触发 upsert
- **THEN** 更新后的凭据 MUST 保留 external endpoint 元数据

#### Scenario: 旧凭据文件缺三字段仍可加载

- **WHEN** 凭据文件中的条目不含 `tokenEndpoint`、`issuerUrl`、`scopes`
- **THEN** 加载 MUST 成功，三字段为空

#### Scenario: external 不回填 provider

- **GIVEN** 一条 `external_idp` 凭据未提供 `provider`
- **WHEN** 导入执行
- **THEN** 系统 MUST NOT 将 `provider` 设为 `BuilderId` 或任何 IdC provider 值

#### Scenario: external 真实 profileArn 必须保留

- **GIVEN** 一条 `external_idp` 凭据携带非占位的真实 `profileArn`
- **WHEN** 导入并持久化
- **THEN** 该 ARN MUST 原样保留，MUST NOT 被占位识别逻辑清除

### Requirement: 凭据文件容器格式判别必须结构化且可诊断

Credential file loading MUST determine the container shape by inspecting the parsed JSON structure, and MUST NOT rely on untagged deserialization to guess between a single credential and a wrapper object.

Because the credential struct has no required fields, untagged deserialization silently accepts any JSON object as one empty credential. The system MUST recognize a native single credential only when the object carries a credential-identifying key (a refresh token or an API key), and MUST fail with a diagnosable error for unrecognized object shapes.

#### Scenario: 未知包装对象必须 fail fast

- **GIVEN** 一个既非原生格式也非已知导入容器的 JSON 对象
- **WHEN** 启动加载执行
- **THEN** 加载 MUST 失败，错误 MUST 指出 JSON 位置与已识别的顶层 key 名
- **AND** MUST NOT 产生一条字段为空的启用凭据
- **AND** MUST NOT 报告「已加载 1 个凭据配置」
- **AND** 错误信息 MUST NOT 包含任何字段值，只含 key 名

#### Scenario: 原生格式判别先行

- **GIVEN** 一个含 `refreshToken` 的平铺对象
- **WHEN** 加载执行
- **THEN** MUST 按原生单凭据处理，MUST NOT 触发导入容器适配或迁移

#### Scenario: 原生数组与优先级排序不变

- **WHEN** 加载原生凭据数组
- **THEN** 反序列化结果与优先级排序 MUST 与本变更前一致

### Requirement: 凭据文件写入必须原子

All writes to the credentials file MUST be atomic: write to a temporary file in the same directory, then replace the target. A process interrupted mid-write MUST NOT leave a truncated credentials file.

This MUST apply to both the format-migration write and the routine post-refresh persist path. Having one atomic and one non-atomic write path to the same file is worse than having neither, because a reader cannot tell which path produced the current contents.

#### Scenario: 常规回写原子

- **WHEN** 刷新后回写凭据文件
- **THEN** 写入 MUST 经临时文件与替换完成
- **AND** 目标文件 MUST NOT 在写入过程中处于部分写状态

#### Scenario: 覆盖已存在文件

- **WHEN** 原子替换的目标文件已存在
- **THEN** 替换 MUST 成功
- **AND** 该行为 MUST 在实际运行的测试中被验证，MUST NOT 依赖对平台语义的假设

### Requirement: 导入容器迁移必须备份且失败不破坏原文件

When the startup loader recognizes an import-tool container format, it MAY normalize and rewrite the file to the native format. Doing so MUST create a backup of the original file first, and MUST perform the replacement atomically.

Migration failure MUST NOT prevent startup and MUST NOT damage the original file: the credentials are already correctly parsed in memory, so the system MUST continue running and retry on a later start.

#### Scenario: 迁移前备份原文件

- **GIVEN** 凭据文件为已知导入容器格式
- **WHEN** 启动加载执行规范化回写
- **THEN** 原文件 MUST 先被备份到同目录下可识别的备份文件名
- **AND** 目标文件写入后 MUST 为原生格式且内容与内存中的规范化结果等价

#### Scenario: 备份失败不写回

- **WHEN** 备份步骤失败
- **THEN** 系统 MUST NOT 写回目标文件
- **AND** MUST 记录告警并以内存中的规范化结果继续运行

#### Scenario: 原子替换失败保留原文件

- **WHEN** 临时文件写入或原子替换失败
- **THEN** 原凭据文件内容 MUST 保持不变
- **AND** 启动 MUST NOT 因此失败

### Requirement: 两个导入入口对同一文件必须产出等价凭据

The Admin import path and the startup file-load path MUST share the same container adapter and the same authentication classifier, so that the same input file yields equivalent normalized credentials through either entry point.

Before this change, an `external_idp` account imported through the Admin UI was classified as `idc` and refreshed against AWS SSO OIDC, while the same account loaded directly from the credentials file was refreshed against the Kiro Social endpoint. Two entry points disagreeing about the same file is the defect this requirement closes.

#### Scenario: 同一 fixture 两条路径等价

- **GIVEN** 同一份脱敏导入 fixture
- **WHEN** 分别经 Admin 导入与启动加载处理
- **THEN** 两条路径产出的规范化凭据 MUST 在认证类型、endpoint 元数据、
  身份字段与 region 字段上逐一相等

#### Scenario: external 账号两条路径同一刷新去向

- **GIVEN** 一条 `external_idp` 账号
- **WHEN** 分别经两条路径入库后触发刷新
- **THEN** 两者 MUST 发往同一个已校验的 Microsoft token endpoint
