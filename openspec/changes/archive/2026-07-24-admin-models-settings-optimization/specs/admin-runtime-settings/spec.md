## ADDED Requirements

### Requirement: Admin 可读写出站代理设置

Admin API MUST allow authenticated operators to read and update the global outbound proxy configuration (proxy URL and optional proxy auth), persist it to the server config file, and apply it to subsequent upstream requests without requiring a process restart.

#### Scenario: 读取代理设置

- **WHEN** GET /api/admin/settings/proxy（或等价）且 Admin 已认证
- **THEN** 返回当前 proxyUrl（可为空）及是否配置了用户名/密码的指示；MUST NOT 回传明文密码

#### Scenario: 更新合法代理

- **WHEN** PUT 合法 http/https/socks5 proxyUrl（可选用户名密码）
- **THEN** 配置落盘成功，后续上游请求使用新的全局代理（凭据级 proxy 仍优先于全局）

#### Scenario: 非法 URL 拒绝

- **WHEN** proxyUrl 非空且无法解析为支持的代理 URL
- **THEN** 返回 400，且 MUST NOT 修改内存中的生效代理与落盘配置

#### Scenario: 清空全局代理

- **WHEN** 将 proxyUrl 更新为空字符串（或明确 clear）
- **THEN** 全局代理被清除，凭据未配置 proxy 时直连（或系统默认无代理行为）

### Requirement: Admin 可配置默认 Kiro 端点

Admin API MUST allow reading and updating config.defaultEndpoint to a name that is registered in the server endpoint registry. Unknown endpoint names MUST be rejected. Multi-endpoint automatic URL fallback is out of scope unless separately specified.

#### Scenario: 读取端点设置

- **WHEN** GET /api/admin/settings/endpoint
- **THEN** 返回 defaultEndpoint 与已注册端点名称列表

#### Scenario: 设置合法 defaultEndpoint

- **WHEN** PUT defaultEndpoint 为已注册名称（当前至少 ide）
- **THEN** 配置落盘，未显式指定 endpoint 的凭据使用该默认值

#### Scenario: 拒绝未知端点名

- **WHEN** PUT 未注册的 endpoint 名称
- **THEN** 返回 400 且不修改现有 defaultEndpoint

### Requirement: Admin 可配置客户端 API Key 验证开关

The system MUST support a requireApiKey configuration (default true for backward compatibility). When requireApiKey is false, Anthropic/client API routes MUST accept requests without a matching client apiKey. When requireApiKey is true, requests MUST present a valid apiKey (constant-time compare against configured key); if no apiKey is configured, authentication MUST fail closed. Admin API authentication via adminApiKey remains independent and MUST stay enforced when Admin is enabled.

#### Scenario: 默认保持校验

- **WHEN** 配置未设置 requireApiKey 或为 true，且 apiKey 已配置
- **THEN** 缺少或错误的客户端 key 被拒绝（与现网一致）

#### Scenario: 关闭校验

- **WHEN** requireApiKey 为 false
- **THEN** 客户端路由不因缺少 x-api-key/Bearer 而拒绝（Admin 路由仍需 adminApiKey）

#### Scenario: 开启但无 key fail-closed

- **WHEN** requireApiKey 为 true 且 apiKey 为空
- **THEN** 客户端请求一律认证失败

#### Scenario: 轮换 apiKey 脱敏

- **WHEN** Admin GET settings/auth
- **THEN** 仅返回 requireApiKey、hasApiKey 与 mask 形式，不返回完整 apiKey 明文

#### Scenario: 热更新鉴权配置

- **WHEN** Admin PUT 更新 requireApiKey 和/或 apiKey 成功
- **THEN** 配置落盘且后续客户端请求按新规则鉴权，无需重启进程

### Requirement: 设置变更安全与校验

Runtime settings updates MUST validate input before applying, MUST NOT log full secrets at info level, and MUST use the same Admin authentication as other Admin APIs.

#### Scenario: 未认证写入拒绝

- **WHEN** 无 adminApiKey 调用 settings 写接口
- **THEN** 返回 401/403 且配置不变

#### Scenario: 写失败不半更新

- **WHEN** 校验通过但持久化失败
- **THEN** API 返回错误；系统 MUST NOT 在无落盘保障下静默宣称成功（实现应优先保证内存与磁盘一致或明确报告不一致）
