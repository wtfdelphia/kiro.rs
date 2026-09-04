# Admin UI 凭据列表展示与查询优化方案

> 状态：设计文档（未实现）
> 日期：2026-08-26
> 范围：
> 1. 卡片标题同时展示 email 与凭据 id（当前二选一）
> 2. 类型徽章由三枚（authMethod / provider / endpoint）收敛为「当前有效」的最小集合
> 3. 凭据管理支持按订阅等级、禁用状态、类型、优先级、email 模糊、profile 状态、id 模糊查询
> 4. 修复「订阅等级」与「剩余用量」首屏恒为「未知」
> 5. 服务端分页（借鉴 GitHub REST API 契约 + Primer Pagination 组件设计）
>
> 分析手段：kiro-rs CodeGraph 增量索引（161 files / 3,290 nodes / 10,242 edges）+ 源码精读 + 172.20.66.24 上 pm2 托管的 kiro-rs 线上进程核实 + GitHub 分页设计调研
>
> 前置约束：本方案改动 Admin API 与凭据管理面，按 `AGENTS.md` 必须先建 OpenSpec change 再实现（见第 9 节）
>
> 实现期修订（2026-08-26）：`Link` 响应头（RFC 8288）方案在 `optimize-admin-ui-credential-list` 评审时被否决，分页导航只由响应体 `pageInfo` 提供。全文凡提到 `Link` 头的段落都留作决策记录，不再是待实现项，包括 G5 目标、第 5 节「分页元数据必须同时出现在响应体与响应头」、GitHub 契约借鉴表、`Link` 头格式与 `handler 返回 (HeaderMap, Json<T>)` 的实现要求、`link_header_*` 测试项、8.4 节验收第 9/12/14 项、实施步骤 8，以及 6.5 节、第 7 节风险表、第 9 节能力清单里的相应条目。事实源是该 change 的 `design.md` 与 `specs/admin-credential-list-query/spec.md`。

---

## 1. Context and motivation

### 1.1 背景

Admin UI 的凭据管理页（`admin-ui/src/components/dashboard.tsx`）已具备批量验活、批量刷新 Token、批量余额/订阅查询、定时刷新、清除已禁用等运维能力，卡片（`admin-ui/src/components/credential-card.tsx`）已展示优先级、失败次数、Profile 状态、模型缓存数等。

但列表本身的「可读性」与「可检索性」停留在早期形态：标题只能显示 email 或 id 之一，徽章区无条件并列三枚同源信息，没有任何筛选控件，订阅等级与剩余用量在页面打开时必然显示「未知」，分页则是后端全量返回、前端 `slice` 切片的「假分页」。这五个问题都落在同一条数据链上，适合一次性设计。

### 1.2 问题陈述

| # | 问题 | 根因 | 影响 |
| --- | --- | --- | --- |
| P1 | email 与 id 只能看到一个 | `credential-card.tsx:213-215` 标题为 `credential.email \|\| 凭据 #${id}` 的二选一 | 有 email 时无法对照 id 做批量操作/日志比对；无 email 时看不出账号身份 |
| P2 | 类型区三枚徽章冗余 | `credential-card.tsx:237-262` 无条件并列 authMethod、provider、endpoint | `IdC` 与 `BuilderId` 表达同一事实；`endpoint` 绝大多数等于默认端点，占位无信息量 |
| P3 | 无法按条件查询列表 | 后端 `get_all_credentials()` 无任何 Query 参数；前端仅 `slice` 客户端分页（`dashboard.tsx:82-85`） | 凭据变多后只能翻页目视查找 |
| P4 | 订阅等级 / 剩余用量恒为「未知」 | 三处叠加，见 3.1 | 首屏零信息，必须先手动点「批量余额/订阅」才有值 |
| P5 | 分页是「假分页」 | 后端全量返回，前端 `slice` 切片（`dashboard.tsx:82-85`）；`itemsPerPage = 12` 硬编码；分页控件仅上一页/下一页 + 页码文本 | 规模增长后响应体线性膨胀；无法跳页、无法改每页条数、页码不可分享；筛选若留在客户端会与分页语义冲突 |

### 1.3 Goals

- G1：卡片同时呈现 email 与 `#id`，无 email 时有确定的兜底身份（nickname → userId → `#id`）
- G2：徽章区按「信息增量」收敛，同源信息只出现一次，与默认值相同的端点不占位
- G3：凭据列表支持组合查询：订阅等级、禁用状态、类型、优先级区间、email 模糊、profile 状态、id 模糊
- G4：页面首屏即能展示订阅等级与剩余用量，且明确区分「缓存值（含缓存时间）」与「真的未知」
- G5：服务端分页，契约借鉴 GitHub REST API（`page` / `per_page` + `Link` 头 + 响应体 `pageInfo`），UI 借鉴 Primer Pagination（首尾锚点 + 当前页邻域 + 省略号 + 无障碍标签）
- G6：筛选与分页在服务端组合正确，「筛选出 N 条」表达的是**筛选全集**基数而非当前页
- G7：不违反 `openspec/specs/admin-ui-balance-ops/spec.md` 既有要求（批量验活必须 force、批量余额入口允许命中缓存且不得暗示强制刷新、定时刷新默认关闭）
- G8：零新增编译告警；文档先行，实现前建 OpenSpec change

### 1.4 Non-goals

- 不改余额 TTL 语义本身（`BALANCE_CACHE_TTL_SECS = 300` 保留）
- 不新增「验活」口径，也不让列表内联的缓存余额参与验活判定
- 不改 Admin 鉴权（`x-api-key` 常量时间比较）
- 不动 `profile_arn` 解析策略（Profile 恒为「未解析」是另一条线，见 `docs/builderid-403-profile-arn-analysis-and-optimization-design.md`）
- 不引入 cursor / keyset 分页（凭据集合是内存中的有序小集合，无游标必要；见 6.5 取舍）
- 不引入 GraphQL 式 `edges`/`node` 包装（REST 风格保持一致）
- 不引入前端路由库（`react-router` 未在依赖内；分页状态用 `history.replaceState` 同步查询串，见 6.6）
- 不做无限滚动（运维列表需要稳定的可寻址页码与批量选择边界）

---

## 2. CodeGraph 对照与现状数据流

### 2.1 索引规模

| 项目 | 规模 | 本次相关核心符号 |
| --- | --- | --- |
| kiro-rs | 161 files / 3,290 nodes / 10,242 edges（node:sqlite + WAL） | `get_all_credentials`、`CredentialStatusItem`、`snapshot`、`CredentialEntrySnapshot`、`get_balance`、`fetch_balance`、`get_usage_limits_for`、`CachedBalance`、`load_balance_cache_from`、`save_balance_cache`、`spawn_warmup_models`、`CredentialCard`、`getCredentials` |

### 2.2 影响面（`codegraph impact CredentialStatusItem`）

11 个受影响符号，跨前后端：

```tex
src/admin/types.rs          struct CredentialStatusItem:26
src/admin/service.rs        method get_all_credentials:100
                            fn credentials_status_includes_model_count_default_zero:1965
                            fn credentials_status_model_count_from_cache:1975
admin-ui/src/types/api.ts   interface CredentialStatusItem:10
                            interface CredentialsStatusResponse:2
admin-ui/src/api/credentials.ts     function getCredentials:35
admin-ui/src/components/credential-card.tsx  CredentialCardProps:32 / CredentialCard:58
```

含义：契约扩字段是一次「窄而深」的改动，只有 1 个后端构造点、1 个前端消费组件、2 个已有单测需要同步，没有隐藏的第三方消费者。`codegraph callers get_all_credentials` 确认仅 `GET /credentials`（`src/admin/router.rs:30`）与两个单测调用它。

### 2.3 余额相关调用链（`codegraph callers get_usage_limits_for`）

```tex
get_usage_limits_for  <- fetch_balance (src/admin/service.rs:307)
                      <- ingest_from_request (src/admin/service.rs:353)
```

