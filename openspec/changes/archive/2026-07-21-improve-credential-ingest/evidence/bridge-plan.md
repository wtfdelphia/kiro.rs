# Bridge Plan: improve-credential-ingest

> 生成时间：2026-07-21  
> Skill：openspec-superpowers-bridge  
> 变更：`openspec/changes/improve-credential-ingest`  
> 分支：`dev` @ `9bbab39`  
> 参考设计：`docs/add-account-optimization-design.md`

## 1. 状态检查

| 检查项 | 结果 |
| --- | --- |
| openspec status | 4/4 artifacts complete（proposal/design/specs/tasks = done） |
| openspec validate | `improve-credential-ingest` **valid** |
| isComplete / blocked | complete；**非 blocked**；可进入 apply |
| 工作区敏感文件 | `config.json` / `credentials.json` **不存在**；gitignore 已忽略 |
| 未跟踪 | `docs/add-account-optimization-design.md`、`openspec/changes/improve-credential-ingest/` |
| 真实密钥入库风险 | 当前无（仅 example 占位符） |

**结论：允许开始实现（建议按 P0 → P1 → P2 分批）。**

## 2. 范围 / 非目标 / 关键设计决策

### 范围（本 change）

1. **P0**：统一 `ingest_credential`；`userId`/`nickname`/`startUrl`；GetUserInfo best-effort；userId upsert；扩展 POST /credentials；POST /credentials/import  
2. **P1**：POST /credentials/import/batch；Admin Batch/KAM UI 主路径改 batch  
3. **P2**：在线授权 `/auth/builderid/*`、`/auth/iam-sso/*`、`/auth/sso-token` + UI 向导  
4. 保持与 `credential-import` 既有 provider/profileArn / verified_warn 语义兼容  

### 非目标

- 不搬 Kiro-Go 审计/request logs/完整 API Key 面板  
- 不改 Anthropic SSE、负载均衡选号算法  
- 不改为 UUID id / 不引入 DB  
- 不做完整 Kiro-Go 导出互操作产品化  

### 关键决策（与 design 对齐，实现不得偏离）

| ID | 决策 |
| --- | --- |
| D1 | 唯一入库管道 ingest（所有入口最终调用） |
| D2 | 新增可选 user_id，**不**改自增 u64 id |
| D3 | onConflict：裸 POST 默认 reject；import/batch 默认 upsert（有 userId 时） |
| D4 | GetUserInfo best-effort；请求体优先 |
| D5 | batch concurrency 默认 1；无跨条目事务 |
| D6 | 在线授权会话 TTL + 完成后 ingest |
| D7 | 全 optional 字段，旧客户端兼容 |

## 3. 高风险项

| 风险 | 类型 | 缓解 |
| --- | --- | --- |
| 凭据 upsert 误合并 | 凭据/Admin | userId 来自上游或显式入参；无 userId 禁止 silent upsert |
| refresh 失败半残入库 | Token | 保持「OAuth refresh MUST succeed 才写盘」 |
| GetUserInfo 端点/字段漂移 | 上游协议 | mockable；best-effort；对齐 Kiro-Go `auth.GetUserInfo`（见 §5） |
| 批量限流 | 上游 | concurrency=1 默认 |
| 改动波及选号/代理主路径 | 协议旁路 | **禁止**改 select_next / anthropic converter / SSE；仅 admin + token_manager 入库与元数据 |
| 密钥泄漏 | 安全 | 响应/日志不回传 refreshToken/clientSecret/apiKey；`git status` 门禁 |
| profileArn 回归 | 既有能力 | ingest 不得丢弃 provider/profileArn；保留 resolve + verified_warn |

风险类型（AGENTS 矩阵）：**Admin API / 凭据管理 / Token 多凭据**。

## 4. CodeGraph 证据

### 命令（本会话真实运行）

```text
codegraph status
codegraph context "credential ingest add import userId upsert admin"
codegraph impact "add_credential"
codegraph callers "add_credential"
codegraph callees "add_credential"
```

