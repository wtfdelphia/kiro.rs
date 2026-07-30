# Verification Before Completion: admin-ui-balance-refresh-fixes

日期：2026-07-29
门禁：最终回复 / 归档前验证（`verification-before-completion`）
结论：**通过（可归档）** — 关键验证在本会话真实运行；前端运行时行为为 SKIPPED，已逐项写明风险

> 本次工作区同时存在三个 active change，三者文件归属互不重叠。
> 本报告的验证覆盖整个工作区（测试与构建无法按 change 切分），
> 但范围判定与文档同步只针对本 change 的
> `admin-ui/src/components/dashboard.tsx` 与 `credential-card.tsx`。

## Verification 列表

全部命令在本会话真实执行，输出为实际粘贴。AGENTS.md 高风险矩阵对
`admin-ui` 要求 `pnpm build`（及已有测试）：

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `cd admin-ui && pnpm build` | `tsc -b && vite build` → `✓ 1777 modules transformed` / `✓ built in 1m`；产物 `index-Cuqb0uRX.js` 471.63 kB | ✅ 通过（含 `tsc -b` 类型检查） |
| `cargo test --bin kiro-rs` | `570 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` | ✅ 后端未改，零回归（含 `admin::service::tests::get_balance_force_bypasses_cache`） |
| `cargo build` | `10 warnings` / `Finished dev profile` | ✅ 警告零增量 |
| `openspec validate --all --strict` | `Totals: 20 passed, 0 failed (20 items)` | ✅ 通过 |
| `openspec status --change <name> --json` | `isComplete: true`，proposal/design/specs/tasks 全 `done` | ✅ 工件完整 |
| `openspec list` | 本 change `✓ Complete`（24/24 tasks） | ✅ 无未勾选任务 |
| `git status --short` | 16 条，全部为预期文件 | ✅ 无敏感文件 |
| `git check-ignore -v` | `config.json`(.gitignore:2) / `credentials.*`(:9) / `.codegraph/`(:14) / `admin-ui/dist/`(:7) 均被忽略 | ✅ 构建产物不入库 |

两条 `pnpm build` WARN（`pnpm.onlyBuiltDependencies` 字段失效、caniuse-lite 数据陈旧）
为既有问题，与本 change 无关，不隐藏。

静态核实（代码层，非运行时）：

```
$ grep -n "localBalance|displayBalance" admin-ui/src/components/credential-card.tsx
（零命中 —— tasks 3.3 的删除已完成）

$ grep -n "RefreshCw" admin-ui/src/components/dashboard.tsx
2:import { RefreshCw, ... Wallet, Timer } from 'lucide-react'
681, 762  → 仍有两处使用，未成孤儿 import（tasks 2.1 成立）

$ grep -n "applyBalance|onBalanceRefreshed" admin-ui/src/components/dashboard.tsx
142  applyBalance 定义（balanceMap 唯一写入口）
409  手动批量查询   459  定时刷新   894  onBalanceRefreshed={applyBalance}
（三处调用共用单一写入口，回流链路完整）

$ git diff --stat -- admin-ui/package.json admin-ui/pnpm-lock.yaml
（空 —— 未引入新 npm 依赖，无 typosquatting 风险）

$ git diff --stat -- src/admin/
（空 —— 后端未改，force 语义已有）
```

四个易错点逐项核实（这些是前端定时逻辑最容易写错的地方）：

| 点 | 位置 | 核实结果 |
| --- | --- | --- |
| 防重入 ref 复位在 `finally` | `dashboard.tsx:446` 置位 / `:470-472` 复位 | ✅ 单轮抛异常不会永久卡死定时刷新 |
| `useEffect` 依赖仅两项 + ref 持有最新闭包 | `:482` 每次 render 赋值 `autoRefreshTaskRef.current` / `:487` 调用 / `:490` 依赖 `[autoRefreshEnabled, autoRefreshInterval]` | ✅ 不会读到过期的 `currentCredentials`，翻页不重建 timer |
| 清理函数存在 | `:489` `return () => clearInterval(timer)` | ✅ 关闭开关与卸载均触发 |
| 请求串行非并发 | `:450-469` `for` + `await`，非 `Promise.all` | ✅ tasks 4.2 成立 |

