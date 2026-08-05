## MODIFIED Requirements

### Requirement: 凭据卡片提供真实推理测试入口

Each credential card MUST expose a test control that calls POST /api/admin/credentials/{id}/test with an optional model, and displays success metrics or a diagnosable failure. The test UI MUST offer selecting a model from that credential's cached model list (via GET /api/admin/credentials/{id}/models) in addition to free-form manual input.

#### Scenario: 默认模型测试入口

- **WHEN** 操作者打开测试入口且不填写 model 后提交
- **THEN** 请求体不强制错误的 model 字段（可省略或 null），并展示后端返回的 success/model/latencyMs/reply 或错误

#### Scenario: 指定模型测试

- **WHEN** 操作者输入合法 model 字符串并提交
- **THEN** 请求携带该 model，并展示对应结果

#### Scenario: 从缓存列表选择模型测试

- **WHEN** 凭据存在非空模型缓存且操作者打开测试入口
- **THEN** UI 展示可选模型列表（来自该凭据 models API），选择后提交请求携带所选 model

#### Scenario: 测试失败不泄露密钥

- **WHEN** test 失败
- **THEN** UI 展示错误摘要且不包含 refreshToken/accessToken 明文

### Requirement: 凭据卡片提供模型查看与刷新入口

Each credential card MUST expose controls to view that credential's cached model list and to refresh that credential's models via the existing Admin endpoints. After a successful refresh, the UI MUST update any open models view and SHOULD refresh credential list fields that surface model cache metadata (e.g. modelCount).

#### Scenario: 查看模型缓存

- **WHEN** 操作者点击某凭据的「查看模型」
- **THEN** UI 调用 GET /api/admin/credentials/{id}/models（默认缓存）并展示 models 列表；若有 lastError/updatedAt 则一并展示

#### Scenario: 单凭据刷新

- **WHEN** 操作者点击「刷新模型」且后端成功
- **THEN** UI 提示成功并包含 count（或 models 数量）信息；可选刷新查看对话框内容

#### Scenario: 单凭据刷新失败可诊断

- **WHEN** 后端返回错误（如上游 403）
- **THEN** UI 展示可诊断错误消息，且不含密钥明文

#### Scenario: 从查看模型发起测试

- **WHEN** 操作者在模型列表中选择某 modelId 并选择「用此模型测试」（或等价）
- **THEN** 打开测试入口且预填该 modelId

## ADDED Requirements

### Requirement: 凭据列表展示模型缓存元数据

The credentials status API consumed by Admin UI MUST expose optional per-credential model cache metadata (at least modelCount, and SHOULD include modelsUpdatedAt and/or modelsLastError when known) so operators can see cache readiness without opening the models dialog.

#### Scenario: 列表含 modelCount

- **WHEN** 某凭据已有非空 model 缓存
- **THEN** GET /api/admin/credentials 对应该项包含 modelCount >= 1（或等价字段）

#### Scenario: 无缓存时

- **WHEN** 凭据尚无模型缓存
- **THEN** modelCount 为 0 或字段省略，UI 不得崩溃

### Requirement: 凭据卡片提供余额强制刷新入口

Each credential card MUST expose a primary control to refresh balance/usage for that credential with cache bypass (force), complementary to the dashboard batch「查询信息」action. The legacy「重置失败」control MAY remain but MUST NOT be the only/primary recovery path when balance refresh is available.

#### Scenario: 单卡刷新余额

- **WHEN** 操作者点击「刷新余额」（或等价）
- **THEN** UI 以 force 方式请求该凭据 balance/usage 并更新卡片展示的订阅/剩余信息

#### Scenario: 与批量查询信息互补

- **WHEN** 操作者使用顶栏「查询信息」
- **THEN** 行为仍为当前页批量 balance 查询；与单卡 force 刷新可并存，不互相删除

#### Scenario: 重置失败降级

- **WHEN** 凭据 failureCount 与 refreshFailureCount 均为 0 且未因失败禁用
- **THEN** 「重置失败/恢复」类入口可禁用或弱化展示，且主路径不依赖该按钮完成余额更新
