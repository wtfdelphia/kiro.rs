## Context

动机见 `proposal.md` 的 Why，完整审计与实测见 `docs/admin-ui-responsive-layout-optimization-design.md`。这里只记影响改法选择的现状约束。

`.container` 未在 `tailwind.config.js` 里自定义（`theme.extend` 只有 `colors` 与 `borderRadius`），走 Tailwind 默认阶梯，构建产物止于 `@media (min-width: 1536px){.container{max-width:1536px}}`。扣掉 `px-8` 的 64px，内容宽上限恒为 1472px。这是「凭据管理」标题行永远放不下的根因，也意味着任何靠加宽视口解决的思路都不成立。

`buttonVariants` 基类含 `whitespace-nowrap`（`ui/button.tsx:7`），所有按钮不可收缩。flex 把压缩全落到唯一可压项，也就是中文 `h2`，中文最小宽度是单字宽度，于是逐字竖排。

`cn()` 是 `twMerge(clsx(...))`。这一点决定了 `DialogContent` 基类改动的传播方式：同 group 的类由调用方覆盖（`max-h-[90vh]` 被 `max-h-[85vh]` 覆盖、`grid` 被 `flex` 覆盖、`w-full` 被 `w-[calc(...)]` 覆盖），不同 group 的类叠加。基类新增 `overflow-y-auto` 会落到全部 12 个弹窗上，调用方只有显式写同 group 的类才能抵消。

测试侧无法观测真实布局。jsdom 不做布局计算，`offsetWidth` 与 `getBoundingClientRect()` 恒为 0；`vite.config.ts` 的 test 块无 `css: true`，Tailwind 样式表不进测试环境；`package.json` 无 Playwright、无 `@vitest/browser`；`src/test/setup.ts` 未 stub `matchMedia`。仓库已有的样式类断言先例在 `credential-card.test.tsx`。

## Goals / Non-Goals

Goals：

- 让「凭据管理」标题行在所有受支持宽度下横排
- 选中态下批量操作集中到独立工具栏行，标题行只留全局操作，桌面宽度下标题行单行
- 用一处基类改动收口 12 个弹窗的高度上限与滚动通路，同时不产生双滚动条
- 不引入新依赖、不新增 CI 环节

Non-Goals：

- 不自定义 `.container` 的宽度阶梯。放开 1536px 上限会改变全站每个页面的排版，代价远超本次收益
- 不引入浏览器测试。真实布局断言的价值在这类静态缺陷上不足以抵消新依赖与新 CI 环节的成本
- 不为 320px 做专项适配。但 M1 与 M6 不属于 320px 专有问题，见下
- 不重做整页信息架构。第二轮只动标题行这一处：低频操作收进溢出菜单、批量操作拆独立工具栏行。其余区域（顶栏、筛选栏、卡片）不动

## Decisions

### `flex-wrap` 与收纳菜单同批落地，不再拆分

原计划只做 `flex-wrap` 止血，把收纳菜单拆出去单独推进，理由是两者叠加相对单独折行只省 44px 行高，不值得阻塞。第一轮落地后在真实部署上复测，推翻了「单独折行够用」这个前提：选中态右组 12 个控件仍折成 2 到 3 行，标题区吃掉 118 到 152px，用户反馈指向的就是这个混排。只加 `flex-wrap` 没有消除「12 个控件挤在标题行」这个根因，收纳因此从非目标转为本次范围。

两个方案本身仍是叠加关系：`flex-wrap` 是必要条件，没有它收纳后 1440px 以下仍竖排（右组收纳后需求 1133px，加左组 511px 共 1644px，仍超容器上限）。收纳则是把行高压回单行的手段。缺任一个都达不到目标。

考虑过的另两条路：给 `h2` 加 `shrink-0`（能救标题，但压缩会转移到下一个可压项，且不解决横向滚动）；给右侧容器加 `overflow-x-auto`（横向滚动条从页面级移到局部，仍然要横向找按钮，是把问题换个地方摆）。

### 收纳的具体形态：溢出菜单 + 批量工具栏行

收纳分两步，作用对象不同。

