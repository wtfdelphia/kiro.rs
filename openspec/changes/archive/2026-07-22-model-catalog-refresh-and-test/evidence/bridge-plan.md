# Bridge Plan: model-catalog-refresh-and-test

日期：2026-07-22  
状态：READY（工件齐全、validate 通过、工作区无真实密钥文件；实现前检查点）  
分支：dev  
设计输入：docs/model-refresh-and-test-optimization-design.md

## 范围

- 上游 ListAvailableModels 客户端 + 类型 + 单测
- 双层模型缓存（凭据级 set + 全局聚合 catalog）
- select_next_credential 按 model set 过滤（冷启动乐观）+ 保留 supports_opus 与 priority/balanced
- Admin API：单/全量 models refresh、查看缓存、POST /credentials/{id}/test
- GET /v1/models 读全局缓存 + thinking 变体；空缓存静态 fallback
- 生命周期：添加/启用后异步 refresh；删除清理缓存
- README Admin API 表同步；Admin UI 为可选任务（6.2）

## 非目标

- 不重写负载均衡算法语义（priority / balanced 候选集内逻辑保持）
- 不强制首期持久化 model_catalog.json（P1 可选，不阻塞）
- 不完整复刻 Kiro-Go Admin UI 视觉
- 不用动态目录完全取代 map_model
- 不改 SSE 流解析 / 工具调用转换核心
- 不改 Docker / CI（除非后续验证需要加测）
- 不提交 config.json / credentials.json / 真实 token / Cookie / .codegraph/
- 不 push / merge / PR / archive（除非用户另行要求）

## 关键设计决策（执行约束）

| 决策 | 结论 | 实现约束 |
| --- | --- | --- |
| D1 双层缓存 | per-id set + global 聚合 | 路由与列表分离；禁止只做全局列表 |
| D2 冷启动乐观 | 无/空 set 不因缓存拒绝 | 与 Go ccountHasModel 对齐 |
| D3 host 固定 us-east-1 | CodeWhisperer 固定 host | 对齐 profile.rs；不跟 api_region |
| D4 /v1/models 不异步全量阻塞 | 读缓存 / 静态 fallback | 禁止在 GET 路径同步刷全量账号 |
| D5 test 真实最小 generate | 默认 claude-sonnet-4.6，小 max_tokens | 非 mock-only health |
| D6 保留 supports_opus | 与 model set 组合 | Free + opus 仍拒绝 |
| D7 模块落点 | kiro 新模块 + token_manager + admin + get_models | Surgical；不顺手重构 converter/SSE |

### 一致性检查（proposal / design / tasks / specs）

| 能力 | proposal | specs | tasks |
| --- | --- | --- | --- |
| model-catalog | 有 | specs/model-catalog/spec.md | 1.x, 2.1–2.2, 2.4, 3.x, 4.x |
| model-aware-routing | 有 | specs/model-aware-routing/spec.md | 2.3, 2.5 |
| credential-model-test | 有 | specs/credential-model-test/spec.md | 5.x |
| Modified 能力 | 无 | 无 delta | — |

结论：范围一致，无 blocked 工件，可进入实现。

## 高风险项

| 风险 | 等级 | 处理 |
| --- | --- | --- |
| Admin 新 API 暴露上游错误/体 | 中 | 截断 body；禁止 token 明文；沿用 Admin 认证（adminApiKey） |
| 选号过滤改坏现有 acquire | 高 | 冷启动乐观 + 保留 opus 过滤；补 token_manager 单测；改动仅 select_next_credential 过滤条件 |
| 并发写 catalog | 中 | Mutex/RwLock；按凭据更新再 merge |
| test 消耗配额 | 低 | max_tokens 极小；默认模型固定 |
| 失败 refresh 清空缓存 | 高 | 失败 MUST 保留旧缓存（spec 强制） |
| 误提交密钥 / 本地缓存 | 高 | 提交前 git status；不 add credentials/config/.codegraph |
| 改动扩散到 SSE | 中 | 禁止改 parser/stream 核心；PR 聚焦新文件与薄接入 |
| 上游 modelId 与 map_model 不一致 | 中 | 保留 map_model；动态列表以上游 id 为准 |

