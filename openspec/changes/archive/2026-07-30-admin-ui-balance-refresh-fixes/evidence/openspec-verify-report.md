# OpenSpec Verify Report: admin-ui-balance-refresh-fixes

日期：2026-07-29
审查类型：归档前工件与证据核验（`openspec-verify-change`）
总体状态：**WARN**（可归档；非阻塞缺口：前端无测试框架，定时器行为缺自动化覆盖）

> 本次工作区同时存在三个 active change，三者文件归属互不重叠。本报告只核验本 change
> 的工件、tasks 与 evidence；`src/openai/**` 与 `src/kiro/**` 归属另两个 change。

## 三维结论

| 维度 | 状态 | 结论 |
| --- | --- | --- |
| Completeness | **PASS** | `openspec status --json` 四类工件全部 `status: "done"`，`isComplete: true`。tasks **24/24 勾选，0 未勾**。evidence：`bridge-plan.md`（231 行）、`spec-compliance-report.md`。4 个 Requirement 各有 Scenario（共 23 个），无空 Requirement。 |
| Correctness | **WARN** | 23 个 Scenario 全部有代码对应，四个易错点（`balanceMap` 单一写入口、`finally` 防重入、ref 持有最新闭包、timer 清理）经逐项静态核实均正确。但**运行时行为无任何自动化验证**——`tsc -b` 与 `vite build` 覆盖不到 timer 生命周期与防重入。 |
| Coherence | **PASS** | design / tasks / spec 一致。UI 文案「5 分钟」与后端 `BALANCE_CACHE_TTL_SECS = 300`（`src/admin/service.rs:25`）核对一致——这类前后端耦合的数字最容易失同步，已专门查过。README `:179` 已有 `?force=true` 端点说明，无需改写。 |

## 工件完整性（openspec status）

```
$ openspec status --change admin-ui-balance-refresh-fixes --json
"isComplete": true
artifacts: proposal(done) / design(done) / specs(done) / tasks(done)
specs 实存：specs/admin-ui-balance-ops/spec.md
            specs/admin-ui-model-ops/spec.md
nextSteps: All planning artifacts are complete
```

```
$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)
  ✓ change/admin-ui-balance-refresh-fixes
```

## tasks 勾选项的证据支撑

| 任务 | 声称 | 本会话实跑核验 |
| --- | --- | --- |
| 1.1（批量验活带 force） | `getCredentialBalance(id, true)` | ✅ `dashboard.tsx:545`（在 `handleBatchVerify` 循环内），注释说明了理由 |
| 1.2（失败分支不变） | 仍呈现 failed + 错误文案 | ✅ `:558-568` `status:'failed'` + `extractErrorMessage` |
| 2.1（Wallet 图标 + 保留 RefreshCw） | `:592`/`:673` 仍在用 | ✅ `RefreshCw` 现存 `:681`/`:762` 两处使用，未成孤儿 import；`Wallet` 在 `:795` |
| 2.2（title 披露缓存语义） | 说明最长 5 分钟缓存 | ✅ `:794` 文案与后端 TTL(300s) 一致 |
| 2.3（手动批量请求行为不变） | 仍不带 force | ✅ `:407` `getCredentialBalance(id)` 无第二参 |
| 3.1（CredentialCard 唯一调用点） | 仅定义与 `:760` | ✅ 现为 `:894`（行号随改动位移），grep 确认唯一 |
| 3.3（localBalance/displayBalance 零残留） | 文件内无残留 | ✅ grep 零命中 |
| 3.4/3.5（回流链路） | 单卡 → 父级 `applyBalance` | ✅ `credential-card.tsx:39` prop → `:164` 调用 → `dashboard.tsx:894` 绑定；`applyBalance`（`:142`）为 `balanceMap` 唯一写入口，三处调用共用 |
| 4.1（默认关闭 / 120s / 下界 10s） | 常量提模块级 | ✅ `:32-34` `AUTO_REFRESH_DEFAULT_SECS=120` / `MIN_SECS=10` / `PRESETS=[30,60,120,180,300]` |
| 4.2（逐个串行 force） | `for` + `await` 非并发 | ✅ `:450-469` 串行循环，`:458` 带 force |
| 4.3（防重入） | running ref + 三个手动操作标志 | ✅ `:435` ref 短路、`:437` `queryingInfo\|\|verifying\|\|batchRefreshing`；**`:470-472` 复位在 `finally`**（单轮抛异常不会永久卡死——这是最关键的一点） |
| 4.4（依赖仅两项 + ref 持有闭包） | 避免 render 重建 timer | ✅ `:490` 依赖 `[autoRefreshEnabled, autoRefreshInterval]`；`:482` 每次 render 赋值 `autoRefreshTaskRef.current`，`:487` 调用它 |
| 4.5（开启不立即执行） | 无首轮立即调用 | ✅ `:484-490` effect 内只 `setInterval` |
| 4.7（输入校验） | 正整数且 ≥10 | ✅ `:495` `!Number.isInteger(parsed) \|\| parsed < 10`，覆盖 0 / 负数 / 小数 / `abc`(NaN) / 空串(`Number("")`→0 被下界拦) |
| 4.8（静默策略） | 成功不弹，失败一次 | ✅ 成功路径无 toast；`:474-476` 仅 `failCount > 0` 时一条 |
| 4.9（清理 interval） | 关闭或卸载时 | ✅ `:489` `return () => clearInterval(timer)` |
| 4.10（无启用凭据静默） | 不弹 error | ✅ `:444` `if (ids.length === 0) return`，位于置位 ref 之前 |
| 5.1（pnpm build） | tsc -b && vite build 成功 | ✅ 复跑：1777 modules，通过 |
| 5.2（validate） | 19 passed | ✅ 现 20 passed（另两 change 各增一项） |
| 5.3（改动范围） | 仅两个 tsx | ✅ `git diff --numstat` 确认；`package.json`/`pnpm-lock.yaml` 零改动 |
| 5.4（诚实声明未验证项） | 列出 6 项 | ✅ tasks「验证状态」章节如实列出 |

