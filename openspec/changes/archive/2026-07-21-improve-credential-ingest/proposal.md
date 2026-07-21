# Proposal: improve-credential-ingest

## Why

Admin 添加/导入账号仍以「手工粘贴 token + 前端串行入库」为主。相对 Kiro-Go，缺少稳定身份（userId）驱动的 upsert、服务端批量 import，以及 BuilderId / IAM SSO / SSO Token 在线授权。结果是换 token 易产生重复凭据、批量导入脆弱、上手成本高。设计依据见 `docs/add-account-optimization-design.md`。

## What Changes

- 统一凭据入库管道 `ingest_credential`（验活、拉用户信息、去重/upsert、持久化）
- 凭据模型与 Admin 状态增加可选 `userId` / `nickname` / `startUrl`
- 添加/导入后 best-effort 拉取用户信息（email、userId）；OAuth 仍强制 refresh 成功才写盘
- 支持按 userId upsert（复活已禁用账号）；无 userId 时保留现有 refreshToken/apiKey hash 去重（默认 reject）
- 扩展 `POST /credentials` 响应（action/userId，向后兼容）
- 新增 `POST /credentials/import` 与 `POST /credentials/import/batch`
- Admin UI 批量/KAM 导入逐步改走 batch API；单条对话框展示创建/更新结果
- 分阶段新增在线授权 API 与 UI：BuilderId device code、IAM SSO、SSO Token（P2）
- **非 BREAKING**：旧客户端只传 refreshToken/authMethod 行为保持兼容

## Capabilities

### New Capabilities

- `credential-ingest`: 统一 ingest 管道、身份字段、userId upsert、GetUserInfo、单条/批量 import API 与默认冲突策略
- `credential-online-auth`: Admin 在线授权流（BuilderId / IAM SSO / SSO Token）会话与入库

### Modified Capabilities

- `credential-import`: 扩展 KAM/Admin 导入身份字段与验活/入库语义（与 ingest 对齐；不削弱既有 provider/profileArn 要求）

## Impact

- 源码：`src/kiro/model/credentials.rs`、`src/kiro/token_manager.rs`、`src/admin/{types,service,handlers,router}.rs`；P2 新增 auth 会话模块；`admin-ui` 导入对话框与 API 客户端
- API：`/api/admin/credentials*`、新增 `/credentials/import*`、P2 `/auth/*`
- 持久化：`credentials.json` 可选新字段；单凭据格式回写限制不变
- 风险类型：Admin API / 凭据管理 / Token 多凭据（高风险，实现前须 bridge）
- 参考设计：`docs/add-account-optimization-design.md`
- 示例配置/README：字段说明同步（无真实密钥）
