# Tasks: improve-credential-ingest

## 1. 模型与请求类型

- [x] 1.1 为 KiroCredentials 增加可选 user_id / nickname / start_url（serde camelCase）及序列化 roundtrip 测试
- [x] 1.2 扩展 AddCredentialRequest / CredentialStatusItem / AddCredentialResponse（userId、nickname、startUrl、onConflict、action）
- [x] 1.3 更新 credentials.example.*.json 与 README/Admin 字段说明（无真实密钥）

## 2. GetUserInfo 与 ingest 核心（P0）

- [x] 2.1 实现 mockable get_user_info（对照 Kiro-Go），失败 best-effort
- [x] 2.2 将 MultiTokenManager::add_credential 重构为内部 ingest_credential（默认行为兼容现网）
- [x] 2.3 实现冲突规则：apiKey hash；userId upsert；refreshToken hash reject；无 userId 禁止 silent upsert
- [x] 2.4 upsert 时保留原 id，更新 token/auth 字段，clear disabled + disabled_reason
- [x] 2.5 单测：refresh 失败不落盘；hash 重复 reject；同 userId upsert 复活；API Key 共存与去重；GetUserInfo mock

## 3. Admin 单条 API（P0）

- [x] 3.1 POST /credentials 走 ingest；响应附加 action/userId（向后兼容）
- [x] 3.2 新增 POST /credentials/import（import 默认 onConflict=upsert 策略按 design）
- [x] 3.3 错误分类覆盖 duplicate / invalid refresh / upstream
- [x] 3.4 Admin handler/service 测试或等价模块测

## 4. 批量 import API 与 UI（P1）

- [x] 4.1 实现 POST /credentials/import/batch（summary + results；concurrency 默认 1）
- [x] 4.2 stopOnError / fetchBalance 选项按 spec 生效
- [x] 4.3 admin-ui API 客户端增加 import/batch 方法
- [x] 4.4 BatchImportDialog 改为主路径调用 batch（可 chunk），渲染 per-item 状态
- [x] 4.5 KamImportDialog 映射 userId/nickname/startUrl，走 batch；保留 KAM 新旧格式 normalize
- [x] 4.6 AddCredentialDialog 展示 created/updated；类型定义同步

## 5. 在线授权（P2）

- [x] 5.1 会话存储 + TTL 清理（BuilderId / IAM SSO）
- [x] 5.2 POST /auth/builderid/start|poll 并对齐 device code 流程，完成后 ingest
- [x] 5.3 POST /auth/iam-sso/start|complete，完成后 ingest 并保留 startUrl
- [x] 5.4 POST /auth/sso-token 多行导入与部分成功响应
- [x] 5.5 Admin UI 多 Tab 向导（BuilderId / IAM SSO / SSO Token）
- [x] 5.6 会话过期/未授权测试；日志脱敏检查

## 6. 与既有 profile/import 语义对齐

- [x] 6.1 确认 ingest 不丢弃请求中的 provider/profileArn（兼容 credential-import）
- [x] 6.2 导入后继续 resolve profile + usage；UI 保留 verified vs verified_warn
- [x] 6.3 batch 条目级 profile 警告可区分且默认不失败整批

## 7. 验证与门禁

- [x] 7.1 运行 openspec-superpowers-bridge 并落 evidence
- [x] 7.2 cargo test（token_manager / admin / 相关模块）
- [x] 7.3 若改 admin-ui：pnpm build
- [x] 7.4 openspec validate --all
- [x] 7.5 spec-compliance-check + openspec-verify-change + verification-before-completion evidence
- [x] 7.6 git status --short 确认无真实凭据与 .codegraph/ 误入
