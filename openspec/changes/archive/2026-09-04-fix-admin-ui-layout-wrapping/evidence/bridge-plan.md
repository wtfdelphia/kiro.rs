# Bridge Plan：fix-admin-ui-layout-wrapping

日期：2026-08-26　分支：`dev`　状态：可开始实现

`openspec status --change fix-admin-ui-layout-wrapping --json`：`isPlanningComplete: true`，四个 artifact 全 `done`，无 blocked。`openspec validate fix-admin-ui-layout-wrapping --strict` 通过。

## 范围

五项 className 层面的改动，全部落在 `admin-ui/src/`：

1. 「凭据管理」标题行两层容器加折行（`dashboard.tsx:777` 与其右侧容器）
2. 顶栏两层容器加折行，`h-14` 改 `min-h-14`（`dashboard.tsx:702`）
3. `DialogContent` 基类加高度上限与滚动通路，`w-full` 改 `w-[calc(100%-2rem)]`（`ui/dialog.tsx:38`），4 个 `flex flex-col` 弹窗加 `overflow-y-visible` 抵消
4. 三处不可断长文本加 `break-words`
5. M1 统计行加 `flex-wrap`（`kam-import-dialog.tsx:304`），M6 超时网格改 `grid-cols-1 sm:grid-cols-2`（`settings-panel.tsx:352`）

## 非目标

- 不自定义 `.container` 宽度阶梯（会改全站排版）
- 不引入 Playwright 或 vitest browser mode
- 不做 320px 专项适配。M1 与 M6 原按「只在 320px 触发」排除，复测推翻：M1 改前要 503px 视口才单行，M6 要 471px，两者在 375px 都折行，故纳入范围。M7 排除的理由改为「`sm:max-w-md` 在 640px 处把弹窗收窄到 448px、列宽掉到 191px，改列数救不了这段，得连弹窗宽度一起定」；M6 剩余四处 `grid-cols-2` 排除的理由分两类，三处格内只有 `Input`（`w-full`，无文本节点，不折行），`settings-panel:335` 是说明文本够短（`text-xs` 下 123.3px，全档单行）
- 不做溢出菜单收纳（设计文档 2.1，与本次是叠加关系，单独推进）
- 不做定时刷新限并发与开关持久化（设计文档第 5 节）

## 关键设计决策

`max-h` 与滚动通路必须同批落地。基类是 `grid` 且 `overflow: visible`，只加 `max-h` 时钳制的是边框盒而 grid 行仍按内容 auto 撑开，`translate-y-[-50%]` 参照钳制后的边框盒使内容整体下移。667px 视口 812px 内容：改前上下各溢出 72.5px，只加 `max-h-[90vh]` 后底边到 845.5px，超视口 178.5px，footer 比改前更远。

`cn()` 是 `twMerge(clsx(...))`，基类新增的类按 group 被调用方覆盖或叠加。选「基类兜底 + 4 处显式抵消」而非「基类只加 max-h + 5 处各自补 overflow」，理由是让安全那一侧成为默认，新增弹窗忘写不会复现同一 bug。

## 高风险项

