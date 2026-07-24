## MODIFIED Requirements

### Requirement: GET /v1/models 优先使用全局缓存

GET /v1/models MUST prefer the global model catalog when non-empty, generating thinking variants per project rules, and MUST fall back to the existing static model list when the catalog is empty. When building the response from the catalog, the system MUST use upstream modelId as the canonical id and MUST NOT include catalog entries that cannot be accepted by the existing map_model mapping (unless an explicit configuration enables exposing unmapped models). Static fallback model ids MUST each be mappable by map_model.

#### Scenario: 缓存非空

- **WHEN** 全局 catalog 含至少一个上游 modelId
- **THEN** 响应 data 包含该 modelId 及对应 thinking 变体（若启用），且字段结构仍为 models 列表对象

#### Scenario: 缓存为空兼容

- **WHEN** 全局 catalog 为空
- **THEN** 响应回退到现有静态模型列表行为，关键兼容模型 id 仍可被客户端发现

#### Scenario: catalog 路径不暴露无法映射的 id

- **WHEN** 全局 catalog 含某 modelId 且 map_model(modelId) 为 None，且未启用 exposeUnmappedModels
- **THEN** 该 modelId 不得出现在 GET /v1/models 的 data 中（thinking 变体亦不得单独暴露）

#### Scenario: 静态 fallback 均可映射

- **WHEN** 全局 catalog 为空并返回静态 fallback 列表
- **THEN** 列表中每个 model id（含 thinking 变体中的基座 id 经既有规则解析后）均可被 map_model 接受为合法上游模型

## ADDED Requirements

### Requirement: 空缓存时可后台预热模型目录

When the global model catalog is empty at process start or when serving GET /v1/models fallback, the system MUST attempt a limited-concurrency asynchronous refresh of enabled credentials when practical, or document why skipped, and MUST ensure client GET /v1/models is never blocked on full refresh of models for enabled credentials without failing client requests if refresh fails.

#### Scenario: 启动预热不阻塞

- **WHEN** 进程启动且存在至少一个启用凭据
- **THEN** 系统可后台尝试 ListAvailableModels 预热；失败仅记录日志，不影响监听与处理 HTTP

#### Scenario: /v1/models 不因预热阻塞

- **WHEN** 客户端请求 GET /v1/models 且 catalog 仍为空
- **THEN** 立即返回 fallback（或当前缓存），MUST NOT 同步等待全量上游刷新完成

### Requirement: Admin 可查看全局模型 catalog 摘要

Admin API MUST provide a read of the aggregated global model catalog (model ids, count, and optional updatedAt) for operators, without requiring a specific credential id.

#### Scenario: 读取全局 catalog

- **WHEN** 已认证 Admin 调用 GET /api/admin/models/catalog（或等价路径）
- **THEN** 返回 success、count 与 models 列表（可为空），且响应不含 accessToken/refreshToken 明文

#### Scenario: 未认证拒绝

- **WHEN** 无有效 adminApiKey 访问该路径
- **THEN** 返回 401/403 且不泄露 catalog 细节给未授权方（行为与其他 Admin API 一致）
