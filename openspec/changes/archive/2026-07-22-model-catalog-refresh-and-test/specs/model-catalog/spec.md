## ADDED Requirements

### Requirement: 系统能从上游拉取可用模型目录

The system MUST call Kiro CodeWhisperer ListAvailableModels for a credential with a valid access token, attaching optional profileArn and using the credential effective proxy and standard Kiro REST headers.

#### Scenario: 成功拉取

- **WHEN** 凭据 token 有效且上游返回 200 与 models 数组
- **THEN** 解析出每个 model 的 modelId（及可用元数据），供缓存写入

#### Scenario: 上游失败保留旧缓存

- **WHEN** ListAvailableModels 返回非 200 或网络失败
- **THEN** 返回可诊断错误，且 MUST NOT 用空列表静默覆盖该凭据已有 model 缓存

### Requirement: 双层模型缓存

The system MUST maintain a per-credential model id set and a global aggregated model catalog derived from successful refreshes.

#### Scenario: 单凭据刷新写入

- **WHEN** 管理员对凭据 id 执行 models refresh 且上游成功
- **THEN** 该凭据 model set 更新为本次结果，且全局 catalog 合并去重后包含这些 modelId

#### Scenario: 删除凭据清理

- **WHEN** 凭据被删除
- **THEN** 其 per-credential 模型缓存被移除，且全局 catalog 不再仅依赖已删除凭据的独有条目（或在下次聚合刷新时收敛）

### Requirement: Admin 可刷新与查看模型缓存

Admin API MUST provide endpoints to refresh one credential, refresh all enabled credentials, and read a credential model list (cached; optional live fetch).

#### Scenario: 单凭据刷新

- **WHEN** POST /api/admin/credentials/{id}/models/refresh 且凭据存在
- **THEN** 返回 success 与 count/models（或等价字段），并更新缓存

#### Scenario: 全量刷新统计失败

- **WHEN** POST /api/admin/credentials/models/refresh 且部分凭据失败
- **THEN** 响应包含 refreshed 与 failed 计数及失败明细，成功凭据缓存仍被更新

#### Scenario: 凭据不存在

- **WHEN** 对不存在的 id 刷新或查看模型
- **THEN** 返回 404 且不修改全局缓存

### Requirement: GET /v1/models 优先使用全局缓存

GET /v1/models MUST prefer the global model catalog when non-empty, generating thinking variants per project rules, and MUST fall back to the existing static model list when the catalog is empty.

#### Scenario: 缓存非空

- **WHEN** 全局 catalog 含至少一个上游 modelId
- **THEN** 响应 data 包含该 modelId 及对应 thinking 变体（若启用），且字段结构仍为 models 列表对象

#### Scenario: 缓存为空兼容

- **WHEN** 全局 catalog 为空
- **THEN** 响应回退到现有静态模型列表行为，关键兼容模型 id 仍可被客户端发现

### Requirement: 生命周期触发异步刷新

After a credential is successfully added while enabled, or transitions from disabled to enabled, the system MUST attempt an asynchronous models refresh for that credential without failing the original admin operation if refresh fails.

#### Scenario: 添加后异步刷新

- **WHEN** 新凭据添加成功且处于启用状态
- **THEN** 后台尝试 ListAvailableModels；失败仅记录日志，添加 API 仍返回成功
