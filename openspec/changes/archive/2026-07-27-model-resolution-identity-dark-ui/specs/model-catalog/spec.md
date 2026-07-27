## MODIFIED Requirements

### Requirement: GET /v1/models 优先使用全局缓存

GET /v1/models MUST prefer the global model catalog when non-empty, generating thinking variants per project rules for models that support them, and MUST fall back to the existing static model list when the catalog is empty. When building the response from the catalog, the system MUST expose only model ids that the model-resolution pipeline accepts for client requests under current configuration (mapped Claude ids and, when enabled, catalog-passthrough ids). Static fallback model ids MUST each be resolvable by the same pipeline. Compatibility alias ids (such as auto or gpt-4o) MAY be included when configured to expose them, and SHOULD be distinguishable from upstream Anthropic ids when exposed.

#### Scenario: 缓存非空

- **WHEN** 全局 catalog 含至少一个上游 modelId 且该 id 可被 resolve 接受
- **THEN** 响应 data 包含该 modelId 及对应 thinking 变体（若适用），且字段结构仍为 models 列表对象

#### Scenario: 缓存为空兼容

- **WHEN** 全局 catalog 为空
- **THEN** 响应回退到现有静态模型列表行为，关键兼容模型 id 仍可被客户端发现

#### Scenario: catalog 路径不暴露永远不可请求的 id

- **WHEN** 全局 catalog 含某 modelId，且在当前配置下 resolve_model 拒绝该 id
- **THEN** 该 modelId 不得出现在 GET /v1/models 的 data 中（thinking 变体亦不得单独暴露）

#### Scenario: 透传开启时暴露 catalog 上游 id

- **WHEN** allowCatalogPassthrough 为 true 且 catalog 含可透传上游 id 且 resolve 接受
- **THEN** GET /v1/models 可包含该上游 id 作为可请求模型

#### Scenario: 静态 fallback 均可解析

- **WHEN** 全局 catalog 为空并返回静态 fallback 列表
- **THEN** 列表中每个 model id（含 thinking 变体中的基座 id 经既有规则解析后）均可被 resolve_model 接受

## ADDED Requirements

### Requirement: Admin catalog 可附带解析元数据

Admin global catalog and per-credential model list responses MUST include per-id resolution metadata (at least whether the id is resolvable/testable and the resolved target id/kind when known), so operators can distinguish raw upstream ids from client-callable ids without guessing.

#### Scenario: 查看 raw 与 testable

- **WHEN** 已认证 Admin 读取 credentials/{id}/models 且缓存含 auto 与 claude-sonnet-4.6
- **THEN** 响应仍能展示 raw 列表，并可通过元数据或并列字段判断哪些 id 对 test/chat 可解析
