# 添加账号能力优化方案（参考 Kiro-Go）

> 状态：设计文档（未实现）  
> 日期：2026-07-21  
> 范围：对比 Kiro-Go 与当前 kiro-rs 的「添加账号 / 凭据」实现，给出可落地的优化方案  
> 分析手段：双项目 CodeGraph 索引 + 源码精读（Admin API / Token 管理 / Admin UI）

---

## 1. Context and motivation

### 1.1 背景

kiro-rs 与 Kiro-Go 都是 Anthropic/Claude 兼容代理，核心能力之一是**管理多个 Kiro 上游账号（凭据）**。
当前 kiro-rs 的添加入口已可用，但相对 Kiro-Go 仍偏「手工粘贴 token」：

| 维度 | kiro-rs（现状） | Kiro-Go（参考） |
| --- | --- | --- |
| 主入口 | 手工表单 + JSON 批量 + KAM 导入 | 多 Tab 向导（BuilderId / IAM SSO / SSO Token / 本地缓存 / Cookie / JSON） |
| 服务端导入语义 | POST /api/admin/credentials 单接口（含 refresh 验活） | 原始添加 + 专用 import 通道（refresh 强制成功 + 拉用户信息） |
| 身份字段 | 无稳定 userId，email 可选 | UserId + Email，按 userId upsert 去重复活 |
| 去重策略 | refreshToken / kiroApiKey 的 SHA-256 | 主要按 UserId 合并；无 userId 则追加 |
| 批量 | 前端串行调单条 API + 验活 + 失败回滚 | 服务端 import + 前端循环；另有账号批处理 enable/disable/refresh/delete |
| 交互登录 | 无 | Device Code（BuilderId）、IAM SSO start/complete、SSO bearer 导入 |

### 1.2 问题陈述

1. **上手成本高**：用户需要自己从 IDE / Cookie / 导出文件中抠 token，容易截断、字段不全。
2. **去重语义偏弱**：同账号换 refreshToken 会变成「新凭据」；无法像 Kiro-Go 按稳定身份 upsert 并复活被禁用账号。
3. **批量与导入职责在前端过重**：批量导入走 N 次 HTTP，失败回滚依赖前端「先禁用再删除」，链路长、并发/超时难控。
4. **缺少在线授权流**：无法在 Admin 内完成 BuilderId / IAM SSO 登录闭环。
5. **身份元数据不完整**：无 userId / nickname / startUrl，后续审计、去重、展示受限。

### 1.3 Goals

- G1：在**不破坏**现有 POST /credentials 与 credentials.json 兼容的前提下，补齐「导入」与「在线授权」能力。
- G2：建立**稳定身份**（优先 userId，次选 refreshToken hash / apiKey hash）上的 upsert / 去重策略。
- G3：把「验活 + 拉用户信息 + 写盘 + 可选余额」收敛到服务端，前端只做编排与展示。
- G4：分阶段交付：先提升导入质量，再加批量 API，最后再做在线登录。
- G5：方案可验证：每个阶段有明确验收与测试清单。

### 1.4 Non-goals（首期不做）

- 不做 Kiro-Go 的完整审计日志 / request logs / API Key 管理面板搬迁。
- 不改变 Anthropic 兼容代理的请求路径与 SSE 协议。
- 不强制改成 UUID 账号 ID（继续使用现有自增 u64 id，稳定身份另字段承载）。
- 不在本设计中实现「账号导出到 Kiro-Go 格式」的完整互操作（可列为后续）。
- 不引入数据库；继续 JSON 文件持久化。

---

## 2. 现状深度分析

### 2.1 kiro-rs 添加账号链路

```text
Admin UI
  ├─ AddCredentialDialog  ──► POST /api/admin/credentials
  ├─ BatchImportDialog    ──► N × POST /credentials + balance 验活 + 失败回滚
  └─ KamImportDialog      ──► N × POST /credentials（兼容 KAM 嵌套/平铺 JSON）
         │
         ▼
Admin handlers → AdminService::add_credential
         │
         ▼
MultiTokenManager::add_credential
  1. 校验 refreshToken / kiroApiKey
  2. hash 去重（refreshToken 或 kiroApiKey）
  3. OAuth：refresh_token() 强制网络刷新；API Key：跳过
  4. 分配自增 id，合并元数据（priority/proxy/region/endpoint…）
  5. persist_credentials() → credentials.json（仅多凭据格式）
         │
         ▼
AdminService 再调 get_usage_limits_for（订阅等级，失败仅 warn）
```

