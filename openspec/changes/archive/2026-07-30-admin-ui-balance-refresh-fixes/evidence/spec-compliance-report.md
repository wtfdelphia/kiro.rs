# Spec Compliance Report: admin-ui-balance-refresh-fixes

日期：2026-07-29
审查类型：实现后 / 提交前合规（`spec-compliance-check`）
总体状态：**WARN**（非阻塞：前端行为仅静态核实，运行时未抓包）

> 说明：本次工作区同时存在三个 active change。三者文件归属互不重叠，本报告只审查
> `admin-ui/src/components/dashboard.tsx` 与 `admin-ui/src/components/credential-card.tsx`；
> `src/openai/**` 与 `src/kiro/**` 的改动归属另两个 change，不计入本 change 的越界。

## 六维表

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | **PASS** | 属本 change 的仅两个 tsx 文件（+136/-9 与 +8/-8，`git diff --numstat`），与 proposal Impact 一致。`src/admin/` 零改动（后端 force 语义已有），`admin-ui/package.json` 与 `pnpm-lock.yaml` 零改动（未引入新依赖）。`admin-ui/dist/` 被 `.gitignore:7` 覆盖，构建产物不入库。 |
| Design | **PASS** | `applyBalance`（`dashboard.tsx:142`）为 `balanceMap` 唯一写入口，三处调用（`:409` 手动批量 / `:459` 定时 / `:894` 单卡回调）共用。定时刷新默认关闭、默认 120s、下界 10s 与 design 一致。防重入 ref 复位在 `finally`。 |
| Scenarios | **PASS** | 23 个 Scenario（balance-ops 18 + model-ops 5）全部有代码对应，详见覆盖表。 |
| Project Rules | **PASS** | 属 AGENTS.md OpenSpec 条件「Admin API 或凭据管理」，已建 change，`evidence/bridge-plan.md`（231 行）齐备。高风险矩阵 admin-ui 要求的 `pnpm build` 已实跑。无真实凭据。 |
| Verification | **WARN** | `pnpm build`（含 `tsc -b`）与 `openspec validate` 本会话已复跑通过。tasks「验证状态」章节诚实列出 6 项未手动验证的运行时行为与无前端测试框架的事实——记录合规，但**定时器与防重入的运行时行为确实未被任何自动化验证覆盖**（见 WARN-1）。 |
| README/AGENTS Sync | **PASS** | 不改启动、构建、部署、API 入口。`README.md:179` 已有 `?force=true` 端点说明，本 change 只是前端改为调用它，无需改写。 |

## Requirement / Scenario 对照

### admin-ui-balance-ops（ADDED）

#### Requirement: 批量验活必须绕过余额缓存

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 每个凭据都带 force | `dashboard.tsx:545` `getCredentialBalance(id, true)`，在 `handleBatchVerify`（`:504`）的逐凭据循环内 | PASS（静态） |
| TTL 内失效的凭据不得报成功 | 同上 force 绕过 `BALANCE_CACHE_TTL_SECS`（`src/admin/service.rs:25` = 300）；失败落 `:558-568` `status:'failed'` + `extractErrorMessage` | PASS |
| 验活口径为额度查询 | `:555` 成功结果为 `${balance.currentUsage}/${balance.usageLimit}` | PASS |

#### Requirement: 批量余额查询入口不得暗示强制刷新

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 图标不与强制操作共用 | `:795` 改为 `Wallet`；`RefreshCw` 仍用于 `:681`、`:762`（未成孤儿 import，`:2` 保留） | PASS |
| 披露缓存语义 | `:794` title「可能返回最近 5 分钟内的缓存结果…」，与后端 `BALANCE_CACHE_TTL_SECS = 300` 一致（已核对） | PASS |
| 请求行为保持不变 | `:407` `getCredentialBalance(id)` 不带 force（`handleQueryCurrentPageInfo` 未改语义） | PASS |

