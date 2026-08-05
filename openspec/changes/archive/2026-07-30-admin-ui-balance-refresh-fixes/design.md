## 当前实现

### 余额获取链路

```
credential-card「刷新余额」 ──force=true──┐
dashboard「批量余额/订阅」 ──force=false─┤
dashboard「批量验活」      ──force=false─┼─→ GET /api/admin/credentials/{id}/balance
balance-dialog（useCredentialBalance）──┘        │
                                                 ▼
                              AdminService::get_balance(id, force)
                                                 │
                          force=false ──→ balance_cache 命中（<300s）→ 返回缓存
                                     └──→ 未命中 / force=true → fetch_balance（上游）
                                                 │
                                          写缓存 + save_balance_cache（落盘）
```

关键位置：

| 位置 | 现状 |
| --- | --- |
| `admin-ui/src/api/credentials.ts:81-89` | `force` 为真才挂 `?force=true`，否则不带 query |
| `src/admin/types.rs:115-119` | `BalanceQuery { force: bool }`，`#[serde(default)]` → 缺省 false |
| `src/admin/handlers.rs:75-83` | 直接把 `query.force` 传给 service |
| `src/admin/service.rs:25` | `BALANCE_CACHE_TTL_SECS = 300`，编译期常量 |
| `src/admin/service.rs:256-267` | `!force` 读缓存并 early return；`force` 记 debug 日志后落到上游 |
| `src/admin/service.rs:1673-1704` | 已有测试 `get_balance_force_bypasses_cache` 覆盖该分支 |

后端语义是正确的，问题全部在前端调用方与展示层。

### 三处缺陷的精确位置

**批量验活读缓存**

`dashboard.tsx:456` `const balance = await getCredentialBalance(id)` —— 无第二参数。
成功即写 `status: 'success'` 与 `usage`（`dashboard.tsx:460-468`）。
缓存命中路径下这两个值都可能是最长 300s 前的快照。

**批量按钮图标**

`dashboard.tsx:699-707`：`onClick={handleQueryCurrentPageInfo}`，
图标 `RefreshCw` 且 `queryingInfo` 时 `animate-spin`。同一文件 `dashboard.tsx:673` 的
「批量刷新 Token」用相同图标，但那个确实是强制操作，图标语义被混用。

**卡片双数据源**

```
credential-card.tsx:73   const [localBalance, setLocalBalance] = useState<BalanceResponse | null>(null)
credential-card.tsx:162  setLocalBalance(resp)          ← 单卡 force 刷新写入
credential-card.tsx:173  const displayBalance = localBalance ?? balance
credential-card.tsx:337  displayBalance?.subscriptionTitle
credential-card.tsx:360-364 displayBalance.remaining / usageLimit / usagePercentage
```

父级 `balanceMap` 经 `balance` prop 传入（`dashboard.tsx:766`）。
`localBalance` 一旦非空即永久优先（挂载期内）。`BalanceResponse` 无时间戳，
无法在两源之间判断新旧。

## 目标设计

### 1. 批量验活带 force

单行改动：

```ts
// dashboard.tsx:456
const balance = await getCredentialBalance(id, true)
```

不引入 `testCredential`。理由记在 proposal 的 What Changes；此处补充协议层依据：
`fetch_balance`（`service.rs:289-314`）调 `token_manager.get_usage_limits_for(id)`，
该调用需要有效 token 且上游返回真实额度，失效凭据会经 `classify_balance_error` 转为错误，
验活的 failed 分支（`dashboard.tsx:469-480`）已能正确呈现。

### 2. 批量按钮表意

```
图标：RefreshCw → Wallet（lucide-react，已在 credential-card.tsx:416 使用）
title：「查询当前页启用凭据的余额/订阅，可能返回最近 5 分钟内的缓存结果；
        需要最新数据请用单卡「刷新余额」或开启定时刷新」
```

不改 `handleQueryCurrentPageInfo` 的请求行为——它是有意保留的低成本路径，
force 化会让每次点击都必然打上游，与「定时刷新」职责重叠。

### 3. 卡片单一数据源

```
父级 dashboard
  balanceMap ──balance prop──→ credential-card（唯一读取源）
       ▲
       └──onBalanceRefreshed(id, balance)── 单卡 force 刷新回调
```

`credential-card.tsx` 改动：

- 删 `localBalance` state 与 `displayBalance`，展示处直接用 `balance`。
- props 增 `onBalanceRefreshed?: (id: number, balance: BalanceResponse) => void`。
- `handleRefreshBalance` 成功后调 `onBalanceRefreshed?.(credential.id, resp)` 取代 `setLocalBalance`。

`dashboard.tsx` 传入的回调复用现有 `setBalanceMap` 写法（与 `dashboard.tsx:388-392` 同形）。

prop 设为可选，避免其他调用点（若有）编译失败；实现时须确认调用点只有 `dashboard.tsx:760`。

### 4. 定时批量刷新

状态：

```ts
const [autoRefreshEnabled, setAutoRefreshEnabled] = useState(false)   // 默认关闭
const [autoRefreshInterval, setAutoRefreshInterval] = useState(120)   // 秒
const autoRefreshRunningRef = useRef(false)                           // 防重入
```

