# Bridge Plan: admin-models-settings-optimization

日期：2026-07-24
状态：READY（工件齐全、validate 通过、工作区无真实密钥文件；实现前检查点）
分支：dev
设计输入：`docs/admin-models-settings-optimization-design.md`

## 范围

- **model-catalog**：`GET /v1/models` catalog 路径默认过滤 `map_model` 失败项；静态 fallback 契约（均可映射）；空缓存不阻塞 + 后台限并发预热；可选 Admin 全局 catalog 读接口
- **admin-ui-model-ops**：`CredentialStatusItem` 暴露 `modelCount/modelsUpdatedAt/modelsLastError`；查看/刷新/测试共享凭据模型缓存；卡片主操作改为 force 刷新余额；reset 降级
- **credential-model-test**：测试 UI Select + 手输；后端 test API 形状不变；缓存列表中的可映射 modelId 可用于 tes
- **admin-runtime-settings（新）**：Admin 热更新出站代理、`defaultEndpoint`、`requireApiKey`+`apiKey`（脱敏）；校验 → 落盘 → 内存生效

## 非目标

- 多 API Key 配额面板 / Kiro-Go ApiKeys 完整复刻
- CodeWhisperer / AmazonQ 多 URL 自动 fallback（首期仅已注册 endpoint 名，当前 `ide`）
- 模型 catalog DB 持久化
- 重写 priority / balanced 负载均衡
- Admin Cookie 密码鉴权
- 改 SSE 流解析 / 工具调用转换核心
- 不提交 `config.json` / `credentials.json` / 真实 token / Cookie / `.codegraph/`
- 不 push / merge / PR / archive（除非用户另行要求）

## 关键设计决策（执行约束）

| 决策 | 结论 | 实现约束 |
| --- | --- | --- |
| D1 catalog 过滤 unmapped | `models_from_catalog` 对 `map_model` 失败跳过；thinking 基于 canonical | 默认不暴露 unmapped；Admin 查看 raw 缓存可保留完整上游列表 |
| D2 空缓存 S1 | 立即 fallback；后台限并发预热 | **禁止** `GET /v1/models` 同步全量 List；并发上限 2；失败仅 log |
| D3 测试 UI 读 models | Select 选项来自 `GET .../models` | 不改 `POST .../test` 请求体形状；省略 model 仍用服务端默认 |
| D4 刷新余额 vs reset | 主按钮 force balance；reset 保留 API/UI 条件显示 | `balance?force=true` 或等价；默认 force=false 保持旧 TTL |
| D5 设置热更新 | 校验 → `Config::save` → 更新内存 | 校验失败 400 且不改内存；写盘失败不静默成功 |
| D6 requireApiKey 默认 true | 旧 config 缺省 = true | true+空 apiKey fail-closed；Admin 鉴权独立 |
| D7 endpoint 白名单 | 仅已注册名（现 `ide`） | 不实现多 URL fallback；未知名 400 |
| D8 模块落点 | handlers/converter 契约 + token_manager 元数据/预热 + admin settings + middleware 热读 + admin-ui | Surgical；不顺手重构 SSE/选号主循环 |

### 一致性检查（proposal / design / tasks / specs）

| 能力 | proposal | specs | tasks |
| --- | --- | --- | --- |
| model-catalog（MODIFIED） | 有 | `specs/model-catalog/spec.md` | 1.x |
| admin-ui-model-ops（MODIFIED） | 有 | `specs/admin-ui-model-ops/spec.md` | 2.1, 3.x, 4.x |
| credential-model-test（MODIFIED） | 有 | `specs/credential-model-test/spec.md` | 3.1–3.2 + 后端已有 test |
| admin-runtime-settings（NEW） | 有 | `specs/admin-runtime-settings/spec.md` | 5.x, 6.x |
| 验证收尾 | 有 | — | 7.x |

结论：范围一致；`openspec status` 4/4 artifacts complete；非 blocked；可进入实现。

