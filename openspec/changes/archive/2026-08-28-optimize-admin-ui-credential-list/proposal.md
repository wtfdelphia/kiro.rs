## Why

Admin UI 凭据管理页已具备批量验活、批量刷新 Token、定时刷新等运维能力，但列表本身的可读性与可检索性停留在早期形态：卡片标题只能显示 email 或 id 之一，徽章区无条件并列三枚同源信息，没有任何筛选控件，订阅等级与剩余用量在页面打开时必然显示「未知」，分页则是后端全量返回、前端 `slice` 切片的假分页。

其中「订阅等级恒为未知」是最值得优先修的一项：线上核实（172.20.66.24，pm2 托管进程）显示 10/10 凭据已把 `subscriptionTitle` 落盘、10 条余额缓存全部在 TTL 内且额度非零。数据齐备，缺的只是列表契约里的暴露路径。这五个问题落在同一条数据链上，逐个打补丁会反复改同一批构造点，所以一次做完。

完整分析见 `docs/admin-ui-credential-list-display-and-query-optimization-design.md`。

## What Changes

- **列表契约暴露既有数据**：`CredentialEntrySnapshot` 与 `CredentialStatusItem` 增加 `subscription_title`；`CredentialStatusItem` 增加只读余额缓存快照 `balance`（含 `cachedAt` / `ageSecs` / `stale`）。列表接口保持同步无 I/O，MUST NOT 调用 `get_balance`。
- **服务端组合筛选**：`GET /api/admin/credentials` 接受 `CredentialsQuery`（订阅等级、禁用状态、类型、优先级区间、email 模糊、profile 状态、id 模糊），条件间 AND。筛选只在服务端实现一次，不做前端本地兜底。类型维度覆盖 `AuthMethod` 的全部四个变体，含 `external_idp`。
- **BREAKING — 服务端分页**：`GET /api/admin/credentials` 从「返回全量数组」变为「返回一页 + `pageInfo`」。契约借鉴 GitHub REST API 的三条做法：`page` / `perPage`（缺省 12，上限 100）、`perPage` 有硬上限、越界返回空数组而非 4xx。导航信息只由 `pageInfo` 提供，不写 `Link` 响应头。旧客户端不传参数会从「拿到全部凭据」变成「拿到前 12 条」。
- **BREAKING — 排序全序化**：`sort_by_key(|c| c.priority)` 改为 `sort_by_key(|c| (c.priority, c.id))`。`priority` 可重复，原排序不构成全序，分页下会导致翻页时记录重复或漏项。假分页把这个缺陷掩盖了。
- **新增 `GET /api/admin/credentials/facets`**：返回全集去重后的 `subscriptionTitles` / `authMethods`（纯内存聚合，无 I/O）。服务端分页后筛选下拉框无法再从当前页数据去重生成。
- **卡片展示收敛**：标题改为「身份名 + `#id`」共存（身份名优先级 email → nickname → userId → 「未获取身份」）；徽章按信息增量渲染，能由 `authMethod` 推导的 provider 与等于 `defaultEndpoint` 的 endpoint 不再占位。
- **分页 UI**：借鉴 Primer Pagination 的 `marginPageCount` / `surroundingPageCount` 折叠模型，替换现有「上一页 / 页码文本 / 下一页」控件。新增每页条数选择器（持久化到 localStorage）、查询串同步（`history.replaceState`，不引入路由库）、越界自动回退末页。
- **新增全选控件**：现有实现只有逐个勾选，勾满一页要点十二次。补上全选，作用域 MUST 限定为当前页，不隐式扩展到筛选后全集；已选凭据跨页保留，控件呈现未选 / 部分选 / 全选三态，跨页已选数在 UI 明示。
- **选中状态自带属性快照**：不能再按 id 回列表响应反查。分页后跨页 id 反查得空值，凭据会被静默剔除并计入「已跳过」。四处反查（已选禁用数、批量删除、批量恢复、批量刷新 Token）全部改为读快照，快照至少含 `disabled` 与 `failureCount`。
- **分页后的三处前端回归**：实时余额缓存的清理判据须从「不在当前页」改为「已从系统删除」，否则翻页会丢弃已查到的实时值；「清除所有已禁用凭据」的删除范围须从当前页数组改为逐页取回的全量 id，否则全量措辞下只清了一页；该入口的可用性判据与文案条数改用 `total - available`，不再用当前页计数。
- 余额缓存预热（让全新部署首屏也有值）**不列入本次范围**，理由见 Impact 中的取舍说明。

## Capabilities

### New Capabilities

- `admin-credential-list-query`: `GET /api/admin/credentials` 的筛选与分页契约。覆盖 `CredentialsQuery` 各字段语义（AND 组合、大小写不敏感、模糊与精确的边界、区间闭合、`__unknown__` 哨兵值、四个 `authMethod` 取值）、分页规则（`page`/`perPage` 默认与上限、以 `i64` 反序列化使负值能进 clamp、clamp 而非报错、越界返回空数组、导航信息只由 `pageInfo` 提供、`(priority, id)` 稳定全序、`filter -> sort -> paginate` 顺序不可交换）、`total` / `available` / `filteredTotal` / 当前页条数四个基数的语义边界、分页不改变单项字段契约，以及 facets 端点与批量操作作用域。
- `admin-ui-credential-list-view`: 凭据卡片与列表的展示契约。覆盖标题的 email + id 共存与身份名兜底顺序、徽章的信息增量渲染规则（含四个类型取值的文案与 `external_idp` 的 provider 不可推导）、订阅等级三来源的优先级、余额的三态视图（实时 / 缓存 / 未查询）及其文案、分页控件的页码折叠形态与无障碍要求（`nav aria-label`、`aria-current="page"`、省略号 `aria-hidden` 且不可聚焦、边界禁用而非移除、`aria-live` 播报当前页），以及「声称作用于全量的操作不得静默退化为当前页」。