| 风险 | 判定 | 处置 |
| --- | --- | --- |
| 基类改动覆盖 12 个 `<DialogContent>`，twMerge 覆盖判定出错会静默产生双滚动条 | 已用真实 `twMerge` 逐个验证，见下方证据 | 抵消类写进测试断言 |
| className 断言守不住真实布局（类名存在但被更高优先级规则压过） | 确实守不住 | 落地后用 headless Chrome 加载构建产物核一次实测几何，一次性验收不进 CI |
| `min-h-14` 让 sticky 顶栏在窄屏变高，推动下方内容 | 只在 437px 以下发生（实测阈值，右侧控件组恒宽 336px） | 接受，从「按钮点不到」换成「按钮占两行」 |
| 断言「不含 `h-14`」用子串或正则会把 `min-h-14` 判为命中，正确实现被判失败 | 已实测：`includes('h-14')` 与 `/\bh-14\b/` 均为真，只有 `split(' ').includes('h-14')` 为假 | 判据写死为按空格切分比对 token，已写入 tasks 2.3 与 spec |
| 基类改 `w-[calc(100%-2rem)]` 使弹窗内宽少 32px，把 M1 的单行阈值往上推 | 落地后用新产物逐 1px 复扫：改前要 503px 视口才单行，375px 处每项 60px 高、320px 处 80px 高。此前记的 453px 与 476px 都是按 4 项无符号文案估的，真实文案是 5 项带前缀符号 | M1 的 `flex-wrap` 必须同批落地，让折行成为受控回流而非行内挤压 |
| Rust 侧 `RustEmbed` 嵌入 `admin-ui/dist`，前端改动需重新 build 才生效 | `src/admin_ui/router.rs:14` 确认 `#[folder = "admin-ui/dist"]` | 验收前跑 `pnpm build` |

## CodeGraph 证据

`codegraph status`：161 文件 / 3290 节点 / 10242 边，含 15 个 component 节点，索引覆盖 `admin-ui/src`。

`codegraph query "DialogContent"`：定位到 `admin-ui/src/components/ui/dialog.tsx:29`，是 `component` 节点。

`codegraph impact "DialogContent"`：26 个受影响符号，跨 12 个文件。组件清单与 `rg` 结果一致：`add-credential-dialog`、`balance-dialog`、`batch-import-dialog`、`batch-verify-dialog`、`credential-card`、`credential-models-dialog`、`credential-test-dialog`、`kam-import-dialog`、`models-refresh-result-dialog`、`online-auth-dialog`、`public-api-panel`、`settings-panel`，另含 `dashboard.tsx` 与 `Dashboard`（经子组件间接受影响，`dashboard.tsx` 自身不渲染 `DialogContent`，`rg -c` 命中 0）。

`codegraph callers "DialogContent"`：20 项（10 个 function + 10 个 file）。`settings-panel.tsx` 只以 file 节点出现、无对应 function 节点，是 CodeGraph 的解析盲点，实际调用点在 `settings-panel.tsx:183`，已由 `rg` 补上。

结论：`DialogContent` 的影响面就是这 12 处，没有 CodeGraph 之外的渲染点。

## rg 与源码补盲

`rg -n "<DialogContent" admin-ui/src` 命中 12 处，与 `codegraph impact` 的文件集合完全对应。逐处 className：

| 位置 | 现有 className | 分组 |
| --- | --- | --- |
| `settings-panel.tsx:183` | `sm:max-w-lg max-h-[90vh] overflow-y-auto` | 已有 max-h + 自身滚动 |
| `public-api-panel.tsx:91` | `sm:max-w-3xl max-h-[90vh] overflow-y-auto` | 同上 |
| `add-credential-dialog.tsx:111` | `sm:max-w-lg max-h-[85vh] flex flex-col` | 需抵消 |
| `online-auth-dialog.tsx:236` | `sm:max-w-lg max-h-[85vh] flex flex-col` | 需抵消 |
| `kam-import-dialog.tsx:240` | `sm:max-w-2xl max-h-[80vh] flex flex-col` | 需抵消 |
| `batch-import-dialog.tsx:313` | `sm:max-w-2xl max-h-[85vh] flex flex-col` | 需抵消 |
| `batch-verify-dialog.tsx:40` | `sm:max-w-lg` | 吃基类 |
| `models-refresh-result-dialog.tsx:27` | `sm:max-w-lg` | 吃基类 |
| `credential-models-dialog.tsx:94` | `sm:max-w-lg` | 吃基类 |
| `credential-test-dialog.tsx:131` | `sm:max-w-md` | 吃基类 |
| `balance-dialog.tsx:31` | `sm:max-w-md` | 吃基类 |
| `credential-card.tsx:577` | 无 className | 吃基类 |