## 高风险项

| 风险 | 等级 | 处理 |
| --- | --- | --- |
| catalog 过滤后列表变短 / 客户端「模型消失」 | 中 | Admin 查看仍展示凭据 raw models；过滤打 warn 日志；文档说明可用性契约 |
| 静态 fallback id 与 map_model 失配 | 高 | 契约单测：每个静态 id（含 thinking 基座）`map_model` 为 Some；必要时只调 id 不改 map 语义 |
| AppState / Config / proxy 热更新并发 | 高 | apiKey/requireApiKey 用可热读结构（Arc+Mutex/RwLock 或 Atomic）；proxy 同时更新 `MultiTokenManager` 与 `KiroProvider`（或统一入口）；in-flight 请求允许旧 client |
| Config 写盘失败半更新 | 高 | 先校验再写盘；写盘失败返回错误；内存与磁盘一致性策略按 design D5 |
| requireApiKey=false 裸奔 | 高 | UI 二次确认 + README 警告；Admin 始终鉴权 |
| 误把 Kiro-Go 三端点 fallback 塞进 rs | 中 | Non-goal 硬约束；endpoint 仅名称白名单 |
| force balance 打爆上游 / 预热打爆 | 中 | 预热并发 ≤2；force 仅单卡/显式；失败保留旧余额展示 |
| 密钥回传 / 误提交 | 高 | settings GET mask；不 log 明文；`git status` 门禁 |
| middleware 改为热读后测试/路由装配回归 | 中 | 四象限单测 + 保持 Admin middleware 独立 |

## CodeGraph 证据

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `codegraph status` | Exit 0；100 files / 1,655 nodes / 4,299 edges；Index up to date | 索引可用 |
| `codegraph impact models_from_catalog` | 4 symbols：`models_from_catalog`、`get_models`、handlers 文件、单测 | catalog 改动隔离性好；主改 `handlers.rs` |
| `codegraph impact get_models` | 2 symbols：`get_models` + handlers | 对外列表入口清晰 |
| `codegraph callers map_model` | `get_context_window_size`、`convert_request`、`test_credential` + map 单测 | **不要**改 map 语义；列表侧过滤/契约对齐即可 |
| `codegraph context`（models/balance/settings） | 命中 map_model、test_credential、CredentialsConfig | 测试与映射真源已定位 |
| `codegraph query` balance/reset/router/CredentialStatusItem | Admin 路由含 balance/refresh/models；**无** settings；UI 有 CredentialStatusItem 与 refreshAllModels | settings 为新增面；balance force 扩现有 get_balance |

### 建议实现落点（由 impact / 源码导出）

| 能力 | 主要符号 / 文件 |
| --- | --- |
| catalog 过滤 + 静态契约 | `src/anthropic/handlers.rs`：`models_from_catalog`、`static_fallback_models`、相关单测；`map_model` 只读 |
| 启动预热 | `src/main.rs` + `MultiTokenManager::spawn_refresh_models_arc` / `refresh_models_for`（限并发包装） |
| modelCount 元数据 | `token_manager::get_credential_models_cached` → `admin/service` list status → `types::CredentialStatusItem` → `admin-ui` types/card |
| balance force | `AdminService::get_balance`（TTL 300s）+ handlers query `force`；UI `getCredentialBalance` 扩展 |
| test Select | `admin-ui/.../credential-test-dialog.tsx` + `getCredentialModels`；可选 models dialog「用此模型测试」 |
| 运行时 settings | 新 admin handlers/routes；`Config::{require_api_key,save}`；`MultiTokenManager` proxy/config 可变更新；`AppState` 鉴权热读；`KiroProvider.default_endpoint`/`global_proxy` 同步策略 |
| 鉴权四象限 | `src/anthropic/middleware.rs` + 单测；`main.rs` 启动策略按 requireApiKey/fail-closed 规则处理 |

## rg / 源码补盲