`get_usage_limits_for`（`src/kiro/token_manager.rs:2126`）在成功后会把 `subscription_title` 写回 `entry.credentials` 并 `persist_credentials()`。订阅等级因此已经落盘，却在 UI 上看不到。

### 2.4 当前数据流（问题现场）

```tex
GET /api/admin/credentials
  -> AdminService::get_all_credentials()
       -> MultiTokenManager::snapshot()          // 不含 subscription_title
       -> get_credential_models_cached(id)        // 只补模型缓存
       -> sort_by_key(priority)                   // 无过滤参数
  -> CredentialStatusItem[]                       // 无 subscriptionTitle / 无任何余额字段

GET /api/admin/credentials/{id}/balance?force=
  -> AdminService::get_balance()
       -> balance_cache (HashMap<u64, CachedBalance>, TTL 300s, 落盘 kiro_balance_cache.json)
       -> miss/force -> fetch_balance -> get_usage_limits_for -> 写回 subscription_title + persist
  -> BalanceResponse { subscriptionTitle, currentUsage, usageLimit, remaining, usagePercentage, nextResetAt }

Admin UI
  dashboard.balanceMap: Map<number, BalanceResponse> = new Map()   // 初始为空
    写入点仅 4 处：handleQueryCurrentPageInfo（允许缓存）
                  onBalanceRefreshed（单卡）
                  handleBatchVerify（force）
                  runAutoRefresh（force，默认关闭）
  credential-card: balance={balanceMap.get(id) || null}
    -> 订阅等级 = balance?.subscriptionTitle || '未知'
    -> 剩余用量 = balance ? `${remaining}/${usageLimit}` : '未知'
```

两条链完全不相交：已落盘的订阅等级与已缓存的余额都停在后端，列表接口一个字节都不带出来。

---

## 3. 根因分析

### 3.1 P4「订阅等级 / 剩余用量恒为未知」——三处叠加

| 层 | 事实 | 位置 |
| --- | --- | --- |
| 契约层 | `CredentialStatusItem` 既无 `subscriptionTitle` 也无任何余额字段，尽管 `KiroCredentials.subscription_title` 已持久化 | `src/admin/types.rs:26-85`、`src/kiro/model/credentials.rs:96` |
| 快照层 | `snapshot()` 逐字段搬运 entry，唯独没有导出 `subscription_title`，因此 `get_all_credentials` 即使想带也拿不到 | `src/kiro/token_manager.rs:1987-2054` |
| 前端层 | `balanceMap` 初始为空 `Map`，只被手动/定时动作填充，首屏必然落到 `\|\| '未知'` 分支 | `dashboard.tsx:48`、`credential-card.tsx:337/360-369` |

结论：这不是「数据没有」，而是**已有数据没有暴露路径**。远端核实（第 4 节）显示 10/10 凭据已落盘 `subscriptionTitle`、10 条余额缓存全部在 TTL 内且 `usageLimit` 非零。UI 显示「未知」的同时，后端手里握着完整答案。

修好契约层与快照层即可让首屏有值，无需任何额外上游请求。

### 3.2 P2「三枚徽章冗余」——同源信息的三次表达

`snapshot()` 已把 `auth_method` 归一化：API Key 凭据 → `"api_key"`；`builder-id` 与 `iam` → 统一 `"idc"`；其余原样透传（如 `social`）。而 `provider` 是上游 identity provider 原值（`BuilderId` / `Github` / ...）。

于是用户看到的 `IdC` + `BuilderId` 是同一件事说了两遍，`idc` 本就是 `builder-id` 归一化后的结果。第三枚 `endpoint` 在绝大多数部署里等于 `config.defaultEndpoint`（`src/admin/service.rs:134` 明确以默认端点兜底），因此也是零信息量。

### 3.3 P3「无查询能力」

后端 `get_all_credentials(&self)` 无入参，handler（`src/admin/handlers.rs:19`）也不接 `Query`；前端只做 `slice` 分页。想按订阅等级找 FREE 账号、按 email 域名找一组凭据，都只能翻页目视。

顺带注意：筛选所需的 7 个维度里，**订阅等级与 profile 状态目前不在列表契约中**（`subscriptionTitle` 缺失；`hasProfileArn` 已有）。所以 P3 的前置条件恰好是 P4 的修复项，两者必须一起做，这也是本方案把多点合并的原因。

### 3.4 P5「假分页」，以及它与 P3 的强耦合

现状（`dashboard.tsx:82-85`）：

```tsx
const totalPages = Math.ceil((data?.credentials.length || 0) / itemsPerPage)  // itemsPerPage = 12 硬编码
const currentCredentials = data?.credentials.slice(startIndex, endIndex) || []
```

后端全量返回、前端切片，带来几个后果：

1. **响应体随凭据数线性膨胀**。`useCredentials` 有 30s `refetchInterval`，等于每 30 秒传一次全量。P4 给每条记录再加一个 `balance` 对象后单条体积上升，膨胀系数被放大。
2. **分页不可寻址**：`currentPage` 只是组件 state，刷新即回第 1 页，页码无法分享给同事。
3. **控件能力最小**：只有上一页/下一页 + 一行页码文本，无法跳页，也无法调整每页条数。

**关键耦合（这是本次修订的核心）**：分页一旦下沉到服务端，筛选就**不能**留在客户端。原因是二者的执行顺序不可交换：

```tex
错误组合（服务端分页 + 客户端筛选）：
  后端 page=2&perPage=12 -> 返回第 13..24 条 -> 前端在这 12 条里筛 email
  结果：命中数取决于当前页，「筛选出 N 条」变成谎言；翻页时筛选结果跳变

正确组合（服务端筛选 + 服务端分页）：
  后端 filter -> sort -> paginate
  结果：filteredTotal 是筛选全集基数，页码基于筛选全集计算
```

因此本次修订把上一版「后端提供参数 + 前端本地筛选兜底」的双实现方案**收敛为服务端单一实现**：筛选与分页在同一个函数里按 `filter -> sort -> paginate` 顺序执行，语义只有一处。这既消除了前后端语义漂移的风险，也让 6.3 里原先需要「同表驱动保证一致」的测试负担消失。

---

## 4. 线上核实（172.20.66.24，pm2）

| 核实项 | 结果 |
| --- | --- |
| pm2 进程 | `kiro-rs`（id=4，pid 2117680，online 约 6h） |
| 版本一致性 | 运行中二进制 md5 与 `kiro-rs-v2026.8.13-Linux-x64` 完全一致，确认线上即 2026.8.13（与本地 `Cargo.toml` 同版本） |
| 监听 | `172.20.66.24:18990` |
| Admin 鉴权 | 未带 `x-api-key` 请求 Admin 端点返回 401，鉴权链正常 |
| 凭据规模 | 10 个凭据 |
| `subscriptionTitle` 落盘 | 10 / 10 有该字段（`null_subscription_title = 0`） |
| `email` / `userId` 落盘 | 各 9 / 10 |
| `profileArn` 落盘 | **0 / 10** |
| 余额缓存 `kiro_balance_cache.json` | 10 条，全部在 TTL 内；`subscriptionTitle` 非空，`usageLimit` 非零（`zero_usage_limit = 0`） |
| 线上二进制字段探针 | `strings` 确认含 `hasProfileArn` / `maskedApiKey` / `refreshFailureCount`；订阅相关字符串仅出现在余额路径，与「列表契约不含 subscriptionTitle」一致 |

未能核实的项（如实声明）：

- 线上 `config.json` 的具体配置未读取（读取被权限策略拒绝，且 `AGENTS.md` 明令永不提交/披露该文件）。因此 `defaultEndpoint` 的线上实际取值未确认；6.2 节的端点收敛规则以「与 `settings/endpoint` 返回的默认端点比较」实现，不依赖对该文件的假设。
- 上述所有远端数据均以聚合计数形式获取，未读取任何明细值，文档中不含任何真实密钥、token、邮箱或账号。

