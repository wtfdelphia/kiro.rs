# Capability: admin-ui-credential-list-view

## Purpose

定义 Admin UI 凭据卡片与列表的展示契约：一眼能同时看清凭据的身份与内部 id、徽章只承载有增量的信息、订阅等级与用量按数据来源区分新鲜度、分页控件在页数很多时的折叠形态与键盘、读屏可用性，以及批量操作的措辞与其实际作用范围保持一致。

## Requirements

### Requirement: 卡片标题同时展示身份名与 id

凭据卡片标题 SHALL 同时展示身份名与内部 id，不再二选一。id SHALL 以 `#<id>` 形式呈现并可被文本选中，便于与日志、API 参数对照。

身份名 SHALL 按下列优先级取第一个非空值：`email` → `nickname` → `userId`。三者均为空时 SHALL 显示「未获取身份」。

身份名过长时 SHALL 截断并保留完整值于 `title` 属性，`#<id>` MUST NOT 被截断。

#### Scenario: email 与 id 共存

- **WHEN** 凭据 id 为 7、email 为 `alice@example.com`
- **THEN** 卡片标题显示 `alice@example.com` 与 `#7`

#### Scenario: email 缺失时回落 nickname

- **WHEN** 凭据无 email 但 `nickname` 为 `alice`
- **THEN** 标题显示 `alice` 与 `#<id>`

#### Scenario: 身份信息全缺

- **WHEN** 凭据的 email、nickname、userId 均为空
- **THEN** 标题显示「未获取身份」与 `#<id>`

#### Scenario: 长身份名截断不影响 id

- **WHEN** 身份名长度超出可用宽度
- **THEN** 身份名被截断且 `title` 属性保留完整值，`#<id>` 完整可见

### Requirement: 徽章按信息增量渲染

凭据卡片的徽章区 SHALL 只渲染带来新信息的徽章，MUST NOT 无条件并列同源信息的多种表达。

- 当 `provider` 的取值可由 `authMethod` 推导时（例如 `authMethod=idc` 对应 `builderid` / `builder-id` / `iam` / `awsbuilderid`），SHALL 省略 provider 徽章
- `external_idp` 凭据的 `provider` 承载真实的 IdP 信息，MUST NOT 被判定为可推导
- 当 `endpoint` 等于当前生效的默认 endpoint 时 SHALL 省略 endpoint 徽章；不等时 SHALL 渲染，以提示该凭据走了非默认端点
- 默认 endpoint 取值 SHALL 来自 `GET /api/admin/settings/endpoint`；该请求失败时 SHALL 退化为渲染 endpoint 徽章，MUST NOT 因取值缺失而误隐藏真实差异

`authMethod` 徽章 SHALL 始终渲染，它是凭据类型的唯一权威来源。其文案 SHALL 覆盖 `social` / `idc` / `external_idp` / `api_key` 四个取值，MUST NOT 让其中任一取值落到裸值兜底。

当前仅注册了一个上游端点，默认 endpoint 恒等于它，且列表接口已把缺省值兜底为默认端点。因此「不等时渲染」这一分支在现状下不会触发。保留该分支是为将来注册多端点时无需再改判断逻辑，不是当前可观察行为。

#### Scenario: idc 凭据省略同源 provider

- **WHEN** 凭据 `authMethod=idc`、`provider=BuilderID`
- **THEN** 只渲染类型徽章，不渲染 provider 徽章

#### Scenario: provider 带来新信息时保留

- **WHEN** 凭据 `authMethod=social`、`provider=Google`
- **THEN** 类型徽章与 provider 徽章都渲染

#### Scenario: external_idp 的 provider 始终保留

- **WHEN** 凭据 `authMethod=external_idp`、`provider=Azure`
- **THEN** 类型徽章与 provider 徽章都渲染

#### Scenario: 四个类型取值都有文案

- **WHEN** 凭据的 `authMethod` 分别为 `social`、`idc`、`external_idp`、`api_key`
- **THEN** 四者各自渲染为可读文案，无一落到裸值兜底

#### Scenario: 默认端点下省略 endpoint 徽章

- **WHEN** 凭据的 endpoint 等于默认 endpoint（当前仅注册一个端点，所有凭据均属此情况）
- **THEN** 不渲染 endpoint 徽章

#### Scenario: 非默认 endpoint 显式提示

- **WHEN** 凭据的 endpoint 与默认 endpoint 不同
- **THEN** 渲染 endpoint 徽章

#### Scenario: 默认 endpoint 取值不可用时保守渲染

- **WHEN** `GET /api/admin/settings/endpoint` 请求失败
- **THEN** 所有凭据的 endpoint 徽章照常渲染

### Requirement: 订阅等级与用量的三态展示

订阅等级与剩余用量 SHALL 按数据来源区分三种状态，MUST NOT 把「缓存有值」与「无数据」都显示为「未知」。