| 补盲点 | 证据 | 结论 |
| --- | --- | --- |
| `/v1/models` 现状 | `handlers.rs`：catalog 非空透传 `model_id`；空则 `static_fallback_models`；**无** map 过滤 | 1.1 必须改 |
| map_model | `converter.rs:80`：sonnet/opus/haiku 关键字；接受 `4-6` 与 `4.6` | 静态连字符 id 大多可映射；仍需契约测试防回归 |
| balance 缓存 | `AdminService::get_balance` TTL 300s；`fetch_balance` 无 force 旁路 | 2.2 扩 query/参数 |
| reset | `POST /credentials/{id}/reset` + UI「重置失败」 | API 保留；UI 降级条件显示 |
| 模型缓存读 | `get_credential_models_cached` 已有 models/updated_at/last_error | 2.1 直接映射到 status 字段 |
| 预热钩子 | add/enable 已 `spawn_refresh_models_arc` | 启动预热可复用，限并发即可 |
| Config save | `Config::save` 依赖 `config_path` | settings 写盘已有基础；需确保 load 路径写入 runtime 持有 Config |
| proxy 双份状态 | `MultiTokenManager.proxy` + `KiroProvider.global_proxy` + client_cache | 热更时两边一致；新 client 按需重建缓存项 |
| defaultEndpoint | Provider 字段 + Config；endpoint 注册表仅 `ide` | PUT 白名单 = 注册表 keys |
| 客户端鉴权 | `AppState.api_key: String` + 总是校验；main 缺 apiKey exit(1) | requireApiKey 需改 AppState 与启动策略 |
| Admin 鉴权 | `admin/middleware` 独立 adminApiKey | 保持；settings 走同一 Admin 路由层 |
| Admin 路由 | 无 `/settings/*` | 新增不冲突 |
| config.example | 有 apiKey/adminApiKey/defaultEndpoint；**无** requireApiKey | 6.2 同步 example + README |
| 前端测试对话框 | 仅 Input 手输 model | 3.1 Select+手输 |
| 前端卡片 | 「重置失败」+ forceRefreshToken（token） | 改「刷新余额」force balance；勿与 token force-refresh 混淆 |
| 工作区 | `?? docs/admin-models...` + `?? openspec/changes/admin-models...` | 可提交 docs+change；无 credentials/config 脏文件 |
| Docker/CI | 本 change 非目标 | 不改 workflow，除非后续加测 |

## 任务到执行步骤

| Task | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1 | `models_from_catalog`：`map_model` 失败跳过；thinking 用 canonical；补单测 unmapped 不出现 | `cargo test models_from_catalog` | 过滤策略与 Admin raw 查看语义冲突未决 |
| 1.2 | 静态 fallback 每个 id 断言 `map_model` Some；必要时调静态 id | `cargo test static_fallback` | 必须改 map_model 语义才能通过（回退 design） |
| 1.3 | 启动 spawn 限并发预热启用凭据；失败 log | 编译 + 逻辑审查；可选单测调度 | 预热阻塞监听端口或无上限 |
| 1.4（可选） | `GET /api/admin/models/catalog` | Admin 认证单测/路由 smoke | 路径与现有冲突 |
| 2.1 | status 填 modelCount/modelsUpdatedAt/modelsLastError | admin 单测或序列化断言 | 缓存锁路径死锁 |
| 2.2–2.3 | balance force 旁路 TTL；命中 vs force 单测 | `cargo test` admin balance | force 默认破坏旧缓存语义 |
| 3.1–3.4 | TestDialog Select+手输；ModelsDialog 用于测试；refresh invalidate；卡片徽章 | `pnpm --dir admin-ui build` | UI 与 API 字段不一致 |
| 4.1–4.3 | 主按钮刷新余额；reset 条件显示；顶栏批量保留 | build + 手工 smoke（密钥不入库） | 与 token force-refresh 按钮文案混淆 |
| 5.1 | Config.requireApiKey 默认 true；load 兼容 | config 单测 | 破坏现有 config 反序列化 |
| 5.2 | settings/proxy GET/PUT；校验 URL；落盘；更新全局 proxy | admin 单测 400/成功 | 双份 proxy 不一致 |
| 5.3 | settings/endpoint 白名单 | 非法名 400 | 未注册名被接受 |
| 5.4–5.5 | settings/auth mask + 热更；middleware 四象限 | middleware/admin 单测 | Admin 鉴权被 requireApiKey 关闭连带影响 |
| 6.1–6.3 | Settings 面板 + example/README + pnpm build | build 通过；文档 diff | 关闭鉴权无二次确认 |
| 7.1–7.4 | 全量验证 + git status + 完成证据 | 见「必跑验证」 | 测试失败隐瞒或密钥入 status |

