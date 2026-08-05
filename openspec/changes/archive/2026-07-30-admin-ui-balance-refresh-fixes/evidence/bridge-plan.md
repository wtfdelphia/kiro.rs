# bridge-plan: admin-ui-balance-refresh-fixes

执行前检查点。分支 `dev`，change 状态：4 个工件（proposal/design/specs/tasks）全部 `done`，非 blocked。
`openspec validate admin-ui-balance-refresh-fixes --strict` 已通过。

## 范围

仅两个前端文件：

- `admin-ui/src/components/dashboard.tsx`
- `admin-ui/src/components/credential-card.tsx`

四项内容：

1. 批量验活改带 `force=true`（正确性缺陷修复）
2. 「批量余额/订阅」按钮图标 `RefreshCw` → `Wallet` + 补 `title`（表意，行为不变）
3. 卡片删 `localBalance`，改父级单一数据源 + `onBalanceRefreshed` 回调
4. 新增定时批量刷新（默认关闭、默认 120s、档位 30/60/120/180/300 + 手动输入、每轮 force、防重入）

## 非目标

- 不改后端 `src/admin/`（`force` 语义与 TTL 已正确实现）
- 不改 `BALANCE_CACHE_TTL_SECS = 300`，不新增 TTL 配置项
- 不改 `save_balance_cache` 落盘策略
- 不持久化定时刷新设置到 localStorage
- 不引入跨页定时刷新
- 不改 `useCredentialBalance` 与余额详情弹窗
- 不改单卡「刷新余额」「刷新 Token」既有行为

## 关键设计决策

| 决策 | 依据 |
| --- | --- |
| 验活用 `getCredentialBalance(id, true)` 而非 `testCredential` | 后者每凭据一次真实推理，批量有 token 成本与封号风险；force 的 usage-limits 已能验证 token 有效性 |
| 批量查询按钮保持不带 force | 有意保留的低成本路径；force 化会与定时刷新职责重叠 |
| 卡片单一数据源，非时间戳比较 | `BalanceResponse`（`admin-ui/src/types/api.ts:39-47`）无时间戳字段，需造前端专属字段并两处维护比较逻辑 |
| 定时刷新每轮必须 force | 后端 TTL 固定 300s，不带 force 时 ≤300s 的档位全部命中缓存，功能失去意义 |
| 默认关闭 + 默认 120s | 上游速率容忍度是假设而非已验证事实 |
| 手动输入下界 10s | 低于此值单轮几乎必然与下轮重叠，防重入会持续吃掉 tick，间隔失效 |
| 开启时不立即执行首轮 | 否则「开开关」等价于一次强制批量请求，与 opt-in 取向冲突 |
| 防重入不加请求间延迟 | 防护重点是并发堆叠而非瞬时频率 |
| 用原生 `title` 而非 Tooltip 组件 | 见下方 rg 补盲结论 |

## 高风险项

| 风险 | 说明 | 处置 |
| --- | --- | --- |
| 上游请求频率放大 | 30s 档 × 12 凭据（`itemsPerPage = 12`，`dashboard.tsx:53`）≈ 每分钟 24 次 usage-limits | 默认关闭、默认 120s、`title` 文案说明；开启为操作者显式动作 |
| 后端落盘 I/O 放大（已知，本 change 不修） | 每次 force 成功都持锁序列化整个缓存并同步写 `kiro_balance_cache.json`（`src/admin/service.rs:272-283`、`848` 起） | 记入 proposal Risks；改落盘策略属后端范围，超出本 change |
| timer 依赖数组写错导致每次 render 重建 | 若把 `currentCredentials` 放进 `useEffect` 依赖，间隔会被无限推迟或提前触发 | design 已给出 ref 持有闭包的写法；tasks 4.4 单列验证 |
| 验活整体变慢 | force 必然打上游，失去缓存快路径 | 获得正确结论的必要代价，接受 |
| admin-ui 无测试框架 | `package.json` 无 test script、无 vitest/jest 依赖 | 验证依赖 `pnpm build` + 手动核对，须在最终报告声明 |

## CodeGraph 证据

索引状态：`codegraph status` → 127 文件 / 2459 节点 / 6544 边，`Index is up to date`；
语言含 `tsx 26` / `typescript 8`，admin-ui 在索引覆盖内。