- 实时：本次会话内查询所得，SHALL 直接展示数值
- 缓存：来自列表响应内联的余额快照，SHALL 展示数值并标注新鲜度（相对时间），`stale` 为 `true` 时 SHALL 视觉上弱化或加注过期提示
- 未查询：既无实时结果也无缓存快照，SHALL 显示「未查询」而非「未知」，并提供触发查询的入口

订阅等级有三个可能来源，取值优先级 SHALL 固定为：本次会话的实时查询结果 → 列表响应的顶层 `subscriptionTitle` → 内联余额快照中的订阅等级。三者均缺省时显示「未知等级」。

顶层 `subscriptionTitle` 优先于快照内的同名字段：前者是凭据落盘值，在额度查询成功且取值变化时回写；后者是余额缓存写入时的附带值，可能滞后于落盘值。两者不一致时以落盘值为准。

#### Scenario: 首屏展示缓存值

- **WHEN** 列表响应中某凭据带有 `balance` 快照且 `stale` 为 `false`
- **THEN** 卡片直接显示用量数值与「N 分钟前」形式的新鲜度标注，无需操作者点击查询

#### Scenario: 过期缓存显式标注

- **WHEN** 某凭据的 `balance.stale` 为 `true`
- **THEN** 数值仍然展示，同时给出过期提示

#### Scenario: 实时结果覆盖缓存

- **WHEN** 操作者对某凭据发起余额查询并成功返回
- **THEN** 卡片改为展示实时结果，不再显示缓存新鲜度标注

#### Scenario: 无任何数据时的措辞

- **WHEN** 某凭据既无实时结果也无 `balance` 快照
- **THEN** 显示「未查询」，MUST NOT 显示「未知」

#### Scenario: 订阅等级来自列表响应

- **WHEN** 列表响应中某凭据 `subscriptionTitle` 为 `Pro`
- **THEN** 卡片在首屏即显示 `Pro`，不需要额外请求

#### Scenario: 顶层订阅等级优先于快照内的值

- **WHEN** 某凭据顶层 `subscriptionTitle` 为 `Pro`，而内联余额快照中的订阅等级为 `Free`
- **THEN** 卡片显示 `Pro`

#### Scenario: 仅快照带订阅等级时回落

- **WHEN** 某凭据顶层 `subscriptionTitle` 缺省，但内联余额快照中带有订阅等级
- **THEN** 卡片显示快照中的取值，MUST NOT 显示「未知等级」

### Requirement: 筛选栏与列表查询状态

凭据管理页 SHALL 提供筛选控件，覆盖订阅等级、禁用状态、类型、优先级区间、email、profile 状态、id 七个维度，并把筛选条件下发到服务端。

- 文本类输入（email、id）SHALL 做输入防抖，避免逐字符发起请求
- 订阅等级与类型的可选值 SHALL 来自 `GET /api/admin/credentials/facets`，MUST NOT 从当前页数据去重生成
- 当前筛选与分页状态 SHALL 同步到 URL 查询串，使链接可分享、刷新后状态保留
- 修改任一筛选条件时 SHALL 重置到第 1 页
- 请求进行中 SHALL 保留上一次结果并给出加载指示，MUST NOT 闪回空列表

#### Scenario: 筛选条件下发到服务端

- **WHEN** 操作者选择类型为 `idc` 并勾选「仅未禁用」
- **THEN** 发出的请求携带 `authMethod=idc&disabled=false`，列表按服务端返回结果渲染

#### Scenario: 文本输入防抖

- **WHEN** 操作者在 email 输入框连续键入多个字符
- **THEN** 在输入停止后才发起一次请求

#### Scenario: 可选值覆盖全集

- **WHEN** 当前页凭据的订阅等级只有 `Pro`，而系统中还存在 `Free`
- **THEN** 订阅等级下拉框同时提供 `Pro` 与 `Free`

#### Scenario: 改变筛选回到第一页

- **WHEN** 操作者处于第 3 页时修改筛选条件
- **THEN** 请求的 `page` 为 1

#### Scenario: 状态同步到 URL

- **WHEN** 操作者设置筛选与页码后复制地址并在新标签打开
- **THEN** 页面恢复相同的筛选条件与页码

#### Scenario: 加载中保留上一次结果

- **WHEN** 翻页请求正在进行
- **THEN** 上一页内容保持可见并显示加载指示，列表 MUST NOT 先清空

### Requirement: 分页控件的页码折叠与无障碍

分页控件 SHALL 以页码条形式呈现，并在页数较多时折叠中间页码。

