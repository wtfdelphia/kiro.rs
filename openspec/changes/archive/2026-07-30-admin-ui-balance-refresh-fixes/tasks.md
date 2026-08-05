## 1. 批量验活改为真实探测

- [x] 1.1 `admin-ui/src/components/dashboard.tsx:456` 改为 `getCredentialBalance(id, true)`
  → 验证：DevTools Network 中批量验活的每个请求 URL 含 `?force=true`
- [x] 1.2 确认失败分支仍正确呈现（`dashboard.tsx:469-480` 不需改动）
  → 验证：对一个已知失效凭据验活，结果为 failed 且带错误文案，非 success

## 2. 批量按钮表意修正

- [x] 2.1 `dashboard.tsx:705` 图标由 `RefreshCw` 换为 `Wallet`；`dashboard.tsx:2` import 新增
  `Wallet`，**保留 `RefreshCw`**（`:592` 与 `:673` 仍在使用，详见 evidence/bridge-plan.md 补盲 2）
  → 验证：`pnpm build` 通过；`:592`、`:673` 图标未变；按钮渲染为钱包图标
- [x] 2.2 为该按钮补 `title`，说明可能返回最长 5 分钟内的缓存结果
  → 验证：悬停显示提示文案
- [x] 2.3 确认 `handleQueryCurrentPageInfo` 的请求行为未变（仍不带 force）
  → 验证：Network 中该按钮请求 URL 不含 `force`

## 3. 卡片余额单一数据源

- [x] 3.1 确认 `CredentialCard` 的唯一调用点是 `dashboard.tsx:760`
  → 验证：`rg "CredentialCard" admin-ui/src` 仅命中定义与该调用点
- [x] 3.2 `credential-card.tsx` props 增加 `onBalanceRefreshed?: (id: number, balance: BalanceResponse) => void`
  → 验证：`pnpm build` 类型通过
- [x] 3.3 删除 `localBalance` state（`credential-card.tsx:73`）与 `displayBalance`（`:173`），
  展示处（`:337`、`:360-364`）改用 `balance` prop
  → 验证：文件内无 `localBalance` / `displayBalance` 残留
- [x] 3.4 `handleRefreshBalance` 成功后调用 `onBalanceRefreshed?.(credential.id, resp)`
  → 验证：单卡刷新后卡片数值更新（数据经父级回流）
- [x] 3.5 `dashboard.tsx` 传入回调写入 `balanceMap`
  → 验证：单卡刷新 → 手动批量查询 → 该卡数值随批量结果变化，不再被遮蔽
  → 实现：新增 `applyBalance(id, balance)` 作为 `balanceMap` 唯一写入口，
    单卡回调、手动批量查询、定时刷新三处共用

## 4. 定时批量刷新

- [x] 4.1 新增 state：`autoRefreshEnabled`（默认 `false`）、`autoRefreshInterval`（默认 `120`）、
  `autoRefreshRunningRef`
  → 验证：首次进入 dashboard 无自动请求发出
  → 实现：另加 `autoRefreshInput` 承载输入框草稿值，常量 `AUTO_REFRESH_DEFAULT_SECS` /
    `AUTO_REFRESH_MIN_SECS` / `AUTO_REFRESH_PRESETS` 提到模块级
- [x] 4.2 实现 `runAutoRefresh`：取当前页启用凭据，逐个 `getCredentialBalance(id, true)`，
  逐个写 `balanceMap`
  → 验证：开启后每轮请求均带 `?force=true`，逐个串行而非并发
- [x] 4.3 防重入：running ref 为真，或 `queryingInfo`/`verifying`/`batchRefreshing` 为真时跳过本轮
  → 验证：设 30s 档并让单轮耗时超过 30s，确认不出现并发轮次
- [x] 4.4 `useEffect` + `setInterval`，依赖仅 `[autoRefreshEnabled, autoRefreshInterval]`，
  执行体经 ref 持有最新闭包
  → 验证：翻页、勾选凭据等 render 不重建 timer（tick 间隔稳定在设定值）