**核心代码落点：**

| 层 | 路径 | 职责 |
| --- | --- | --- |
| 路由 | src/admin/router.rs | GET/POST /credentials、禁用/优先级/删除/刷新/余额 |
| Handler | src/admin/handlers.rs | HTTP 适配 |
| Service | src/admin/service.rs | 端点名校验、组装 KiroCredentials、错误分类 |
| 领域 | src/kiro/token_manager.rs | 去重、refresh 验活、持久化、负载均衡 |
| 模型 | src/kiro/model/credentials.rs | 字段与 region/proxy 语义 |
| UI | admin-ui/src/components/*-credential*.tsx、*-import-dialog.tsx | 单条 / 批量 / KAM |

**已有优势（应保留）：**

- 添加前强制 refresh，避免「半死不活」token 入库（与 Kiro-Go apiImportCredentials 同思路）。
- API Key 凭据独立路径（kiroApiKey / auth_method=api_key）。
- 前端 + 后端双层 hash 去重；批量导入有验活与回滚。
- 凭据级 proxy / auth_region / api_region / endpoint / priority 比 Go 更细。
- 删除需先禁用，降低误删风险。

**主要缺口：**

- 无 userId / nickname / startUrl。
- 无服务端「按身份 upsert」；换 token = 新记录。
- 无 BuilderId / IAM SSO / SSO Token 在线导入。
- 无服务端批量 import API；批量全靠前端。
- email 依赖请求体传入，不保证从上游拉取。
- CodeGraph impact：add_credential 影响面约 86 符号，改动需聚焦 token_manager + admin + UI，避免扩散到 proxy 主路径。

### 2.2 Kiro-Go 添加账号链路

```text
Admin UI (AddAccountModal Tabs)
  ├─ BuilderId device code  → POST /admin/api/auth/builderid/start|poll
  ├─ IAM SSO                → POST /admin/api/auth/iam-sso/start|complete
  ├─ SSO Token (可多行)     → POST /admin/api/auth/sso-token
  ├─ Local cache / Cookie / JSON → 组装后走 import 或 accounts
  └─ ImportAccountsModal    → 循环 POST /admin/api/auth/credentials
         │
         ▼
Handler 专用导入接口（刷新成功才入库 + GetUserInfo）
         │
         ▼
config.AddAccountReturning
  - 若 UserId 命中已有账号 → 覆盖 token / 复活 Enabled / 清 Ban
  - 否则 append + UUID id
         │
         ▼
pool.Reload() + 可选异步拉取模型列表 + AuditLog
```

**核心代码落点：**

| 层 | 路径 | 职责 |
| --- | --- | --- |
| 路由分发 | proxy/handler.go handleAdminAPI | accounts CRUD、auth/* 导入流 |
| 配置 | config/config.go Account / AddAccountReturning | 持久化、userId 去重 upsert |
| 认证 | auth/builderid.go auth/iam_sso.go auth/sso_token.go auth/oidc.go | 设备码 / SSO / refresh |
| 池 | pool/account.go | Reload、选号、冷却 |
| UI | web/src/components/AddAccountModal.jsx 等 | 多方式向导 |

**值得借鉴的设计点：**

1. **导入与裸添加分离**：apiImportCredentials 强制 refresh + GetUserInfo；apiAddAccount 偏原始写入。
2. **稳定身份 upsert**：UserId 命中则更新凭据并复活，避免重复账号。
3. **多入口在线授权**：Device Code / IAM SSO / SSO bearer 降低人工拷贝。
4. **导入后副作用明确**：pool.Reload、模型缓存预热、审计日志。
5. **批量运维 API**：/accounts/batch 支持 enable/disable/refresh/delete。

**不必照搬的点：**

- UUID 账号 ID（rs 已用自增 id + hash 去重，迁移成本高）。
- 把 usage 原始 JSON 整包塞进账号结构（rs 已有 balance 缓存路径）。
- Admin 密码 Cookie 鉴权方式（rs 使用 admin API Key，保持现状）。
- 账号与全局配置混在同一 config 文件的模型（rs 已分离 config / credentials）。

### 2.3 对照矩阵（能力级）

| 能力 | kiro-rs | Kiro-Go | 建议 |
| --- | --- | --- | --- |
| 手工添加 OAuth | 有 | 有 | 保留并增强 |
| API Key 凭据 | 有 | 部分（另有 client API Key 体系） | 保留 rs 优势 |
| 强制 refresh 验活 | 有 | 有（import 路径） | 保留 |
| refreshToken hash 去重 | 有 | 部分 | 保留 + 叠加 userId |
| userId upsert | 无 | 有 | **P0 引入** |
| 拉用户 email/userId | 部分（可选入参） | 有 GetUserInfo | **P0 引入** |
| KAM / JSON 批量 | 有（前端） | 有（前端） | 服务端化（P1） |
| 批量失败回滚 | 有（前端） | 弱 | 服务端事务语义（P1） |
| BuilderId 登录 | 无 | 有 | P2 |
| IAM SSO | 无 | 有 | P2 |
| SSO Token 导入 | 无 | 有 | P2 |
| 凭据级 proxy/endpoint | 有 | 部分 | 保留 rs |
| 审计日志 | 无 | 有 | P3 可选 |

---

## 3. Implementation considerations

### 3.1 设计原则

1. **Surgical Changes**：只改 admin / token_manager / credentials 模型 / admin-ui 导入相关文件；不动 anthropic 转换与 SSE。
2. **向后兼容**：现有 credentials.json 缺字段可读；旧前端只调 POST /credentials 仍可用。
3. **服务端权威**：验活、去重、upsert、用户信息以服务端为准；前端 hash 去重可保留为 UX 优化。
4. **导入必须可用**：OAuth 导入失败（refresh 失败）不得写入半残凭据（与现有 + Go import 一致）。
5. **安全**：响应中永不回传完整 refreshToken / clientSecret / apiKey；继续 hash/脱敏展示。
6. **OpenSpec 门禁**：本方案落地属于「Admin API / 凭据管理」高风险变更，**实现前必须建立 OpenSpec change**，并走 bridge → apply → compliance → verify。

### 3.2 约束

- 持久化仍为多凭据数组 JSON；单凭据格式不回写（现有 is_multiple_format 语义保留）。
- 异步运行时：网络 refresh / GetUserInfo 走 Tokio；写盘继续 block_in_place 或等价。
- 不在文档/日志打印真实 token。
- Windows 开发环境路径与现有 admin-ui（Vite + pnpm）保持兼容。

### 3.3 关键 trade-off

| 决策 | 选择 | 原因 |
| --- | --- | --- |
| 身份主键 | 新增 user_id，不改自增 id | 兼容现有 API / UI / 负载均衡引用 |
| 去重优先级 | userId > refreshToken hash > apiKey hash | 对齐 Go 的「同人合并」，同时保留 token 级防呆 |
| 同 userId 冲突策略 | upsert 覆盖 token 并复活（可配置） | 运营重导账号的主流场景 |
| 批量导入 | 新增服务端 batch 接口，前端逐步切换 | 降低 N 次 RTT 与回滚复杂度 |
| 在线登录 | 独立 /auth/* 路由模块，后置 | 依赖 OIDC 设备流，风险与范围更大 |

---

## 4. High-level behavior（目标态）

### 4.1 用户视角入口（目标）

Admin「添加凭据」统一为多 Tab：

1. **快速粘贴**（现有表单，增强自动识别 social/idc/api_key）
2. **JSON / KAM 导入**（调用服务端 batch import）
3. **BuilderId 登录**（device code + 轮询）
4. **IAM SSO**（startUrl + callback）
5. **SSO Token**（多行 bearer）
6. **API Key**（现有）

### 4.2 服务端统一「入库管道」

所有入口最终进入同一内部流程 ingest_credential(input) -> IngestResult：

```text
normalize(auth_method, provider, regions)
  → validate_shape
  → if oauth: refresh_token (must succeed)
  → if oauth: fetch_user_info (best-effort, 失败不阻断但记 warn)
  → resolve_identity (user_id / email / hashes)
  → match existing:
       - user_id hit → upsert (update tokens, clear disabled if revive)
       - refresh/api hash hit → reject duplicate OR upsert（策略开关）
       - else insert new id
  → persist
  → best-effort usage_limits / profile
  → return { id, action: created|updated, email, user_id }
```

### 4.3 与现有 API 的关系

| API | 行为变化 |
| --- | --- |
| POST /credentials | 默认走 ingest；响应增加 action / userId（可选字段，向后兼容） |
| POST /credentials/import（新） | 单条 import，语义=强制 refresh + user info（对标 Go /auth/credentials） |
| POST /credentials/import/batch（新） | 数组导入，逐条 ingest，返回 per-item 结果 |
| POST /auth/builderid/start|poll（新，P2） | 设备码会话 |
| POST /auth/iam-sso/start|complete（新，P2） | IAM SSO |
| POST /auth/sso-token（新，P2） | SSO bearer 批量 |

---

## 5. Domain design

### 5.1 数据模型增量

在 KiroCredentials 增加（全部 optional，序列化 camelCase 兼容 JSON）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| user_id | Option&lt;String&gt; | Kiro 稳定用户 ID；upsert 主键 |
| nickname | Option&lt;String&gt; | 展示名（KAM label / 手动） |
| start_url | Option&lt;String&gt; | IAM SSO start URL，便于再次登录 |

CredentialStatusItem 同步暴露：

- userId、nickname（可无则省略）
- 现有 email、hashes、endpoint 等保持

**不改动：**

- 自增 id: u64
- priority 越小越高
- disabled + disabled_reason
- kiro_api_key 路径

### 5.2 去重与 upsert 规则

处理顺序：

1. **API Key 凭据**：仅按 kiroApiKey hash 去重；命中 → 409 duplicate（默认不覆盖，因 key 轮换少见）。
2. **OAuth 且 user_id 非空**：
   - 命中已有 → **upsert**：更新 access/refresh/client/expires/profile/provider/region；Enabled 复活（clear disabled + reason）；保留原 id、priority（除非请求显式带 priority）、machine_id（空则不覆盖已有）。
   - 未命中 → 插入。
3. **OAuth 无 user_id**：回退 refreshToken hash：
   - 命中 → 409（与现网一致）
   - 未命中 → 插入
4. **可选策略开关**（请求参数 onConflict）：
   - reject（默认，兼容现网 hash 行为）
   - upsert（按 userId；无 userId 时不允许 silent upsert）
   - replace_token_only（仅更新 token 字段）

> 对齐 Kiro-Go：AddAccountReturning 在 UserId 命中时覆盖并复活；空 UserId 总是 append。

### 5.3 用户信息拉取

新增 get_user_info(access_token, proxy)：

- 成功：写 email、user_id
- 失败：不阻断入库；日志 warn；前端可提示「已添加但未取到邮箱」
- 来源优先级：请求体显式 email/userId > GetUserInfo > 空

### 5.4 批量导入语义

请求核心字段：

- items[]：兼容 AddCredentialRequest + KAM 平铺字段
- options.onConflict：reject | upsert | replace_token_only
- options.stopOnError：默认 false
- options.fetchBalance：默认 true
- options.concurrency：默认 1，上限 4

响应含 summary{created,updated,duplicate,failed} 与 results[]（index/status/credentialId/email/userId/error/balance）。

约束：

- 默认串行，避免上游限流
- 单条失败不回滚其它成功项
- 单条内部仍是「refresh 失败则不写盘」

### 5.5 在线授权（P2）会话模型

内存会话表（进程内 + TTL）：

- BuilderIdSession: session_id, device_code, client_id/secret, region, interval, expires_at
- IamSsoSession: session_id, start_url, region, pkce/state, expires_at

Poll/complete 成功后调用统一 ingest_credential，不要再写一套 AddAccount。

安全：

- 会话仅 Admin 鉴权可访问
- TTL 建议 10–15 分钟
- 完成后立即删除会话

---

## 6. API design（建议）

前缀保持 /api/admin，鉴权继续 x-api-key。

### 6.1 扩展现有添加

POST /credentials

- Body：现有 AddCredentialRequest + 可选 userId / nickname / startUrl / onConflict
- Response：现有字段 + 可选 action(created|updated) / userId

### 6.2 新接口

| Method | Path | 说明 | 阶段 |
| --- | --- | --- | --- |
| POST | /credentials/import | 单条 import（强制 refresh + user info） | P0 |
| POST | /credentials/import/batch | 批量 import | P1 |
| POST | /auth/builderid/start | 启动 device code | P2 |
| POST | /auth/builderid/poll | 轮询并入库 | P2 |
| POST | /auth/iam-sso/start | 启动 IAM SSO | P2 |
| POST | /auth/iam-sso/complete | 完成并入库 | P2 |
| POST | /auth/sso-token | SSO bearer 导入（支持多行） | P2 |

错误类型沿用 AdminErrorResponse：invalid_request / invalid_credential / upstream_error / not_found / internal_error。

### 6.3 UI 映射

| UI | 现网 | 目标 |
| --- | --- | --- |
| AddCredentialDialog | POST /credentials | 同左 + 展示 action/email |
| BatchImportDialog | N×POST | batch import；保留进度 UI |
| KamImportDialog | N×POST + normalize | 先 normalize，再 batch |
| 新 Login 向导 | 无 | auth/* |

---

## 7. Error handling and UX

| 场景 | 服务端 | 前端 |
| --- | --- | --- |
| refresh 失败 | 400 invalid_credential，不写盘 | toast「Token 无效/过期」 |
| 重复（reject） | 409/400 + 已存在 | 标 duplicate，显示已有 email |
| upsert 成功 | 200 action=updated | toast「已更新并复活 #id」 |
| user info 失败 | 仍 200，warn 日志 | 次要提示 |
| 批量部分失败 | 200 + results | 表格展示每条状态 |
| 限流 | 429/upstream | 降并发、退避 |
| 在线登录 pending | 200 completed=false | 继续 poll |
| 会话过期 | 400/404 | 引导重新 start |

回滚策略：

- 单条：失败即未入库，无需回滚。
- 批量：不自动删除已成功项。
- 废弃前端「先添加再 balance 失败再 disable+delete」作为主路径（可保留兼容开关）。

---

## 8. Implementation outline（分阶段）

### Phase P0 — 身份与导入管道（最高 ROI）

1. KiroCredentials / Admin 类型增加 user_id / nickname / start_url。
2. 实现 get_user_info（参考 Kiro-Go auth.GetUserInfo）。
3. 重构 MultiTokenManager::add_credential → 内部 ingest_credential（默认兼容现网）。
4. 支持 onConflict=upsert（userId 命中时更新并复活）。
5. POST /credentials 响应补充 action / userId。
6. 可选新增 POST /credentials/import。
7. 单测：refresh 失败不入库；hash 重复 reject；同 userId upsert 复活；API Key 不受影响。
8. Admin UI：toast 区分创建/更新；KAM 传入 nickname/userId。

### Phase P1 — 服务端批量导入

1. POST /credentials/import/batch + 结果聚合。
2. Batch / KAM 对话框改为 batch API。
3. concurrency 默认 1；大列表分 chunk（如 20）。
4. 集成测：混合 created/updated/duplicate/failed。
5. 文档：更新 README Admin 段与 example 字段说明（无真实密钥）。

### Phase P2 — 在线授权

1. 移植/重写 BuilderId device flow、IAM SSO、SSO Token（Rust）。
2. 会话存储 + TTL 清理。
3. Admin UI 多 Tab 向导。
4. 安全审查：会话泄漏、同源 admin、日志脱敏。
5. 手工验收：沙箱账号（禁止密钥入库）。

### Phase P3 — 体验与运维（可选）

1. 批量 enable/disable/refresh/delete 服务端 API。
2. 轻量审计（jsonl）。
3. 导入 dry-run。
4. 与 Kiro-Go 导出格式互转脚本。

---

## 9. Testing approach

### 9.1 单元测试（Rust）

- token_manager: ingest 创建 / upsert / duplicate / api_key
- get_user_info：HTTP mock
- auth 会话：start/poll pending/complete/expire（P2）
- batch：顺序结果与 stopOnError

### 9.2 集成 / Admin

- cargo test admin + token_manager 模块
- 路由层 handler 测试
- 前端：KAM 新旧格式；batch 结果渲染

### 9.3 手工

- 添加 social refreshToken → 有 email/userId
- 再次导入同 user 新 token → 更新而非双份
- API Key 添加 / 去重
- 批量 10 条混合失败
- （P2）BuilderId 真机授权

### 9.4 回归保护

参考 Kiro-Go import_credentials_test.go：

- refresh 失败拒绝入库
- 成功时使用上游 expiresAt，禁止盲猜短 TTL

---

## 10. Acceptance criteria

### P0

- [ ] OAuth 添加前 refresh 失败 → 不写 credentials.json
- [ ] 同 userId 二次导入 → 同一 id，token 更新，disabled 被清除
- [ ] 无 userId 时 refreshToken hash 重复 → 明确错误
- [ ] API Key 添加与 OAuth 共存、互不误伤
- [ ] 列表 API 可展示 userId/nickname（有则显示）
- [ ] 相关 cargo test 全绿

### P1

- [ ] 一次 batch 请求完成多凭据导入并返回 per-item 状态
- [ ] Admin 批量/KAM 对话框不再依赖「失败后前端回滚删除」作为主路径
- [ ] 大列表不默认高并发打爆上游

### P2

- [ ] BuilderId：展示 userCode + verificationUri，授权完成后账号出现在列表
- [ ] IAM SSO：start → 浏览器 → complete 闭环
- [ ] SSO Token 多行部分成功时返回成功列表与错误列表

---

## 11. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| upsert 误合并不同人 | 串号 | userId 必须来自上游；无 userId 禁止 upsert |
| GetUserInfo 端点变更 | 无邮箱 | best-effort；字段兼容多 JSON 形态 |
| 批量触发限流 | 导入失败 | 默认串行 + 退避 |
| 在线登录实现偏差 | 授权失败 | 对齐 Go 实现与 IDE 区域列表；沙箱验证 |
| credentials 字段膨胀 | 兼容性 | 全 optional + skip_serializing_if |
| 改动波及选号逻辑 | 代理故障 | 不改 select_next；仅入库与元数据 |

---

## 12. 建议落地顺序（给实现会话）

1. OpenSpec：openspec-new-change / openspec-propose 建 change（例如 improve-credential-ingest）。
2. Bridge：openspec-superpowers-bridge 绑定风险与验证命令。
3. 按 P0 → P1 → P2 实现；每阶段 spec-compliance-check + verification-before-completion。
4. 文档：本文件为设计源；实现细节与任务清单写入 openspec/changes/&lt;name&gt;/。
5. README 仅在用户可见启动/API 行为变化时同步。

---

## 13. 附录

### 13.1 CodeGraph 证据摘要

**kiro-rs**

- Entry：KiroCredentials、CredentialStatusItem、AddCredentialDialogProps
- Impact(add_credential)：admin service/handlers/router + MultiTokenManager + 相关测试
- Callers：POST /credentials 与 token_manager 单测
- Callees：refresh_token、persist_credentials、validate_refresh_token、sha256_hex、get_usage_limits_for

**Kiro-Go**

- Entry：Account、AccountPool
- AddAccountReturning callers：apiAddAccount、apiImportCredentials、apiImportSsoToken、apiPollBuilderIdAuth、apiCompleteIamSso
- 去重测试：config/dedup_test.go
- 导入回归：proxy/import_credentials_test.go

### 13.2 关键源码索引

| 项目 | 文件 |
| --- | --- |
| kiro-rs | src/admin/{router,handlers,service,types}.rs |
| kiro-rs | src/kiro/token_manager.rs、src/kiro/model/credentials.rs |
| kiro-rs | admin-ui/src/components/{add-credential,batch-import,kam-import}-dialog.tsx |
| Kiro-Go | proxy/handler.go（admin API / auth import） |
| Kiro-Go | config/config.go（Account / AddAccountReturning） |
| Kiro-Go | auth/{builderid,iam_sso,sso_token,oidc}.go |
| Kiro-Go | web/src/components/{AddAccountModal,ImportAccountsModal}.jsx |

### 13.3 与现网行为兼容说明

- 旧客户端只 POST refreshToken + authMethod → 行为与现网一致（refresh + hash 去重 + 自增 id）。
- 新客户端传 userId/onConflict 才启用 upsert。
- 旧 credentials.json 无新字段可正常加载。
- 单凭据文件格式仍不通过 Admin 回写（现有限制保留，文档可提示迁移到多凭据）。

---

## 14. 结论

kiro-rs 在「强制验活、API Key、凭据级代理/区域/端点、前端批量回滚」上已有良好基础；相对 Kiro-Go 的主要差距是 **稳定身份（userId）驱动的 upsert**、**服务端批量导入** 与 **在线授权多入口**。

推荐以 **统一 ingest 管道** 为中轴：

1. P0 补身份与 upsert（立刻减少重复账号与「换 token 变新号」问题）
2. P1 批量入库服务端化（降低前端复杂度与失败窗口）
3. P2 引入 BuilderId / IAM SSO / SSO Token（显著降低人工操作成本）

该路径兼容现有 API 与文件格式，满足项目「最小改动、可验证、高风险先 OpenSpec」的协作纪律。
