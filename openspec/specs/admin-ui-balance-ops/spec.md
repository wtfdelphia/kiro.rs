# Capability: admin-ui-balance-ops

## Purpose

Define how the Admin UI reads credential balance/usage: which paths MUST bypass the Admin balance TTL cache and which MAY read it, how those paths present themselves so operators are not misled about data freshness, and how an optional interval-based refresh polls the current page without stacking rounds or flooding the upstream. Also covers the read-only cache snapshot inlined into the credentials list response — a third path that serves first paint without any upstream call — and the lifetime of live query results held by the frontend across pagination. Consumes the existing Admin balance endpoint (`GET /api/admin/credentials/{id}/balance`, with `force=true` bypassing the cache). Complements `admin-ui-model-ops`, which owns the per-card balance refresh control.

## Requirements

### Requirement: 批量验活必须绕过余额缓存

Batch credential verification MUST request balance/usage with cache bypass (force) for every credential it checks. Verification asserts that a credential is currently usable, and the Admin balance endpoint returns a cached snapshot for up to its TTL when force is absent. Reporting success from a cached snapshot means a credential revoked or expired within that window is reported healthy with stale usage numbers, which makes the verification result unsound.

Verification MUST NOT be reduced to a cache read as an optimisation, even when the same credential was queried moments earlier.

#### Scenario: 每个凭据都带 force

- **WHEN** 操作者对任意数量的凭据执行批量验活
- **THEN** 每个凭据的 balance/usage 请求 MUST 带 force，MUST NOT 命中 TTL 缓存

#### Scenario: TTL 内失效的凭据不得报成功

- **GIVEN** 某凭据在 TTL 窗口内已被上游吊销或其 token 已失效，且该窗口内存在其成功的余额缓存
- **WHEN** 操作者执行批量验活
- **THEN** 该凭据 MUST 被判定为失败并展示上游错误原因
- **AND** MUST NOT 依据缓存快照报告成功或展示缓存中的额度数字

#### Scenario: 验活口径为额度查询

- **WHEN** 批量验活探测某凭据
- **THEN** 探测 MUST 为 force 的 balance/usage 查询，验证 token 有效且能取得额度信息
- **AND** MUST NOT 为每个凭据发起真实模型推理调用（批量场景下会产生 token 成本与账号风险）
- **AND** 成功结果 MUST NOT 被表述为对话链路完全就绪（与 credential-import 中 profile 前置条件的既有边界一致）

### Requirement: 批量余额查询入口不得暗示强制刷新

The dashboard's batch balance/subscription action queries with cache reads allowed and is intentionally kept as the low-cost path. Its control MUST NOT present itself as a forced refresh: an icon shared with genuinely forcing actions leads operators to expect fresh upstream data and read a cache hit as the button being broken.

The control MUST disclose that its result may come from cache, and MUST point to the paths that do bypass cache.

#### Scenario: 图标不与强制操作共用

- **WHEN** 渲染批量余额/订阅按钮
- **THEN** 其图标 MUST NOT 与执行强制刷新的控件（如强制刷新 Token）共用
- **AND** 宜采用与单卡余额入口一致的图标以表达「余额」而非「刷新」

#### Scenario: 披露缓存语义

- **WHEN** 操作者悬停或聚焦该按钮
- **THEN** 提示 MUST 说明结果可能来自缓存及其有效期
- **AND** MUST 指向可获得最新数据的路径（单卡强制刷新或定时刷新）

#### Scenario: 请求行为保持不变

- **WHEN** 操作者点击该按钮
- **THEN** 行为 MUST 仍为当前页启用凭据的批量查询且允许命中缓存
- **AND** MUST NOT 改为强制刷新（该职责归属定时刷新与单卡刷新）

### Requirement: 定时批量余额刷新

The dashboard MUST offer an interval-based balance refresh over the current page's enabled credentials, so operators can watch balances change without repeated clicking.

Each round MUST use force. The Admin balance TTL is a fixed 300 seconds, so any interval at or below that value would return cached data without force and the feature would report nothing new.

The feature MUST default to off. Every round issues one upstream request per enabled credential on the page; an on-by-default poll would consume upstream quota whenever the page sits open unattended, and the upstream's tolerance for this rate is an assumption rather than a verified fact.

#### Scenario: 默认关闭

- **WHEN** 操作者进入 dashboard
- **THEN** 定时刷新 MUST 处于关闭状态，MUST NOT 发出任何自动余额请求

#### Scenario: 每轮强制刷新

- **WHEN** 定时刷新触发一轮
- **THEN** 该轮对当前页每个启用凭据的请求 MUST 带 force
- **AND** 结果 MUST 写入卡片读取的共享余额数据源

#### Scenario: 间隔可选与手动输入

- **WHEN** 操作者配置刷新间隔
- **THEN** 单位 MUST 为秒，默认 MUST 为 120
- **AND** MUST 提供 30 / 60 / 120 / 180 / 300 预设档位
- **AND** MUST 允许手动输入具体秒数

#### Scenario: 拒绝非法间隔

- **WHEN** 操作者手动输入非正整数或低于最小允许值的间隔
- **THEN** 该输入 MUST 被拒绝且当前生效间隔保持不变
- **AND** MUST NOT 以非法值启动 timer

#### Scenario: 防重入而非排队

- **GIVEN** 上一轮刷新尚未完成
- **WHEN** 下一次间隔到达
- **THEN** 本轮 MUST 被跳过
- **AND** MUST NOT 排队堆叠或与上一轮并发执行

#### Scenario: 手动批量操作优先