| 命令 | 结论 |
| --- | --- |
| `codegraph callers getCredentialBalance` | 仅 2 个调用方：`CredentialCard`（`credential-card.tsx:57`）与 `Dashboard`（`dashboard.tsx:28`）。改动面确认闭合在本 change 的两个文件内 |
| `codegraph impact CredentialCard` | 仅 2 个受影响符号，均在 `credential-card.tsx` 自身。新增 prop 不外溢 |
| `codegraph impact handleBatchVerify` | 符号未找到（组件内部箭头函数未建节点）。此为 CodeGraph 对 tsx 内部闭包的覆盖限制，改用 rg 补盲 |

`codegraph impact` 只接受 `<symbol>`，无 `--file` 选项；文件级影响面靠 rg 补齐。

## rg / 源码补盲

CodeGraph 不覆盖的部分，逐项用 rg 与源码精读确认：

**1. `CredentialCard` 调用点唯一（对应 tasks 3.1）**

```
rg -n "CredentialCard" admin-ui/src
→ dashboard.tsx:9 (import), dashboard.tsx:760 (唯一调用点),
  credential-card.tsx:32/57/64 (定义)
```

结论：新增 `onBalanceRefreshed` 只需改一处调用点。prop 仍设为可选以免约束未来调用方。

**2. `RefreshCw` import 不可删（更正 tasks 2.1 的表述）**

```
rg -n "RefreshCw" admin-ui/src/components/dashboard.tsx
→ :2 (import), :592, :673, :705
```

`dashboard.tsx` 有三处使用 `RefreshCw`：`:592`（页面标题区）、`:673`（批量刷新 Token）、
`:705`（本次要改的批量余额）。**只替换 `:705`，import 必须保留**。
tasks 2.1 原文写「同步调整 import」易被误读为删除，实现时以本条为准：
新增 `Wallet` 到 import，不删 `RefreshCw`。

**3. `Wallet` 图标已在项目中使用，无新依赖**

```
rg -n "Wallet" admin-ui/src
→ credential-card.tsx:3 (import), :416 (单卡刷新余额), :520 (查看余额详情)
```

`lucide-react@^0.460.0` 已在 `admin-ui/package.json` dependencies。

**4. Tooltip：用原生 `title`，不引入 Radix Tooltip**

```
rg -ln "react-tooltip|TooltipProvider" admin-ui/src  → 无命中
rg -c 'title=' admin-ui/src/components/*.tsx
→ dashboard.tsx:6, credential-card.tsx:11, public-api-panel.tsx:3, credential-models-dialog.tsx:1
```

`@radix-ui/react-tooltip` 在 `package.json` 中，但 `admin-ui/src` 内**没有任何封装或使用**。
项目惯例是原生 `title` 属性（dashboard 已用 6 处，如 `credential-card.tsx:438`）。
按 Surgical Changes，沿用 `title`，不为本 change 新建 Tooltip 组件体系。

**5. 定时刷新 UI 无需新增组件**

```
ls admin-ui/src/components/ui/
→ badge button card checkbox dialog input progress select sonner switch
```

`switch.tsx`（开关）、`select.tsx`（档位）、`input.tsx`（手动输入）齐备。
`select.tsx` 签名为 `{ value, onValueChange, options: SelectOption[], ... }`
（基于 `@radix-ui/react-dropdown-menu`），已有三处使用先例
（`settings-panel.tsx`、`add-credential-dialog.tsx`、`credential-test-dialog.tsx`）。

**6. 前端产物嵌入链路（构建纪律相关）**

```
src/admin_ui/router.rs:10  use rust_embed::Embed;
src/admin_ui/router.rs:14  #[folder = "admin-ui/dist"]
src/admin_ui/router.rs:82  "Admin UI not built. Run 'pnpm build' in admin-ui directory."
```

Admin UI 经 `rust_embed` 从 `admin-ui/dist` 嵌入二进制。
`admin-ui/dist` 已被 gitignore（`.gitignore:7`）且 `git ls-files` 确认未跟踪，
所以 `pnpm build` 产物不会进提交，但**不构建则改动不会出现在运行时**。
CI 侧 `.github/workflows/build.yaml` 与 `build-dev-release.yaml` 均含 pnpm 步骤，发布链路已覆盖。