推论：P4 在线上是 100% 可复现的「假未知」，数据齐备，只差暴露。`profileArn` 0/10 与仓库既有的 BuilderId 403 分析互相印证；本方案的 profile 筛选维度因此在线上会呈现「全部未解析」，这是真实状态而非筛选缺陷。

---

## 5. 设计原则

1. **先打通已有数据，再考虑新增请求。** P4 的最优解是暴露既有落盘值与既有缓存，而不是让列表接口去打上游。列表接口 MUST 保持无 I/O、同步、可被 30s 轮询安全调用。
2. **缓存值必须自带时间戳与新鲜度标记。** `admin-ui-balance-ops` 要求缓存语义必须向操作者披露；内联余额不能伪装成实时值。
3. **区分三种状态而不是两种。** 「实时值」/「缓存值（含年龄）」/「从未查询过」是三种不同信息，UI 不应把后两者一起塌缩成「未知」。
4. **筛选与分页共处服务端，顺序固定为 `filter -> sort -> paginate`。** 语义只有一处实现，无前后端漂移可能（见 3.4）。
5. **分页元数据必须同时出现在响应体与响应头。** 借鉴 GitHub REST API：`Link` 头（RFC 8288）服务脚本/CLI 消费者，响应体 `pageInfo` 服务 UI；两者由同一份数据渲染，不可能不一致。
6. **分页参数越界要收敛而非报错。** GitHub 对超出末页的请求返回空数组而非 4xx；本方案对 `page` 做 clamp 并在 `pageInfo` 中回报**实际生效**的页码，让前端能自我纠正。
7. **徽章按信息增量渲染。** 一个徽章若能被另一个徽章推导出来，或等于全局默认值，就不渲染。
8. **统计与筛选分离三个基数。** `total`（系统全量）/ `filteredTotal`（筛选全集）/ `credentials.len()`（当前页）各有明确语义，UI 不得混用。
9. **不破坏既有 Requirement。** 内联缓存余额只用于展示与筛选，MUST NOT 参与验活成功判定（`admin-ui-balance-ops` 的「TTL 内失效的凭据不得报成功」）。

---

## 6. 方案设计

### 6.1 P1：email 与 id 同时展示

**后端**：无改动（`email`、`userId`、`nickname` 已在契约内）。

**前端**（`credential-card.tsx` 标题区）：主标题取身份名，`#id` 作为固定后缀始终存在。

```tsx
// 身份名优先级：email -> nickname -> userId -> 无
const displayName = credential.email || credential.nickname || credential.userId || null

<div className="flex items-baseline gap-2 min-w-0">
  <CardTitle className="... break-all min-w-0 flex-1" title={displayName ?? `凭据 #${credential.id}`}>
    {displayName ?? '未获取身份'}
  </CardTitle>
  <span className="shrink-0 font-mono text-sm text-muted-foreground tabular-nums">
    #{credential.id}
  </span>
</div>
```

要点：

- `#id` 用 `shrink-0` + `font-mono` + `tabular-nums`，长 email 折行时 id 不被挤走，也不参与 `break-all`。
- 无身份时显示「未获取身份」而非 `凭据 #N`，避免和右侧 `#id` 重复表达同一个数字。
- `userId` 通常较长，作为标题时靠 `break-all` 折行；若需要复制，沿用卡片内已有的「点击展示/复制」范式即可（可选项，不作为必须）。

### 6.2 P2：类型徽章收敛

规则（按序判定，只渲染有信息增量的徽章）：

| 徽章 | 渲染条件 | 文案 |
| --- | --- | --- |
| 类型 | 恒渲染（`authMethod` 缺失时显示「未知类型」） | `api_key` → `API Key`；`idc` → `IdC`；`social` → `Social`；其他 → 原值 |
| provider | 仅当 `provider` **不能由 `authMethod` 推导**时渲染 | provider 原值 |
| endpoint | 仅当 `endpoint !== defaultEndpoint` 时渲染 | endpoint 原值 |

provider 可推导判定（前端纯函数，便于单测）：

```ts
// provider 与 authMethod 表达同一事实时不再单独渲染
const PROVIDER_IMPLIED_BY: Record<string, string[]> = {
  idc: ['builderid', 'builder-id', 'iam', 'awsbuilderid'],
}

function isProviderRedundant(authMethod?: string | null, provider?: string | null): boolean {
  if (!authMethod || !provider) return false
  const implied = PROVIDER_IMPLIED_BY[authMethod.toLowerCase()]
  return !!implied?.includes(provider.toLowerCase().replace(/\s+/g, ''))
}
```

于是线上的 `IdC` + `BuilderId` + `ide` 三枚收敛为一枚 `IdC`；而 `social` + `Github` 会保留两枚（Github 无法从 `social` 推出，属于真实信息增量）。

`defaultEndpoint` 来源：`GET /api/admin/settings/endpoint` 已返回 `defaultEndpoint`（前端类型 `EndpointSettings` 已存在于 `admin-ui/src/types/api.ts:181`）。dashboard 拉一次并透传给卡片；请求失败时退化为「恒渲染 endpoint」，即当前行为，不会误隐藏。

补充：`provider` 在卡片下方 Profile 行还有一处 `provider=` 文本（`credential-card.tsx:388-390`），它只在 `!hasProfileArn` 时出现、用于诊断 403，与徽章区职责不同，本次不动。

### 6.3 P3：组合查询（服务端单一实现）

**决策：筛选只在服务端实现一次。** 不做前端本地筛选兜底。

理由（相对上一版的修订）：既然分页下沉到服务端（6.5），客户端手里只有当前页数据，本地筛选在结构上就无法给出正确的筛选全集基数（见 3.4）。保留双实现只会制造两套语义和一份必须持续对齐的测试负担，而收益（省一次往返）在 10 条规模下不可感知。

**后端契约**（`src/admin/types.rs` 新增）：

```rust
/// GET /api/admin/credentials 查询参数（筛选字段全部可选，缺省表示不过滤）
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialsQuery {
    // ---- 筛选 ----
    /// 订阅等级精确匹配（大小写不敏感）；"__unknown__" 匹配无订阅等级的凭据
    pub subscription_title: Option<String>,
    /// 禁用状态
    pub disabled: Option<bool>,
    /// 类型（归一化后的 authMethod：api_key / idc / social ...），大小写不敏感
    pub auth_method: Option<String>,
    /// 优先级闭区间下界
    pub priority_min: Option<u32>,
    /// 优先级闭区间上界
    pub priority_max: Option<u32>,
    /// email 模糊匹配（子串，大小写不敏感）
    pub email: Option<String>,
    /// Profile ARN 是否已解析
    pub has_profile_arn: Option<bool>,
    /// id 模糊匹配（对 id 的十进制字符串做子串匹配）
    pub id: Option<String>,

    // ---- 分页（详见 6.5）----
    /// 页码，1-based；缺省 1
    pub page: Option<u32>,
    /// 每页条数；缺省 12，上限 100
    pub per_page: Option<u32>,
}
```

筛选语义定义（写入 spec，避免实现分歧）：

- 所有条件之间为 **AND**；未提供的字段不参与过滤。
- 字符串匹配统一 `trim()` 后比较；空串等价于未提供。
- `subscriptionTitle`：精确匹配（不模糊），大小写不敏感；哨兵值 `__unknown__` 用于筛出无订阅等级的凭据。
- `email`：子串匹配，大小写不敏感；`email` 为 `None` 的凭据在提供该条件时一律不匹配。
- `id`：对 `id.to_string()` 做子串匹配（`"1"` 匹配 1 / 10 / 21），符合「id 模糊」的原始需求。
- `priorityMin > priorityMax` 时返回空结果（不报错），保持接口幂等好用。
- **执行顺序固定为 `filter -> sort -> paginate`**，不可交换。

三个基数的语义（MUST 在 spec 中固定，UI 不得混用）：

| 字段 | 含义 | 受筛选影响 |
| --- | --- | --- |
| `total` | 系统全量凭据数 | 否 |
| `available` | 系统全量未禁用数 | 否 |
| `pageInfo.filteredTotal` | 筛选全集基数（分页前） | 是 |
| `credentials.len()` | 当前页条数 | 是 |

