# Bridge Plan: admin-ui-model-ops-entrypoints

生成时间：2026-07-22  
Change 状态：`openspec status` → planning artifacts complete；**非 blocked**；`applyRequires: tasks`  
分支：`dev`  
HEAD：`028ed87` feat(model-catalog)...

## 1. 范围

**做：**

- Admin UI 消费既有后端 model-catalog / credential-model-test API
- 类型 + API 封装 + Dashboard「刷新全部模型」+ 卡片「查看模型 / 刷新模型 / 测试」
- 错误可诊断、loading 防重复、不泄露密钥
- README Admin UI 一句补充；`pnpm build`；`openspec validate --all`

**不做（Non-Goals）：**

- 不改 Rust Admin 契约 / 选号 / `/v1/models` 后端逻辑
- 不新建独立 Models 路由页
- 不做批量并发 test
- 不改 online-auth / import 对话框
- 不强制上游 test 200（账号 suspended 时错误路径验收即可）

**与工件一致性检查：**

| 工件 | 结论 |
| --- | --- |
| proposal ↔ design | 一致：仅 UI 入口 |
| design ↔ tasks | 一致：API → Dashboard → Card → 安全 → 验证 |
| tasks ↔ specs | 四条 ADDED Requirements 均有对应任务 1.x–3.x、4.x |
| 非目标 | 前后端/批量 test 均未写入 tasks 强制项 |

无互相矛盾；可进入实现。

## 2. 关键设计决策（执行时遵守）

| ID | 决策 | 实现要点 |
| --- | --- | --- |
| D1 | Dashboard + Card，不新开路由 | 改 `dashboard.tsx`、`credential-card.tsx`；可新增轻量 dialog 组件 |
| D2 | 查看模型用 Dialog | 展示 models / updatedAt / lastError；支持重新刷新；**live 开关可用**（后端已支持 `?live=true`） |
| D3 | 测试 Dialog 可选 model | 空 model → body 省略或 `{}`；展示 success/model/latencyMs/reply |
| D4 | 沿用 axios + x-api-key | 扩 `api/credentials.ts` + `types/api.ts`；hooks 可选 |
| D5 | 全量刷新摘要 | toast + failed 时 errors 明细（Dialog 或 description） |
| D6 | 不改后端 | 字段名对齐 serde camelCase：`credentialId`、`globalCount`、`latencyMs`、`lastError`、`updatedAt` |

**Open Question  closure（本 bridge 定稿）：**

- 批量测试选中凭据：**首期不做**
- `?live=true`：**做**。查看模型 Dialog 提供「实时拉取」开关/按钮，调用 `getCredentialModels(id, true)`

## 3. 高风险项

| 风险 | 级别 | 缓解 / 停止条件 |
| --- | --- | --- |
| 误改后端协议或 credentials schema | 高 | 禁止改 `src/admin/*` 除非发现阻断性契约 bug；若必须改 → 停下来扩 scope |
| 响应/日志泄露 token | 高 | 只展示后端 `error.message` / 业务字段；禁止把 request body 密钥打到 toast |
| 把探测临时文件 / 真实密钥提交进仓 | 高 | 实现前清理 `tmp_*`、`admin_creds_*`、`tmp_admin_key.txt`；提交前 `git status` |
| 上游 403 被当成 UI 失败验收失败 | 中 | 验收标准是「入口可见 + 请求发出 + 错误可展示」，不要求 test 200 |
| 未 rebuild 嵌入 UI 导致线上仍旧 | 中 | 文档写明需 `admin-ui pnpm build` + 重编二进制；本 change 至少 `pnpm build` |
| 全量刷新耗时长导致重复点击 | 中 | loading/disabled |
| test 触发真实上游费用 | 中 | 文案标明「真实推理探测」；后端已限制小探测 |

**未写入规格的新高风险？** 未发现。后端路由与类型已存在；本次纯前端。

## 4. CodeGraph 证据

### 命令

```
codegraph status
codegraph context admin-ui
codegraph query "admin models refresh test credentials"
codegraph impact "refresh_all_models"
codegraph impact "test_credential"
codegraph callers "get_credential_models"
```

### 结论