**7. 构建命令确认**

`admin-ui/package.json` → `"build": "tsc -b && vite build"`。
含 `tsc -b`，所以 `pnpm build` 同时是类型检查，可作为本 change 的主要自动化验证。
**无 test script，无 vitest/jest 依赖** —— 前端无单元测试可跑，这是现状限制。

**8. 后端 force 语义已有测试覆盖**

```
src/admin/service.rs:1673  async fn get_balance_force_bypasses_cache()
```

已覆盖 force 跳过缓存的分支。本 change 不改后端逻辑，不新增 Rust 测试。

## 任务到执行步骤映射

| 任务 | 执行步骤 | 如何验证 | 何时停止 |
| --- | --- | --- | --- |
| 1.1 验活带 force | `dashboard.tsx:456` 加第二参数 `true` | DevTools Network：验活每个请求 URL 含 `?force=true` | 若发现其他验活入口也读缓存 → 停，回 proposal 补范围 |
| 1.2 失败分支确认 | 只读核对 `dashboard.tsx:469-480` | 失效凭据验活结果为 failed 且带错误文案 | 若失败分支需改造 → 停，超出「单行改动」预期 |
| 2.1 图标替换 | `dashboard.tsx:2` import 加 `Wallet`（**保留 `RefreshCw`**）；`:705` 替换图标 | `pnpm build` 通过；`:592`/`:673` 图标未变 | 若 `pnpm build` 报未使用 import → 说明误删，立即回退 |
| 2.2 补 title | `dashboard.tsx:699-707` 按钮加 `title`，沿用原生属性 | 悬停显示缓存语义提示 | 若倾向引入 Radix Tooltip → 停，属范围外新体系 |
| 2.3 行为不变确认 | 不改 `handleQueryCurrentPageInfo` | Network：该按钮请求 URL 不含 `force` | — |
| 3.1 调用点唯一 | 已由 rg 确认（见补盲 1） | 已完成 | — |
| 3.2 新增 prop | `credential-card.tsx:32-38` interface 加可选 `onBalanceRefreshed`；`:57-64` 解构 | `pnpm build` 类型通过 | — |
| 3.3 删双数据源 | 删 `:73` state、`:173` `displayBalance`；`:337`、`:360-364` 改用 `balance` | `rg "localBalance\|displayBalance" credential-card.tsx` 零命中 | 若展示处还有第三个数据源 → 停，重新核对 |
| 3.4 回调上报 | `:157-171` `handleRefreshBalance` 用回调替代 `setLocalBalance` | 单卡刷新后卡片数值更新 | — |
| 3.5 父级接收 | `dashboard.tsx:760` 传回调，复用 `setBalanceMap` 写法（同 `:388-392`） | 单卡刷新 → 手动批量查询 → 该卡随批量结果变化 | — |
| 4.1 新增 state | 三个 state/ref，默认 `false` / `120` | 首次进入 dashboard 无自动请求 | — |
| 4.2 执行体 | 仿 `handleQueryCurrentPageInfo`（`:354-413`），差异为 force 与静默 | 每轮请求带 `?force=true`，串行非并发 | — |
| 4.3 防重入 | running ref + `queryingInfo`/`verifying`/`batchRefreshing` 判空 | 30s 档下单轮超时不出现并发轮次 | — |
| 4.4 timer | `useEffect` 依赖仅 `[enabled, interval]`，执行体经 ref | 翻页/勾选后 tick 间隔仍稳定 | 若必须把 `currentCredentials` 入依赖 → 停，设计前提被推翻 |
| 4.5 不立即执行 | 不在 effect 内先调一次 | 开启到首次请求间隔 = 设定秒数 | — |
| 4.6 UI | 复用 `ui/switch`、`ui/select`、`ui/input` | 档位切换后 tick 间隔随之变化 | 若需新建 UI 组件 → 停，重估范围 |
| 4.7 输入校验 | 正整数且 ≥10，非法保留原值 | 0 / -5 / abc / 5 均被拒 | — |
| 4.8 静默策略 | 成功不 toast，轮末失败汇总一次 | 正常不刷 toast；断 key 后每轮一条 | — |
| 4.9 清理 | effect cleanup `clearInterval` | 关闭后无新请求；登出同样停止 | — |
| 4.10 空页静默 | 无启用凭据直接 return，不 toast | 全禁用时开启无提示无请求 | — |
| 5.1 构建 | `cd admin-ui && pnpm build` | 成功，无类型错误、无新增告警 | 失败即停，不得声称完成 |
| 5.2 校验 | `openspec validate --all` | 全通过 | — |
| 5.3 工作区 | `git status --short` | 改动仅两个前端文件 + 本 change 目录 | 出现 `src/admin/` 改动 → 停 |
| 5.4 报告 | 写明无前端测试框架、未手动验证项、剩余风险 | 无未运行即声称通过的表述 | — |