前后端耦合数字核对（这类最容易失同步）：
UI 文案「可能返回最近 5 分钟内的缓存结果」（`:794` title）与后端
`BALANCE_CACHE_TTL_SECS = 300`（`src/admin/service.rs:25`）**一致**。

输入校验覆盖核实：`:495` `!Number.isInteger(parsed) || parsed < AUTO_REFRESH_MIN_SECS`
覆盖 0 / 负数 / 小数 / `abc`（`Number("abc")`→NaN）/ 空串（`Number("")`→0，被下界拦）。

## SKIPPED（未运行的验证）

| 项 | SKIPPED 原因 | 剩余风险 |
| --- | --- | --- |
| **前端单元测试** | `admin-ui/package.json` **无 test script、无 vitest/jest 依赖**，项目当前无前端测试框架 | **中。定时器生命周期、防重入、闭包新鲜度三项回归保护为零。** 逻辑经本轮静态核实无缺陷（见上表四个易错点），但后续改动可能静默破坏。建议后续以独立 change 引入 vitest + fake timers 覆盖「不立即执行 / 防重入 / 卸载清理」 |
| 验活请求实际带 `?force=true` | 需浏览器 DevTools 抓包，本会话无浏览器环境 | 低。代码层已确认（`dashboard.tsx:545`），后端 force 语义由 `get_balance_force_bypasses_cache` 覆盖 |
| 定时刷新 tick 间隔与防重入的运行时行为 | 同上，需浏览器与计时观察 | 中（与第 1 项同源）。依赖数组、ref 闭包、`finally` 复位均已静态核实正确 |
| 单卡刷新后批量结果可见 | 需浏览器交互 | 低。单一数据源改造已确认（`localBalance` 零残留 + `applyBalance` 唯一写入口） |
| 非法间隔输入被拒的交互反馈 | 需浏览器交互 | 低。校验逻辑已确认覆盖全部非法输入形态 |
| 关闭开关 / 登出后 timer 停止 | 需浏览器与 Network 面板观察 | 低。清理函数存在（`:489`） |
| 定时刷新控件在窄屏下的换行表现 | 需浏览器多分辨率实测 | 低。顶栏容器为 `flex-wrap`，最坏情况为布局不美观，非功能缺陷 |
| `cargo test` 针对本 change 的专项测试 | 后端 `src/admin/` 未改动，force 语义已有既有测试覆盖 | 低 |

## Documentation Sync 表

| 文档 | 是否需要同步 | 理由 |
| --- | --- | --- |
| `README.md` | **否** | 不改启动、构建、部署、API 入口。`:179` 已列 `GET /api/admin/credentials/{id}/balance?force=true`「强制刷新余额（跳过 TTL 缓存）」——本 change 只是前端改为调用既有端点，未新增或变更后端接口 |
| `AGENTS.md` | **否** | 未改变 AI 协作纪律、OpenSpec 条件、高风险矩阵或验证命令。admin-ui 的 `pnpm build` 要求已在矩阵中 |
| `CLAUDE.md` | **否** | 规则入口未变 |
| `spec/requirements.md` / `spec/design.md` / `spec/structure.md` | **否** | 未改变模块划分或长期架构事实；改动限于两个既有组件内部 |
| `openspec/specs/admin-ui-balance-ops/spec.md` | **待归档时创建** | 本 change 的 `admin-ui-balance-ops` 为 **ADDED 新能力**（长期 `openspec/specs/` 下当前无该目录），归档时由 `openspec-archive-change` / `openspec-sync-specs` 创建 |
| `openspec/specs/admin-ui-model-ops/spec.md` | **待归档时同步** | 含 1 个 MODIFIED Requirement（5 Scenario），归档时合并 |
| `docs/tooling-sources.md` | **否** | 未引入新工具或依赖（`Wallet` / `Timer` 来自既有 `lucide-react`；`ui/switch`、`ui/select`、`ui/input` 均为既有组件） |
| `config.example.json` | **否** | 定时刷新间隔为前端会话内 state，非后端配置项，不落盘 |

