# Admin UI 批量启用禁用与能力补充方案

> 状态：方案草案，未实现　日期：2026-09-04
> 来源：用户在 `fix-admin-ui-layout-wrapping` 第二轮（批量操作拆工具栏行、低频操作收溢出菜单）落地后提出「新增批量启用和禁用」，并要求盘点 admin-ui 还能补哪些能力。
> 范围：第 1 节是批量启用/禁用的实现方案，第 2 节是能力缺口盘点与各项改动方案，第 3 节给拆分与顺序建议。所有结论基于 2026-09-04 的代码事实，未写实现代码。

## 1. 批量启用 / 批量禁用

### 1.1 现状与事实基础

单条能力已经完整，缺的只是批量入口。

| 层 | 现状 | 位置 |
| --- | --- | --- |
| 后端接口 | `POST /api/admin/credentials/{id}/disabled`，body `{disabled: bool}` | `src/admin/router.rs:40` |
| 后端实现 | `set_disabled`：启用时清零 `failure_count`、`refresh_failure_count`、`disabled_reason`；禁用时记 `DisabledReason::Manual`；每条一次持久化 | `src/kiro/token_manager.rs:2063` |
| 前端 API 封装 | `setCredentialDisabled(id, disabled)` 已存在 | `admin-ui/src/api/credentials.ts:77` |
| 前端单条入口 | 卡片上的启用/禁用 `Switch` | `admin-ui/src/components/credential-card.tsx:245` |
| 选中快照 | `CredentialSelection {id, disabled, failureCount}` 已含 `disabled` 字段，批量判据不需要改数据结构 | `admin-ui/src/lib/credential-selection.ts:11` |
| 批量操作先例 | 现有 5 个批量操作（验活、刷新 Token、恢复异常、删除、余额）全部是前端串行循环单条接口的模式 | `admin-ui/src/components/dashboard.tsx` |
| 工具栏行 | 第二轮布局已拆出独立 `role="toolbar"` 行，现有 5 个按钮：批量余额/订阅、批量验活、批量刷新 Token、恢复异常、批量删除 | `dashboard.tsx:907` |

### 1.2 方案对比

方案 A：前端串行循环，只改前端。照 `handleBatchResetFailure`（`dashboard.tsx:292`）的模式加 `handleBatchSetDisabled(disabled: boolean)`：从 `selected` 按判据筛 id，逐条调 `setCredentialDisabled`，统计成功、失败、跳过。

方案 B：新增后端批量端点 `POST /credentials/batch/disabled`，一次锁、一次持久化，返回结构化结果。先例是 `/credentials/models/refresh` 全量端点。

| 维度 | 方案 A 前端串行 | 方案 B 后端批量端点 |
| --- | --- | --- |
| 改动面 | 仅 `dashboard.tsx` 与测试 | 新路由、handler、types、service、测试、spec |
| 模式一致性 | 与现有 5 个批量操作一致 | 「选择集批量」唯一走服务端的操作，形成两套模式 |
| 持久化 | N 条 N 次 `persist_credentials()` 全量写盘 | 一次写盘 |
| 部分失败汇报 | 与批量删除同款「成功 X 失败 Y 跳过 Z」 | 需另定响应结构 |

倾向方案 A。理由：现有 5 个批量操作全是这个模式，一致性的价值高于省几次写盘；批量删除、批量恢复异常同样每条一次持久化，属于既有模式的固有成本，百条量级无感。方案 B 的真实动机（客户端拿不到全集）在这里不成立：选择集的 id 客户端本来就有，模型刷新走服务端全量是因为要翻页遍历全集，两者不同。如果将来批量规模到几百条且写盘成为瓶颈，再单独做 B。

### 1.3 推荐方案细节（方案 A）

处理函数形态：

```ts
const handleBatchSetDisabled = async (disabled: boolean) => {
  // 批量禁用：筛未禁用的（已禁用的跳过）；批量启用：筛已禁用的（已启用的跳过）
  // 逐条 setCredentialDisabled，累加 successCount / failCount
  // skippedCount = selected.size - targetIds.length
  // toast 汇报，然后 deselectAll()
}
```