顶部三张统计卡继续用 `total` / `available` / `currentId`，表达系统状态；筛选提示用 `filteredTotal`。

**前端筛选栏**（dashboard，凭据管理标题行下方）：

| 控件 | 类型 | 请求触发 |
| --- | --- | --- |
| id 模糊 | Input | 防抖 300ms |
| email 模糊 | Input | 防抖 300ms |
| 订阅等级 | Select | 立即 |
| 类型 | Select | 立即 |
| 状态 | Select（全部 / 启用 / 已禁用） | 立即 |
| Profile | Select（全部 / 已就绪 / 未解析） | 立即 |
| 优先级 | 两个 number Input（min / max） | 防抖 300ms |

两个文本输入 MUST 防抖，否则逐字符触发请求并让 react-query 缓存碎片化。`useCredentials` 改为接收查询对象并进 `queryKey`：

```ts
export function useCredentials(query: CredentialsQuery) {
  return useQuery({
    queryKey: ['credentials', query],       // 查询变化即独立缓存条目
    queryFn: () => getCredentials(query),
    refetchInterval: 30000,
    placeholderData: (prev) => prev,        // 翻页/改筛选时保留上一页内容，避免闪空
  })
}
```

`placeholderData: (prev) => prev` 是这里的关键细节。服务端分页后每次翻页都是新的 `queryKey`，没有它就会在每次翻页时闪出加载态。

**Select 选项来源**：订阅等级与类型的可选值不能再从「当前页数据」去重生成（服务端分页后当前页看不到全集）。两个方案：

- 推荐：新增轻量 `GET /api/admin/credentials/facets`，返回全集去重后的 `subscriptionTitles` / `authMethods`（纯内存聚合，无 I/O）。选项完整且不随翻页跳变。
- 退化：硬编码已知枚举 + 「未知」项。上游新增订阅名时会漏项，不推荐。

交互约束：

- 筛选条件变化时 `page` 重置为 1。
- 提供「清空筛选」按钮；有任一条件生效时显示「筛选出 N 条」（N = `filteredTotal`）。
- **批量操作 MUST 只作用于当前页可见且已勾选的 id**。服务端分页后「全选」的语义边界必须收紧为当前页，否则操作者无法预期作用范围。翻页时保留已勾选 id 但在 UI 上明确显示「已选择 N 个（含其他页 M 个）」。

### 6.4 P4：首屏即有订阅等级与剩余用量

三步，逐层打通已有数据。

**Step 1 — 快照导出订阅等级**（`src/kiro/token_manager.rs`）

`CredentialEntrySnapshot` 增加：

```rust
/// 订阅等级（来自上次成功的 usage 查询并已持久化）
#[serde(skip_serializing_if = "Option::is_none")]
pub subscription_title: Option<String>,
```

`snapshot()` 中填 `subscription_title: e.credentials.subscription_title.clone()`。纯内存读取，无 I/O。

**Step 2 — 列表契约暴露订阅等级 + 余额缓存快照**（`src/admin/types.rs` / `service.rs`）

`CredentialStatusItem` 增加：

```rust
/// 订阅等级（来自持久化凭据；从未成功查询过时为 None）
#[serde(skip_serializing_if = "Option::is_none")]
pub subscription_title: Option<String>,
/// 余额缓存快照（仅当 Admin 余额缓存中存在该凭据条目时出现；不触发上游请求）
#[serde(skip_serializing_if = "Option::is_none")]
pub balance: Option<CredentialBalanceSnapshot>,
```

```rust
/// 列表内联的余额缓存快照：只读、可能过期，绝不代表实时值
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialBalanceSnapshot {
    pub current_usage: f64,
    pub usage_limit: f64,
    pub remaining: f64,
    pub usage_percentage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_reset_at: Option<String>,
    /// 缓存写入时间（RFC3339）
    pub cached_at: String,
    /// 缓存年龄（秒）
    pub age_secs: i64,
    /// 是否已超出 TTL（超出仍返回，由 UI 标记为陈旧）
    pub stale: bool,
}
```

`get_all_credentials` / `query_credentials` 中一次性取锁快照，避免逐条加锁：

```rust
let balances: HashMap<u64, CachedBalance> = self.balance_cache.lock().clone();
// ... 构造 item 时：
subscription_title: entry.subscription_title,
balance: balances.get(&entry.id).map(|c| Self::balance_snapshot(c, now)),
```

其中 `stale = (now - cached_at) >= BALANCE_CACHE_TTL_SECS`。

关键设计点：

- **超 TTL 的条目仍然返回，只是标 `stale: true`。** `load_balance_cache_from`（`service.rs:979-1011`）在**加载时**已丢弃超 TTL 条目，所以进程内的陈旧条目只可能来自本次运行期间的自然老化。返回并标记，比塌缩成「未知」信息量更大。
- **列表接口 MUST NOT 调用 `get_balance`**（它会打上游）。列表只读缓存，保持同步无 I/O。
- 订阅等级来自持久化凭据字段而非余额缓存，所以余额缓存整体被清空或进程刚重启时订阅等级依然可见（线上 10/10 已落盘，重启即可见）。

**Step 3 — 前端「实时优先、缓存兜底」**

`admin-ui/src/types/api.ts` 同步扩展 `CredentialStatusItem`（加 `subscriptionTitle?`、`balance?: CredentialBalanceSnapshot`）。

卡片解析为一个显式的三态视图模型：

```ts
type BalanceView =
  | { kind: 'live'; data: BalanceResponse }                       // 本次会话查询所得
  | { kind: 'cached'; data: CredentialBalanceSnapshot }           // 列表内联缓存
  | { kind: 'none' }                                              // 从未查询过

// balanceMap（本次会话实时值）优先于列表内联缓存
const view: BalanceView =
  balance ? { kind: 'live', data: balance }
  : credential.balance ? { kind: 'cached', data: credential.balance }
  : { kind: 'none' }
```

渲染规则：

| 字段 | live | cached | none |
| --- | --- | --- | --- |
| 订阅等级 | `balance.subscriptionTitle` | `credential.subscriptionTitle`（持久化值） | `credential.subscriptionTitle` 有则显示，否则「未知」 |
| 剩余用量 | 正常显示 | 正常显示 + 尾随「缓存 N 分钟前」标记；`stale` 时标记转为警示色 | 「未查询」+ 指向单卡刷新的提示 |

要点：

- 订阅等级在 `cached` / `none` 两态下都优先用持久化的 `credential.subscriptionTitle`，因此**只要该凭据历史上成功查过一次 usage，首屏就有值**（线上即 10/10）。
- 「未知」的文案改为「未查询」，并在 `title` 中说明获取路径（单卡「刷新余额」或批量余额/订阅），把无信息的死路改成可操作的入口。
- 缓存标记必须可见，这是 `admin-ui-balance-ops`「披露缓存语义」要求在卡片层的自然延伸。
- `handleBatchVerify` 的成功判定 MUST 继续只依据其 force 请求的返回，MUST NOT 读 `credential.balance`，否则会违反「TTL 内失效的凭据不得报成功」。

**可选增强（Step 4，标记为可选）**：启动后台预热余额缓存，比照 `spawn_warmup_models(2)`（`src/main.rs:164`）的 Semaphore 限并发范式，让全新部署的首屏也有余额值。

不列入必须项的理由：会在启动时对每个启用凭据发起一次上游请求，与 `admin-ui-balance-ops`「定时刷新默认关闭、不得发出未经操作者授权的自动请求」的克制取向相冲突。若实施，MUST 满足：默认关闭、由配置显式开启、限并发、失败仅 log 不影响启动。建议先只做 Step 1-3，观察 Step 1-3 之后首屏还有多少「未查询」再决定。

补充：服务端分页后，「批量余额/订阅」与「定时刷新」作用于「当前页启用凭据」的既有语义**天然保持成立**，只是「当前页」从前端切片结果变成服务端返回结果。`admin-ui-balance-ops` 三条 Requirement 的文字无需修改。