## 安全核查

- `git status --short` 16 条全部为预期文件；`--untracked-files=all` 全量展开后无意外条目。
- `config.json` / `credentials.*` / `.codegraph/` 经 `git check-ignore -v` 确认被忽略。
- 本会话 `pnpm build` 产物落在 `admin-ui/dist/`，被 `.gitignore:7` 覆盖，不会进入候选提交。
- 无真实凭据、token、Cookie 进入 diff 或 change 文档。
- **未引入新 npm 依赖**（`package.json` 与 `pnpm-lock.yaml` 零改动），无 typosquatting 风险。
- **上游请求频率放大属已知影响**（见 Residual Risk 1）：定时刷新每轮对每个启用凭据打一次
  usage-limits 请求。已用默认关闭 + 默认 120s + title 披露三重缓解。

## Residual Risk

| 风险 | 说明 |
| --- | --- |
| **未 archive** | 本 change 尚未执行 `openspec-archive-change`；`admin-ui-balance-ops` 作为新能力尚未写入长期 spec |
| **未 commit / push / PR** | 改动仍在工作区未提交状态（当前分支 `dev`，主分支 `master`）。本会话未执行任何 git 提交、推送或 PR 操作 |
| **三个 change 共存于同一工作区** | 提交时需按 change 分离 staging（本 change 对应两个 `admin-ui/src/components/*.tsx`），否则三个变更会混入同一 commit |
| **定时逻辑零回归保护** | 前端无测试框架（SKIPPED 第 1 项），是本 change 最主要的未验证面 |
| **上游请求频率放大** | 30s 档 × 最多 12 凭据 ≈ 每分钟 24 次 usage-limits 请求。上游速率容忍度仍是**假设**，未实测。已用默认关闭 + 默认 120s + title 文案缓解 |
| **后端落盘 I/O 放大** | 已知且本 change 不修：每次 force 成功都持锁序列化整个缓存并同步写盘（`src/admin/service.rs:272-283`），定时刷新会放大该频率 |
| **验活整体变慢** | force 必然打上游，失去缓存快路径。这是「验活结果可信」的必要代价 |
| **可访问性** | 新增 `Switch`/`Select` 无 `aria-label`；但项目全部 4 处 `Switch` 用法均如此，属既有约定，按「Surgical Changes」未在本 change 统一修。建议作为独立 a11y change 处理 |
| **工具限制** | 无浏览器环境，全部运行时行为依赖代码审读；前端无 coverage 工具 |

## 结论

**通过，可归档。** 本会话真实运行的 8 类验证全部通过：`pnpm build`（含 `tsc -b` 类型检查）
1777 modules 成功、`cargo test` 570 passed / 0 failed（后端零回归）、`cargo build` 警告零增量、
`openspec validate` 20/20、工件 `isComplete: true`、tasks 24/24、`git status` 无敏感文件。

四个前端易错点（`finally` 防重入、ref 持有最新闭包、依赖数组、timer 清理）逐项静态核实正确，
UI 文案「5 分钟」与后端 `BALANCE_CACHE_TTL_SECS = 300` 核对一致。

8 项 SKIPPED 已逐条写明原因与剩余风险。其中「前端无测试框架导致定时器行为零回归保护」
是本 change 最主要的未验证面，已如实标为中等风险——**逻辑正确性经审读确认，但不等同于运行时验证过，
这一点不写成通过**。

不存在被隐藏的失败（两条 `pnpm build` WARN 为既有问题，已如实列出）。

下一步：`openspec-archive-change`。归档前可补 proposal 的 `## Capabilities` 章节。
提交时注意按 change 分离 staging。