- Index OK：102 files / 含 20 tsx + 6 ts；后端 Admin 与 admin-ui 均在索引中。
- 后端路由已索引：
  - `POST /credentials/models/refresh`
  - `POST /credentials/{id}/models/refresh`
  - `GET /credentials/{id}/models`
  - `POST /credentials/{id}/test`
- 前端 hooks 仅有 credentials CRUD / balance / forceRefresh / load-balancing，**无 models/test 符号** → 与缺口一致。
- `refresh_all_models` impact 窄（handler + route）；`test_credential` impact 主要在 `AdminService` 方法簇与测试，**不要求本 change 改 Rust**。
- `get_credential_models` callers：route + 单测；UI 无调用者。

**对实现的含义：** 安全改动面是 `admin-ui/**` + README 一句；无需碰 token_manager / converter。

## 5. rg / 源码补盲

### 命令与发现

```
rg models/refresh|/test|extractErrorMessage admin-ui/src src/admin
rg Admin|models/refresh README.md
rg live|ModelsLiveQuery src/admin
rg embed|admin_ui|admin-ui/dist src
```

| 发现 | 含义 |
| --- | --- |
| `admin-ui/src/api/credentials.ts` 无 models/test 函数 | 任务 1.2 必做 |
| `dashboard.tsx` / `credential-card.tsx` 无模型入口 | 任务 2.x / 3.x |
| `extractErrorMessage` 已在多对话框使用 | 任务 4.1 复用，不新造错误栈 |
| `ModelsLiveQuery.live` + service `live=true` 先刷新 | Dialog 暴露 live |
| 嵌入：`src/admin_ui/router.rs` `#[folder = "admin-ui/dist"]` | 必须 `pnpm build` 才进二进制 |
| README 已列 API，Admin UI 仅 `GET /admin` 一句 | 任务 5.1 补「页面入口」 |
| 工作区残留 `tmp_*` / `admin_creds_*` / `tmp_admin_key.txt`（上一轮 HTTP 探测） | **实现/提交前必须删除，禁止入库** |

### 后端契约（前端类型对齐）

```text
ModelsRefreshResponse: success, credentialId, count, models[], updatedAt
ModelsRefreshAllResponse: success, refreshed, failed, globalCount, errors[{credentialId,error}]
CredentialModelsResponse: success, models[], updatedAt?, lastError?
TestCredentialRequest: model?
TestCredentialResponse: success, model, reply?, latencyMs
Auth: x-api-key = adminApiKey（与现有 storage.getApiKey 登录一致）
```

## 6. 任务 → 执行步骤映射

| Task | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1 类型 | 编辑 `admin-ui/src/types/api.ts` 增加 camelCase 接口 | tsc/build 无类型错误 | 字段与后端 serde 不一致且无法仅前端修复 → 停 |
| 1.2 API | 编辑 `api/credentials.ts` 四函数；`getCredentialModels(id, live?)` query `live` | 路径字符串与 router 一致（代码 review） | 需要改后端路径 → 停 |
| 1.3 hooks（可选） | 需要则加 mutation；否则组件内直接调 API + loading state | invalidate 不破坏现有 credentials 轮询 | — |
| 2.1–2.3 Dashboard | 列表操作区加「刷新全部模型」；loading；toast；failed 明细 Dialog | 按钮可见；mock/真服务点击后状态正确 | UI 无法表达 partial fail → 补 Dialog 不得省略 |
| 3.1–3.2 卡片刷新 | 「刷新模型」按钮 + toast count | 成功/失败文案 | — |
| 3.3 查看模型 | 新 Dialog：列表滚动、lastError、刷新、live | 打开即请求 GET models | — |
| 3.4 测试 | 新 Dialog：model 输入、提交、结果区 | 省略 model 时 body 合法 | — |
| 4.1–4.3 体验安全 | 统一 extractErrorMessage；disabled 策略；pending 禁用 | 人工检查无 token 明文 | 发现会打印密钥 → 停并修 |
| 5.1 README | Admin UI  bullet 补模型/测试入口 |  diff 仅文档一句 | — |
| 5.2 build | `pnpm --dir admin-ui build`（或 cd + pnpm build） | exit 0 | build 失败 → 不得声称完成 |
| 5.3 冒烟 | 有本地 Admin 则点按钮；或确认 network 路径；上游 403 可接受 | 证据记入 evidence | 无服务时写明未跑 UI E2E 与剩余风险 |
| 5.4 validate/status | `openspec validate --all`；`git status --short` | 无密钥/tmp 误入 | 有 credentials/config/tmp 敏感 → 清理后继续 |