### Modified Capabilities

- `admin-ui-balance-ops`: 新增两条 Requirement。一是「列表内联余额是只读缓存快照」，MUST 携带 `cachedAt`（Unix 秒，与 `nextResetAt` 同一时间表示）/ `ageSecs` / `stale` 并在 UI 显式标记新鲜度，MUST NOT 触发上游请求，MUST NOT 参与批量验活的成功判定。二是「前端实时余额结果不因翻页丢弃」，清理判据 MUST 是「凭据已从系统删除」而非「不在当前页响应中」。既有三条 Requirement 的文字不变：「作用于当前页启用凭据」的语义在服务端分页下自动成立，只是「当前页」的来源从前端切片结果变成服务端返回结果。

## Impact

**后端**：`src/admin/types.rs`（`CredentialsQuery`、`PageInfo`、`CredentialBalanceSnapshot`，扩 `CredentialStatusItem` 与 `CredentialsStatusResponse`）、`src/admin/service.rs`（`get_all_credentials` 改造为 `query_credentials` + 薄封装，排序全序化）、`src/admin/handlers.rs`（接 `Query<CredentialsQuery>`，返回类型不变，仍是 `Json`）、`src/admin/router.rs`（facets 路由）、`src/kiro/token_manager.rs`（`CredentialEntrySnapshot` 与 `snapshot()`）。`available` 的既有口径不动。

**前端**：`admin-ui/src/types/api.ts`、`admin-ui/src/api/credentials.ts`、`admin-ui/src/hooks/use-credentials.ts`（接收查询对象进 `queryKey`，加 `placeholderData`）、`admin-ui/src/components/credential-card.tsx`、`admin-ui/src/components/dashboard.tsx`、新增 `admin-ui/src/lib/pagination.ts`、`admin-ui/src/lib/storage.ts`（加 `getPerPage`/`setPerPage`）。

**API 消费者**：`GET /api/admin/credentials` 的响应形态变更是 breaking。本仓库 Admin UI 经 rust-embed 与后端同版本发布，前后端无版本错配窗口；外部脚本消费者需显式传 `perPage` 并按 `pageInfo` 逐页取回。MUST 在版本说明与 README 中标注，`README.md:786` 的「获取所有凭据状态」一行同时改写为分页语义。

**既有测试**：`src/admin/service.rs:1965` 与 `:1975` 两个列表单测不会因分页失败。两者都用 `manager_with_one()`，只有 1 条凭据，默认 `perPage=12` 完全容纳，`status.total == 1` 与 `credentials[0]` 的断言照旧成立。这两个用例只在 `get_all_credentials` 签名变化时需要跟随调整，不涉及断言语义。

**跨 capability**：`admin-ui-model-ops` 要求列表项携带 `modelCount`，分页后字段构成不变，但该约束的作用域从「全部凭据」变为「当前页凭据」。「遍历一次列表即可发现缺模型缓存的凭据」这个用法随之失效，需逐页取回或改用筛选。

**依赖**：无新增。分页 UI 复用既有 Radix Select 与 Tailwind，不引入 `react-router`（`package.json` 已确认无该依赖）。

**取舍记录**：已评估并否决「不带 `page` 参数时返回全量」的兼容折中。它让接口有两种互斥响应形态、`pageInfo` 语义变可选，且默认的 30s 轮询路径仍全量传输，恰好绕开了要解决的问题。

`Link` 响应头（RFC 8288）同样评估后不做。它的唯一潜在受益者是外部脚本消费者，而本仓库的实际调用方只有同版本发布的内嵌 Admin UI，后者按 `pageInfo` 导航。写这份头意味着同一事实有两个来源、handler 返回类型要改成 `(HeaderMap, Json<T>)`，两处不一致时无从判断以哪个为准。`pageInfo` 的 `hasPrev` / `hasNext` / `totalPages` 已足以推出四个方向的页码。

余额缓存启动预热也不做：会在启动时对每个启用凭据发起上游请求，与 `admin-ui-balance-ops` 「定时刷新默认关闭、不发出未经操作者授权的自动请求」的取向冲突；先观察契约打通后首屏还剩多少「未查询」再定。

**endpoint 徽章的实际收益**：`src/main.rs:120-123` 只注册了 `ide` 一个端点，`src/model/config.rs:339` 的默认值也是 `ide`，且 `src/admin/service.rs:134` 已把凭据缺省的 endpoint 兜底为默认值。因此现状下所有凭据的 endpoint 恒等于默认，徽章恒被省略，「不等时渲染」是不可达分支。这条收敛的实际收益就是去掉一枚永远冗余的徽章；保留判断逻辑是为将来注册多端点时无需再改，不作为当前可观察行为写入验收。此前记录的「`defaultEndpoint` 取值未确认」已由仓库内证据解决，不需要读线上 `config.json`。