- **GIVEN** 手动批量余额查询、批量验活或批量刷新 Token 正在进行
- **WHEN** 定时刷新的间隔到达
- **THEN** 本轮 MUST 被跳过，让手动操作独占

#### Scenario: 开启时不立即执行

- **WHEN** 操作者开启定时刷新
- **THEN** 首轮 MUST 在一个完整间隔后才执行
- **AND** MUST NOT 把开启动作本身变成一次强制批量请求

#### Scenario: render 不重建 timer

- **WHEN** 组件因翻页、勾选或其他状态变化重新渲染，而开关与间隔均未改变
- **THEN** 既有 timer MUST 保持不变
- **AND** 下一次触发时刻 MUST NOT 因此提前或推迟

#### Scenario: 作用于当前页

- **GIVEN** 定时刷新已开启
- **WHEN** 操作者翻页
- **THEN** 后续轮次 MUST 作用于新的当前页启用凭据

#### Scenario: 静默成功，失败仅提示一次

- **WHEN** 一轮定时刷新全部成功
- **THEN** MUST NOT 弹出成功提示（周期性提示会淹没其他反馈）
- **WHEN** 一轮中存在失败
- **THEN** MUST 在轮末给出一次汇总提示，MUST NOT 逐个凭据提示

#### Scenario: 关闭与卸载时停止

- **WHEN** 操作者关闭开关，或 dashboard 被卸载（如登出）
- **THEN** timer MUST 被清理且 MUST NOT 再发起新轮次
- **AND** 已在途的请求可自然结束，防重入标记 MUST 被复位

#### Scenario: 当前页无启用凭据时静默

- **GIVEN** 当前页没有启用状态的凭据
- **WHEN** 定时刷新的间隔到达
- **THEN** 本轮 MUST 静默跳过，MUST NOT 报错提示（周期性错误提示无操作价值）

### Requirement: 列表内联余额为只读缓存快照

凭据列表响应中内联的余额信息 SHALL 是余额缓存的只读快照，与操作者主动触发的余额查询是两条独立路径。

- 快照 SHALL 携带 `cachedAt`、`ageSecs`、`stale` 三个字段，使调用方能自行判断新鲜度
- `cachedAt` SHALL 是 Unix 秒时间戳，与同一结构体内的 `nextResetAt` 采用同一种时间表示。缓存本身以 Unix 秒记录写入时刻，快照不做格式转换
- 缓存未命中的凭据 SHALL 省略该字段，MUST NOT 用零值或占位数值填充
- 生成快照 MUST NOT 触发任何上游余额查询，也 MUST NOT 刷新缓存的 TTL
- 该快照 MUST NOT 参与批量验活的成功判定。验活的口径仍是每个凭据一次带 `force` 的额度查询，缓存命中不得计为验活成功
- UI 展示快照时 SHALL 显式标注数据来自缓存及其新鲜度，MUST NOT 让操作者把缓存值误认为实时值

这条要求不改变既有的「批量验活必须绕过余额缓存」与「批量余额查询入口不得暗示强制刷新」：内联快照只服务于首屏展示，两处批量操作的请求行为一概不变。

#### Scenario: 列表内联快照不触发上游

- **WHEN** 请求凭据列表且部分凭据有余额缓存
- **THEN** 响应中带出这些凭据的余额快照，期间不产生任何上游额度查询请求

#### Scenario: 快照携带新鲜度字段

- **WHEN** 某凭据的余额缓存写入于 120 秒前
- **THEN** 该凭据的快照中 `cachedAt` 为写入时刻的 Unix 秒、`ageSecs` 约为 120、`stale` 为 `false`

#### Scenario: 超过 TTL 的快照标记为过期

- **WHEN** 某凭据的余额缓存年龄超过缓存 TTL
- **THEN** 快照的 `stale` 为 `true`，数值仍然带出

#### Scenario: 缓存未命中时省略字段

- **WHEN** 某凭据无余额缓存
- **THEN** 该凭据的响应对象不含余额快照字段，UI 显示「未查询」

#### Scenario: 内联快照不得替代验活

- **WHEN** 操作者对一个有新鲜缓存快照的凭据发起批量验活
- **THEN** 仍然发出一次带 `force` 的额度查询，验活结论以该请求结果为准

#### Scenario: UI 区分缓存值与实时值

- **WHEN** 卡片展示的用量数值来自内联快照
- **THEN** 数值旁标注缓存新鲜度；操作者主动查询成功后该标注消失

### Requirement: 前端实时余额结果不因翻页丢弃

前端持有的实时余额查询结果 SHALL 按凭据 id 保留，其存活范围 MUST NOT 被当前页的凭据集合限定。

清理该缓存的判据 SHALL 是「凭据已从系统中删除」，MUST NOT 是「凭据不在当前页响应中」。服务端分页后这两个条件不再等价：翻页会让其他页的凭据暂时不出现在响应里，按后者清理等于每次翻页都丢掉已查到的实时值。

丢弃实时值会让卡片从实时态退回缓存态，与「实时查询成功后新鲜度标注消失」的要求直接冲突：操作者翻页往返一次，刚查到的实时值就变回带新鲜度标注的缓存值。

#### Scenario: 翻页往返后实时值仍在

- **WHEN** 操作者在第 1 页查询某凭据余额成功，翻到第 2 页后翻回第 1 页
- **THEN** 该凭据仍展示实时结果，MUST NOT 退回缓存态或显示新鲜度标注

#### Scenario: 凭据删除后清理实时值

- **WHEN** 某凭据被删除，后续列表响应中系统内已无该 id
- **THEN** 其实时余额结果被清理，MUST NOT 残留于界面