`ls admin-ui/src/components/ui/` 只有 10 个原语，无 `AlertDialog` / `SheetContent` / `PopoverContent`（`rg -l` 命中 0），所以弹窗类改动只需覆盖 `dialog.tsx` 一处。

`rg -n "max-h-|overflow"` 确认三个 `break-words` 目标的父级滚动容器：`batch-verify-dialog.tsx:74` 是 `max-h-[400px] overflow-y-auto`，`kam-import-dialog.tsx:322` 是 `max-h-[300px] overflow-y-auto`，`kam-import-dialog.tsx:245` 是外层 `flex-1 overflow-y-auto`。父级 `overflow-y-auto` 使横向溢出计算为 `auto`，这是横向滚动条的成因。

`break-words` 目标共三处（比 tasks 原列的两处多一处，实现时按此清单）：

- `batch-verify-dialog.tsx:105-107` 的 `result.error` 容器（`text-xs mt-1 opacity-90`）
- `kam-import-dialog.tsx:277` 预览行含 `item.path`，容器 className 在 `:271-275` 的三元里
- `kam-import-dialog.tsx:346` 结果区的 `result.error`（`text-xs text-red-600 dark:text-red-400 mt-1`），与 `:277` 同属长不可断文本，父级同样是 `overflow-y-auto`

`rg -n "admin-ui|dist|RustEmbed" src/admin_ui/` 命中 `router.rs:14` 的 `#[folder = "admin-ui/dist"]`，确认前端改动需 `pnpm build` 才进二进制。Rust 源码不需要改。

`git ls-files | grep -iE "config\.json$|credentials"` 只命中 `credentials.example.*.json` 五份与源码文件名，`.gitignore:2-11` 覆盖 `/config.json`、`/credentials.json`、`/credentials.*` 并对 example 做例外，`:16` 忽略 `.codegraph/`。工作区无待提交的真实凭据。

## twMerge 覆盖行为实测

用仓库依赖的真实 `twMerge` + `clsx` 跑了 12 处的最终 className（脚本临时放在 `admin-ui/` 内以解析依赖，跑完已删）。新基类取 `... grid w-[calc(100%-2rem)] max-w-lg max-h-[90vh] overflow-y-auto ...`：

```
settings-panel:183     grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
public-api-panel:91    grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
add-credential:111     flex flex-col overflow-y-auto max-h-[85vh] w-[calc(100%-2rem)]   ← 需抵消
online-auth:236        flex flex-col overflow-y-auto max-h-[85vh] w-[calc(100%-2rem)]   ← 需抵消
kam-import:240         flex flex-col overflow-y-auto max-h-[80vh] w-[calc(100%-2rem)]   ← 需抵消
batch-import:313       flex flex-col overflow-y-auto max-h-[85vh] w-[calc(100%-2rem)]   ← 需抵消
batch-verify:40        grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
models-refresh:27      grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
credential-models:94   grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
credential-test:131    grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
balance:31             grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
credential-card:577    grid overflow-y-auto max-h-[90vh] w-[calc(100%-2rem)]
```

加 `overflow-y-visible` 抵消后，那 4 处变为 `flex flex-col overflow-y-visible max-h-[8xvh] w-[calc(100%-2rem)]`，外层滚动被关掉，内层滚动区独占。

group 归属逐条确认：

```
overflow-y-auto + overflow-y-visible → overflow-y-visible   同 group，抵消成立
grid + flex                          → flex                 同 group，4 处保持 flex
w-full + w-[calc(100%-2rem)]         → w-[calc(100%-2rem)]  同 group，调用方无覆盖则吃基类
max-h-[90vh] + max-h-[85vh]          → max-h-[85vh]         同 group，6 处保留自身值
```