判据与文案对齐既有批量操作：

- 批量禁用的目标集是 `selected` 中 `disabled === false` 的 id；混选时对已禁用项报「跳过 N 个已禁用凭据」，与批量删除跳过未禁用项的文案同款
- 批量启用相反，跳过已启用项
- 目标集为空时 `toast.error` 提示（如「选中的凭据中没有已禁用项」），对齐恢复异常的空集处理
- 两个显式按钮「批量启用」「批量禁用」，不做成单个切换按钮：混选状态下切换语义不明
- 按钮可带计数，如 `批量禁用 (3)`，参照 `清除已禁用 (N)` 的写法；计数来源是 `selected` 快照里的 `disabled` 字段，无需额外请求
- 不加 `confirm`：禁用可逆、启用安全，对齐恢复异常，不对齐不可逆的批量删除

### 1.4 语义细节与边界

1. 与「恢复异常」的语义重叠需要说清。恢复异常走 `/reset`（`reset_and_enable`），对 `InvalidConfig` 禁用的凭据有显式拦截（`token_manager.rs:2113`）；启用走 `/disabled`，不带这个拦截。所以批量启用会放过 `InvalidConfig` 的凭据：启用后配置仍缺字段（如 `authMethod=api_key` 但无 `kiroApiKey`），下次请求仍会失败并再次被自动禁用。建议前端不加额外拦截，与单条 `Switch` 行为保持一致，把这条写进 spec 的已知边界；若要拦，改在后端 `set_disabled` 补同款判断，单条与批量一起生效。
2. 禁用当前活跃凭据不会立刻触发重选。`set_disabled` 不调用 `select_highest_priority`（那是 `set_priority` 的行为），切换发生在下一次请求时 `select_next_credential` 过滤掉禁用项之后。批量禁用若包含当前凭据，服务不会中断，只是下一条请求才切走。这是既有单条行为的延续，不是新问题。
3. 启用会清零失败计数（后端行为），列表刷新后自然反映，前端不需要额外处理。
4. 快照时效：`selected` 里存的是勾选那一刻的 `disabled` 状态，跨页期间若他处改了状态，判据按快照走。这与批量删除「已选凭据被他处删除时如实计为失败」的既有取舍一致，逐条调用时后端返回真实结果，不会误伤。

### 1.5 布局影响

工具栏行现有 5 个按钮实测约 709px。加「批量启用」「批量禁用」两个约 114px 的按钮后约 937px：

- 1280px 及以上：容器内容宽 1216px 起，单行，不受影响
- 1024px：内容宽 960px，接近但仍在单行边缘，折行也可接受
- 768px 及以下：折成 2 行，工具栏行本身允许折行（`flex-wrap`），不破坏第二轮布局结论

### 1.6 测试策略

沿用 `dashboard.test.tsx` 既有批量操作的测试骨架（内存版 `fakeServer` + 跨页选中）：

- 混选时批量禁用只请求未禁用项，已禁用项计入跳过文案
- 批量启用只请求已禁用项，后端被调用的 id 集合与预期一致
- 目标集为空时不发请求、给出错误提示
- 完成后选中集合清空
- 归属断言：两个按钮在 `role="toolbar"` 行内，不在标题行框线容器内（第二轮布局的回归护栏同款写法）

### 1.7 OpenSpec 归属

属于 Admin API 与凭据管理变更，按 `AGENTS.md` 必须建 change，建议名 `add-admin-ui-batch-enable-disable`。spec 落在既有 capability `admin-ui-credential-list-view` 或新开 `admin-ui-batch-credential-actions`，提案时定。

## 2. 其他能力缺口盘点

按「后端已具备 / 前端零消费」「前端可独立完成」「需要前后端协同」三类扫过后端全部 28 条路由与前端消费面，得出以下七项。前六项来自能力面扫描，第七项来自用户反馈的凭据 #11「未获取身份」案例。

### 2.1 全局模型目录查看