## Requirement / Scenario 完整性

| Requirement | Scenario 数 | 对应 |
| --- | --- | --- |
| 批量验活必须绕过余额缓存（ADDED） | 3 | 全覆盖（静态） |
| 批量余额查询入口不得暗示强制刷新（ADDED） | 3 | 全覆盖 |
| 定时批量余额刷新（ADDED） | 12 | 全覆盖（静态） |
| 凭据卡片提供余额强制刷新入口（MODIFIED） | 5 | 全覆盖 |

无空 Requirement。逐条对照表见 `spec-compliance-report.md`。

`admin-ui-balance-ops` 为新增能力，spec 头部正确标注 `## ADDED Requirements`
（长期 `openspec/specs/` 下无该能力目录，validate 通过佐证标注无误）。

## proposal / design 与实际改动的一致性

范围核实：改动仅 `dashboard.tsx`（+136/-9）与 `credential-card.tsx`（+8/-8）。
`src/admin/` 零改动（后端 force 语义已有，由 `admin::service::tests::get_balance_force_bypasses_cache` 覆盖）。
未引入新 npm 依赖，无 typosquatting 风险。`admin-ui/dist/` 被 `.gitignore:7` 覆盖，构建产物不入库。

风险声明与实际一致：proposal Risks 的「上游请求频率放大」已用默认关闭 + 默认 120s +
title 披露三重缓解；「后端落盘 I/O 放大」如实标注为已知且本 change 不修。

## evidence 核验

| 证据 | 状态 | 核验 |
| --- | --- | --- |
| `bridge-plan.md` | ✅ 231 行 | Skills 门禁「开始实现前」产物；其「补盲 2」提示的 `RefreshCw` 保留已在 2.1 落实 |
| `spec-compliance-report.md` | ✅ | 本轮上游门禁，状态 WARN（同源缺口） |
| 敏感数据 | ✅ 零命中 | 无真实凭据、token、Cookie |

只记录真实命令：tasks「验证状态」章节区分了「已运行」「未运行及原因」「未手动验证」三类，
未把静态核实说成运行时验证——这一点符合验证纪律。

## 失败项

无。

## WARN-1：定时器与防重入的运行时行为无自动化覆盖（MEDIUM，不阻塞归档）

- **事实**：`admin-ui/package.json` 无 test script、无 vitest/jest 依赖，项目当前无前端测试框架。
  timer 生命周期、防重入跳过、闭包新鲜度三项只能靠代码审读确认。
  tasks 已诚实列出 6 项未手动验证的浏览器行为（force 抓包、tick 间隔、单卡→批量可见、
  非法输入反馈、关闭/登出停止、窄屏换行）。