### 6.5 P5：分页契约（借鉴 GitHub REST API）

GitHub REST API 的分页设计有三点值得直接照搬，因为它们解决的正是运维列表的真实痛点：

| GitHub 的做法 | 借鉴理由 |
| --- | --- |
| `page` / `per_page` 查询参数，`per_page` 有服务端上限（GitHub 为 100） | 参数语义直观、可手写 curl；上限防止客户端要求全量而使分页形同虚设 |
| `Link` 响应头（RFC 8288）携带 `first` / `prev` / `next` / `last` | 脚本与 CLI 可以「跟着链接走」而不必自己算页码；越界与末页判断由服务端负责 |
| 超出末页返回**空数组**而非 4xx | 分页是导航行为不是错误；数据在两次请求间变少时客户端不该看到报错 |

不照搬的一点：GitHub 的 `Link` 头**不含总数**（大集合下 count 昂贵）。本方案的集合是内存中的 `Vec`，`len()` 是 O(1)，所以额外在响应体给出 `filteredTotal` 与 `totalPages`，让 UI 能渲染完整页码条（6.6）。这是在 GitHub 契约之上加强，不是偏离。

**响应契约**（`src/admin/types.rs`）：

```rust
/// 分页元信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// 实际生效的页码（1-based；越界请求已被 clamp）
    pub page: u32,
    /// 实际生效的每页条数
    pub per_page: u32,
    /// 筛选后的总条数（分页前基数）
    pub filtered_total: u32,
    /// 总页数（filtered_total == 0 时为 1）
    pub total_pages: u32,
    pub has_prev: bool,
    pub has_next: bool,
}
```

`CredentialsStatusResponse` 增加 `pub page_info: PageInfo;`（非 `Option`，始终存在）。`total` / `available` / `currentId` / `credentials` 四个既有字段语义不变，`credentials` 变为当前页切片。

分页语义（写入 spec）：

- `page` 缺省 1；`per_page` 缺省 **12**（与现有 `itemsPerPage` 一致，保证行为连续），上限 **100**，允许值集合建议 `[12, 24, 48, 100]` 但不强制枚举校验。
- `page = 0` 或负值（解析失败）→ clamp 为 1；`per_page = 0` → clamp 为默认值；`per_page > 100` → clamp 为 100。**clamp 而非报错**，且 `pageInfo` 回报 clamp 后的实际值，前端据此纠正自身状态。
- `page > total_pages` → 返回**空 `credentials` 数组**，`pageInfo.page` 回报请求的页码（不 clamp），`hasNext = false`。这与 GitHub 一致，让「数据变少导致停留在越界页」的场景可被前端识别并自动回退。
- `filtered_total == 0` 时 `total_pages = 1`、`page = 1`、`hasPrev = hasNext = false`。
- 排序仍为 `priority` 升序（分页的前提是稳定全序）。**`priority` 可重复，因此必须加 `id` 作为次级键**：`sort_by_key(|c| (c.priority, c.id))`。否则同 priority 的凭据在两次请求间顺序可能不同，导致翻页时出现重复或漏项。这是原有 `sort_by_key(|c| c.priority)` 遗留的隐患，分页把它放大了。

**`Link` 响应头**（保留查询参数，仅替换 `page`）：

```tex
Link: </api/admin/credentials?disabled=false&page=1&perPage=12>; rel="first",
      </api/admin/credentials?disabled=false&page=2&perPage=12>; rel="prev",
      </api/admin/credentials?disabled=false&page=4&perPage=12>; rel="next",
      </api/admin/credentials?disabled=false&page=9&perPage=12>; rel="last"
```

- `prev` 仅当 `hasPrev` 时出现；`next` 仅当 `hasNext` 时出现；`first` / `last` 恒出现（除 `totalPages == 1` 时整个头省略，与 GitHub「单页则省略 Link 头」一致）。
- 所有 URL MUST 对查询值做百分号编码（email 模糊词可能含 `@`、`+`、空格）。
- handler 返回 `(HeaderMap, Json<T>)` 以同时写头与体；`Link` 头的值由 `pageInfo` 渲染，两者不可能不一致。

**实现**（`src/admin/service.rs`）：

```rust
const DEFAULT_PER_PAGE: u32 = 12;
const MAX_PER_PAGE: u32 = 100;

pub fn get_all_credentials(&self) -> CredentialsStatusResponse {
    self.query_credentials(&CredentialsQuery::default())
}

pub fn query_credentials(&self, q: &CredentialsQuery) -> CredentialsStatusResponse {
    // 1. 构造全量 item（含 subscription_title 与 balance 快照）
    // 2. filter:   items.retain(|c| Self::matches_query(c, q));
    // 3. sort:     items.sort_by_key(|c| (c.priority, c.id));
    // 4. paginate: 计算 PageInfo 后 items = items.into_iter().skip(offset).take(per_page).collect();
}
```

注意 `get_all_credentials()` 的薄封装含义变了：`CredentialsQuery::default()` 的 `page/per_page` 为 `None` ⇒ 走默认分页 ⇒ **只返回前 12 条**。两个既有单测（`service.rs:1965/1975`）若断言全量长度会失败，需改为显式传 `per_page: Some(100)` 或直接断言首页内容。这是本次修订唯一的破坏性内部改动，必须在 tasks 中显式列出。

**向后兼容评估（重要）**：这是一个**破坏性 API 变更**。旧客户端调用 `GET /api/admin/credentials` 会从「拿到全部凭据」变成「拿到前 12 条」。处理方式：

- 本仓库 Admin UI 与后端同版本发布（rust-embed 嵌入静态资源），前后端天然同步，无版本错配窗口。
- 外部脚本消费者需跟随 `Link` 头或显式传 `perPage`。MUST 在 change 的 proposal 中标注为 breaking，并在 README / 版本说明中提及。
- 已评估但**不采用**的折中：「不带 `page` 参数时返回全量」。理由是它让接口有两种互斥的响应形态，`pageInfo` 语义变得可选，且默认路径（30s 轮询）仍然全量传输，恰好绕开了 P5 想解决的问题。宁可一次把契约做对。

### 6.6 P5：分页 UI（借鉴 Primer Pagination）

Primer（GitHub 设计系统）的 Pagination 用两个参数刻画页码条形态：`marginPageCount`（首尾各保留几页）与 `surroundingPageCount`（当前页左右各保留几页），中间用省略号折叠；窄视口下自动缩减。这套模型比「上一页/下一页」信息量高得多，且实现是一个纯函数，适合单测。

**页码序列生成**（纯函数，`admin-ui/src/lib/pagination.ts`）：

```ts
export type PageItem = number | 'ellipsis'

/**
 * 生成 Primer 风格页码序列。
 * marginPageCount: 首尾各固定显示的页数
 * surroundingPageCount: 当前页左右各显示的页数
 */
export function buildPageItems(
  current: number,
  totalPages: number,
  marginPageCount = 1,
  surroundingPageCount = 2,
): PageItem[] {
  const pages = new Set<number>()
  for (let i = 1; i <= Math.min(marginPageCount, totalPages); i++) pages.add(i)
  for (let i = Math.max(1, totalPages - marginPageCount + 1); i <= totalPages; i++) pages.add(i)
  for (
    let i = Math.max(1, current - surroundingPageCount);
    i <= Math.min(totalPages, current + surroundingPageCount);
    i++
  ) pages.add(i)

  const sorted = [...pages].sort((a, b) => a - b)
  const items: PageItem[] = []
  let prev = 0
  for (const p of sorted) {
    // 仅当间隔 >1 时插省略号；间隔恰为 2 时直接补出中间页（省略号占位比页码还宽）
    if (prev && p - prev === 2) items.push(prev + 1)
    else if (prev && p - prev > 2) items.push('ellipsis')
    items.push(p)
    prev = p
  }
  return items
}
```