现状：`GET /api/admin/models/catalog` 后端已提供，返回 `{success, count, models, modelItems, updatedAt}`（`src/admin/types.rs:196`），前端零消费，`rg "models/catalog" admin-ui/src` 命中 0。现在只有单凭据的模型目录弹窗（`credential-models-dialog.tsx`），没有全局聚合视图。这是唯一完全闲置的后端能力。

方案：在「更多操作」溢出菜单加一项「全局模型目录」，打开只读弹窗，列模型 id、解析标记（`modelItems` 里的 `resolvable`、`resolveTo`）与 `updatedAt`。弹窗结构照 `credential-models-dialog` 改，列表源换成新端点。

影响面：前端新增一个 API 封装、一个弹窗组件、一个菜单项。纯只读，无风险点。

建议归属：可与批量启用/禁用同一个 change，或单独一个小 change。

### 2.2 凭据元数据导出

现状：导入有三个入口（单条、批量、KAM 文档），导出为零。备份与迁移目前只能直接复制服务器上的 `credentials.json`。

边界先划清：`credentials.json` 含 `refreshToken`、`clientSecret` 等原始密钥，只能留在服务器磁盘上，不做浏览器下载。列表接口返回的是脱敏形态（`refreshTokenHash`、`apiKeyHash`、`maskedApiKey`），所以可导出的是元数据清单：id、身份名、认证方式、订阅等级、优先级、禁用状态与原因、余额快照、模型数、最近使用时间。用途是盘点、交接、批量核对，不是凭据备份。

方案：前端实现。入口放「更多操作」菜单，导出当前筛选后的全集（复用 `fetchAllDisabledIds` 的翻页遍历模式，去掉 `disabled: true` 条件），前端组装 CSV 与 JSON 两种格式，`Blob` + `a[download]` 触发下载。CSV 注意身份名含逗号时的引号转义。

影响面：纯前端。风险点是分页遍历期间数据变化会轻微不一致，属可接受范围，与 `清除已禁用` 的全量遍历同款。

### 2.3 批量优先级调整

现状：优先级只有卡片上逐条「提高/降低」（±1，`credential-card.tsx:481`），选中多条后没有批量手段。后端单条接口 `POST /credentials/{id}/priority`，`set_priority` 每次调用立即重选当前凭据（`token_manager.rs:2091`）。

这一项的难点在语义而不是实现。优先级是全局排序，数字越小越优先，允许并列（`select_highest_priority` 取 `min_by_key`，并列时取条目顺序第一个）。批量调整有三种候选语义：

| 语义 | 行为 | 问题 |
| --- | --- | --- |
| 相对偏移 | 选中项优先级统一 ±N | 与其他凭据交叉后顺序不可预期 |
| 置顶 | 选中项按现序分配 0..n-1，其余顺延 | 需要改写未选中项，一次操作影响全部凭据 |
| 全量重排 | 选中项排前、未选中排后，全部重新编号 | 同上，且未选中项的相对顺序也要定义 |

建议：先做「置顶」一个语义。实现上倾向新增后端批量端点（一次锁内完成全量重排 + 一次持久化 + 一次重选），前端循环调单条接口会在中途失败时留下半重排状态，且每次调用触发一次重选，行为噪音大。这一项因此是盘点里唯一明确建议动后端的。

建议归属：单独一个 change，先把语义定稿再动手。

### 2.4 定时刷新的两项欠账

现状：`docs/admin-ui-responsive-layout-optimization-design.md` 第 5 节已把两项写好，本次只摘要，细节以该文档为准。

5.1 串行改限并发：`runAutoRefresh` 现在是串行逐条查余额。改法是固定 3 个 worker 从共享游标取任务（不用 `Promise.all` 分批，分批会被每批最慢的拖住），失败计数改为按任务收集再汇总。注意这是仓库里第一次对同类批量循环用并发；且后端 `refresh_lock` 是全局单锁（`token_manager.rs:839`），token 临近过期时刷新段仍串行，收益是 2x 到 3x 的区间而非定值。真正的约束在上游：`force=true` 跳过缓存，并发放大后单位时间上游请求量上升。