这条证据推翻不了设计里的判断，反而确认了两点：`max-h-[85vh]` / `max-h-[80vh]` 会覆盖基类的 `max-h-[90vh]`，那 6 处行为不变；`w-[calc(100%-2rem)]` 会落到全部 12 处，包括 6 个已有 `max-h` 的，需在实测验收时确认宽度变化可接受。

## Tailwind 类有效性实测

`npx tailwindcss` 单独编译六个新类，全部生成规则：

```
.max-h-[90vh]           → max-height: 90vh
.min-h-14               → min-height: 3.5rem
.w-[calc(100%-2rem)]    → width: calc(100% - 2rem)
.flex-wrap              → flex-wrap: wrap
.gap-y-2                → row-gap: 0.5rem
.overflow-y-visible     → overflow-y: visible
.sm:grid-cols-2         → @media(min-width:640px) grid-template-columns: repeat(2,minmax(0,1fr))
```

`min-h-14` 与 `overflow-y-visible` 都在默认 scale 内，不需要扩 `tailwind.config.js`。任意断点写法 `min-[453px]:grid-cols-2` 实测也能生成，但仓库无先例，未采用，理由见 design.md。

反过来，这些类在改动前的构建产物 `dist/assets/index-m5Vlt9-N.css` 里全部 0 命中：Tailwind JIT 只生成源码扫到的类。用旧产物 CSS 验证「改后」样式会得到类缺失时的回落值，而非真实效果。本次审核一度用旧产物测 `w-[calc(100%-2rem)]`，量出弹窗盒 237.8px，那是 `width` 声明整体缺失后 grid 项按内容收缩的结果，与 `calc(100% - 2rem)` 无关。所以 6.2 把 `grep -c` 列为 6.3 实测的前置门禁。

## 任务到执行步骤映射

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1 / 1.2 | 改 `dashboard.tsx:777` 外层加 `flex-wrap gap-y-2`，其内右侧 `flex gap-2` 容器加 `flex-wrap justify-end` | `pnpm test` 绿 | 若右侧容器有 `justify-end` 之外的对齐依赖，先停下确认 |
| 1.3 | `dashboard.test.tsx` 补两条 className 断言 | 断言先失败再通过（改前先跑一次确认会红） | 无法用 testing-library 定位到容器时停下 |
| 2.1 / 2.2 | 改 `dashboard.tsx:702`，`h-14` → `min-h-14` 并加 `flex-wrap gap-y-2`；`:707` 右侧容器加 `flex-wrap justify-end` | `pnpm test` 绿 | sticky 顶栏高度变化影响到 `z-50` 层叠或下方 `container` 起始位置时停下 |
| 2.3 | 断言外层含 `flex-wrap` 与 `min-h-14`；「不含 `h-14`」写成 `className.split(' ')` 后 `not.toContain` | 同上 | 用 `not.toContain('h-14')` 或 `/\bh-14\b/` 会误判，见高风险项 |
| 3.1 | 改 `ui/dialog.tsx:38` 基类 | `pnpm test` 绿 | 无 |
| 3.2 | 4 处加 `overflow-y-visible` | 已由 twMerge 实测预验证 | 无 |
| 3.3 | 逐个核对 12 处最终 className | 用本文档的实测表逐行对照 | 出现表外的组合时停下 |
| 3.4 / 3.5 | 新增 `ui/dialog.test.tsx` | 断言 `max-h-[90vh]` 与 `overflow-y-auto` 同时存在；抵消类存在 | jsdom 下 Radix Portal 渲染取不到节点时改用直接渲染 `DialogContent` |
| 4.1 - 4.4 | 三处加 `break-words`：`batch-verify-dialog.tsx:104`、`kam-import-dialog.tsx:277`、`:346` | 各补 className 断言 | 无 |
| 5.1 / 5.2 | `kam-import-dialog.tsx:304` 加 `flex-wrap`；`settings-panel.tsx:352` 改 `grid-cols-1 gap-2 sm:grid-cols-2` | `pnpm test` 绿 | 无 |
| 5.3 | 不改 `settings-panel:208`、`add-credential:172`、`:291` | 已实测格内只有 `Input`，320px 列宽 131px 容得下 126px 的最长 placeholder | 出现表外的含文本网格时停下 |
| 5.4 | 不改 `settings-panel:335` | 已实测第二列文本 123.3px，320px 到 640px 全档单行；`Select` 内部截断是既有行为 | 同上 |
| 5.5 | 断言统计行含 `flex-wrap`、超时网格含 `grid-cols-1` 与 `sm:grid-cols-2` | 同上 | 无 |
| 6.1 | `pnpm test` + `pnpm build` | 148 项全绿（当前基线），build 无新增告警 | 任一红则停 |
| 6.2 | `grep -c` 确认新类已进 CSS | `w-\[calc`、`min-h-14`、`overflow-y-visible`、`gap-y-2`、`sm:grid-cols-2` 在新产物里命中非 0 | 命中 0 说明 build 未生效，测出的是类缺失回落值，停下 |
| 6.3 | headless Chrome 加载新 `dist/` 实测几何 | 1440px / 1920px 标题行 `h2` 横排约 80px 无横向滚动；375px 顶栏无横向滚动；667px 高视口批量验活弹窗 footer 可达；375px 统计行与超时网格折行整齐 | 实测与预期不符则停下，不靠改测试来对齐 |
| 6.4 | `cargo check --release --all-targets` | 告警数不高于基线 | 有新增则停 |
| 6.5 | `git status --short` | 无 `config.json` / `credentials.json` / `.codegraph/` | 有则停 |
| 6.6 | 更新 `docs/admin-ui-responsive-layout-optimization-design.md`：第 6 节进度 + 修正 `:158`-`:162`、`:259`、`:260`、`:437` 的 320px 结论 | 过 `humanizer-zh` | 无 |