第一步把 5 个低频操作收进「更多操作」溢出菜单：KAM 导入（244px，最宽项）、批量导入、在线授权、刷新全部模型、清除已禁用。判据是它们都不是逐条高频操作，前三个是一次性导入，后两个是全量维护动作。这一步把右组从 12 项降到 7 项，但 768 到 1440px 下仍折 2 行，因为剩下的 4 个选中态批量按钮加定时刷新控件组仍超宽。

第二步把批量操作拆出独立工具栏行。这是关键，因为它改变了容器结构而不是只在原容器里删项：选中态的批量验活、批量刷新 Token、恢复异常、批量删除移到标题行下方的工具栏行，批量余额/订阅也随行（它是对当前页的批量操作，与选中态批量同类）。标题行右组只剩定时刷新控件组、添加凭据、更多操作三项，总需求约 715px，加左组 511px 共约 1242px，1280px 及以上容器内容宽 1216 到 1448px，从 1280px 起单行。

两步分开做的理由：第一步收窄的是「控件总数」，第二步收窄的是「哪些控件在标题行」。只做第一步，桌面宽度下选中态仍是两行混排；只做第二步，标题行右组还有 8 项仍超宽。两步合起来才让 ≥1280px 单行。

实测最终几何（本地构建，全选后）：

| 视口 | 标题行行数 | 标题行高 | 批量工具栏行数 | 横向滚动 |
| --- | --- | --- | --- | --- |
| 1920 / 1536 / 1440 / 1280 | 1 | 64px | 1 | 无 |
| 1024 / 768 | 1 | 108px | 1 | 无 |
| 414 / 375 | 2 | 196px | 2 到 3 | 无 |
| 320 | 2 | 196px | 3 | 11px |

对比改前：1920px 选中态标题行 152px、3 行，375px 460px。320px 那 11px 横向滚动超出本规范 375px 最小支持宽度，不作保证，与既有契约一致。

### `DialogContent` 基类同时加 `max-h-[90vh]` 与 `overflow-y-auto`，用 twMerge 抵消

只加 `max-h` 会让结果比改前更差。基类是 `grid` 且 `overflow: visible`，钳制的是边框盒而 grid 行仍按内容 auto 撑开，`translate-y-[-50%]` 参照钳制后的边框盒，整块内容向下平移。667px 视口 812px 内容下算出来：改前上下各溢出 72.5px，只加 `max-h-[90vh]` 后内容底到 845.5px，超出视口 178.5px，footer 反而更远。

所以两者必须同批。传播方式靠 `twMerge`：

- 6 个已写 `max-h-*` 的（`settings-panel:183`、`public-api-panel:91`、`add-credential:111`、`online-auth:236`、`kam-import:240`、`batch-import:313`）保留自己的值，基类 `max-h-[90vh]` 被覆盖，行为不变
- 4 个用 `flex flex-col` 配内层滚动区的（`add-credential`、`online-auth`、`kam-import`、`batch-import`）会继承基类 `overflow-y-auto`，多出一层外层滚动条。这 4 个显式加 `overflow-y-visible` 抵消
- 5 个既无 `max-h` 也无 `flex flex-col` 的（`batch-verify`、`models-refresh-result`、`credential-models`、`credential-test`、`balance`）与 1 个裸 `<DialogContent>`（`credential-card:577`）直接吃到基类的两项，这是想要的效果

替代方案是基类只加 `max-h`、5 个未补的各自补 `overflow-y-auto`。改动面更小，但默认值落在「没有滚动通路」这一侧，下次新增弹窗忘了补就复现同一个 bug。选基类兜底 + 显式抵消，是让安全的那一侧成为默认。

### M1 与 M6 纳入范围，M7 与 M6 剩余四处排除

设计文档 `docs/admin-ui-responsive-layout-optimization-design.md` 的触发档位表把 M1 / M6 / M7 归为「只在 320px 触发」。复测推翻了 M1 与 M6 这两条。测法是取真实源码的 DOM 结构与文案，加载构建产物的 CSS，在 headless Chromium 下逐 1px 扫元素矩形：