预设档位 `[30, 60, 120, 180, 300]` + 手动输入。手动输入校验：正整数，
下界 10s（低于此值单轮请求几乎必然与下一轮重叠），非法输入不提交、保留原值。

执行体（与 `handleQueryCurrentPageInfo` 的差异只在 force 与静默）：

```
tick →
  若 autoRefreshRunningRef.current → 跳过（上一轮未完成）
  若 queryingInfo || verifying || batchRefreshing → 跳过（手动操作优先）
  取当前页启用凭据 id（与 dashboard.tsx:360-362 同口径）
  逐个 await getCredentialBalance(id, true)，逐个写 balanceMap
  失败计数，不弹 toast.success；仅当有失败时提示一次
  finally 复位 running ref
```

timer 用 `useEffect` + `setInterval`，依赖 `[autoRefreshEnabled, autoRefreshInterval]`。
执行体通过 ref 持有最新闭包，避免把 `currentCredentials` 放进依赖导致
每次 render 重建 interval、间隔被无限推迟或提前触发：

```ts
const autoRefreshTaskRef = useRef<() => Promise<void>>()
autoRefreshTaskRef.current = runAutoRefresh        // 每次 render 更新，不触发 effect
useEffect(() => {
  if (!autoRefreshEnabled) return
  const timer = setInterval(() => { void autoRefreshTaskRef.current?.() }, autoRefreshInterval * 1000)
  return () => clearInterval(timer)
}, [autoRefreshEnabled, autoRefreshInterval])
```

开启时不立即执行一轮：立即执行会让「开启开关」等价于一次强制批量请求，
与 opt-in 的谨慎取向不符；操作者需要立刻刷新时有现成的手动按钮。

### 数据流影响面

```
改动前：balanceMap ─→ card.balance ─┐
        localBalance ──────────────┴─→ displayBalance（localBalance 优先）

改动后：balanceMap ─→ card.balance ─→ 展示
             ▲                ▲
             │                └── 定时刷新（force）
             └── 单卡 force 刷新回调 / 手动批量查询（无 force）
```

所有写入汇聚到 `balanceMap`，后写覆盖先写。既有的「列表变化时清理失效 id」逻辑
（`dashboard.tsx:85-116`）自动覆盖新增的写入源，无需扩展。

## 异常路径

| 场景 | 行为 |
| --- | --- |
| 定时刷新中某凭据请求失败 | 计入失败数，继续下一个；轮末一次性 `toast.warning`，不逐个弹 |
| 定时刷新整轮全失败（如 Admin key 失效） | 同上，仅一次提示；不自动关闭开关（避免掩盖问题） |
| 上一轮未跑完就到下一个 tick | 跳过本轮，不排队；无提示（正常背压） |
| 手动批量查询/验活进行中到 tick | 跳过本轮，让手动操作独占 |
| 定时刷新进行中操作者翻页 | 当前轮沿用开始时取到的 id 列表跑完，下一轮取新页 |
| 定时刷新进行中关闭开关 | interval 被清理，当前轮仍跑完（不中断 in-flight 请求）；running ref 正常复位 |
| 定时刷新进行中卸载 dashboard（登出） | `clearInterval` 清理；in-flight 的 `setBalanceMap` 作用于已卸载组件，React 18 不再告警，无副作用 |
| 手动输入非法间隔（0、负数、非数字） | 不接受，保留原值，输入框提示 |
| 当前页无启用凭据 | 定时刷新静默跳过（不像手动按钮那样 `toast.error`） |
| 批量验活 force 后失效凭据 | 走既有 failed 分支，展示 `extractErrorMessage` 结果 |

## 回滚

改动仅涉及两个前端文件，无后端、无配置 schema、无持久化状态变更。
`git revert` 单个提交即可完整恢复；已落盘的 balance 缓存文件格式未变，无需清理。

## 验证策略

| 项 | 命令 / 方式 | 归属 |
| --- | --- | --- |
| admin-ui 构建与类型 | `pnpm build`（`admin-ui/`） | AGENTS.md 高风险矩阵 admin-ui |
| OpenSpec 工件 | `openspec validate --all` | AGENTS.md 高风险矩阵 OpenSpec |
| 后端未改动确认 | `git status --short` 中不出现 `src/admin/` | 验证纪律 |
| 验活 force 生效 | 浏览器 DevTools Network 确认请求带 `?force=true` | 手动 |
| 定时刷新间隔与防重入 | 设 30s 档观察 tick 间隔与不堆叠 | 手动 |
| 单卡刷新后批量更新可见 | 单卡刷新 → 手动批量查询 → 该卡数值随之更新 | 手动 |

后端已有 `get_balance_force_bypasses_cache`（`src/admin/service.rs:1673`）覆盖 force 语义，
本 change 不改后端逻辑，不新增 Rust 测试。admin-ui 无既有测试框架，
前端验证以 `pnpm build` 加手动核对为准，此为现状限制，需在最终报告中说明。