「间隔恰为 2 时补出中间页而非省略号」是 Primer 的细节：用一个省略号替换一个页码没有节省空间，反而丢了一个可点击目标。

渲染形态（`totalPages = 9`、`current = 5`）：

```tex
[上一页]  1  …  3  4  [5]  6  7  …  9   [下一页]     每页 [12 ▾]   第 5 / 9 页 · 共 103 条
```

窄视口（`sm` 以下）：`marginPageCount = 1`、`surroundingPageCount = 0`，退化为 `1 … 5 … 9`，并隐藏「每页」选择器。

**无障碍**（照 Primer 的可访问性要求）：

- 容器用 `<nav aria-label="凭据列表分页">`，内部为 `<ul>/<li>`。
- 每个页码是 `<button>`（本方案无路由，不用 `<a>`），带 `aria-label="第 3 页"`；当前页额外 `aria-current="page"`。
- 上一页/下一页 `aria-label` 为「上一页」「下一页」，禁用时用 `disabled` 而非移除，保持焦点顺序稳定。
- 省略号用 `<li aria-hidden="true">`（非交互，不进 tab 序）。
- 分页导航后向 `aria-live="polite"` 区域播报「第 5 页，共 9 页」，让屏幕阅读器用户知道内容已更新。

**每页条数选择器**：复用现有 `admin-ui/src/components/ui/select.tsx`，选项 `[12, 24, 48, 100]`。改变时 `page` 重置为 1，并将选择持久化到 `localStorage`（复用 `admin-ui/src/lib/storage.ts` 的模式新增 `getPerPage`/`setPerPage`）。每页条数是稳定的个人偏好，值得记住；页码与筛选条件是临时导航状态，不持久化。

**URL 同步**：项目未引入 `react-router`（已确认 `package.json` 无该依赖），不为分页新增路由库。用 `history.replaceState` 把 `page` 与筛选条件同步到查询串，初始化时从 `location.search` 读回：

```ts
// 挂载时读回，变更时 replaceState（不产生历史记录堆积，浏览器返回键不会逐页回退）
```

这样页码与筛选条件可复制分享、刷新后保持，而无需引入路由依赖。选用 `replaceState` 而非 `pushState` 的理由：运维翻页不应污染浏览器历史，否则「返回」需要按很多次才能离开页面。

**越界自纠**：若 `pageInfo.credentials` 为空且 `pageInfo.page > pageInfo.totalPages`（数据在两次轮询间变少），前端自动 `setPage(pageInfo.totalPages)` 并提示「原页码已越界，已跳转至末页」。这正是 6.5 中「越界返回空数组且回报请求页码」的设计目的。

---

## 7. 兼容性与风险

| 项 | 评估 |
| --- | --- |
| **分页是破坏性变更** | 旧客户端从「全量」变「前 12 条」。前后端同版本嵌入发布，UI 侧无窗口；外部脚本需跟 `Link` 头或显式传 `perPage`。MUST 在 proposal 标 breaking 并写入版本说明。已评估并否决「无 `page` 参数则返回全量」的折中（见 6.5） |
| 排序不稳定导致翻页重复/漏项 | `priority` 可重复，原 `sort_by_key(|c| c.priority)` 在分页下会出问题。修正为 `(priority, id)` 全序；以单测锁定 |
| 新增字段兼容 | `subscriptionTitle` / `balance` 为 `Option` + `skip_serializing_if`，旧解析器不受影响；`pageInfo` 为新增必选字段（属于上一条 breaking 的一部分） |
| 30s 轮询开销 | 新增开销为「一次 `balance_cache` 加锁克隆」+ 每条一次 `HashMap::get`，无 I/O、无上游请求；分页后响应体从 O(n) 降为 O(perPage)，净收益为负开销 |
| 锁竞争 | `balance_cache` 用 `parking_lot::Mutex`，只在克隆瞬间持有；MUST NOT 在持锁期间构造 item 或调用 token_manager |
| 缓存被误读为实时值 | 由 `cachedAt` / `ageSecs` / `stale` 三字段 + UI 标记规避；这是本方案最需要评审关注的点 |
| 验活语义污染 | 明确禁止验活路径读取 `credential.balance`；以单测锁定 |
| 徽章误隐藏 | `defaultEndpoint` 获取失败时退化为恒渲染（当前行为），不会隐藏真实差异 |
| 批量操作作用域 | 服务端分页后「全选」必须收紧为当前页；跨页已选 id 需在 UI 明示计数。这是分页引入的最主要误操作风险，需以 Scenario 固定 |
| 三个基数混用 | `total` / `filteredTotal` / 当前页条数各有语义；单测断言「筛选不改变 total/available/currentId」 |
| 页码越界 | 数据在轮询间变少时前端自动回退末页并提示；依赖 6.5 的「越界返回空数组 + 回报请求页码」 |
| facets 接口的必要性 | 若不做 facets，Select 选项只能从当前页去重（服务端分页后不完整）。这是分页带来的连带改动，不可省略 |
| react-query 缓存碎片 | 查询对象进 `queryKey` 后每种组合一个条目；文本输入必须防抖 300ms，并用 `placeholderData: (prev) => prev` 避免翻页闪空 |

---

## 8. 验证计划

### 8.1 后端单测（`src/admin/service.rs` tests）

- `credentials_status_includes_subscription_title`：entry 有 `subscription_title` 时列表带出该值
- `credentials_status_subscription_title_absent_when_never_queried`：从未查过时字段缺省
- `credentials_status_includes_cached_balance_snapshot`：缓存命中时 `balance` 存在且 `stale == false`
- `credentials_status_marks_stale_balance`：注入超 TTL 的 `cached_at`，断言 `stale == true` 且条目仍返回
- `credentials_status_omits_balance_when_no_cache`：无缓存时 `balance` 为 `None`
- `credentials_list_must_not_call_upstream`：构造一个会让上游调用失败的 manager，断言列表接口仍成功返回（证明无 I/O）
- 筛选语义逐条：`filter_by_disabled` / `filter_by_auth_method_case_insensitive` / `filter_by_priority_range` / `filter_by_priority_range_inverted_returns_empty` / `filter_by_email_substring_case_insensitive` / `filter_by_email_excludes_none_email` / `filter_by_has_profile_arn` / `filter_by_id_substring` / `filter_by_subscription_title_exact` / `filter_by_subscription_title_unknown_sentinel` / `filter_combined_is_and` / `filter_preserves_priority_id_sort` / `filter_does_not_affect_total_available_current`

分页语义：

- `pagination_defaults_to_page_1_per_page_12`
- `pagination_clamps_per_page_to_max_100`：`perPage=500` → `pageInfo.perPage == 100`
- `pagination_clamps_zero_and_invalid_page_to_1`
- `pagination_beyond_last_page_returns_empty_without_error`：断言 `credentials.is_empty()` 且 `hasNext == false` 且 `pageInfo.page` 回报请求页码
- `pagination_empty_result_reports_total_pages_1`
- `pagination_filtered_total_is_pre_pagination_count`：筛选命中 30 条、`perPage=12` → `filteredTotal == 30`、`totalPages == 3`、`credentials.len() == 12`
- `pagination_last_page_partial`：33 条 / 12 → 第 3 页 9 条，`hasNext == false`
- `pagination_stable_across_duplicate_priority`：构造多条同 `priority` 凭据，遍历所有页并断言 id 集合无重复、无遗漏（锁定 `(priority, id)` 全序）
- `pagination_order_is_filter_then_sort_then_paginate`：筛选条件命中的记录必须出现在页码计算中，而非被先分页后筛掉
- `link_header_omitted_when_single_page`
- `link_header_contains_first_last_and_conditional_prev_next`
- `link_header_preserves_and_encodes_filters`：email 含 `@` / `+` / 空格时百分号编码正确，且 `page` 被替换而非追加
- `link_header_matches_page_info`：`Link` 头中的 `page` 值与 `pageInfo` 一致（同源渲染的回归保护）

### 8.2 前端单测（vitest，`pnpm test`）