| 项 | 改前恢复单行所需视口 | 375px | 320px |
| --- | --- | --- | --- |
| M1 统计行 | 503px | 每项 60px 高（折 3 行） | 每项 80px 高（折 4 行） |
| M6 长 label | 471px | 折 2 行 | 折 2 行 |

M1 的旧阈值「内宽 < 403px」是按 4 项无符号文案估的，真实文案是 5 项带前缀符号（`✓ 成功`、`⚠ profile`、`⚠ 重复`、`✗ 失败`、`○ 跳过`），实际要 503px 视口才够。M6 的 `turn 间空闲超时（秒，0=不启用）` 在 `text-xs` 下自然宽 190.2px，改前列宽要到 471px 视口才够。两处都在 375px 支持范围内，改动各一处 className。

M7 也不是「只在 320px 触发」，但排除理由与前两项不同。`balance-dialog` 用 `sm:max-w-md`，640px 处 `sm:` 生效把弹窗从 `calc(100%-2rem)` 收窄到 448px，列宽从 205px 掉到 191px，640px 及以上重新折 2 行。改列数只救得了窄档，救不了这段，得连弹窗宽度一起定，超出本次范围。

另外四处 `grid-cols-2` 排除，理由分两类。`settings-panel:208`、`add-credential:172`、`:291` 里装的是 `Input`，基类是 `w-full` 且不含文本节点，不存在折行；320px 下列宽 131px，最长 placeholder「代理用户名（可选）」在 `text-sm` 下 126px，仍容得下。`settings-panel:335` 是 `Select` 配一段说明文本，文本「活跃连接：N（只读）」在 `text-xs` 下 123.3px，实测 320px 到 640px 全档单行，也不需要改。这处的 `Select` 最长选项「passthrough（预留）」自然宽 143.1px，超过 320px 下的列宽，触发器内部会截断文本，但那是 `Select` 组件的既有行为，与网格列数无关，不在本次范围。

改法沿用仓库先例 `settings-panel.tsx:238` 的 `grid-cols-1 gap-2 sm:grid-cols-3`，即 `grid-cols-1 sm:grid-cols-2`。代价是 `sm` 是 640px 而非 453px，453px 到 639px 这段原本正常的两列会退化成单列。用 `min-[453px]:grid-cols-2` 能精确卡在阈值上，实测该类可正常生成，但仓库现在一处任意断点都没用过，为一个折行问题引入新的断点写法不划算。单列在这段宽度里只是纵向更长，不影响可读性与可点击。

### 顶栏 `h-14` 改 `min-h-14`

`flex-wrap` 加在固定高容器上等于白加：折行后的第二行被 `h-14` 截掉。`min-h-14` 保持未折行时的视觉高度不变，折行时能长。`header` 是 `sticky`，高度变化会连带推动下方内容，但只发生在 437px 以下，不影响桌面。

### 测试判据是 className，覆盖到具体位置

jsdom 下真实几何不可观测，判据只能是静态类名。这不能证明布局正确，只能锁住已知的必要条件。要让它有意义，断言必须钉到具体位置而不是笼统检查：

- 标题行外层与右侧容器各含 `flex-wrap`
- 顶栏外层含 `flex-wrap`、右侧容器含 `flex-wrap`、外层含 `min-h-14` 且不含独立的 `h-14`
- `DialogContent` 基类同时含 `max-h-[90vh]` 与 `overflow-y-auto`（缺一则改动无效）
- 4 个 `flex flex-col` 弹窗含 `overflow-y-visible`
- `batch-verify-dialog` 的错误信息、`kam-import-dialog` 的预览路径与结果错误共三处含 `break-words`
- `kam-import-dialog` 统计行含 `flex-wrap`，`settings-panel` 超时网格含 `grid-cols-1` 与 `sm:grid-cols-2`
- 第二轮：批量操作四项与批量余额/订阅都在 `role="toolbar"` 行内，标题行框线容器内不含这五项；低频五项都在「更多操作」菜单内，标题行内不出现同名按钮

「不含独立的 `h-14`」这一条不能用仓库现有的 `.className).not.toContain()` 写法。改后 className 是 `min-h-14`，而 `'min-h-14'.includes('h-14')` 为真，`/\bh-14\b/` 也为真（`-` 构成词边界），两种写法都会把正确的实现判成失败。唯一可靠的判定是按空格切分后比对整个 token：

