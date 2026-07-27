## MODIFIED Requirements

### Requirement: 凭据卡片提供真实推理测试入口

Each credential card MUST expose a test control that calls POST /api/admin/credentials/{id}/test with an optional model, and displays success metrics or a diagnosable failure. The test UI MUST offer selecting a model from that credential cached model list (via GET /api/admin/credentials/{id}/models) in addition to free-form manual input. The model selector MUST remain readable in dark mode when expanded, and SHOULD prefer listing only resolvable/testable model ids when the backend provides that metadata.

#### Scenario: 默认模型测试入口

- **WHEN** 操作者打开测试入口且不填写 model 后提交
- **THEN** 请求体不强制错误的 model 字段（可省略或 null），并展示后端返回的 success/model/latencyMs/reply 或错误

#### Scenario: 指定模型测试

- **WHEN** 操作者输入合法 model 字符串并提交
- **THEN** 请求携带该 model，并展示对应结果

#### Scenario: 从缓存列表选择模型测试

- **WHEN** 凭据存在非空模型缓存且操作者打开测试入口
- **THEN** UI 展示可选模型列表（来自该凭据 models API），选择后提交请求携带所选 model

#### Scenario: dark 模式下模型列表可读

- **WHEN** 操作者在 dark 模式下展开测试模型选择列表
- **THEN** 列表背景与文字对比清晰，不得出现白底不可读

#### Scenario: 优先可测模型

- **WHEN** 后端 models 响应包含 testable/resolvable 元数据
- **THEN** 测试下拉默认仅展示 testable 项，或明确区分不可测项且不可误提交为假可用

#### Scenario: 测试失败不泄露密钥

- **WHEN** test 失败
- **THEN** UI 展示错误摘要且不包含 refreshToken/accessToken 明文