#### Requirement: 定时批量余额刷新

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 默认关闭 | `autoRefreshEnabled` 初值 `false`；`:485` `if (!autoRefreshEnabled) return` 不建 timer | PASS |
| 每轮强制刷新 | `:458` `getCredentialBalance(id, true)` | PASS |
| 间隔可选与手动输入 | `AUTO_REFRESH_PRESETS = [30,60,120,180,300]`（`:34`）+ `ui/input` 草稿值 `autoRefreshInput` | PASS |
| 拒绝非法间隔 | `:495` `!Number.isInteger(parsed) \|\| parsed < 10` → toast.error + 回填原值。覆盖 0 / 负数 / 小数 / `abc`（`Number("abc")`→NaN）/ 空串（`Number("")`→0，被下界拦） | PASS |
| 防重入而非排队 | `:435` running ref 为真则 `return`（不排队）；`:446` 置位、`:470-472` **`finally` 内复位**——单轮抛异常不会永久卡死 | PASS |
| 手动批量操作优先 | `:437` `queryingInfo \|\| verifying \|\| batchRefreshing` 任一为真则跳过本轮 | PASS |
| 开启时不立即执行 | `:484-490` effect 内只 `setInterval`，无首次立即调用 | PASS |
| render 不重建 timer | `:490` 依赖数组仅 `[autoRefreshEnabled, autoRefreshInterval]`；执行体经 `autoRefreshTaskRef`（`:482` 每次 render 赋值）持有最新闭包，`:487` 调 `autoRefreshTaskRef.current()`——不会读到过期的 `currentCredentials` | PASS |
| 作用于当前页 | `:439-441` `currentCredentials.filter(c => !c.disabled)` | PASS |
| 静默成功，失败仅提示一次 | 成功路径无 toast；`:474-476` 仅当 `failCount > 0` 时一条 `toast.warning` | PASS |
| 关闭与卸载时停止 | `:489` `return () => clearInterval(timer)`；关闭时 `autoRefreshEnabled` 变化触发清理 | PASS |
| 当前页无启用凭据时静默 | `:444` `if (ids.length === 0) return`（在置位 ref 之前，不影响后续轮次） | PASS |

### admin-ui-model-ops（MODIFIED）

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 单卡刷新余额 | `credential-card.tsx:164` `handleRefreshBalance` 成功后回调 | PASS |
| 与批量查询信息互补 | 卡片展示改用 `balance` prop（`localBalance` / `displayBalance` 已删，grep 零残留） | PASS |
| 重置失败降级 | 既有逻辑未改（`credential-card.tsx` diff 仅 +8/-8） | PASS |
| 单卡刷新后批量结果仍可见 | 单一数据源：卡片不再持本地 state，批量写入 `balanceMap` 后卡片随之更新，不被遮蔽 | PASS |
| 单卡刷新结果回流父级 | `credential-card.tsx:39` prop 定义 → `:164` 调用 → `dashboard.tsx:894` `onBalanceRefreshed={applyBalance}`，链路完整 | PASS |

## 发现项

### WARN-1：定时器与防重入的运行时行为无自动化覆盖（MEDIUM）

- **事实**：`admin-ui/package.json` 无 test script、无 vitest/jest 依赖，项目当前无前端测试框架。
  timer 生命周期、防重入跳过、闭包新鲜度这三项只能靠代码审读确认，`tsc -b` 与 `vite build` 都覆盖不到。
  tasks「未手动验证」章节已诚实列出 6 项（force 抓包、tick 间隔、单卡→批量可见、非法输入反馈、
  关闭/登出停止、窄屏换行）。
- **影响**：逻辑经本次静态核实无缺陷（依赖数组正确、ref 持有最新闭包、`finally` 复位、清理函数存在），
  但回归保护为零——后续改动可能静默破坏 timer 行为。
- **建议**：不阻塞本 change。若后续继续扩展定时逻辑，建议引入 vitest + `@testing-library/react`
  并用 fake timers 覆盖「不立即执行 / 防重入 / 卸载清理」三条，属独立 change。

### INFO-1：新增控件的可访问性与项目既有约定一致（LOW）

- **事实**：新增的 `Input` 有 `aria-label="自定义刷新间隔（秒）"`（`dashboard.tsx` 定时刷新区），
  两个按钮有 `title`。但 `Switch`（`:806`）与 `Select`（`:810`）无 `aria-label`／`htmlFor` 关联，
  相邻的 `<span>定时刷新</span>`（`:805`）对屏幕阅读器不构成可编程标签
  （`ui/switch.tsx` 为 Radix `Switch.Root` 透传 props，本处未传）。
- **判定**：项目中全部 4 处 `Switch` 用法（`credential-card.tsx:219`、`dashboard.tsx:806`、
  `settings-panel.tsx:215`/`:256`）均无 `aria-label`，属既有约定而非本 change 引入的退步。
  按 AGENTS.md「Surgical Changes：不顺手重构」，不宜在本 change 统一修。
- **建议**：如需提升可访问性，作为独立的 admin-ui a11y change 一并处理 4 处。

### INFO-2：proposal 缺 `## Capabilities` 章节（LOW）

- **事实**：`proposal.md` 章节为 Why / What Changes / Non-Goals / Assumptions / Impact /
  Success Criteria / Risks，无 `## Capabilities`。归档样本
  （`archive/2026-07-24-admin-models-settings-optimization/proposal.md:13`）有该章节。
- **判定**：`openspec validate --all --strict` 20/20 通过，工具不强制。能力归属可从
  `specs/` 目录结构推断（`admin-ui-balance-ops` 为 ADDED 新能力、`admin-ui-model-ops` 为 MODIFIED）。
- **建议**：归档前补一节以对齐历史 change 的记录粒度。

## CRITICAL

无。

## 安全核查

