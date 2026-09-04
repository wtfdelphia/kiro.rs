# Capability: admin-credential-list-query

## Purpose

定义 Admin 凭据列表接口的检索契约：运维者可以按订阅等级、禁用状态、认证类型、优先级区间、email、profile 状态、id 组合筛选凭据，并以稳定的分页顺序逐页取回，不必把全量凭据拉到客户端再过滤。分页导航信息只由响应体的 `pageInfo` 提供。

## Requirements

### Requirement: 凭据列表支持组合筛选

`GET /api/admin/credentials` SHALL 接受下列查询参数，多个参数之间以 AND 组合。任一参数缺省时该维度不参与筛选。

| 参数 | 类型 | 匹配语义 |
| --- | --- | --- |
| `subscriptionTitle` | string | 精确匹配，大小写不敏感；哨兵值 `__unknown__` 匹配所有无订阅等级的凭据 |
| `disabled` | bool | 精确匹配禁用状态 |
| `authMethod` | string | 精确匹配，大小写不敏感，取值 `social` / `idc` / `external_idp` / `api_key` |
| `priorityMin` | u32 | 闭区间下界 |
| `priorityMax` | u32 | 闭区间上界 |
| `email` | string | 子串匹配，大小写不敏感 |
| `hasProfileArn` | bool | `true` 匹配已解析出 profile ARN 的凭据，`false` 匹配未解析的 |
| `id` | string | 对 id 的十进制字符串形式做子串匹配 |

`authMethod` 的取值集合 SHALL 与 `AuthMethod` 枚举的四个变体保持一致，`external_idp` 不得遗漏。该类型的凭据在 `provider` 语义上与 `idc` 有明确区别，无法被其他取值覆盖。

筛选 SHALL 只在服务端实现，响应中不包含被筛掉的凭据。客户端 MUST NOT 对返回结果再做一遍本地过滤。

#### Scenario: 单条件筛选禁用凭据

- **WHEN** 请求 `GET /api/admin/credentials?disabled=true`
- **THEN** 响应的 `credentials` 数组只包含 `disabled` 为 `true` 的凭据

#### Scenario: 多条件 AND 组合

- **WHEN** 请求 `GET /api/admin/credentials?authMethod=idc&disabled=false&priorityMin=1&priorityMax=5`
- **THEN** 响应只包含同时满足「类型为 idc」「未禁用」「优先级落在 1 到 5 闭区间内」的凭据

#### Scenario: email 子串大小写不敏感

- **WHEN** 某凭据 email 为 `Alice@Example.com`，请求 `GET /api/admin/credentials?email=alice@ex`
- **THEN** 该凭据出现在响应中

#### Scenario: 订阅等级哨兵值匹配未知

- **WHEN** 请求 `GET /api/admin/credentials?subscriptionTitle=__unknown__`
- **THEN** 响应只包含未落盘订阅等级的凭据，且不包含任何已有订阅等级的凭据

#### Scenario: 优先级区间只给单侧

- **WHEN** 请求 `GET /api/admin/credentials?priorityMin=3`
- **THEN** 响应包含所有优先级大于等于 3 的凭据，上界不受限制

#### Scenario: id 子串匹配

- **WHEN** 存在 id 为 `12` 与 `120` 的凭据，请求 `GET /api/admin/credentials?id=12`
- **THEN** 两条凭据都出现在响应中

#### Scenario: 筛选 external_idp 类型

- **WHEN** 系统同时存在 `idc` 与 `external_idp` 凭据，请求 `GET /api/admin/credentials?authMethod=external_idp`
- **THEN** 响应只包含 `external_idp` 凭据，不包含任何 `idc` 凭据

#### Scenario: 无匹配结果

- **WHEN** 筛选条件不匹配任何凭据
- **THEN** 响应状态码为 200，`credentials` 为空数组，`pageInfo.filteredTotal` 为 0

### Requirement: 凭据列表服务端分页

`GET /api/admin/credentials` SHALL 返回一页凭据而非全量数组，并在响应体中携带 `pageInfo`。