## CodeGraph 证据

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| codegraph status | Exit 0；94 files / 1,502 nodes / 3,787 edges；Index up to date | 索引可用 |
| codegraph impact select_next_credential | 8 symbols：主要 cquire_context + token_manager 测试 | 选号改动影响面可控，但必须补选号单测 |
| codegraph impact get_models | 2 symbols：get_models + handlers 文件 | 对外列表改动隔离性好；需 State 注入 catalog |
| codegraph impact acquire_context | 72 symbols（大量测试/manager 方法） | **不要**重写 acquire 主循环；只经 select 过滤接入 |
| codegraph query AdminService force_refresh get_balance add_credential | 命中 dmin/service.rs add/balance/refresh、handlers、token_manager | Admin 扩展点清晰；生命周期挂钩点在 add / set_disabled / delete |

### 建议实现落点（由 impact 导出）

| 能力 | 主要符号 / 文件 |
| --- | --- |
| List + headers 模式 | 新 models_api；模板：profile.rs ListAvailableProfiles、	oken_manager::get_usage_limits、http_client::build_client |
| catalog / 选号 | MultiTokenManager：select_next_credential、dd_credential、set_disabled、delete_credential |
| Admin | src/admin/{router,handlers,service,types}.rs（仿 balance / force_refresh） |
| 对外 models | src/anthropic/handlers.rs::get_models + 可能 main/state 注入 |
| test | Admin service + 内部调用 provider/token 最小 generate（避免经完整 HTTP 中间件环） |

## rg / 源码补盲

| 补盲点 | 证据 | 结论 |
| --- | --- | --- |
| Admin 路由现状 | src/admin/router.rs：credentials CRUD、refresh、balance、import、LB；**无** models/test | 新增路由不冲突；注意 models/refresh 与 /{id}/refresh 路径顺序 |
| README API 表 | README ~460–467 Admin 列表；~384 /v1/models | **必须同步** README（task 6.1） |
| main 启动日志 | src/main.rs 打印部分 admin 路由 | 新增 endpoint 时同步启动日志（若项目惯例要求） |
| REST host 模式 | profile.rs 固定 codewhisperer.us-east-1.amazonaws.com；usage 用区域化 host | ListAvailableModels 跟 profile，不跟 usage Q host |
| 代理/TLS | http_client::build_client + 凭据 effective proxy | 新客户端必须走同一构建路径 |
| 生命周期钩子 | AdminService::add_credential / set_disabled / delete_credential → TokenManager | 异步 refresh 挂在 service 或 manager 成功路径；失败不失败主操作 |
| 示例凭据 | 仅 *.example.json；工作区无 credentials.json/config.json | 安全基线 OK |
| 工作区脏文件 | ?? docs/model-refresh...、?? openspec/changes/model-catalog...、?? models_list.txt、若干 	mp_* | 可提交 docs+change；**勿**提交 tmp_* /.codegraph；models_list.txt 非本 change 必须文件，实现勿依赖其入库 |
| Docker/CI | 本 change 非目标 | 无需改 workflow，除非后续加集成测试 job |

## 任务到执行步骤

