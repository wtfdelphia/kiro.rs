# Design: improve-credential-ingest

## Context

参考设计：`docs/add-account-optimization-design.md`。

**当前实现（kiro-rs）**

- 唯一入库入口：`POST /api/admin/credentials` → `AdminService::add_credential` → `MultiTokenManager::add_credential`
- OAuth：校验 refreshToken → SHA-256 去重 → **强制 refresh** → 自增 id → persist（仅多凭据格式）
- API Key：`kiroApiKey` 校验与 hash 去重，跳过 refresh
- 前端 Batch/KAM：N 次单条 API + 余额验活；失败则 disable+delete 回滚
- 模型无稳定 `userId`；email 依赖请求体；无在线授权

**参考实现（Kiro-Go）**

- `apiImportCredentials`：refresh 成功 + GetUserInfo 再入库
- `AddAccountReturning`：按 `UserId` upsert 并复活
- BuilderId / IAM SSO / SSO Token 多入口

**约束**

- 继续 JSON 文件持久化与自增 u64 id；Admin 鉴权保持 x-api-key
- 不改 Anthropic/SSE 主路径
- 实现须 OpenSpec bridge → apply → compliance → verify
- 不落真实密钥；日志脱敏

## Goals / Non-Goals

**Goals:**

- G1：统一 `ingest_credential`，所有添加入口共用
- G2：可选身份字段 userId/nickname/startUrl；OAuth best-effort GetUserInfo
- G3：userId 命中可 upsert 复活；无 userId 保持 hash reject（默认）
- G4：单条 import + batch import API；前端逐步去 N 次回滚主路径
- G5：P2 在线授权最终也走 ingest
- G6：向后兼容旧客户端与旧 credentials.json

**Non-Goals:**

- 不搬迁 Kiro-Go 审计/request logs/完整 API Key 面板
- 不改负载均衡选号算法、SSE 协议
- 不改为 UUID 账号 ID、不引入数据库
- 不在本 change 做完整 Kiro-Go 导出互操作产品化
- P2 可与 P0/P1 分 PR，但规格先完整定义

## Decisions

### D1：统一 ingest 管道

所有入口（POST /credentials、import、batch、在线授权完成）最终调用内部 `ingest_credential`：

1. normalize auth_method/provider/regions  
2. validate_shape  
3. OAuth：refresh MUST succeed（失败不写盘）  
4. OAuth：GetUserInfo best-effort  
5. resolve identity（请求体优先，其次 GetUserInfo）  
6. match：userId upsert / hash 冲突 / insert  
7. persist + best-effort usage_limits / profile 既有逻辑  

**备选**：多 handler 各写一套 → 拒绝（重复与行为漂移）。

### D2：身份主键用 userId，不改自增 id

- 新增 `user_id: Option<String>`（JSON `userId`）  
- 外部 API 仍用 u64 id  
- 无 userId 时禁止 silent upsert  

**备选**：改 UUID → 迁移成本高，拒绝。

### D3：冲突策略 onConflict

- `reject`（默认）：hash 重复 → invalid_credential/duplicate（兼容现网）  
- `upsert`：userId 命中更新 token 并 clear disabled；无 userId 不得 silent upsert  
- `replace_token_only`：仅 token 字段更新（可选，任务可后置）  

API Key 默认仅 hash reject，不按 userId 覆盖。

### D4：GetUserInfo best-effort

- 失败不阻断入库；warn 日志；响应可缺 email/userId  
- 来源优先级：请求体 > GetUserInfo > 空  
- HTTP 实现 mockable，单测不打真实网  

### D5：批量 API 服务端权威

`POST /credentials/import/batch`：

- items[] 兼容 AddCredentialRequest + KAM 平铺  
- options：onConflict、stopOnError=false、fetchBalance、concurrency 默认 1（上限 4）  
- 返回 summary + per-item results  
- 无跨条目事务  

前端 Batch/KAM 改为调用 batch（可分 chunk）；前端 hash 预检可保留作 UX。

### D6：在线授权独立模块（P2）

- 进程内会话 + TTL 10–15min  
- 路由：`/auth/builderid/start|poll`、`/auth/iam-sso/start|complete`、`/auth/sso-token`  
- 成功后只调 ingest  
- 对齐 Kiro-Go 流程与区域列表；沙箱手工验收  

### D7：兼容与字段

- 旧 POST body 行为不变  
- 新字段全 optional + skip_serializing_if  
- 响应新增 action/userId 为附加字段  
- 保留既有 provider/profileArn 导入语义（credential-import）

## Risks / Trade-offs

| Risk | Mitigation |
| --- | --- |
| userId 误合并导致串号 | userId 优先来自上游；无 userId 禁止 upsert |
| GetUserInfo 端点/字段变更 | best-effort + 多形态解析 + mock 测 |
| 批量打爆上游限流 | 默认 concurrency=1 + 退避 |
| 在线授权实现偏差 | 对齐 Go；会话 TTL；P2 独立验收 |
| 改动波及选号 | 不改 select_next；仅入库/元数据 |
| 前端回滚路径残留 | P1 后主路径走 batch；旧回滚可删或兼容开关 |

## Migration Plan

1. 部署含可选新字段的后端（旧 UI 仍可用）  
2. 启用 import/batch；切换 Admin UI  
3. 再上线 P2 在线授权 Tab  
4. **回滚**：停用新路由/开关；旧 POST 路径保留；文件中新字段可忽略  

## Open Questions

1. GetUserInfo 的准确上游 URL/字段是否与 Kiro-Go 完全一致？实现时对照 `auth.GetUserInfo` 并记录在 evidence。  
2. batch 是否需要 dry-run？默认 P3，除非实现中成本极低。  
3. onConflict 默认是否对「已有 userId 的二次导入」自动 upsert？规格默认需显式 `onConflict=upsert` 或 import 接口默认 upsert——**建议 import/batch 默认 upsert，裸 POST /credentials 默认 reject**，以兼容现网并改善导入 UX。

## 验证策略

- 单测：ingest 创建/upsert/duplicate/api_key；refresh 失败不落盘；GetUserInfo mock  
- batch：mixed results、stopOnError、concurrency=1  
- P2：会话 pending/complete/expire  
- `cargo test` admin + token_manager；改 UI 则 `pnpm build`  
- `openspec validate --all`  
- 手工：同 user 二次导入合并；禁止真实密钥入库  