- `page` 缺省为 1，`perPage` 缺省为 12，`perPage` 上限为 100
- `page` 与 `perPage` SHALL 以 `i64` 反序列化，使负值能进入 clamp 逻辑而非在参数提取阶段被拒
- 超出取值范围的 `perPage` SHALL 被 clamp 到合法区间，MUST NOT 返回 4xx
- `page` 小于 1（含 0 与负值）时 SHALL 被 clamp 为 1
- `page` 超过最后一页时 SHALL 返回空数组，MUST NOT 返回 4xx
- 参数值无法解析为整数时（例如 `page=abc`）返回 400 是可接受的，clamp 只承诺覆盖「类型正确但取值越界」
- `pageInfo` SHALL 包含 `page`、`perPage`、`filteredTotal`、`totalPages`、`hasPrev`、`hasNext`
- `pageInfo.page` 在 `page` 小于 1 时 SHALL 回显 clamp 后的 1；`page` 越过末页时 clamp 不生效，SHALL 原样回显请求值，客户端据 `totalPages` 纠正自身状态
- `page` 极大值（`(page - 1) * perPage` 溢出 `i64`）SHALL 与普通越界页同样返回空数组，MUST NOT 因整数环绕返回有效数据

处理顺序 SHALL 固定为「筛选 → 排序 → 切页」。这个顺序不可交换：先切页再筛选会让每页条数不确定，先切页再排序会让页边界跟着内容漂移。

#### Scenario: 默认分页

- **WHEN** 请求 `GET /api/admin/credentials` 且系统有 30 条凭据
- **THEN** 响应包含前 12 条凭据，`pageInfo` 为 `page=1`、`perPage=12`、`filteredTotal=30`、`totalPages=3`、`hasPrev=false`、`hasNext=true`

#### Scenario: 请求末页

- **WHEN** 系统有 30 条凭据，请求 `GET /api/admin/credentials?page=3&perPage=12`
- **THEN** 响应包含第 25 至 30 条共 6 条凭据，`hasPrev=true`、`hasNext=false`

#### Scenario: perPage 超上限被 clamp

- **WHEN** 请求 `GET /api/admin/credentials?perPage=500`
- **THEN** 响应状态码为 200，`pageInfo.perPage` 为 100，返回不超过 100 条凭据

#### Scenario: perPage 为 0 被 clamp

- **WHEN** 请求 `GET /api/admin/credentials?perPage=0`
- **THEN** 响应状态码为 200，`pageInfo.perPage` 为默认值 12

#### Scenario: 页码越界返回空数组

- **WHEN** 系统有 30 条凭据，请求 `GET /api/admin/credentials?page=99&perPage=12`
- **THEN** 响应状态码为 200，`credentials` 为空数组，`pageInfo.page` 为 99，`totalPages` 为 3，`hasPrev=true`、`hasNext=false`

#### Scenario: page 极大值不因整数溢出返回数据

- **WHEN** 系统有 30 条凭据，请求 `page` 取 `i64::MAX` 或 `2^62 + 1`（两者的 `(page - 1) * perPage` 都溢出 `i64`，后者的乘积对 `2^64` 取模后为 0）
- **THEN** 响应状态码为 200，`credentials` 为空数组，`pageInfo.page` 原样回显请求值，`totalPages` 为 3，`hasNext=false`，且服务端不 panic

#### Scenario: page 为负值被 clamp

- **WHEN** 请求 `GET /api/admin/credentials?page=-1`
- **THEN** 响应状态码为 200，`pageInfo.page` 为 1，返回第一页内容

#### Scenario: 筛选与分页同时生效

- **WHEN** 40 条凭据中有 25 条未禁用，请求 `GET /api/admin/credentials?disabled=false&page=2&perPage=10`
- **THEN** 响应包含未禁用凭据中的第 11 至 20 条，`pageInfo.filteredTotal` 为 25，`totalPages` 为 3

#### Scenario: 零凭据

- **WHEN** 系统无任何凭据，请求 `GET /api/admin/credentials`
- **THEN** 响应状态码为 200，`credentials` 为空数组，`pageInfo.totalPages` 为 0，`hasPrev` 与 `hasNext` 均为 `false`

### Requirement: 分页排序必须构成全序