- [x] 4.5 开启开关时不立即执行首轮
  → 验证：开启后到第一次请求的间隔等于所设秒数
- [x] 4.6 UI：开关 + 档位选择（30/60/120/180/300）+ 手动输入秒数
  → 验证：各档位切换后 tick 间隔随之变化
  → 实现：复用 `ui/switch`、`ui/select`、`ui/input`，未新建组件
- [x] 4.7 手动输入校验：正整数且 ≥10，非法值不提交并保留原值
  → 验证：输入 0、-5、abc、5 均被拒绝，间隔不变
  → 实现：`commitAutoRefreshInput` 在 blur 与 Enter 时校验，非法则 toast.error 并回填原值
- [x] 4.8 静默策略：成功不弹 toast，仅当轮内有失败时提示一次
  → 验证：正常运行不刷 toast；断开 Admin key 后每轮仅一条提示
- [x] 4.9 关闭开关或卸载时清理 interval
  → 验证：关闭后 Network 不再出现新请求；登出后同样停止
- [x] 4.10 当前页无启用凭据时静默跳过（不弹 error）
  → 验证：全部凭据禁用时开启定时刷新，无 toast、无请求

## 5. 验证与收尾

- [x] 5.1 `cd admin-ui && pnpm build`
  → 已运行：`tsc -b && vite build` 成功，1777 modules，45.04s。
    两条 WARN（pnpm.onlyBuiltDependencies 字段失效、caniuse-lite 数据陈旧）为既有问题，与本 change 无关
- [x] 5.2 `openspec validate --all`
  → 已运行：19 passed, 0 failed
- [x] 5.3 `git status --short` 确认改动仅限 `admin-ui/src/components/dashboard.tsx`、
  `admin-ui/src/components/credential-card.tsx` 与本 change 目录
  → 已确认：本 change 仅改上述两文件（+145/-16 与 +16/-... 见 diff --stat）。
    `src/openai/*` 的未提交改动属上一个 change，未受影响。无 `src/admin/` 改动，
    无凭据文件、无 `.codegraph/` 混入
- [x] 5.4 最终报告写明：admin-ui 无既有测试框架，前端验证依赖 `pnpm build` 与手动核对；
  列出实际未手动验证的项与剩余风险
  → 见下方「验证状态」章节

## 验证状态（本会话实际执行）

已运行：

- `cd admin-ui && pnpm build` → 通过（含 `tsc -b` 类型检查）
- `openspec validate --all` → 19/19 通过
- `git status --short` / `git diff --stat` → 改动范围符合预期
- `rg "localBalance|displayBalance" admin-ui/src` → 零残留

未运行及原因：

- `cargo test`：后端 `src/admin/` 未改动，force 语义已有
  `get_balance_force_bypasses_cache`（`src/admin/service.rs:1673`）覆盖
- 前端单元测试：`admin-ui/package.json` 无 test script、无 vitest/jest 依赖，
  项目当前无前端测试框架

未手动验证（需在浏览器中核对，本会话未执行）：

- 验活请求实际带 `?force=true`（代码层面已确认，运行时未抓包）
- 定时刷新 tick 间隔与防重入的运行时行为
- 单卡刷新后批量更新可见
- 非法间隔输入被拒的交互反馈
- 关闭开关/登出后 timer 停止
- 定时刷新控件在窄屏下的换行表现（顶栏容器为 flex-wrap，未实测）

剩余风险：

- 上游请求频率放大：30s 档 × 最多 12 凭据 ≈ 每分钟 24 次 usage-limits 请求。
  已用默认关闭 + 默认 120s + title 文案缓解，但上游速率容忍度仍是假设
- 后端落盘 I/O 放大（已知，本 change 不修）：每次 force 成功都持锁序列化整个缓存并同步写盘
  （`src/admin/service.rs:272-283`），定时刷新会放大该频率
- 验活整体变慢：force 必然打上游，失去缓存快路径
