## Why

Admin UI 的余额链路有三处行为与操作者预期不符，其中一处是正确性缺陷。

**1. 批量验活读缓存，可把失效凭据报成 success（真实缺陷）**

`handleBatchVerify`（`admin-ui/src/components/dashboard.tsx:456`）调用
`getCredentialBalance(id)` 不传 `force`。前端仅在 `force` 为真时挂 `?force=true`
（`admin-ui/src/api/credentials.ts:86`），后端 `force=false` 时先读 `balance_cache`，
命中 300s TTL 就直接返回、不访问上游（`src/admin/service.rs:256-264`，
`BALANCE_CACHE_TTL_SECS = 300` 见 `src/admin/service.rs:25`）。

后果：凭据在 5 分钟内被上游封禁或 token 失效，验活仍会凭缓存显示
`status: 'success'` 并给出旧的 usage 数字。缓存还会落盘
（`src/admin/service.rs:283` → `save_balance_cache`），进程重启也不能绕过该窗口
（加载时按 TTL 过滤，`src/admin/service.rs:834-845`）。
「验活」的语义要求真实探测，读缓存使该功能给出的是无效结论。

**2. 「批量余额/订阅」按钮的图标暗示强制刷新**

按钮文案本身是「批量余额/订阅」，未承诺刷新，但它使用 `RefreshCw` 图标并在进行中旋转
（`admin-ui/src/components/dashboard.tsx:705-706`），与「刷新 Token」按钮同图标。
操作者据此预期点击会拿到最新余额，实际在 TTL 内返回缓存，表现为「点了没反应」。

**3. 卡片 `localBalance` 无条件遮蔽父级数据**

`displayBalance = localBalance ?? balance`（`admin-ui/src/components/credential-card.tsx:173`）。
单卡「刷新余额」写入组件内 `localBalance`（`credential-card.tsx:162`），此后只要卡片保持挂载，
父级 `balanceMap` 的更新就不再显示。`key={credential.id}`（`dashboard.tsx:761`）意味着
翻页或列表增删会卸载卡片并重置该 state，因此这是挂载期内的陈旧显示，不是永久错误。
`BalanceResponse`（`admin-ui/src/types/api.ts:39-47`）无时间戳字段，无法比较新旧。

**4. 缺少定时刷新（新增需求）**

操作者需要在页面挂起时持续观察余额变化，当前只能反复手动点击。

## What Changes

### 批量验活改为真实探测

- `handleBatchVerify` 改为 `getCredentialBalance(id, true)`，跳过 TTL 缓存。
- 不改用 `testCredential`（`admin-ui/src/api/credentials.ts:278`）：它走真实最小推理，
  每个凭据产生一次模型调用，批量场景有 token 成本与封号风险。
  `force` 的 usage-limits 查询已能验证「token 有效且能换到额度信息」，
  与现有 `credential-import` spec 中「余额成功不等于对话就绪」的边界一致。

### 批量按钮表意修正

- 图标由 `RefreshCw` 换为 `Wallet`（与单卡余额按钮一致，`credential-card.tsx:416`）。
- 补 `title`，说明该操作可能返回最长 300s 内的缓存数据。
- 行为不变，仍为当前页启用凭据、不带 `force`。这是有意保留的低成本查询路径。

### 卡片余额单一数据源

- 删除 `credential-card.tsx` 的 `localBalance` state 与 `displayBalance` 兜底。
- 新增 `onBalanceRefreshed?: (id: number, balance: BalanceResponse) => void` prop，
  单卡 force 刷新后回调父级写入 `balanceMap`，卡片只读 `balance` prop。
- 选择单一数据源而非加时间戳比较：后者需给 `BalanceResponse` 造前端专属字段并在两处维护
  比较逻辑，而双源本身没有存在必要。

### 定时批量刷新（新增）