5.2 开关与间隔持久化：照 `storage.ts` 的 `getPerPage` 模式加 `getAutoRefresh`/`setAutoRefresh`，间隔一并存，读回时校验（整数且不小于 10，非法回落默认）。三个保护：恢复为开启时首屏不立刻跑一轮、首次恢复给一次 toast 提示、非法值回落。

两项互不依赖，可分开实施，也可合成一个前端 change。

### 2.5 筛选维度扩展

现状盘点（2026-09-04 核查，比早期印象更全）：

- 后端 `CredentialsQuery` 支持 8 个筛选参数：订阅等级、禁用状态、认证方式、优先级区间（`priorityMin`/`priorityMax`）、email、`hasProfileArn`、id（`src/admin/types.rs:16`）
- 前端筛选栏已暴露 6 项：订阅等级、认证方式、禁用状态、Profile、email、id（`credential-filter-bar.tsx`）
- 真正未暴露的：优先级区间（后端已支持，纯前端加控件）
- 前后端都没有的：按 `disabledReason` 筛选（值为 6 种枚举：Manual、TooManyFailures、TooManyRefreshFailures、QuotaExceeded、InvalidRefreshToken、InvalidConfig）、按 `endpoint` 筛选

方案分两步：

1. 优先级区间筛选：纯前端，筛选栏加两个数字输入，走既有 `priorityMin`/`priorityMax` 参数与 URL 同步
2. `disabledReason` 筛选：后端 `CredentialsQuery` 加字段、`get_credential_facets` 或硬编码枚举提供选项、前端加下拉。凭据多以后「找出所有额度用尽的」是高频需求，这一项价值最高

### 2.6 批量推理测试

现状：`POST /credentials/{id}/test` 只有卡片单条入口（`credential-card.tsx`），请求体 `{model?}` 默认 `claude-sonnet-4.6`，响应含 `success`、`resolvedModel`、`resolveKind`、`reply`、`latencyMs`（`src/admin/types.rs:488`）。批量导入一批新凭据后逐张点开测试很繁琐。

方案：照批量验活的模式加「批量推理测试」：串行循环（推理测试消耗上游配额且每次真跑一轮对话，不做并发），结果弹窗复用 `BatchVerifyDialog` 的结构，每条展示成功与否、实际模型、耗时。`VerifyResult` 类型需要泛化或新开一个结果类型。

影响面：前端新增一个批量操作、一个结果类型、一个弹窗或复用现有弹窗。注意它排在工具栏行的体积，建议与批量启用/禁用同一批评估按钮总数。

### 2.7 凭据 #11 案例：API Key 凭据身份获取缺口

用户反馈凭据 #11 显示「未获取身份」。排查与实测都已完成，结论是这是代码路径缺口，不是数据损坏，并已在部署机上用 #11 的 `kiroApiKey` 直打真实端点验证修复路径可行。

根因链：

- 身份名取值链是 `email → nickname → userId`，全缺回落「未获取身份」（`admin-ui/src/lib/credential-view.ts:14`）
- #11 是 `api_key` 凭据，存储里三个身份字段都为空，只带 `kiroApiKey`
- 身份自动填充只有一条路径：overlay/upsert 流程里调 `get_user_info`（`token_manager.rs:2390`），它打 `getUsageLimits?isEmailRequired=true` 取 email 与 userId
- 这条路径被守卫 `if !new_cred.is_api_key_credential()`（`token_manager.rs:2386`）挡着，api_key 凭据直接跳过
- 且 api_key 凭据没有 `access_token`，就算去掉守卫，内层 `if let Some(access)` 也取不到值

实测（2026-09-04，部署机 172.20.66.24，token 未回传，邮箱已脱敏）：

| 探测 | 请求形态 | 结果 |
| --- | --- | --- |
| 1 | 仅 `Authorization` + KiroAPIProxy UA（现 `get_user_info` 的头） | HTTP 403，`User is not authorized` |
| A | 生产 `get_usage_limits` 完整形态：加 `tokentype: API_KEY` 头、完整 user-agent（含 machineId） | HTTP 200，`userInfo.email` 返回 28 字符（`sh***@mail.delphiedu.com`），`userInfo.userId` 返回 49 字符 |
| B | 同 A 但去掉 `isEmailRequired=true` | HTTP 200，`email` 空串，`userInfo.userId` 仍返回 |