- **判定**：不阻塞归档。逻辑经本轮静态核实无缺陷，四个易错点（依赖数组、ref 闭包、
  `finally` 复位、清理函数）全部正确。风险在于**回归保护为零**：后续改动可能静默破坏 timer 行为。
- **建议**：若继续扩展定时逻辑，引入 vitest + `@testing-library/react`，用 fake timers 覆盖
  「不立即执行 / 防重入 / 卸载清理」三条。属独立 change，不应塞进本 change。

## INFO-1：proposal 缺 `## Capabilities` 章节（LOW）

`proposal.md` 章节为 Why / What Changes / Non-Goals / Assumptions / Impact / Success Criteria / Risks。
归档样本（`archive/2026-07-24-admin-models-settings-optimization/proposal.md:13`）有 `## Capabilities`。
`openspec validate --strict` 20/20 通过说明工具不强制，能力归属可从 `specs/` 目录结构推断
（`admin-ui-balance-ops` ADDED / `admin-ui-model-ops` MODIFIED）。归档前补一节可对齐历史粒度。

## INFO-2：新增控件的可访问性与项目既有约定一致（LOW）

新增 `Input` 有 `aria-label`，两个按钮有 `title`；`Switch`（`:806`）与 `Select`（`:810`）无
`aria-label`／`htmlFor` 关联。但项目中全部 4 处 `Switch` 用法（`credential-card.tsx:219`、
`dashboard.tsx:806`、`settings-panel.tsx:215`/`:256`）均如此，属既有约定而非本 change 引入的退步。
按 AGENTS.md「Surgical Changes」不宜在本 change 统一修；建议作为独立 a11y change 一并处理。

## 剩余风险（可接受，与 compliance 报告一致）

1. **上游请求频率放大**：30s 档 × 最多 12 凭据 ≈ 每分钟 24 次 usage-limits 请求。
   已用默认关闭 + 默认 120s + title 披露缓解，但上游速率容忍度仍是假设。
2. **后端落盘 I/O 放大**（已知，本 change 不修）：每次 force 成功都持锁序列化整个缓存并同步写盘
   （`src/admin/service.rs:272-283`），定时刷新放大该频率。
3. **验活整体变慢**：force 必然打上游，失去缓存快路径。
4. **定时逻辑零回归保护**（WARN-1）。

## 验证记录（本会话真实运行）

```
$ openspec status --change admin-ui-balance-refresh-fixes --json
"isComplete": true；proposal/design/specs/tasks 全部 done

$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)

$ cd admin-ui && pnpm build
$ tsc -b && vite build
✓ 1777 modules transformed.
dist/assets/index-Cuqb0uRX.js   471.63 kB │ gzip: 149.16 kB
✓ built in 3.44s

$ cargo test --bin kiro-rs          # 后端未改，确认零回归
test result: ok. 570 passed; 0 failed
（含 admin::service::tests::get_balance_force_bypasses_cache）

$ grep -c "^- \[x\]" tasks.md → 24 ；grep -c "^- \[ \]" tasks.md → 0
```

两条 `pnpm build` WARN（`pnpm.onlyBuiltDependencies` 字段失效、caniuse-lite 数据陈旧）
为既有问题，与本 change 无关。

## 证据路径

- Bridge：`openspec/changes/admin-ui-balance-refresh-fixes/evidence/bridge-plan.md`
- Compliance：`openspec/changes/admin-ui-balance-refresh-fixes/evidence/spec-compliance-report.md`
- 本报告：`openspec/changes/admin-ui-balance-refresh-fixes/evidence/openspec-verify-report.md`
- Completion：待 `verification-before-completion` 产出

## 结论

**WARN，可归档。** 四类工件完整（`isComplete: true`），tasks 24/24 且每个勾选项都有
代码位置或构建结果支撑，23 个 Scenario 全覆盖，validate 20/20。
四个易错点经逐项静态核实均正确，UI 文案与后端 TTL 一致。
WARN 源于前端无测试框架导致定时器行为缺自动化覆盖（tasks 已诚实声明），
属工具链缺口而非规格或实现缺陷，不构成归档阻塞。
无失败项、无范围越界、无密钥入仓风险、未引入新依赖。

建议下一步：`verification-before-completion`，随后 `openspec-archive-change`。
归档前可补 proposal 的 `## Capabilities` 章节（INFO-1）。