展示相关：

- `isProviderRedundant`：`idc` + `BuilderId` / `builder-id` / `IAM` → true；`social` + `Github` → false；任一为空 → false
- endpoint 徽章：`endpoint === defaultEndpoint` 不渲染；不等则渲染；`defaultEndpoint` 未知时渲染
- 标题：有 email / 仅 nickname / 仅 userId / 三者皆无 四种输入下的 `displayName` 与 `#id` 共存
- `BalanceView` 解析：live 优先于 cached；cached 优先于 none；`stale` 传递正确

`buildPageItems`（`admin-ui/src/lib/pagination.ts`）：

- `totalPages = 1` → `[1]`
- `totalPages = 5`、`current = 3`、`surrounding = 2` → `[1,2,3,4,5]`（无省略号）
- `totalPages = 9`、`current = 5` → `[1,'ellipsis',3,4,5,6,7,'ellipsis',9]`
- `totalPages = 9`、`current = 1` → 首部无省略号
- `totalPages = 9`、`current = 9` → 尾部无省略号
- 间隔恰为 2 时补出中间页而非省略号（如 `totalPages = 7`、`current = 4`、`surrounding = 1` → `[1,2,3,4,5,6,7]`，不出现 `[1,'ellipsis',3,4,5,'ellipsis',7]`）
- 窄视口参数 `margin = 1`、`surrounding = 0` → `[1,'ellipsis',5,'ellipsis',9]`
- 序列中页码严格递增、无重复、首尾必为 1 与 `totalPages`（属性测试）

分页交互：

- 改变筛选条件 → `page` 重置为 1
- 改变 `perPage` → `page` 重置为 1，且写入 localStorage
- 收到越界响应（空数组 + `page > totalPages`）→ 自动跳末页
- 查询串同步：`page`/筛选写入 `location.search`；从 `location.search` 初始化状态

无障碍：

- `<nav>` 有 `aria-label`；当前页 `aria-current="page"`；省略号 `aria-hidden="true"` 且不可聚焦；上一页/下一页在边界时 `disabled` 而非移除

### 8.3 命令

```bash
cargo check --release --all-targets    # 零新增告警（AGENTS.md 准绳）
cargo test
cd admin-ui && pnpm test && pnpm build
openspec validate --all
```

### 8.4 线上验收（172.20.66.24）

在该环境部署新版本后逐条核对（线上 10 个凭据，默认 `perPage = 12` ⇒ 单页，需临时用 `perPage=4` 验证多页行为）：

> 第 9、12、14 项针对 `Link` 响应头。该方案在 `optimize-admin-ui-credential-list` 评审时已否决（见 change 的 `design.md` 与 `specs/admin-credential-list-query/spec.md`），导航只由 `pageInfo` 提供。这三项按「筛选参数在翻页后保持不变」核对查询串即可，不再核对响应头。

1. 打开 Admin UI，**不点任何按钮**：卡片的订阅等级全部有值（对应线上 10/10 落盘）
2. 剩余用量显示缓存值并带「缓存 N 分钟前」标记（对应线上 10 条有效缓存）
3. 每张卡片同时可见 email（9 张）与 `#id`（10 张）
4. 徽章区不再同时出现 `IdC` 与 `BuilderId`；endpoint 徽章在等于默认端点时消失
5. Profile 筛选选「无 Profile ARN」应命中 10 条（对应线上 `profileArn` 0/10）
6. 订阅等级 / 类型 / 状态 / 优先级 / email / id 六类筛选各自生效，组合为 AND
7. 顶部「凭据总数 / 可用凭据 / 当前活跃」在筛选时保持全量口径不变，而筛选提示用 `filteredTotal`
8. `curl '.../credentials?perPage=4'` → 返回 4 条，`pageInfo` 为 `{page:1, perPage:4, filteredTotal:10, totalPages:3, hasPrev:false, hasNext:true}`
9. `curl -I '.../credentials?perPage=4&page=2'` → `Link` 头含 `first`/`prev`/`next`/`last` 四个 rel，`page` 值分别为 1/1/3/3
10. `curl '.../credentials?perPage=4&page=99'` → HTTP 200、`credentials` 为空数组、`hasNext:false`（不是 4xx）
11. `curl '.../credentials?perPage=500'` → `pageInfo.perPage == 100`
12. `curl '.../credentials?perPage=100'` → `Link` 头整体省略（单页）
13. UI 设 `perPage=4` 后：页码条呈现 `1 2 3`，跳第 3 页得 2 条；刷新页面后 `perPage` 保持为 4（localStorage），`page` 从查询串恢复
14. 带筛选翻页：`Link` 头与查询串中的筛选参数在翻页后保持不变
15. 键盘 Tab 可依次聚焦上一页/各页码/下一页，省略号被跳过；屏幕阅读器播报当前页码

---

## 9. OpenSpec 归属

本方案触及 Admin API 契约与凭据管理面，按 `AGENTS.md` **实现前必须建 change**：

对应 change：`openspec/changes/optimize-admin-ui-credential-list/`

| 文件 | 内容 |
| --- | --- |
| `proposal.md` | 五点问题 + 本文档结论摘要；**必须标注分页为 breaking change** |
| `design.md` | 引用本文档，补充实现顺序与两个评审要点（缓存披露语义、分页 breaking 影响面） |
| `tasks.md` | 见下 |
| `specs/` | 见受影响 capability |

受影响 capability：

- **新增** `admin-credential-list-query`：`CredentialsQuery` 的完整语义（AND、大小写、模糊范围、区间闭合、哨兵值）+ 分页契约（`page`/`perPage` 默认与上限、clamp 规则、越界返回空数组、`Link` 头组成、`(priority, id)` 稳定全序、`filter -> sort -> paginate` 顺序）+ 三个基数的语义边界 + 批量操作作用域收紧为当前页
- **修改** `admin-ui-balance-ops`：新增 Requirement「列表内联余额为只读缓存快照」，明确 MUST 携带 `cachedAt`/`ageSecs`/`stale`、MUST NOT 触发上游请求、MUST NOT 参与验活判定；既有三条 Requirement 文字不变（「当前页」语义在服务端分页下自动成立，见 6.4 补充）
- **新增或修改** 卡片展示 capability（若无对应 capability 则在上述新 change 的 spec 中承载）：标题 email + id 共存、徽章信息增量渲染规则
- **新增** 分页 UI 无障碍要求（`nav aria-label`、`aria-current="page"`、省略号 `aria-hidden`、边界禁用而非移除、`aria-live` 播报）

实施顺序（每步独立可验证）：

```tex
阶段一：P4（可独立发布，非 breaking）
1. 快照导出 subscription_title              -> verify: cargo test 新增快照单测通过
2. 列表契约暴露 subscriptionTitle + balance   -> verify: 8.1 前 6 条单测通过，且列表无 I/O
3. 前端三态 BalanceView + 文案                -> verify: pnpm test 视图模型用例通过，首屏有值

阶段二：P1 / P2（纯前端，非 breaking）
4. 标题 email + #id                          -> verify: pnpm test 标题用例通过
5. 徽章收敛 + defaultEndpoint 透传            -> verify: pnpm test 徽章用例通过

阶段三：P3 / P5（breaking，需一起发布）
6. 修正排序为 (priority, id) 全序             -> verify: pagination_stable_across_duplicate_priority
7. 后端 CredentialsQuery 筛选部分              -> verify: 8.1 筛选用例全通过
8. 后端分页 + PageInfo + Link 头               -> verify: 8.1 分页与 Link 用例全通过
9. GET /credentials/facets                    -> verify: 返回全集去重值，且无 I/O
10. buildPageItems 纯函数                     -> verify: pnpm test 页码序列用例通过
11. 前端筛选栏 + 分页组件 + 查询串同步          -> verify: pnpm test 交互与无障碍用例通过
12. 批量操作作用域收紧 + 跨页已选计数提示        -> verify: 手工 + 用例
13. 全量校验                                   -> verify: 8.3 四条命令 + 8.4 线上十五条
```