```ts
expect(header.className.split(' ')).not.toContain('h-14')
expect(header.className.split(' ')).toContain('min-h-14')
```

同类陷阱适用于任何「断言不含某个类」的场景。仓库现有先例 `credential-card.test.tsx:171` 的 `not.toContain('truncate')` 之所以没出问题，是因为没有以 `truncate` 结尾的其他类。

## Risks / Trade-offs

基类改动覆盖 12 个 `<DialogContent>`，twMerge 的覆盖判定出错会静默产生双滚动条 → 逐个手工核对 12 处的最终 className，4 个 `flex flex-col` 的抵消类写进测试断言。

className 断言守不住真实布局。类名存在但被更高优先级规则压过、或 Tailwind 版本变更改了 group 归类，测试仍会绿 → 落地时用 headless Chrome 加载构建产物核一次实测几何，不进 CI，仅作一次性验收。

第二轮工具栏行拆分把「批量余额/订阅」「清除已禁用」等入口的 DOM 位置与交互形态改了：前者从标题行移到工具栏行，后者从按钮变成菜单项。既有测试的查询方式随之调整（`getByRole('button')` 变 `findByRole('menuitem')`），任何依赖这些入口位置的外部脚本也会受影响 → 仓库内无此类脚本，测试已全量覆盖。

`flex-wrap` 后已选中态标题行折成 2 行、高度 118px，比现状未选中态的 112px 高 6px，卡片区起始位置会下移 → 这是可读性换来的，且比现状 166px 的竖排低 48px。第二轮工具栏行拆分后，≥1280px 选中态标题行回到 64px 单行，这个代价只在 768 到 1024px 与更窄档保留。

`min-h-14` 让 `sticky` 顶栏在 437px 以下变高，占更多首屏 → 只在窄屏发生，且是从「按钮点不到」换成「按钮占两行」。

最小支持宽度定 375px 后，顶栏这项的收益依赖于真有人在 375px 到 437px 之间用 Admin UI → 修复成本约 3 行 className，即使收益低也不亏。若后续把最小宽度上调到 437px 以上，这项可以回滚。实测右侧控件组恒宽 336px，溢出的精确阈值是 437px（375px 下溢出 62px），不是设计文档估的 453px。

基类 `w-full` 改 `w-[calc(100%-2rem)]` 会让每个弹窗内宽少 32px，窄屏折行随之增多。M1 统计行的单行阈值因此从 453px 推到 476px（逐 2px 扫档实测），375px 下从折 2 行变折 3 行 → 这正是 M1 要同批加 `flex-wrap` 的原因：`flex-wrap` 让折行成为受控的正常回流，而不是错位。两处必须一起落地，只改宽度会让 M1 在更宽的档位也错位。

`grid-cols-1 sm:grid-cols-2` 的断点是 640px，比实际需要的 453px 宽 → 453px 到 639px 这段原本能正常两列的宽度会变单列，纵向变长。取仓库一致性优先，理由见上文 Decisions。

## Migration Plan

纯前端 className 改动，无数据迁移、无 API 变更、无依赖变更。构建产物替换即生效。

回滚是 revert 对应文件的 className。基类改动与 4 个抵消类必须一起 revert，单独 revert 基类会留下无效的 `overflow-y-visible`（无害但语义混乱），单独 revert 抵消类会让那 4 个弹窗出双滚动条。

## Open Questions

设计文档第 4 节提到的静态正则兜底（扫 `src/**/*.tsx`，命中「同一 className 串含 `justify-between` 却无 `flex-wrap`」即失败）是否值得留，取决于豁免名单是否够短。这个可以在本次改动落地后先跑一遍看命中数再定，不影响本次的 spec、改法与任务拆分。

第二轮把定时刷新控件组留在标题行右组（302px，右组里最宽项），没有拆走。它是当前页余额的辅助开关，语义上介于批量与全局之间，留在标题行与批量工具栏行都说得通。若后续反馈右组在更窄档折行明显，可以再评估把它也移进工具栏行，本次不动。