凭据列表 SHALL 按 `(priority, id)` 升序排序。`priority` 允许重复，单独用它排序不构成全序，在分页下会导致同一条凭据跨页重复出现或整条漏掉。

#### Scenario: 优先级重复时翻页不重不漏

- **WHEN** 多条凭据具有相同 `priority`，依次请求全部页
- **THEN** 各页返回的 id 集合两两不相交，并集等于筛选后的全集

#### Scenario: 排序与请求次数无关

- **WHEN** 在凭据集合不变的情况下对同一页重复发起请求
- **THEN** 每次返回的凭据顺序完全一致

### Requirement: 分页导航由 pageInfo 单一提供

分页导航所需的全部信息 SHALL 只由响应体的 `pageInfo` 提供。`hasPrev`、`hasNext`、`totalPages` 三者足以推出上一页、下一页、首页、末页的页码，客户端只需改写 `page` 参数。

响应 MUST NOT 依赖 `Link` 响应头传递导航信息。放弃 `Link` 头是刻意取舍：本仓库唯一的调用方是同版本发布的内嵌 Admin UI，它按 `pageInfo` 导航；再写一份 RFC 8288 头意味着同一事实有两个来源，且要求 handler 返回 `(HeaderMap, Json<T>)`，两处一旦不一致就无从判断哪个为准。

#### Scenario: 中间页导航信息完备

- **WHEN** 请求 `GET /api/admin/credentials?disabled=false&page=3&perPage=12` 且共有 9 页
- **THEN** `pageInfo` 中 `page=3`、`totalPages=9`、`hasPrev=true`、`hasNext=true`，客户端据此得出上一页为 2、下一页为 4、末页为 9

#### Scenario: 首页无上一页

- **WHEN** 请求第 1 页且共有多页
- **THEN** `pageInfo.hasPrev` 为 `false`、`hasNext` 为 `true`

#### Scenario: 单页结果两侧均无导航

- **WHEN** 筛选后结果不足一页
- **THEN** `pageInfo.hasPrev` 与 `hasNext` 均为 `false`，`totalPages` 为 1

### Requirement: 列表响应的四个基数语义各不相同

`CredentialsStatusResponse` SHALL 同时提供四个基数，且各自语义不得混用。

- `total`：系统内凭据总数，不受筛选与分页影响
- `available`：系统内未禁用的凭据数，不受筛选与分页影响。口径只看禁用状态，不考虑额度是否耗尽
- `pageInfo.filteredTotal`：应用筛选后、切页前的条数
- `credentials.length`：当前页实际返回的条数

UI 中展示「共 N 条」时 SHALL 使用 `filteredTotal`；展示系统健康度时 SHALL 使用 `total` 与 `available`。需要全量已禁用数的场景 SHALL 用 `total - available`，二者都不受筛选与分页影响，无需额外请求或新增字段。

本次变更 MUST NOT 改动 `available` 的既有口径。把额度耗尽纳入该计数会改变系统健康度卡片的含义，属于独立议题。

#### Scenario: 筛选不改变 total 与 available

- **WHEN** 系统有 40 条凭据、25 条未禁用，请求 `GET /api/admin/credentials?authMethod=idc` 匹配到 8 条
- **THEN** 响应中 `total` 为 40、`available` 为 25、`pageInfo.filteredTotal` 为 8

#### Scenario: available 不受额度耗尽影响

- **WHEN** 某未禁用凭据的余额缓存显示额度已耗尽
- **THEN** 该凭据仍计入 `available`

### Requirement: 筛选可选值由 facets 端点提供

`GET /api/admin/credentials/facets` SHALL 返回全集去重后的筛选可选值，至少包含 `subscriptionTitles` 与 `authMethods`。

该端点 SHALL 是纯内存聚合，MUST NOT 发起任何上游请求或磁盘 I/O。返回值 SHALL 覆盖全量凭据，不受任何筛选参数影响。

#### Scenario: 返回全集去重值

- **WHEN** 系统凭据的订阅等级包含 `Pro`、`Pro`、`Free`，请求 `GET /api/admin/credentials/facets`
- **THEN** `subscriptionTitles` 包含 `Pro` 与 `Free` 各一次