- 顶栏新增开关，**默认关闭**（opt-in）。开启后按间隔对当前页启用凭据逐个刷新。
- 间隔默认 120s，可选 30s / 60s / 120s / 180s / 300s，并支持手动输入正整数秒。
- 每轮走 `force=true`：后端 TTL 固定 300s，不带 `force` 时 300s 以下的档位全部命中缓存、
  拿不到新数据，定时刷新将失去意义。
- 防重入：上一轮未跑完，或手动批量查询/批量验活正在进行时，跳过本轮 tick，不排队堆叠。
- 不做请求间延迟。防护重点是并发堆叠而非瞬时频率；批量验活已有的 2000ms 延迟
  （`dashboard.tsx:487`）服务于其自身语义，不套用于此。

## Non-Goals

- 不改后端 `BALANCE_CACHE_TTL_SECS`，不新增 TTL 配置项。
- 不改 `save_balance_cache` 的落盘策略（见 Risks 中的 I/O 放大）。
- 不把定时刷新设置持久化到 localStorage：档位定位为临时切换，刷新页面回到「关闭 + 120s」。
- 不改单卡「刷新余额」与「刷新 Token」的既有行为。
- 不引入全局（跨页）定时刷新；作用域与现有批量操作一致，仅当前页。
- 不改 `useCredentialBalance`（`admin-ui/src/hooks/use-credentials.ts:29`）与余额详情弹窗：
  弹窗按需查看，读缓存可接受。

## Assumptions

- 上游 usage-limits 查询无严格速率限制，30s 档下当前页最多 12 个凭据
  （`itemsPerPage = 12`，`dashboard.tsx:53`）的轮询可被接受。此为假设，非已验证事实，
  故默认关闭且默认档位取 120s。
- 操作者开启定时刷新时在场，能观察到失败提示。
- `force` 的 usage-limits 成功足以判定凭据可用，与现有验活口径一致。

## Impact

- **代码**：`admin-ui/src/components/dashboard.tsx`（验活 force、按钮图标与 title、
  定时刷新 state/effect/UI）；`admin-ui/src/components/credential-card.tsx`
  （移除 `localBalance`，新增 `onBalanceRefreshed`）。
- **不改**：后端 `src/admin/`（`force` 语义与 TTL 均已正确实现，`service.rs:249-286`）；
  `admin-ui/src/api/credentials.ts`（`force` 参数已就绪）。
- **spec**：MODIFIED `admin-ui-model-ops`（既有「凭据卡片提供余额强制刷新入口」需求需
  补齐批量查询的缓存语义与单一数据源约束）；ADDED `admin-ui-balance-ops`（验活探测口径、
  定时刷新）。

## Success Criteria

- 批量验活对每个凭据发出带 `force=true` 的请求；TTL 内失效凭据不再被报成 success。
- 单卡 force 刷新后，同一卡片在不卸载的情况下能反映后续批量查询结果。
- 定时刷新默认关闭；开启后按所选间隔触发，每轮带 `force=true`；
  上一轮未完成或手动批量操作进行中时跳过该轮。
- 翻页后定时刷新作用于新的当前页，且不因 render 重建 timer 而提前触发。
- `pnpm build` 通过（AGENTS.md 高风险矩阵 admin-ui 项）。
- `openspec validate --all` 通过。

## Risks

- **上游请求频率放大**：30s 档 × 12 凭据 ≈ 每分钟 24 次 usage-limits 请求，
  长时间开启可能触发上游限流或风控。缓解：默认关闭、默认 120s、UI 文案说明。
- **后端落盘 I/O 放大（已知，本 change 不修）**：每次 `force` 成功都会
  持锁序列化整个 balance 缓存并同步写文件（`src/admin/service.rs:272-283`、`848` 起）。
  定时刷新会把该写入频率提高到每轮凭据数次。当前凭据规模下影响有限，
  修改落盘策略属后端范围，超出本 change 范围，故仅记录。
- **验活耗时上升**：`force` 必然访问上游，不再有缓存快路径，批量验活整体变慢。
  这是获得正确结论的必要代价。
- **回滚**：全部改动集中在两个前端文件，`git revert` 即可恢复；无数据迁移、无后端状态变更。