**建议落地顺序：** Phase A（1 + 2.1 + 3）→ Phase B（2.2–2.3 + 4）→ Phase C（5.2 + 6 代理）→ Phase D（5.1/5.3/5.4/5.5 + 6 端点/鉴权 + 7）

## 必跑验证

实现过程中按任务增量跑；**声称完成前必须真实执行并记录：**

| 命令 | 目的 |
| --- | --- |
| `cargo test`（至少 handlers / map_model / admin / middleware / config 相关；全量更佳） | 行为与回归 |
| `pnpm --dir admin-ui build` | 前端可构建（改 UI 时） |
| `openspec validate --all` | 工件仍合法 |
| `git status --short` | 无密钥、无 `.codegraph`、无意外 tmp |
| （可选手工，密钥不入库）`GET /v1/models`；查看/刷新模型；Select 测试；force 余额；settings 读写 | 端到端冒烟 |

高风险矩阵映射（AGENTS）：

- Admin / 凭据：admin 测试 + 禁止真实凭据
- 模型映射/目录：map_model 回归 + models 相关测试
- 认证 / API Key：middleware 四象限
- 配置 schema：requireApiKey 兼容 + example/README

## README / AGENTS / spec 同步判断

| 文档 | 是否同步 | 原因 |
| --- | --- | --- |
| `README.md` | **是** | 配置项 requireApiKey；Admin settings/balance force/modelCount；/v1/models 可用性契约 |
| `config.example.json` | **是** | 可选 `requireApiKey` 示例 |
| `AGENTS.md` | 否（首期） | AI 纪律/矩阵无新增类型；已覆盖 Admin/模型/认证/配置 |
| `spec/design.md` / `requirements.md` | **实现后轻量补** | 运行时 settings 与 catalog 可用性契约属长期架构事实 |
| `openspec/specs/*` main | 归档时 | delta 在 change 内；归档阶段 sync |
| `docs/admin-models-settings-optimization-design.md` | 已有 | 设计输入；实现偏差时回写备注 |

## 停止条件

立即停止实现并向用户升级，若：

1. OpenSpec 工件缺失、互相矛盾或状态 blocked（当前否）。
2. 发现未写入规格的高风险影响（例如必须改 SSE、必须多端点 fallback、必须改 map_model 核心语义才能满足列表契约）。
3. 工作区出现会被提交的真实 `config.json` / `credentials.*` / token / Cookie / `.codegraph/`。
4. 无法确定验证命令或无法对高风险项给出剩余风险说明。
5. proxy/auth 热更新无法在不破坏 in-flight 与 Admin 鉴权的前提下落地，且无最小安全方案。

## 实现前结论

- **State**：READY for implementation（`openspec-apply-change` 可启动）。
- **validate**：`openspec validate --all` → 10 passed, 0 failed。
- **工作区**：仅设计文档 + change 目录未跟踪；无密钥脏文件。
- **下一步**：按 tasks Phase A→D 实现；每阶段跑对应测试；完成后 `spec-compliance-check` → `openspec-verify-change` → `verification-before-completion`。
