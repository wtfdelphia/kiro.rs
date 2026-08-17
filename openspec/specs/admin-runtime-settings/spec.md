# Capability: admin-runtime-settings

## Purpose

Allow authenticated Admin operators to safely inspect and update runtime proxy, endpoint, and client API key settings without restarting the process.

## Requirements

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

Runtime settings updates MUST validate input before applying, MUST NOT log full secrets at info level, and MUST use the same Admin authentication as other Admin APIs. This applies to every runtime settings group, including the web search emulation switch.

#### Scenario: 未认证写入拒绝

- **WHEN** 无 adminApiKey 调用 settings 写接口
- **THEN** 返回 401/403 且配置不变

#### Scenario: 写失败不半更新

- **WHEN** 校验通过但持久化失败
- **THEN** API 返回错误；系统 MUST NOT 在无落盘保障下静默宣称成功（实现应优先保证内存与磁盘一致或明确报告不一致）

#### Scenario: 新增设置组同受 Admin 鉴权

- **WHEN** 无有效 adminApiKey 访问 web 搜索代执行开关的读或写接口
- **THEN** 返回 401/403 且配置不变

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

### Requirement: Admin 可读写 web 搜索代执行开关

Admin API MUST allow authenticated operators to read and update a boolean switch controlling server-side web search emulation. Updates MUST be persisted to the server config file and MUST take effect for subsequent requests without requiring a process restart. The switch MUST default to enabled so that existing deployments keep their current behavior when the field is absent from the config file.

The switch MUST only affect the OpenAI Responses endpoint. When disabled, a web search tool declaration MUST be treated as an ordinary tool and forwarded through the normal tool path rather than rejected. The Anthropic endpoints' web search behavior MUST NOT be governed by this switch.

#### Scenario: 读取开关状态

- **WHEN** GET /api/admin/settings/websearch（或等价）且 Admin 已认证
- **THEN** 返回当前开关的布尔值

#### Scenario: 配置缺省时默认启用

- **WHEN** 服务器配置文件中不含该字段
- **THEN** 读取结果 MUST 为启用状态（保持既有部署行为不变）

#### Scenario: 关闭开关并落盘

- **WHEN** PUT 将开关设为关闭且 Admin 已认证
- **THEN** 变更 MUST 落盘，且后续请求 MUST 立即按关闭后的行为处理，无需重启进程

#### Scenario: 重新启用

- **WHEN** 在关闭后 PUT 将开关设为启用
- **THEN** 变更 MUST 落盘并立即生效

#### Scenario: 关闭后不改变端点可用性

- **WHEN** 开关处于关闭状态且请求声明了 web 搜索工具
- **THEN** 该请求 MUST NOT 因此失败，工具 MUST 走正常工具路径

#### Scenario: 不影响 Anthropic 端点

- **WHEN** 开关处于关闭状态且 Anthropic 端点收到 web 搜索工具请求
- **THEN** Anthropic 端点的既有行为 MUST 保持不变（该端点不受此开关约束）

#### Scenario: 响应不含无关密钥

- **WHEN** 读取该开关
- **THEN** 响应 MUST 仅包含该开关状态，MUST NOT 附带 apiKey / adminApiKey / proxy 密码等明文

### Requirement: Admin 可读写 WebSocket 运行时设置

Admin API MUST allow authenticated operators to read and update the WebSocket ingress
settings (`enabled`, `mode`, `max_connections`, first-message timeout, inter-turn idle
timeout, max message bytes, upstream read timeout) without a process restart. Updates MUST
be partial (unspecified fields keep their current values), MUST be persisted to the server
config file, and MUST take effect for subsequent connections immediately. The read endpoint
MUST additionally report the current number of active WebSocket connections. Every update
MUST be logged with old and new values. If persistence fails, the in-memory value MUST
still take effect and the error MUST distinguish "applied but not persisted".

#### Scenario: 读取 WebSocket 设置

- **WHEN** GET `/api/admin/settings/websocket` 且 Admin 已认证
- **THEN** 返回当前全部 WebSocket 设置字段与当前活跃 WS 连接数

#### Scenario: 部分更新并热生效

- **WHEN** PUT 仅携带 `{"enabled": false}`
- **THEN** 其余字段 MUST 保持不变，配置 MUST 落盘，新的 WS upgrade 请求 MUST 立即被拒绝

#### Scenario: 落盘失败区分语义

- **WHEN** 更新成功写入内存但 `save_config` 失败
- **THEN** 内存值 MUST 已生效，响应 MUST 明确区分「已生效未落盘」错误

#### Scenario: 热更新留痕

- **WHEN** 任一 WebSocket 设置被更新
- **THEN** 日志 MUST 记录变更字段的旧值与新值