## 必跑验证

| 命令 | 归属 | 必须 |
| --- | --- | --- |
| `cd admin-ui && pnpm build` | AGENTS.md 高风险矩阵 admin-ui 项；含 `tsc -b` 类型检查 | 是 |
| `openspec validate --all` | AGENTS.md 高风险矩阵 OpenSpec 项 | 是 |
| `git status --short` | AGENTS.md 验证纪律（防密钥与 `.codegraph/` 误入） | 是 |
| `cargo test` | 后端未改动，不必跑 | 否（需在报告说明原因） |

手动核对项（无法自动化，须在最终报告逐项标注是否实际执行）：

- 验活请求带 `?force=true`
- 定时刷新 tick 间隔与防重入
- 单卡刷新后批量更新可见
- 非法间隔输入被拒
- 关闭/登出后停止

## README / AGENTS / spec 同步判断

| 目标 | 是否需要同步 | 理由 |
| --- | --- | --- |
| `README.md` | 否 | 不影响启动、构建、部署、测试或 API 入口；无新增命令、无新增配置项、无新增端点 |
| `AGENTS.md` | 否 | 不改 AI 纪律与验证命令；admin-ui 已在高风险矩阵中 |
| `spec/` | 否 | 无新增长期事实；余额缓存 TTL 与 force 语义为既有实现，未改变 |
| `openspec/specs/admin-ui-model-ops/` | 归档时同步 | 本 change 含 MODIFIED delta（卡片单一数据源约束） |
| `openspec/specs/admin-ui-balance-ops/` | 归档时新建 | 本 change 含 ADDED delta（验活口径、批量按钮表意、定时刷新） |
| `docs/tooling-sources.md` | 否 | 无新增工具来源 |

按 AGENTS.md「单次变更过程只写在 `openspec/changes/<name>/`」，实现期间不动 `openspec/specs/`，
由 openspec-archive-change 在归档时落地。

## 停止条件

- 工件缺失、互相矛盾或状态变为 blocked。
- 发现未写入规格的高风险影响，例如：批量查询之外还有第三处读缓存却按真实探测语义呈现结果的入口。
- `useEffect` 依赖必须包含 `currentCredentials` 才能正确工作 —— design 的 ref 方案前提被推翻，需重做设计。
- 定时刷新需要后端配合（如新增批量端点或调整 TTL）—— 越过「仅前端」范围边界。
- 定时刷新 UI 需要新建 `ui/` 组件 —— 重估范围。
- 工作区出现真实 `config.json`、`credentials.*`、token、Cookie 或 `.codegraph/` 待提交。
  当前状态：`config.json` 与 `credentials.json` 均不存在于工作区；
  `kiro_balance_cache.json`、`.codegraph/`、`admin-ui/dist/` 均已在 `.gitignore`；
  `git status --short` 现有改动为上一个 change（`src/openai/*`）的未提交内容，与本 change 无交集。
- `pnpm build` 失败且原因无法从本 change 改动中定位。
- 无法确定某项验证命令或剩余风险。

## 待更正项（实现时以本文件为准）

`tasks.md` 2.1 写「同步调整 import」，字面易被理解为删除 `RefreshCw`。
实测 `dashboard.tsx` 有三处使用 `RefreshCw`（`:592`、`:673`、`:705`），
**仅替换 `:705`，import 中 `RefreshCw` 必须保留，只新增 `Wallet`**。