#### Scenario: 当前页无某取值时仍出现在 facets

- **WHEN** 当前页凭据全部为 `idc` 类型，但系统中存在 `social` 与 `external_idp` 类型凭据
- **THEN** `authMethods` 同时包含 `idc`、`social` 与 `external_idp`

### Requirement: 分页不改变单个列表项的字段契约

服务端分页只减少 `credentials` 数组的长度，MUST NOT 改变数组内每一项的字段构成。其他 capability 对列表项字段的既有约束在分页后 SHALL 继续成立，作用域从「全部凭据」变为「当前页凭据」。

`admin-ui-model-ops` 要求列表项携带 `modelCount`，该约束在分页后对当前页每一项照旧生效。判断「系统中是否有凭据缺少模型缓存」不能再靠遍历一次列表响应，需要逐页取回或改用筛选条件。

#### Scenario: 当前页每一项都带 modelCount

- **WHEN** 请求任意一页凭据列表
- **THEN** 该页每一项都包含 `modelCount` 字段，语义与分页前一致

### Requirement: 凭据列表接口不得发起上游请求

`GET /api/admin/credentials` 与 `GET /api/admin/credentials/facets` SHALL 是同步的纯内存读取。两者 MUST NOT 调用余额查询、Token 刷新或任何上游 HTTP 接口，也 MUST NOT 写入磁盘。

响应中的订阅等级与余额信息 SHALL 只来自已有的进程内快照与缓存。

#### Scenario: 列表请求不触发上游调用

- **WHEN** 连续请求凭据列表
- **THEN** 不产生任何上游余额查询或 Token 刷新请求，响应时间不随凭据数量出现网络级抖动

### Requirement: 批量操作作用域限定为当前页

凭据列表 SHALL 提供「全选」控件。全选 SHALL 只作用于当前页返回的凭据，MUST NOT 隐式扩展到筛选后的全集。控件 SHALL 呈现未选 / 部分选 / 全选三态，当前页已全选时再次点击 SHALL 只取消本页选择，不影响其他页已选项。

已选凭据可以跨越多页。UI SHALL 明示已选总数，使操作者能区分「已选 5 条」与「当前页 12 条」。

选中状态 SHALL 保存每条凭据做出筛选判断所需的属性，MUST NOT 只保存 id 再回列表响应中反查。分页后列表响应只含当前页，靠 id 反查其他页的凭据会得到空值；空值在布尔判断中退化为「不满足条件」，导致该凭据被静默剔除并计入「已跳过」，操作者看到的计数与实际执行的范围不符。

#### Scenario: 全选只覆盖当前页

- **WHEN** 筛选后共 30 条、当前页显示 12 条，操作者点击「全选」
- **THEN** 被选中的凭据为当前页 12 条，不包含其余 18 条

#### Scenario: 跨页已选计数可见

- **WHEN** 操作者在第 1 页选中 3 条后翻到第 2 页再选 2 条
- **THEN** UI 显示已选 5 条，且批量操作作用于这 5 条

#### Scenario: 翻页不清空已选

- **WHEN** 操作者在第 1 页全选 12 条后翻到第 2 页
- **THEN** 那 12 条仍在选中集合中，全选控件在第 2 页呈现未选态

#### Scenario: 取消全选只影响当前页

- **WHEN** 操作者在第 1 页选中 3 条后翻到第 2 页全选，再次点击第 2 页的全选控件
- **THEN** 第 2 页的选择被取消，第 1 页的 3 条仍在选中集合中

#### Scenario: 跨页已选凭据的属性判断正确

- **WHEN** 操作者在第 1 页选中 2 条已禁用凭据，翻到第 2 页后发起批量删除
- **THEN** 这 2 条凭据被正确识别为已禁用并执行删除，确认框中的计数为 2，MUST NOT 计入「已跳过」

#### Scenario: 已选凭据被他处删除

- **WHEN** 某已选凭据在轮询刷新后已不存在于系统中，操作者发起批量操作
- **THEN** 该凭据从选中集合移除并在结果汇总中如实计为失败或跳过，MUST NOT 与「跨页未加载」混为一谈