**建议实现顺序：** 1.1 → 1.2 → (1.3) → 3.x dialogs + card → 2.x dashboard → 4.x → 5.x

**文件触达白名单（Surgical）：**

- `admin-ui/src/types/api.ts`
- `admin-ui/src/api/credentials.ts`
- `admin-ui/src/hooks/use-credentials.ts`（可选）
- `admin-ui/src/components/dashboard.tsx`
- `admin-ui/src/components/credential-card.tsx`
- 可选新建：`admin-ui/src/components/credential-models-dialog.tsx`、`credential-test-dialog.tsx`、`models-refresh-result-dialog.tsx`
- `README.md`（Admin UI 一句）
- `openspec/changes/admin-ui-model-ops-entrypoints/**`（tasks 勾选、evidence）

**禁止默认触达：** `src/**`（Rust）、`credentials*.json`、`config.json`、Docker/CI。

## 7. 必跑验证

| 命令 | 何时 | 通过标准 |
| --- | --- | --- |
| `pnpm --dir admin-ui build` | UI 改完 | exit 0，产出 dist |
| `openspec validate --all` | 工件/勾选后 | 全绿 |
| `git status --short` | 完成前 | 无 `config.json`/`credentials.json`/token/`tmp_*` 敏感探测文件待提交 |
| 手动 Admin 冒烟（可选但推荐） | 有本地 `adminApiKey` 服务时 | 四入口可见；全量刷新/单卡操作有请求与反馈 |
| `cargo test` / `cargo build` | **本 change 默认不强制**（纯前端）；仅当误触 Rust 时补跑 | — |

**验收成功标准（Goal-Driven）：**

1. Dashboard 有「刷新全部模型」且 loading 可用  
2. 每张卡片有「查看模型」「刷新模型」「测试」  
3. API 封装路径正确  
4. 错误路径可展示且无密钥  
5. `pnpm build` 通过  
6. validate 通过；工作区无敏感误入  

**不作为成功标准：** 上游 ListAvailableModels/test 返回 200（账号可能 suspended）。

## 8. README / AGENTS / spec 同步判断

| 文档 | 是否更新 | 原因 |
| --- | --- | --- |
| README.md | **是（最小）** | Admin UI 目前只写 `GET /admin`；应补一句：页面支持刷新模型目录、查看缓存、凭据测试 |
| AGENTS.md | 否 | 无 AI 纪律/验证矩阵变化 |
| `spec/*` 长期事实 | 否 | 架构边界未变；能力属 Admin UI 操作面 |
| `openspec/specs/*` 主规格 | **实现完成归档前再定**；本 change 可只保留 delta；归档时将 `admin-ui-model-ops` sync 入 main specs | 与 openspec-archive / sync 流程一致 |
| Docker/CI | 否 | 无部署脚本变化 |

## 9. 停止条件

出现以下任一情况，**停止实现并上报**：

1. OpenSpec 状态变 blocked 或工件被改到互相矛盾  
2. 必须修改 Anthropic/Kiro 协议、选号、credentials 文件 schema 才能让 UI 工作  
3. 发现后端 API 与 README/types 严重不一致且无法前端适配  
4. 工作区出现将被提交的真实 `config.json` / `credentials.json` / token / Cookie  
5. 无法运行 `pnpm build` 且无替代验证  
6. 需求扩大到「批量 test / 独立模型页 / 后端改动」而未更新 OpenSpec  

## 10. 实现前卫生清单

- [ ] 删除仓库根目录探测残留：`tmp_*.js`、`tmp_*.json`、`tmp_admin_key.txt`、`admin_creds_*.json`、`tmp_build_check.txt`（勿提交）  
- [ ] 确认只改白名单路径  
- [ ] 实现后按 §6–§7 验证并写 evidence（apply / compliance 阶段）

## 11. 结论

**可以开始实现。**  
范围清晰、后端契约已存在、CodeGraph/rg 未发现隐藏协议影响、验证命令明确。  
下一步：`openspec-apply-change`（或按 tasks 顺序直接改 `admin-ui`）。
