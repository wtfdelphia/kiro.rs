# Capability: admin-ui-model-ops

## Purpose

Expose Admin UI entry points so operators can refresh and inspect the model catalog and run credential generation smoke tests without using raw HTTP clients. Consumes existing Admin APIs only.

## ADDED Requirements

### Requirement: Dashboard 提供全量模型刷新入口

The Admin UI MUST provide a visible control on the main credentials dashboard that calls POST /api/admin/credentials/models/refresh and surfaces a human-readable summary of refreshed, failed, and globalCount, plus failure details when failed > 0.

#### Scenario: 全量刷新按钮可见

- **WHEN** 操作者已登录 Admin 并打开凭据 Dashboard
- **THEN** 页面上存在可点击的「刷新全部模型」（或等价文案）入口

#### Scenario: 全量刷新部分失败

- **WHEN** 后端返回 success 且 failed > 0 与 errors 列表
- **THEN** UI 展示成功/失败计数，并展示至少一条 credentialId + error 摘要，且摘要中不含 accessToken/refreshToken 明文

#### Scenario: 全量刷新进行中

- **WHEN** 刷新请求尚未完成
- **THEN** 入口处于 loading 或 disabled，避免重复提交

### Requirement: 凭据卡片提供模型查看与刷新入口

Each credential card MUST expose controls to view that credential's cached model list and to refresh that credential's models via the existing Admin endpoints.

#### Scenario: 查看模型缓存

- **WHEN** 操作者点击某凭据的「查看模型」
- **THEN** UI 调用 GET /api/admin/credentials/{id}/models（默认缓存）并展示 models 列表；若有 lastError/updatedAt 则一并展示

#### Scenario: 单凭据刷新

- **WHEN** 操作者点击「刷新模型」且后端成功
- **THEN** UI 提示成功并包含 count（或 models 数量）信息；可选刷新查看对话框内容

#### Scenario: 单凭据刷新失败可诊断

- **WHEN** 后端返回错误（如上游 403）
- **THEN** UI 展示可诊断错误消息，且不含密钥明文

### Requirement: 凭据卡片提供真实推理测试入口

Each credential card MUST expose a test control that calls POST /api/admin/credentials/{id}/test with an optional model, and displays success metrics or a diagnosable failure.

#### Scenario: 默认模型测试入口

- **WHEN** 操作者打开测试入口且不填写 model 后提交
- **THEN** 请求体不强制错误的 model 字段（可省略或 null），并展示后端返回的 success/model/latencyMs/reply 或错误

#### Scenario: 指定模型测试

- **WHEN** 操作者输入合法 model 字符串并提交
- **THEN** 请求携带该 model，并展示对应结果

#### Scenario: 测试失败不泄露密钥

- **WHEN** test 失败
- **THEN** UI 展示错误摘要且不包含 refreshToken/accessToken 明文

### Requirement: 前端 API 封装覆盖模型与测试端点

The Admin UI client layer MUST provide typed functions for models refresh (single/all), models list, and credential test, using the same Admin auth header mechanism as existing credential APIs.

#### Scenario: 客户端路径正确

- **WHEN** UI 触发上述操作
- **THEN** 请求分别打到 /credentials/models/refresh、/credentials/{id}/models/refresh、/credentials/{id}/models、/credentials/{id}/test，并携带既有 x-api-key（Admin key）机制

## Non-Goals (spec level)

- MUST NOT require new backend endpoints for this capability
- MUST NOT redesign online-auth or import dialogs already present

### Requirement: 模型/测试入口不被错误固定 profileArn 阻断

When upstream accepts ListAvailableModels or generate without a profileArn but rejects known fixed placeholder profileArn values with 403 unauthorized, the system MUST avoid treating those placeholders as authoritative profile resolution, and MUST recover by retrying without the bad profileArn (and SHOULD clear persisted placeholders) so Admin model refresh/view/test entry points can succeed for otherwise healthy credentials.

#### Scenario: 固定占位 ARN 不阻断 ListAvailableModels

- **WHEN** 凭据存有 BuilderId 固定占位 profileArn 且上游对该 ARN 返回 403 unauthorized，但对无 ARN 请求返回 200
- **THEN** 模型刷新路径最终以无 ARN 成功获取列表，且不得仅因该占位 ARN 永久失败

#### Scenario: 固定占位 ARN 不阻断 generate/test

- **WHEN** 凭据存有上述占位 profileArn 且 generate 返回 User is not authorized
- **THEN** 系统清除或跳过该坏 ARN 后重试，使 test /v1/messages 在账号健康时可成功