### 结论

- 索引：up to date（约 89 files / 1325 nodes）  
- **Entry（UI）**：`AddCredentialDialog`、`useAddCredential`、Dashboard 导入对话框  
- **Impact(`add_credential`)**：约 **86** 符号 — 集中在  
  - `src/admin/{service,handlers,router,types,error}.rs`  
  - `src/kiro/token_manager.rs`（`MultiTokenManager::add_credential`、persist、refresh、usage）  
  - 既有 add 单测  
- **Callers**：`POST /credentials` + token_manager 单测（**无** anthropic handler 直接 caller）  
- **Callees**：`refresh_token`、`persist_credentials`、`validate_refresh_token`、`sha256_hex`、`get_usage_limits_for`、`is_api_key_credential`  

**实现边界（surgical）：**

| 应改 | 慎改 / 不改 |
| --- | --- |
| credentials 模型、token_manager ingest | `select_next_credential` / 负载均衡算法 |
| admin types/service/handlers/router | anthropic converter/SSE |
| admin-ui credentials API + import dialogs | Docker/CI（除非文档提及 API） |
| P2 新 auth 会话模块 | 全局 config schema 行为（除非仅文档） |

## 5. rg / 源码补盲

### 命令

```text
rg credentials.json|adminApiKey|AddCredential|userId|import ...
rg README/config.example/credentials.example/docker
rg (Kiro-Go) GetUserInfo
```

### 发现