- 折叠规则 SHALL 由「首尾保留页数」与「当前页两侧保留页数」两个参数决定
- 折叠产生的间隔 SHALL 渲染为省略号；当被折叠的只有一页时 SHALL 直接渲染该页码而非省略号
- 省略号 MUST NOT 可聚焦，且对读屏隐藏
- 每个页码链接 SHALL 带 `aria-label`（形如 `Page 3`），当前页 SHALL 带 `aria-current="page"`
- 分页容器 SHALL 是带 `aria-label` 的 `nav` 元素
- 首页的「上一页」与末页的「下一页」SHALL 渲染为禁用态，MUST NOT 从 DOM 移除，避免控件位置跳动
- 页码变化 SHALL 通过 `aria-live` 区域播报

#### Scenario: 多页时折叠中间页码

- **WHEN** 共 20 页、当前第 10 页
- **THEN** 页码条包含首页、当前页两侧的页码、末页，其间以省略号占位

#### Scenario: 单页间隔不折叠

- **WHEN** 折叠计算得出被省略的只有第 2 页一页
- **THEN** 渲染页码 `2` 而非省略号

#### Scenario: 页数少时不出现省略号

- **WHEN** 总页数少于可完整展示的页码数
- **THEN** 全部页码依次渲染，无省略号

#### Scenario: 当前页可被读屏识别

- **WHEN** 当前处于第 4 页
- **THEN** 第 4 页元素带 `aria-current="page"`，其余页码带 `aria-label="Page N"`

#### Scenario: 边界按钮禁用而非移除

- **WHEN** 当前处于第 1 页
- **THEN** 「上一页」渲染为禁用态且不可点击，仍占据原有位置

#### Scenario: 键盘可完整操作

- **WHEN** 操作者仅用键盘 Tab 遍历分页控件
- **THEN** 所有页码与上下页按钮可聚焦并可用回车触发，省略号被跳过

### Requirement: 每页条数可选并持久化

分页控件 SHALL 提供每页条数选择器，取值不超过服务端上限 100。选中值 SHALL 持久化到浏览器本地存储，下次进入页面时沿用。

修改每页条数时 SHALL 重置到第 1 页。当前页码因总页数减少而越界时 SHALL 自动回退到末页，末页页码由 `pageInfo.totalPages` 给出。

#### Scenario: 每页条数持久化

- **WHEN** 操作者把每页条数改为 24 后刷新页面
- **THEN** 每页条数仍为 24

#### Scenario: 改每页条数回到第一页

- **WHEN** 操作者处于第 5 页时把每页条数从 12 改为 50
- **THEN** 请求的 `page` 为 1

#### Scenario: 页码越界自动回退

- **WHEN** 操作者处于第 8 页，随后收窄筛选条件使总页数变为 3
- **THEN** 列表自动回退到第 3 页并展示内容，MUST NOT 停留在空白页

### Requirement: 声称作用于全量的操作不得静默退化为当前页

界面上以「所有」「全部」措辞呈现的批量操作，其作用范围 SHALL 与措辞一致，MUST NOT 因服务端分页而静默缩小到当前页。

「清除所有已禁用凭据」这类入口在分页前依据前端持有的全量数据判定范围。分页后同样的实现只能看到当前页，措辞与实际行为脱节，操作者以为清空了全部而实际只清了一页。

这类入口 SHALL 采取下列之一，MUST NOT 保持原样：

- 改为显式的当前页语义，措辞同步改为「清除本页已禁用凭据」并给出准确计数
- 保留全量语义，范围与计数由带对应筛选条件的服务端查询给出，不依赖当前页数据

保留全量语义时，实际执行的对象列表 SHALL 逐页取回至覆盖全集，MUST NOT 用当前页响应中的数组过滤得出。计数正确但执行范围仍为当页，是同一个缺陷的另一种表现。

入口的可用性判据（例如「无已禁用凭据时禁用按钮」）SHALL 与其声称的作用范围采用同一数据来源，MUST NOT 用当前页计数去控制一个全量语义的入口。

#### Scenario: 全量措辞与实际范围一致

- **WHEN** 系统共有 20 条已禁用凭据，当前页只含其中 3 条，操作者点击清除入口
- **THEN** 确认文案中的计数与实际删除的条数一致，两者同为 20 或同为 3，MUST NOT 出现文案写 20 而只删 3

#### Scenario: 全量清除的执行范围覆盖全集

- **WHEN** 系统共有 20 条已禁用凭据分布在多页，操作者确认执行全量语义的清除
- **THEN** 实际发出的删除请求覆盖全部 20 条，执行完毕后系统内不再存在已禁用凭据

#### Scenario: 入口可用性判据与作用范围同源

- **WHEN** 系统存在已禁用凭据，但当前页一条都没有
- **THEN** 全量语义的清除入口保持可用；当前页语义的清除入口显示为禁用并说明原因

#### Scenario: 已选计数区分作用范围

- **WHEN** 操作者跨页选中若干凭据后查看批量操作区
- **THEN** 界面同时给出「已选 N 条」与当前页条数，使操作者能判断即将执行的范围
