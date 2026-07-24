## MODIFIED Requirements

### Requirement: Admin 可对凭据做真实推理探测

Admin API MUST expose POST /api/admin/credentials/{id}/test that ensures a valid token and issues a minimal non-streaming upstream generation for a specified or default model. When model is omitted, the server MUST use a documented default model id. When model is provided, it MUST be resolved through existing map/thinking rules before generate.

#### Scenario: 默认模型成功

- **WHEN** 对有效凭据调用 test 且不指定 model（或使用默认）
- **THEN** 返回 success=true、所用 model，以及非空 reply 或等价上游可展示输出，并包含延迟类指标（可选但推荐）

#### Scenario: 指定模型

- **WHEN** 请求体指定合法 model 字符串
- **THEN** 使用该模型（经既有 map/thinking 规则解析后）发起探测

#### Scenario: 非法或无法映射模型

- **WHEN** 指定无法映射到 Kiro modelId 的 model
- **THEN** 返回 400 类客户端错误且不发起上游 generate

#### Scenario: 客户端可用缓存列表中的 modelId

- **WHEN** 操作者使用该凭据 GET models 返回的某个 modelId 调用 test
- **THEN** 若该 id 可被 map_model 接受，则 test MUST 按指定模型路径处理（成功或上游失败均可诊断），不得因「仅允许手输」而拒绝