| Task | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1 | 新增类型 + fixture 解析 | cargo test 解析用例 | fixture 字段与上游真实 JSON 对不齐且无法确认 |
| 1.2 | 实现 list_available_models | URL/headers/非200 单测 | 需真实密钥才能证明协议且无法 mock |
| 1.3 | mod 导出 | 编译通过 | 模块环依赖无法用薄接入解决 |
| 2.1–2.2 | catalog + refresh_for/all | merge/失败保留旧缓存单测 | 全局 rebuild 策略与 delete 语义冲突未决 |
| 2.3–2.5 | 选号过滤 + 单测 | 有缓存跳过 / 空缓存放行 / Free+opus 拒绝 | 破坏现有 acquire 测试且无法最小修复 |
| 2.4 | 删除清理；add/enable 异步 refresh | 单测或逻辑审查 + 日志约定 | spawn 生命周期/取消语义导致不稳定 |
| 3.1–3.3 | Admin types/routes/service | service 错误映射单测；路由注册 | 路径与现有 /{id}/refresh 冲突 |
| 4.1–4.2 | get_models 动态 + fallback | 空/非空缓存断言 | 响应字段破坏已知客户端 |
| 5.1–5.2 | test 端点 | 非法 model 400；成功路径 mock；无密钥日志 | 必须走完整 SSE 才能完成（则回退设计评审） |
| 6.1 | README Admin 表 | 文档 diff 审查 | — |
| 6.2 | （可选）admin-ui | pnpm build 若改 UI | 首 PR 可 SKIP 并写明 |
| 7.1–7.3 | 全量验证 | 见「必跑验证」 | 测试失败隐瞒或密钥入 status |

**建议落地顺序：** 1 → 2 → 3 → 4 → 5 → 6.1 → 7；（6.2 可选后置）

## 必跑验证

实现过程中按任务增量跑；**声称完成前必须真实执行并记录：**

| 命令 | 目的 |
| --- | --- |
| cargo test（至少 models/catalog/token_manager/admin 相关；全量更佳） | 行为与回归 |
| openspec validate --all | 工件与 specs 仍合法 |
| git status --short | 无密钥、无 .codegraph、无意外 tmp |
| （若改 UI）cd admin-ui && pnpm build | 前端可构建 |
| （可选手工，密钥不入库）refresh → GET /v1/models；POST .../test | 端到端冒烟 |

高风险矩阵映射（AGENTS）：

- Admin / 凭据：admin 测试 + 禁止真实凭据
- 模型映射/目录：map_model 回归 + models 相关测试
- 多凭据选择：token_manager 测试

## README / AGENTS / spec 同步判断

| 文档 | 是否同步 | 原因 |
| --- | --- | --- |
| README.md | **是** | 新增 Admin API；/v1/models 行为从纯静态变为「缓存优先」需说明 |
| AGENTS.md | 否（首期） | AI 纪律/矩阵无变更；高风险类型已覆盖 Admin/模型 |
| spec/design.md / 
equirements.md | **建议实现后轻量补一句** | 长期架构应提及 model catalog 与选号维度；归档前可用 sync-specs 把 delta 并入 main specs |
| openspec/specs/* main | 归档时 | 本 change 为 ADDED 能力，归档阶段同步 main specs |
| docs/model-refresh-and-test-optimization-design.md | 已有 | 设计输入；实现偏差时回写备注 |

## 停止条件

立即停止实现并向用户升级，若：

1. 发现必须改 SSE/parser 核心才能完成 test（超出 non-goals）
2. 选号过滤导致现有 token_manager 测试大面积失败且修复需要重写 acquire
3. 上游协议字段与 fixture 严重不符且无法在不暴露密钥的前提下确认
4. 工作区出现将被提交的真实 config.json / credentials* / token
5. OpenSpec validate 失败且无法从工件修复
6. 需要持久化 schema 变更写回 credentials 主文件（当前非目标）

## 实现前门禁清单

- [x] openspec status：4/4 artifacts complete，非 blocked
- [x] openspec validate model-catalog-refresh-and-test / --all：通过（本会话已跑）
- [x] proposal/design/tasks/specs 一致
- [x] CodeGraph status + impact/query 证据
- [x] rg 补盲 Admin/README/host/proxy/生命周期
- [x] 工作区无真实密钥文件（仅 example）
- [x] 本 Bridge Plan 已写入 vidence/bridge-plan.md

**结论：可以开始 openspec-apply-change / 按 tasks 实现。**