| 区域 | 现状 | 实现注意 |
| --- | --- | --- |
| Admin 路由 | 仅 GET/POST /credentials 及 per-id 操作 | 新增 import、import/batch、auth/* |
| 模型 | 无 user_id/nickname/start_url | 全 optional + camelCase |
| 前端 | 单条/batch/KAM 全走 POST /credentials | P1 改 batch；类型同步 |
| examples | 无 userId 字段 | task 1.3 补充占位说明 |
| README | credentials 字段表、Admin 段 | API 行为变化时同步 |
| config.example | adminApiKey 占位 | **不**改鉴权方式 |
| docker-compose | 挂载 credentials | 无 schema 强制变更 |
| 安全 | gitignore credentials.*；工作区无真实文件 | 保持 |
| Kiro-Go GetUserInfo | `auth/sso_token.go`：GET usage 风格 URL，JSON `userInfo.email` / `userInfo.userId`；Bearer accessToken；失败可忽略 | rs 实现 mockable + 同字段解析；默认 URL 对齐 Go 的 userInfoURL |

**CodeGraph 盲区已补：** 配置示例、README Admin 文档、gitignore 凭据路径、Go 侧 GetUserInfo 契约。

## 6. 规格一致性抽检

| 能力 | Requirements | Scenarios |
| --- | --- | --- |
| credential-ingest | 8 | 17 |
| credential-online-auth | 5 | 10 |
| credential-import (delta) | 5 | 8 |
| tasks 未完成 | 33 open / 0 done | |

proposal 能力列表 ↔ specs 目录：一致（ingest / online-auth 新建；import 修改）。

Open question（design）：import 默认 upsert vs 裸 POST 默认 reject — **按 design 落地**，实现时在 service 层按入口设默认 onConflict，并写测试锁死。

## 7. 任务 → 执行步骤映射

| 任务组 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1 模型与类型 | 改 credentials.rs + admin types + examples/README 字段 | 序列化 roundtrip 单测；example JSON 可 parse | 破坏旧 JSON 加载 |
| 2 ingest 核心 | get_user_info + 重构 add→ingest + 冲突规则 | cargo test token_manager：fail refresh / dup / upsert / api_key | 默认路径行为相对现网回归 |
| 3 Admin 单条 API | router/handlers/service + import | handler/service 测；手动 curl 仅用假数据 | 响应泄露密钥 |
| 4 batch + UI | batch API + admin-ui | cargo test + pnpm build；UI 渲染 per-item | 默认高并发；整批误失败 |
| 5 在线授权 | 会话 + auth 路由 + UI | 单测会话 TTL；沙箱手工（密钥不入库） | 会话不过期/未鉴权可访问 |
| 6 profile 兼容 | 确认 provider/profileArn 路径 | 既有 profile/KAM 测 + verified_warn | 回归「余额可对话否」语义 |
| 7 门禁 | bridge(本文件)→test→validate→compliance→verify→completion | 见 §8 | 任一必跑失败未解释 |

**推荐实现批次：**

1. Tasks 1–3 + 6.1 + 7.2/7.4 子集 → **P0 可合并**  
2. Tasks 4 + 6.2–6.3 → **P1**  
3. Task 5 → **P2**  
4. Task 7 全量在宣称完成 / PR 前  

## 8. 必跑验证

### 每阶段 / 宣称完成前

| 命令 | 目的 |
| --- | --- |
| `openspec validate --all` | 工件合法 |
| `cargo test`（至少 token_manager + admin 相关；全量更佳） | 行为回归 |
| `pnpm build`（改 admin-ui 时，cwd `admin-ui`） | 前端可构建 |
| `git status --short` | 无真实凭据、无 `.codegraph/` |

### 模块级建议

```text
cargo test --lib add_credential
cargo test --lib token_manager
# 或更精确的测试名过滤
```

（以仓库实际 test 目标名为准；未跑不得声称通过。）

### 完成前 skills（AGENTS 门禁）

1. ~~openspec-superpowers-bridge~~（本 evidence）  
2. openspec-apply-change（实现）  
3. spec-compliance-check  
4. openspec-verify-change  
5. verification-before-completion  

### 禁止

- 真实 token/账号/Cookie 入库或贴进 PR/文档  
- 未跑验证却写「已通过」  
- 顺手重构 anthropic/SSE/选号  

## 9. README / AGENTS / spec 同步判断

| 文档 | 是否需要 | 时机 |
| --- | --- | --- |
| README Admin API / credentials 字段 | **是** | P0/P1 暴露新字段或新 endpoint 后 |
| credentials.example.*.json | **是** | 与 task 1.3 同步 |
| AGENTS.md | **否**（除非新增验证命令纪律） | — |
| spec/design.md 长期架构 | **可选** | 归档时若 Admin 边界变化再同步；单次过程只写 change |
| openspec/specs 主规格 | 归档时 sync | apply 完成后 `openspec-archive-change` / sync-specs |
| docs/add-account-optimization-design.md | 保持为设计源 | 不替代 OpenSpec |

## 10. 停止条件（实现中触发则停）

1. OpenSpec 工件互相矛盾或 validate 失败且无法在 change 内修复  
2. 发现规格未覆盖的高风险：例如必须改 SSE/选号/鉴权模型才能完成  
3. GetUserInfo 真实端点与 Go 不一致且无法在 mock + best-effort 下安全落地（需回写 design/open question）  
4. 工作区出现将被提交的真实 config/credentials/token  
5. upsert 语义与生产数据兼容性存疑且用户未确认 onConflict 默认  

当前：**无停止条件触发。**

## 11. 下一步

1. 使用 **openspec-apply-change**（或等价）从 tasks **1.1** 开始实现 P0  
2. 每完成一组任务勾选 `tasks.md` 并跑对应验证  
3. P0 完成后先 compliance 抽检，再进入 P1  

---

## 附录：本会话证据命令清单

```text
openspec status --change improve-credential-ingest --json
openspec validate improve-credential-ingest
git status --short ; git branch --show-current ; git rev-parse --short HEAD
codegraph status
codegraph context "credential ingest add import userId upsert admin"
codegraph impact "add_credential"
codegraph callers "add_credential"
codegraph callees "add_credential"
rg ... (credentials/adminApiKey/examples/README/docker)
# Kiro-Go
rg GetUserInfo auth proxy
# 阅读 auth/sso_token.go GetUserInfo 实现
Test-Path config.json / credentials.json → False
```