两个结论：

1. api_key 凭据打 `isEmailRequired=true` 端点能拿到 email 与 userId，分界线是 `tokentype: API_KEY` 头。这是 API Key 凭据调上游的既有要求，生产余额路径 `get_usage_limits` 已经这么加（`token_manager.rs:497`），`docs/builderid-403-repro-evidence-2026-08-25.txt` 里也有同组对照。现在的 `get_user_info`（`user_info.rs:40`）不带这个头，所以光去掉守卫不够，api_key 凭据照样 403。
2. 余额查询响应里本来就有 `userInfo.userId`，只是 `email` 为空。当前余额路径 `get_usage_limits`（`token_manager.rs:447`）只读 `subscription_title`，userId 被丢弃。

选定完整修复路径：给 `get_user_info` 加 token 类型，放宽守卫，让 api_key 凭据用 `kiroApiKey` 走 `isEmailRequired=true`，email 与 userId 都拿到。改动点：

- `get_user_info` 签名（`user_info.rs:40`）加一个标识参数，如 `is_api_key: bool`，或直接把参数 `access_token` 语义放宽为 bearer token 并加 `is_api_key` 开关
- `is_api_key` 为真时追加 `tokentype: API_KEY` 请求头，`Authorization` 用 `kiroApiKey` 填充
- 放宽 `token_manager.rs:2386` 的守卫：api_key 凭据改走 `kiroApiKey` 作 bearer token 调 `get_user_info`；非 api_key 仍走 `access_token`
- 失败保持现有 best-effort 语义（只打 warn，不阻断入库），与现行为一致

备选轻量路径（未选）：余额/验活查询后顺带回填响应里现成的 `userInfo.userId`。改动更小，但拿不到 email，身份名只有 userId。用户已选定完整路径，此条仅留档备查。

影响面：纯后端（`src/kiro/user_info.rs`、`src/kiro/token_manager.rs`），前端无改动，身份字段回填后取值链自然生效。风险点是放宽守卫会让每次 api_key 凭据导入多打一次上游 `isEmailRequired` 请求，频率与凭据导入一致，可接受。

## 3. 拆分与顺序建议

八项按依赖与风险归成四批：

| 批次 | 内容 | 改动面 | 说明 |
| --- | --- | --- | --- |
| 一 | 批量启用/禁用（第 1 节） | 纯前端 | 用户主诉，模式最成熟，先做 |
| 二 | 全局模型目录（2.1）、元数据导出（2.2）、定时刷新两项（2.4） | 纯前端 | 互不依赖，可并行或任选顺序 |
| 三 | 筛选维度扩展（2.5） | 前后端协同 | 优先级区间纯前端可先做；`disabledReason` 要动后端查询 |
| 四 | 批量优先级（2.3）、批量推理测试（2.6） | 2.3 动后端；2.6 纯前端 | 2.3 先定语义再动手；2.6 与第一批合并评估工具栏体积 |
| 五 | API Key 凭据身份获取（2.7） | 纯后端 | 修复路径已实测可行，独立小件，可先于任何批次落地 |

每批都按 `AGENTS.md` 建独立 OpenSpec change，走 `openspec-propose` 生成提案。批次一若与批次四的 2.6 合并，工具栏行按钮总数会到 8 个，768px 下折 3 行，仍可接受但值得在提案里写明。

## 4. 通用约束

- 所有批次遵守零新增编译告警（`cargo check --release --all-targets`）与 Skills 门禁
- 动到 `dashboard.tsx` 工具栏行的改动必须带归属断言测试（第二轮布局的回归护栏模式）
- 新增文档交付前过 `humanizer-zh`；提交信息走 `caveman-commit`
- 禁止真实凭据进测试与文档；2.2 的导出严格限定元数据，不导出原始密钥