## 必跑验证

```
cd admin-ui && pnpm test               # 基线：12 文件 148 项全绿（本会话已跑）
cd admin-ui && pnpm build              # tsc -b && vite build
cargo check --release --all-targets    # 零新增告警（AGENTS.md 硬性）
openspec validate fix-admin-ui-layout-wrapping --strict
git status --short
```

`pnpm build` 是 admin-ui 变更的矩阵要求（AGENTS.md:113）。`cargo check` 虽无 Rust 改动仍要跑，因为 `RustEmbed` 嵌的是 `admin-ui/dist`，产物变化会走一遍编译。

## 落地结果

`pnpm test` 17 文件 163 项全绿（基线 12 文件 148 项，新增 5 文件 15 项）；`pnpm build` 通过，`index-pJMU6tna.css` 29.12 kB、`index-BT6JXFlw.js` 481.01 kB，无新增告警；`cargo check --release --all-targets` 17.36s 零告警；`openspec validate --strict` 通过；`git status --short` 无敏感文件。

6.2 的 grep 门禁：8 个新类全部进了 CSS。注意构建产物里方括号类名带反斜杠转义，`w-\[calc\(100\%-2rem\)\]`、`max-h-\[90vh\]`、`sm\:grid-cols-2` 都要按转义形式匹配，直接 grep 原类名会得到 0 命中的假阴性。

6.3 实测几何，用新产物 CSS 配真实 DOM 结构，每项都跑了改动前后对照：

| 实测项 | 改动前 | 改动后 |
| --- | --- | --- |
| 标题行 `h2`（1024px 到 1920px） | 1440px 处竖排 20px 宽 4 行，横向溢出 | 横排 80px 单行，全档无溢出 |
| 顶栏（320px 到 1920px） | 453px 以下横向溢出，`h-14` 截掉第二行 | 全档无溢出，≥640px 恒 56px，640px 以下长到 72px，320px 处 120px |
| batch-verify 弹窗（667px 高视口） | 高 1334px，`top: -333px`，上下双向被裁且不可滚动 | 高 600px（=0.9×视口高），标题与 footer 都在视口内，内部可滚 |
| M1 统计行 | 503px 以下每项被压到自身折行，375px 处 60px 高、320px 处 80px | 每项恒 20px 高，375px 与 320px 都折 2 行摆放整齐 |
| M6 超时网格 | 471px 以下长标签折 2 行 | 640px 以下单列、长标签全档单行，640px 起两列 |

