## MODIFIED Requirements

### Requirement: Admin 可对凭据做真实推理探测

Admin API MUST expose POST /api/admin/credentials/{id}/test that ensures a valid token and issues a minimal non-streaming upstream generation for a specified or default model. When model is omitted, the server MUST use a documented default model id. When model is provided, it MUST be resolved through the unified model-resolution pipeline (aliases, normalize, catalog passthrough policy, thinking rules) before generate.

#### Scenario: 默认模型成功

- **WHEN** 对有效凭据调用 test 且不指定 model（或使用默认）
- **THEN** 返回 success=true、所用 model，以及非空 reply 或等价上游可展示输出，并包含延迟类指标（可选但推荐）

#### Scenario: 指定可解析模型

- **WHEN** 请求体指定可被 resolve_model 接受的 model 字符串
- **THEN** 使用解析后的上游 modelId 发起探测，并在响应中报告请求 model 与 resolvedModel（及 resolveKind，若实现暴露）

#### Scenario: auto 可测

- **WHEN** 请求体指定 model=auto
- **THEN** test MUST NOT 因 unmapped 在本地 400 失败；应解析为 defaultChatModel 并进入 generate 路径（成功或上游失败均可诊断）

#### Scenario: catalog 透传模型可测

- **WHEN** 凭据或全局 catalog 含 gpt-5.6-sol，allowCatalogPassthrough 为 true，且 test 指定 model=gpt-5.6-sol
- **THEN** test MUST NOT 返回本地 unmapped 映射失败；应进入上游 generate（成功或上游错误）

#### Scenario: 非法或无法解析模型

- **WHEN** 指定无法被 resolve_model 接受的 model
- **THEN** 返回 400 类客户端错误且不发起上游 generate；错误 message MUST NOT 使用凭据无效作为模型解析失败的前缀

#### Scenario: 客户端可用缓存列表中的可解析 modelId

- **WHEN** 操作者使用该凭据 models 列表中标记为 testable/resolvable 的 modelId 调用 test
- **THEN** test MUST 按指定模型路径处理（成功或上游失败均可诊断），不得因仅允许手输而拒绝

### Requirement: 测试失败可诊断且不泄露密钥

On token refresh failure, model resolution failure, or upstream generate failure, the test endpoint MUST return a clear error without including accessToken, refreshToken, or full sensitive credentials. Model resolution failures MUST be distinguishable from credential/token failures in the message or error type.

#### Scenario: 模型解析失败不等于凭据损坏

- **WHEN** test 因 model 无法解析失败
- **THEN** 响应说明模型无法解析或不可用，且不暗示凭据本身无效，响应体不含密钥明文

#### Scenario: Token 无效

- **WHEN** 凭据无法获得有效 access token
- **THEN** test 失败响应说明 token/refresh 问题，响应体不含密钥明文

#### Scenario: 上游推理失败

- **WHEN** token 有效但上游 generate 返回错误
- **THEN** test 失败并包含可诊断的错误摘要（状态码或截断 body），不含密钥