- 无真实凭据、token、Cookie 进入 diff 或 change 文档。
- 未引入新 npm 依赖（`package.json` / `pnpm-lock.yaml` 零改动），无 typosquatting 风险。
- `git status --short` 16 条全部为预期文件，无 `config.json` / `credentials.*` / `.codegraph/`。
  本次 `pnpm build` 产物落在 `.gitignore` 覆盖的 `admin-ui/dist/`。
- **上游请求频率放大**（见剩余风险 1）：定时刷新每轮对每个启用凭据打一次 usage-limits。
  已用默认关闭 + 默认 120s + title 披露缓解。

## 验证记录（本会话真实运行）

```
$ cd admin-ui && pnpm build
$ tsc -b && vite build
vite v5.4.21 building for production...
✓ 1777 modules transformed.
dist/index.html                   0.48 kB │ gzip:   0.31 kB
dist/assets/index-CKpF_UAH.css   28.76 kB │ gzip:   5.76 kB
dist/assets/index-Cuqb0uRX.js   471.63 kB │ gzip: 149.16 kB
✓ built in 3.44s

$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)

$ cargo test --bin kiro-rs           # 后端未改，确认零回归
test result: ok. 570 passed; 0 failed
（含 admin::service::tests::get_balance_force_bypasses_cache）
```

两条 `pnpm build` WARN（`pnpm.onlyBuiltDependencies` 字段失效、caniuse-lite 数据陈旧）
为既有问题，与本 change 无关。

静态核实命令：

```
$ grep -n "localBalance|displayBalance" admin-ui/src/components/credential-card.tsx
（零命中，tasks 3.3 成立）

$ grep -n "RefreshCw" admin-ui/src/components/dashboard.tsx
2:import { RefreshCw, ... Wallet, Timer } from 'lucide-react'
681:<RefreshCw className="h-5 w-5" />
762:<RefreshCw className={...batchRefreshing...} />
（仍有使用，未成孤儿 import；tasks 2.1 成立）

$ grep -n "applyBalance|onBalanceRefreshed" admin-ui/src/components/dashboard.tsx
142: const applyBalance = ...        # 唯一写入口
409: applyBalance(id, balance)       # 手动批量
459: applyBalance(id, balance)       # 定时刷新
894: onBalanceRefreshed={applyBalance}  # 单卡回流
```

## 未手动验证（沿用 tasks，浏览器行为本会话未执行）

| 项 | 剩余风险 |
| --- | --- |
| 验活请求实际带 `?force=true` | 代码层已确认（`:545`），运行时未抓包 |
| 定时刷新 tick 间隔与防重入的运行时行为 | 逻辑经静态核实正确，无自动化回归保护（WARN-1） |
| 单卡刷新后批量更新可见 | 单一数据源改造已确认，未在浏览器复现 |
| 非法间隔输入被拒的交互反馈 | 校验逻辑已确认覆盖 0/负/小数/非数字/空串 |
| 关闭开关 / 登出后 timer 停止 | 清理函数存在（`:489`），未运行时验证 |
| 定时刷新控件窄屏换行 | 顶栏容器为 flex-wrap，未实测 |

## 证据路径

- Bridge：`openspec/changes/admin-ui-balance-refresh-fixes/evidence/bridge-plan.md`
- 本报告：`openspec/changes/admin-ui-balance-refresh-fixes/evidence/spec-compliance-report.md`
- OpenSpec 工件：`proposal.md` / `design.md` / `tasks.md` / `specs/**/spec.md`（tasks 全勾选）

## 剩余风险（可接受）

1. **上游请求频率放大**：30s 档 × 最多 12 凭据 ≈ 每分钟 24 次 usage-limits 请求。
   默认关闭 + 默认 120s + title 披露已缓解，但上游速率容忍度仍是假设。
2. **后端落盘 I/O 放大**（已知，本 change 不修）：每次 force 成功都持锁序列化整个缓存并同步写盘
   （`src/admin/service.rs:272-283`），定时刷新放大该频率。
3. **验活整体变慢**：force 必然打上游，失去缓存快路径。
4. 前端无测试框架，定时逻辑零回归保护（WARN-1）。

## 结论

**WARN（可继续归档评审）。** 23 个 Scenario 全部有代码对应，
`balanceMap` 单一写入口、`finally` 防重入、ref 持有最新闭包、timer 清理这四个易错点经逐项静态核实均正确，
「5 分钟」文案与后端 TTL(300s) 一致。改动范围严格限于两个声明文件，未引入依赖。
WARN 源于前端无测试框架导致定时器行为缺自动化覆盖（tasks 已诚实声明），非规格缺失。
无 CRITICAL、无范围越界、无密钥入仓风险。

建议下一步：`openspec-verify-change` → `verification-before-completion`；
归档前可补 proposal 的 `## Capabilities` 章节（INFO-2）。