## README / AGENTS / spec / openspec/specs 同步判断

- `README.md`：不需要同步。`rg` 查到的 admin-ui 相关段落讲的是构建步骤、版本策略与端点清单，本次不改任何构建命令、API 入口或启动方式
- `AGENTS.md`：不需要同步，且工作区里它已被他人改动（`M AGENTS.md`），本次不碰
- `spec/design.md` / `spec/requirements.md` / `spec/structure.md`：不需要同步。`rg "响应式|布局|viewport|视口|flex" spec/` 命中 0，长期事实层未记录任何布局约束，本次新增的是 change 级 spec
- `openspec/specs/`：归档时由 `openspec archive` 把 `specs/admin-ui-responsive-layout/spec.md` 落成第 27 个主 spec。现有 26 个不动，proposal 的 Modified Capabilities 为空

## 停止条件

- 任一 OpenSpec artifact 与本计划矛盾
- twMerge 实测表之外出现新的 className 组合
- `pnpm test` 基线从 148 项减少（说明误删了用例）
- `cargo check --release --all-targets` 告警数高于基线
- 6.3 实测几何与预期不符
- 6.2 的 `grep -c` 显示新类未进构建产物（此时任何几何实测都不作数）
- `git status --short` 出现真实 `config.json` / `credentials.json` / token / Cookie / `.codegraph/`
- 顶栏 `min-h-14` 引出 sticky 层叠或下方内容位移的连带问题

## 第二轮增补：收纳与批量工具栏行（2026-09-04）

第一轮上线后用户反馈「全选本页后排列仍有问题」。在真实部署（172.20.66.24:18990，产物哈希与本地一致）与本地 mock 服务上逐视口复测确认：竖排已消失，但选中态右组 12 控件折 2 到 3 行、整行高 152px（375px 下 460px）、左组被 `justify-between` 顶成独占一行。根因是右侧需求约 1914px 恒超容器上限 1472px，`flex-wrap` 不减少控件数。设计文档 2.1 的收纳从非目标转为本次范围。

执行步骤：

| 步骤 | 内容 | 验证 |
| --- | --- | --- |
| 复现 | mock 服务 + playwright 逐视口测量（本地 8473 端口 + 真实部署 18990 双源交叉） | 12 档视口 × 选中/未选中几何表，截图存档 |
| 收纳 | `ui/dropdown-menu.tsx` 薄封装；5 低频项进溢出菜单；批量四项 + 批量余额/订阅拆独立 `role="toolbar"` 行 | 既有 37 项测试绿 + 新增归属断言 |
| 复测 | 重新构建后同口径复测 12 档 | ≥1280px 标题行单行 64px；375px 无横向滚动；证据存 `evidence/toolbar-refactor/` |

实测证据（`/tmp/layout-harness/` 的 measure.py / probe_toolbar.py，改前改后各跑一轮）：

- 改前（真实部署）：1920px 选中态 row h=152，right 行数 2；1440px 同；768px 行数 3；375px row h=460、right 行数 8
- 改后（本地构建）：≥1280px 标题行 h=64、右组 3 项单行；批量工具栏 1280px 起单行；375px 无横向滚动

新增高风险项：工具栏行拆分改变「批量余额/订阅」「清除已禁用」入口的 DOM 位置 → 测试查询方式同步调整，已覆盖；仓库内无依赖这些位置的外部脚本。

README / AGENTS / spec 同步判断不变：本次仍为纯前端布局，长期事实层未记录布局约束，归档时由 `openspec archive` 落主 spec。
