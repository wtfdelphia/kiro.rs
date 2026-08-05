## 1. API 与类型

- [x] 1.1 在 `admin-ui/src/types/api.ts` 增加 ModelsRefreshResponse / ModelsRefreshAllResponse / CredentialModelsResponse / TestCredentialRequest|Response 等 camelCase 类型
- [x] 1.2 在 `admin-ui/src/api/credentials.ts` 实现：
  - `refreshCredentialModels(id)`
  - `refreshAllModels()`
  - `getCredentialModels(id, live?)`
  - `testCredential(id, model?)`
- [x] 1.3 （可选）`hooks/use-credentials.ts` 增加对应 mutation，失败/成功 invalidate credentials 或本地状态

## 2. Dashboard 全量入口

- [x] 2.1 工具栏或列表操作区增加「刷新全部模型」按钮（loading/禁用态）
- [x] 2.2 调用 refreshAllModels；toast 展示 refreshed/failed/globalCount
- [x] 2.3 failed>0 时展示 errors 摘要（Dialog 或可展开 toast 描述）

## 3. Credential 卡片入口

- [x] 3.1 卡片操作区增加「查看模型」「刷新模型」「测试」按钮
- [x] 3.2 「刷新模型」调用单凭据 refresh，成功 toast count/models 摘要
- [x] 3.3 「查看模型」Dialog：列表、updatedAt、lastError；可触发刷新
- [x] 3.4 「测试」Dialog：可选 model 输入、提交、展示 success/model/latencyMs/reply 或错误

## 4. 体验与安全

- [x] 4.1 所有错误经 extractErrorMessage；UI/toast 不展示 token 类密钥
- [x] 4.2 禁用凭据或 API Key 凭据的按钮可用性策略合理（禁用时仍可查看缓存；test/refresh 可禁用或允许后端返回错误）
- [x] 4.3 loading 与重复提交防护

## 5. 文档与验证

- [x] 5.1 README Admin 段补充 UI 入口说明（一句即可）
- [x] 5.2 `pnpm --dir admin-ui build` 通过
- [x] 5.3 对照 Admin 路由做一次手动或脚本冒烟（按钮存在 + API 被调用；上游账号状态不强制成功）
- [x] 5.4 `openspec validate --all`；`git status --short` 无密钥误入

## 6. 错误固定 profileArn 导致 403（增补）

- [x] 6.1 profile.rs：识别固定占位 ARN 为不可信；解析时不短路/不无条件持久化固定 ARN
- [x] 6.2 models_api.rs：ListAvailableModels 带 ARN 收到 403 unauthorized 时无 ARN 重试；成功可清理坏 ARN
- [x] 6.3 provider.rs（及 admin test generate 路径）：generate 遇 User is not authorized 且含 profileArn 时清除坏 ARN 并重试一次
- [x] 6.4 单元测试覆盖占位 ARN 识别与 URL/注入行为；相关 cargo test 通过
- [x] 6.5 本地对 18990（或 cargo test 证据）验证 models/test/messages 不再因固定 ARN 必 403

