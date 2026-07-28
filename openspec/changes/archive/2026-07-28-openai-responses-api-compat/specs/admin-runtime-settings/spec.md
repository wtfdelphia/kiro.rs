## MODIFIED Requirements

### Requirement: 设置变更安全与校验

Runtime settings updates MUST validate input before applying, MUST NOT log full secrets at info level, and MUST use the same Admin authentication as other Admin APIs. This applies to every runtime settings group, including the web search emulation switch introduced by this change.

#### Scenario: 未认证写入拒绝

- **WHEN** 无 adminApiKey 调用 settings 写接口
- **THEN** 返回 401/403 且配置不变

#### Scenario: 写失败不半更新

- **WHEN** 校验通过但持久化失败
- **THEN** API 返回错误；系统 MUST NOT 在无落盘保障下静默宣称成功（实现应优先保证内存与磁盘一致或明确报告不一致）

#### Scenario: 新增设置组同受 Admin 鉴权

- **WHEN** 无有效 adminApiKey 访问本 change 新增的 web 搜索代执行开关读或写接口
- **THEN** 返回 401/403 且配置不变

## ADDED Requirements

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