阶段划分的意义：**阶段一、二不含破坏性变更，可先行合并**，立即解决用户感知最强的 P4 与两个展示问题；阶段三是一次原子的契约变更，必须整体发布并同步版本说明。第 6 步（排序全序化）虽属阶段三，但它是分页正确性的前提，MUST 在第 8 步之前落地。

---

## 10. 源码索引

### 后端

| 位置 | 作用 |
| --- | --- |
| `src/admin/types.rs:12-21` | `CredentialsStatusResponse`（待扩 `pageInfo`） |
| `src/admin/types.rs:26-85` | `CredentialStatusItem`（待扩 `subscriptionTitle` / `balance`） |
| `src/admin/types.rs:117-121` | `BalanceQuery`（`CredentialsQuery` 的实现范式参考） |
| `src/admin/service.rs:100-154` | `get_all_credentials`（待改造为 `query_credentials` + 薄封装；`sort_by_key(priority)` 待全序化为 `(priority, id)`） |
| `src/admin/service.rs:1965/1975` | 两个既有列表单测（分页默认值会改变其断言，需同步） |
| `src/admin/service.rs:29-34` | `CachedBalance`（`cached_at` + `data`） |
| `src/admin/service.rs:267-304` | `get_balance`（TTL 缓存读写，列表接口 MUST NOT 调用） |
| `src/admin/service.rs:307-332` | `fetch_balance`（余额字段计算口径，快照需与其一致） |
| `src/admin/service.rs:979-1032` | 余额缓存加载/落盘（加载时丢弃超 TTL 条目） |
| `src/admin/handlers.rs:19-22` | `get_all_credentials` handler（待接 `Query`） |
| `src/admin/router.rs:30` | `GET /credentials` 路由 |
| `src/kiro/token_manager.rs:709-758` | `CredentialEntrySnapshot`（待扩 `subscription_title`） |
| `src/kiro/token_manager.rs:1987-2054` | `snapshot()`（`auth_method` 归一化现场：builder-id/iam → idc） |
| `src/kiro/token_manager.rs:2126-2250` | `get_usage_limits_for`（成功后写回 subscription_title 并 persist） |
| `src/kiro/token_manager.rs:2686` | `spawn_warmup_models`（Step 4 可选预热的范式） |
| `src/kiro/model/credentials.rs:96` | `KiroCredentials.subscription_title` |
| `src/kiro/model/usage_limits.rs` | `subscription_title()` / `usage_limit()` / `current_usage()` |
| `src/main.rs:164` | `spawn_warmup_models(2)` 调用点 |
| `src/main.rs:210` | `AdminService::new_with_runtime` 实例化点 |

### 前端

| 位置 | 作用 |
| --- | --- |
| `admin-ui/src/types/api.ts:10` | `CredentialStatusItem`（待同步扩字段） |
| `admin-ui/src/types/api.ts:181` | `EndpointSettings.defaultEndpoint`（徽章收敛数据源） |
| `admin-ui/src/api/credentials.ts:35` | `getCredentials` |
| `admin-ui/src/hooks/use-credentials.ts:17` | `useCredentials`（30s `refetchInterval`） |
| `admin-ui/src/components/credential-card.tsx:211-216` | 标题二选一（P1 现场） |
| `admin-ui/src/components/credential-card.tsx:237-262` | 三枚徽章（P2 现场） |
| `admin-ui/src/components/credential-card.tsx:332-339` | 订阅等级（P4 现场） |
| `admin-ui/src/components/credential-card.tsx:354-369` | 剩余用量（P4 现场） |
| `admin-ui/src/components/dashboard.tsx:48` | `balanceMap`（余额唯一数据源） |
| `admin-ui/src/components/dashboard.tsx:65-66` | `currentPage` / `itemsPerPage = 12` 硬编码（P5 现场） |
| `admin-ui/src/components/dashboard.tsx:82-85` | `slice` 切片的假分页（P5 现场，同时是 P3 筛选插入点） |
| `admin-ui/src/components/dashboard.tsx:142` | `applyBalance` |
| `admin-ui/src/components/dashboard.tsx:376` | `handleQueryCurrentPageInfo`（允许命中缓存） |
| `admin-ui/src/components/dashboard.tsx:896-922` | 现有最小分页控件（上一页 / 页码文本 / 下一页，待替换为 Primer 风格页码条） |
| `admin-ui/src/lib/storage.ts` | 仅 `getApiKey`/`setApiKey`/`removeApiKey`，是 `getPerPage`/`setPerPage` 的模式参考 |
| `admin-ui/src/lib/pagination.ts` | **新增**：`buildPageItems` 纯函数（省略号折叠算法） |
| `admin-ui/package.json` | 无 `react-router` 依赖的事实依据（故 URL 同步走 `history.replaceState`） |

### 相关规范与文档

- `openspec/specs/admin-ui-balance-ops/spec.md` — 本方案 MUST 兼容的三条既有 Requirement
- `AGENTS.md` — OpenSpec 前置流程、零新增告警准绳、凭据文件禁提交
- `docs/admin-models-settings-optimization-design.md` — 同类设计文档范式
- `docs/builderid-403-profile-arn-analysis-and-optimization-design.md` — 线上 `profileArn` 0/10 的语境

### 分页设计调研来源

- [GitHub REST API — Using pagination in the REST API](https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api) — `page`/`per_page` 契约、`per_page` 上限 100、`Link` 响应头的四个 `rel`、单页时省略 `Link`、越界返回空数组而非 4xx
- [RFC 8288 — Web Linking](https://www.rfc-editor.org/rfc/rfc8288) — `Link` 头的语法与 `rel` 语义规范
- [Primer — Pagination](https://primer.style/components/pagination) — `marginPageCount` / `surroundingPageCount` 的页码折叠模型、窄视口自动缩减
- [Primer — Pagination accessibility](https://primer.style/components/pagination/accessibility) — `nav` 容器标签、`aria-label="Page N"`、`aria-current="page"`、省略号不可聚焦

---

## 11. 结论

五点需求落在同一条数据链上，所以适合做成一次「契约打通 + 展示收敛 + 分页下沉」，而非五个各自为政的补丁。

P4 成本最低，收益最直接。订阅等级已持久化（线上 10/10），余额已缓存（线上 10 条有效），显示「未知」的原因只是列表契约没给出暴露路径。加两个可选字段、快照里多搬一个字段，首屏就从零信息变成满信息，上游请求数不变。

P3 得排在 P4 后面。按订阅等级查询的前提是订阅等级先出现在列表里。

P1 / P2 是纯前端的信息密度调整，后端零成本，只需要 `defaultEndpoint` 一次额外查询来支撑徽章收敛。

P5 是全案唯一的破坏性变更，而且和 P3 绑死。服务端分页后客户端只持有当前页，本地筛选算不出筛选全集的基数，所以筛选没法保留前后端双实现，必须单一下沉到服务端，`filter -> sort -> paginate` 的顺序也不可交换。响应形态从「全量数组」变成「一页 + `pageInfo`」，`get_all_credentials()` 的两个既有单测断言要跟着改。

`(priority, id)` 全序化是分页能否正确工作的前提。现有 `sort_by_key(|c| c.priority)` 在 priority 重复时不构成全序，翻页会重复或漏记录。假分页把这个缺陷掩盖了，切到服务端分页就会暴露。

评审时最该盯住的是缓存披露语义。内联余额必须自带 `cachedAt` / `ageSecs` / `stale` 并在 UI 显式标记，且绝不参与验活判定，否则 `admin-ui-balance-ops` 已确立的安全边界会被破坏。

实施按 9 节的三阶段 13 步走。阶段一（契约打通，解决 P4）与阶段二（展示收敛，解决 P1/P2）都不含破坏性变更，可以合成一个先行 PR 交付。阶段三（服务端筛选与分页，解决 P3/P5）是一次原子契约变更，须整体发布，并在版本说明里标注 `GET /credentials` 响应形态的变化。
