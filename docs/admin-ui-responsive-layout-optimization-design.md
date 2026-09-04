# Admin UI 响应式布局优化方案

> 状态：设计草案，未实现　日期：2026-08-28　复核：2026-08-28（第一轮）、2026-08-28（第二轮）
> 触发来源：用户反馈「凭据管理整行不随浏览器窗口缩进，『凭据管理』四个字竖排」；布局审计交付后又追加了定时刷新的限并发与开关状态持久化，记入第 5 节。
> 范围：第 1-4 节是布局自适应，第 5 节是定时刷新的两项功能优化（限并发、开关持久化）。不含后端数据契约的改动。
> 第一轮复核：初稿的宽高数字是按 Tailwind 类值笔算的。改用 headless Chrome 加载构建产物 `dist/assets/index-m5Vlt9-N.css`，在 320/375/414 三档视口下实测几何，另核对了 Rust 侧错误串的长度上界。凡与笔算有出入，正文已标注并以实测为准。结果推翻了三处断言：proxyUrl 截断、顶栏 SVG 归因、HIGH-2 的影响面。另外 HIGH-2 的改法拆成两步会失效。
> 第二轮复核：第一轮只测到 414px，把视口档位补到 6000px 后，第 2 节竖排的触发范围被推翻。竖排与视口宽度无关，1920px 桌面同样竖排，根因是 `.container` 在 1536px 封顶而这一行需求 2207px。这项因此不受「最小支持宽度」这个前置约束，第 6 节的顺序随之重排。详见第 2 节。

## 1. 为什么另开文档而不并入 optimize-admin-ui-credential-list

`optimize-admin-ui-credential-list` 已是 `Complete` 状态、62 项任务全部勾选、归档前验证给出「可以归档」结论。往里追加范围会让它退回进行中，那一轮验证的结论随之作废：任务出现未勾选项、`isComplete` 翻回 false、Scenario 覆盖统计要重算。

范围上也不属于它。那个 change 的三份 spec 是 `admin-credential-list-query`、`admin-ui-credential-list-view`、`admin-ui-balance-ops`，`响应式`、`视口`、`flex-wrap`、`窄屏` 等词在其中零命中。这不是漏写：响应式布局是跨组件的全局关注点，涉及顶栏、共享 `DialogContent`、11 个弹窗、设置面板，其中大部分不在凭据列表的能力边界内。它的 Goals 四条最接近的是「分页 UI 在页数很多时可用，且键盘与读屏可完整操作」，讲的是键盘与读屏，不是视口宽度。

缺陷来源也不同。竖排的根因是容器从一开始就没写 `flex-wrap`，右侧控件由历次功能迭代累积到 12 个、需求 1914px。实测可以量化那个 change 的贡献：移除它新增的「全选本页」按钮后整行需求从 2207px 降到 2076px，缺口仍有 604px，1440px 到 2560px 全档位依然竖排。所以它加剧了拥挤但不是根因，把先于它存在的缺陷并入它的验证范围会污染「它是否达成自己的目标」这一判断。

后续实现前需要按仓库规则建 OpenSpec change（`AGENTS.md` 要求 Admin API / 凭据管理变更先建 change；本次是纯前端布局，是否需要建 change 由维护者定）。

## 2. 「凭据管理」竖排：与视口宽度无关，桌面上始终存在

`admin-ui/src/components/dashboard.tsx:777`：

```tsx
<div className="flex items-center justify-between">
  <div className="flex items-center gap-4">   {/* 左：h2「凭据管理」+ 全选按钮 + 已选徽章 */}
  <div className="flex gap-2">                {/* 右：最多 12 个按钮 + 定时刷新控件组 */}
```

两层容器都没有 `flex-wrap`。flex 项在空间不足时会被压缩到内容最小宽度，对中文 `h2` 而言最小宽度是单字宽度，于是「凭据管理」逐字竖排。

同仓库 `credential-card.tsx:252`、`credential-filter-bar.tsx:77`、`pagination-bar.tsx:23`、`credential-card.tsx:446` 都写了 `flex-wrap`。所以这是单点遗漏而非取舍，改法与既有约定一致。

### 触发范围：加宽视口不能消除

初稿与第一轮复核都默认这是窄屏问题。把视口档位从 414px 补到 6000px 后，这个默认不成立：

| 视口 | `h2` 宽 | `h2` 高 | 排布 | 横向滚动条 |
| --- | --- | --- | --- | --- |
| 320px | 20px | 112px | 竖排 4 行 | 1902px |
| 1280px | 20px | 112px | 竖排 4 行 | 958px |
| 1440px | 20px | 112px | 竖排 4 行 | 798px |
| 1920px | 20px | 112px | 竖排 4 行 | 318px |
| 2560px | 20px | 112px | 竖排 4 行 | 无 |
| 6000px | 20px | 112px | 竖排 4 行 | 无 |

二分找不到竖排消失的视口宽度（上界 6000px 仍竖排）。横向滚动条在 2238px 消失，但竖排不随之消失。

根因是 `.container` 的宽度阶梯。项目未自定义 `container`（`tailwind.config.js` 只扩展 colors 与 borderRadius），用的是 Tailwind 默认阶梯，构建产物里止于 `@media (min-width: 1536px){.container{max-width:1536px}}`。扣掉 `px-8` 的 64px，内容宽上限 1472px。而这一行的需求是 2207px：

```
需求 2207px = 左侧 293px + 右侧 1914px
  ├──────────────────────────────────────────────────┤

容器上限 1472px（.container 封顶 1536px 减 px-8）
  ├────────────────────────────────┤
                                    └──── 恒缺 735px ────┘
```

视口超过 1536px 后容器不再变宽，缺口固定在 735px。所以「凭据管理」在任何桌面分辨率下都是竖排。

右侧 12 个控件（11 个按钮加定时刷新控件组）的实测宽度构成，已选中态：

| 控件 | 宽 |
| --- | --- |
| 定时刷新控件组（`:859`） | 302px |
| `Kiro Account Manager 导入` | 244px |
| `批量刷新 Token` | 158px |
| `清除已禁用 (3)` | 152px |
| `批量余额/订阅` | 147px |
| `刷新全部模型` | 142px |
| 其余 6 个按钮 | 约 114px 各 |
| 12 个 `gap-2` | 96px |

定时刷新控件组是单个最宽项，内部 `whitespace-nowrap` 加固定宽 `w-20` / `w-16` 使其不可收缩。`buttonVariants` 基类的 `whitespace-nowrap`（`ui/button.tsx:7`）让每个按钮同样不可收缩，于是压缩全部落在唯一可压的 `h2` 上。

未选中态少 4 个批量按钮加已选徽章组，需求降到 1533px，横向滚动条在 1565px 消失。但 `h2` 仍竖排，1533px 依旧超过 1472px 上限。

### 加 `flex-wrap` 的实测效果

| 场景 | 现状 | 加 `flex-wrap` 后 |
| --- | --- | --- |
| 已选中 @1440px | 竖排 4 行，行高 166px，横向滚动 798px | 横排 80px，行高 118px，右侧 2 行，无滚动 |
| 已选中 @1920px | 竖排 4 行，行高 166px，横向滚动 318px | 横排，行高 118px，右侧 2 行，无滚动 |
| 未选中 @1440px | 竖排 4 行，行高 112px，横向滚动 125px | 横排，行高 118px，右侧 2 行，无滚动 |
| 未选中 @1920px | 竖排 4 行，行高 112px，无滚动 | 横排，行高 74px，右侧 1 行 |

`h2` 从 20px 回到 80px，竖排与横向滚动条同时消失。改法是外层加 `flex-wrap gap-y-2`，右侧容器加 `flex-wrap justify-end`。

代价是垂直空间。已选中态在 1920px 下右侧仍占 2 行，标题行 118px。`flex-wrap` 解决的是「不可读」，不解决「12 个控件挤在标题行」这个上游问题，见 2.1。

### 2.1 根治：把低频操作收进溢出菜单

`flex-wrap` 是止血。它让内容可读，但已选中态在任何桌面宽度下都占 2 行 118px，一个页面标题行吃掉这么多垂直空间。根因没动：右侧需求 1914px 而容器只给 1472px。

把 5 个低频操作收进一个溢出菜单可以把需求压到容器以内。候选是 `Kiro Account Manager 导入`（244px，最宽项）、`批量导入`、`在线授权`、`刷新全部模型`、`清除已禁用`。判据是它们都不是逐条操作的高频入口：前三个是一次性导入、后两个是全量维护动作。留在外面的是 `批量验活`、`批量刷新 Token`、`恢复异常`、`批量删除`（选中后才出现，与当前选择直接相关）、`批量余额/订阅`、`添加凭据`、定时刷新控件组。

不需要新依赖。`@radix-ui/react-dropdown-menu` 已是直接依赖（`package.json`），`ui/select.tsx` 就是它的封装，可照其结构加一个 `ui/dropdown-menu.tsx` 或直接复用。

实测四种组合：

| 方案 | 右侧需求 | 整行需求 | 相对容器上限 1472px |
| --- | --- | --- | --- |
| 现状（已选中，12 控件） | 1914px | 2207px | 超 735px |
| 现状（未选中，8 控件） | 1383px | 1533px | 超 61px |
| 收纳后（已选中，8 控件） | 1133px | 1472px | 恰好 0 |
| 收纳后（未选中，4 控件） | 603px | 1472px | 恰好 0 |

**但收纳单独用仍然竖排。** 那个「恰好 0」是假的：

| 视口 | 收纳后不加 `flex-wrap` | 收纳后 + `flex-wrap` |
| --- | --- | --- |
| 1280px | 竖排 4 行，横向滚动 178px | 横排 80px，右侧 1 行，行高 74px |
| 1440px | 竖排 4 行，横向滚动 18px | 横排 80px，右侧 1 行，行高 74px |
| 1536px | 竖排 2 行（`h2` 46px），无滚动 | 横排 80px，右侧 1 行，行高 74px |
| 1920px | 竖排 2 行（`h2` 46px），无滚动 | 横排 80px，右侧 1 行，行高 74px |

1536px 那行的「需求 1472px 等于上限 1472px」是 `h2` 被压到 46px 双字竖排换来的，不是真放得下。flex 先压缩唯一可压项再报告结果，所以 `scrollWidth` 等于 `clientWidth` 时不能推断「没有压缩发生」。1280px 与 1440px 下容器只有 1216px，收纳后仍超 210px。

所以两者是叠加关系而非替代关系：

```
flex-wrap 单独      → 可读，已选中态 2 行 118px
收纳单独            → 仍竖排，1440px 以下仍横向滚动
flex-wrap + 收纳    → 横排，所有档位 1 行 74px
```

只有组合能在 1280px 到 6000px 全区间做到 `h2` 横排 80px、右侧单行、行高 74px。相比只加 `flex-wrap` 的 118px，省 44px 垂直空间，且不再随选中状态跳变。

收纳是产品决策，涉及哪些操作降级为二级入口，需要维护者定候选名单。`flex-wrap` 不需要拍板，是纯缺陷修复。建议 `flex-wrap` 先落地止血，收纳单独走一轮。

## 3. 全量审计结果

### 前置：最小支持宽度未定义，这是本节各项收益判断的分母

适用范围先划清：这个前置只约束本节列出的项，不约束第 2 节。竖排与视口宽度无关，最小支持宽度定到多大都要修。第一轮复核把竖排当窄屏问题处理，是因为视口只测到 414px。

初稿漏了一个前提：本项目从未声明支持窄屏。`README`、`docs/`、`openspec/specs/` 里「移动端」「手机」「mobile」「平板」「响应式」全部零命中，唯一的例外是这份文档自己。Admin UI 是运维面板，窄屏使用场景没有被论证过。

反向证据也在：`index.html:6` 有 viewport meta，代码里 `sm:` 用了 21 次、`md:` 4 次、`lg:` 2 次。所以不是不管窄屏，而是从没有人定下限宽度。

这个数字直接决定下面各项要不要做，因为实测显示各项的触发档位差别很大：

| 若最小支持宽度定为 | 影响 |
| --- | --- |
| 768px | HIGH-1 与整批 MEDIUM 全部降为无需修复，只剩 HIGH-2（高度问题，与宽度无关）、第 2 节（宽度无关）和第 5 节两项 |
| 375px | HIGH-1 成立（溢出 62px），MEDIUM 那批仍可不做 |
| 320px | 全部成立 |

三档之下第 2 节都要修，它不在任何一档的降级集合里。

实测的触发档位汇总。这份表是落地时用构建产物逐 1px 扫出来的，替换了此前按估算宽度填的版本：

| 项 | 触发条件 | 375px | 320px |
| --- | --- | --- | --- |
| 第 2 节 标题行竖排 | 恒成立 | 竖排 | 竖排 |
| HIGH-1 顶栏 | 宽 < 453px | 溢出 62px | 溢出 117px |
| HIGH-2 batch-verify | 视口高 < 706px | 与宽度无关 | 与宽度无关 |
| M1 kam-import | 视口宽 < 503px | 每项折 3 行 | 每项折 4 行 |
| M6 grid-cols-2 | 视口宽 < 471px | 标签折 2 行 | 标签折 2 行 |
| M7 balance 日期 | 视口宽 < 508px 到 526px（随日期串长度浮动，640px 起再次触发） | 折 2 到 3 行 | 折 3 行 |

初稿写「M1 / M6 / M7 只在 320px 触发」是错的。三项的阈值分别是 503px、471px、508px 起，375px 全部触发，不能按「320px 专有」降级：

- M1 的统计行是 5 项带前缀符号的文案（`✓ 成功`、`⚠ profile`、`⚠ 重复`、`✗ 失败`、`○ 跳过`），`flex` 少了 `flex-wrap`，改前 span 是被压到自身折行而不是换行摆放，375px 处每项 60px 高、320px 处 80px。此前记的 403px 是按 4 项无符号文案估的
- M6 的 `turn 间空闲超时（秒，0=不启用）` 在 `text-xs` 下自然宽 190.2px，改前列宽要到 471px 视口才够，470px 以下一直折 2 行
- M7 的 `toLocaleString('zh-CN')` 日期与前缀 `下次重置：` 同格，阈值随日期串长度浮动：`2026/9/1 08:30:00` 是 508px，`2026/8/31 10:41:23` 是 526px。它还有个反直觉的档位，`balance-dialog` 用 `sm:max-w-md`，640px 处 `sm:` 生效把弹窗从 `calc(100%-2rem)` 收窄到 448px，列宽从 205px 掉到 191px，640px 起重新折 2 行

HIGH-2 仍与宽度无关，只看视口高度。第 2 节列在表内只为对照，不受本节前置约束。优先级排序改按「触发条件的现实性」，见第 6 节。

### 分级

除竖排那处之外，扫全部组件又找出 12 处，按严重程度分级。分级判据：HIGH 是内容不可读或不可操作，MEDIUM 是观感受损但仍可用，LOW 是仅偏挤、不影响读取。

复核后需要降级或删除的有六项：proxyUrl（误报）、M2、M4、L1、L2 断言不成立，顶栏 SVG 应并入 HIGH-1。HIGH-2 的影响面从 5 个弹窗收窄到 1 个。各项已就地标注。

### HIGH-1 顶栏两侧都无 `flex-wrap`，窄屏横向溢出且按钮不可点

`dashboard.tsx:702`：

```tsx
<div className="container flex h-14 items-center justify-between px-4 md:px-8">
```

右侧 6 个控件：1 个文字按钮（`优先级模式` / `均衡负载`）加 5 个 `size="icon"` 按钮（深色模式、刷新、对外 API、运行时设置、登出）。图标按钮是固定 `h-10 w-10`，`buttonVariants` 基类带 `whitespace-nowrap`（`ui/button.tsx:7`），两侧都既不能收缩也不能换行。

实测顶栏最小需求 453px。第一轮复核给的 423px 偏低 30px，第二轮按真实 className 重测修正（模式按钮是 `size="sm"` 的 `h-9 px-3` 而非 default 的 `h-10 px-4`，logo 是 `font-semibold` 无 `text-lg`）。两轮都测到 SVG 被压到 12.92px，说明复现忠实，差异只在按钮尺寸。

| 视口 | 溢出 |
| --- | --- |
| 320px | 117px |
| 375px | 62px |
| 414px | 23px |
| 423px | 14px |
| 453px 及以上 | 无 |

与第 2 节的对比值得记一笔：顶栏总需求 453px，远低于 `.container` 上限 1472px，所以它确实只在窄屏溢出，1920px 下正常。标题行需求 2207px 超上限 735px，所以它在所有宽度下都出问题。两者虽然都是「缺 `flex-wrap`」，触发范围差了一个量级。

`header` 是 `sticky` 且 `h-14` 固定高，溢出后页面出现横向滚动条，登出与设置按钮被推出视口不可点。左侧 `Kiro Admin` 会先在空格处折行再被挤压。

这项的画像值得说清：修复成本约 3 行 className，触发条件是视口 < 453px。它属于低成本项，不是高收益项。若最小支持宽度定在 453px 以上，收益归零。初稿把它排为最高优先级之一，理由是「按钮不可点」，那个后果描述准确，但前提是真有人在那个宽度上点。

改法：外层加 `flex-wrap gap-y-2`，右侧容器加 `flex-wrap justify-end`，`h-14` 改 `min-h-14`（换行后高度需要能长）。

### HIGH-2 共享 `DialogContent` 既无高度上限也无滚动容器

`ui/dialog.tsx:38` 基类：

```
fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 …
```

`translate-y-[-50%]` 做垂直居中，配合无高度上限，内容超过视口高度时会从上下两端同时溢出到视口外，且没有滚动容器。结果是标题与 footer 按钮都点不到，弹窗只能靠 Esc 关闭。

基类缺 `max-h` 与 `overflow` 属实。全仓库 12 处 `<DialogContent>`，其中 6 个各自补了 `max-h-[80~90vh]`（`settings-panel:183`、`public-api-panel:91` 配 `overflow-y-auto`，`add-credential:111`、`online-auth:236`、`kam-import:240`、`batch-import:313` 配 `flex flex-col`）。另外 5 个没补，`className` 只有 `sm:max-w-*`：`batch-verify-dialog:40`、`models-refresh-result-dialog:27`、`credential-models-dialog:94`、`credential-test-dialog:131`、`balance-dialog:31`。剩下 1 处 `credential-card.tsx:577`（删除确认）是裸 `<DialogContent>` 无 className，内容只有两行文字加两个按钮，实际不会超高，但同样受基类改动覆盖。

**初稿说这 5 个「整体仍会超高」，复核后不成立，实际只有 1 个。** 逐个算了最坏高度：

| 弹窗 | 最坏高度 | 667px | 800px 及以上 | 判定 |
| --- | --- | --- | --- | --- |
| `batch-verify-dialog` | ~706px | 溢出约 40px | 不溢出 | **真缺陷** |
| `credential-models-dialog` | ~632px（真实场景） | 压线 | 不溢出 | 轻微 |
| `credential-test-dialog` | ~670-720px | 溢出约 55px | 不溢出 | 轻微 |
| `models-refresh-result-dialog` | ~452-472px | 不溢出 | 不溢出 | **误报** |
| `balance-dialog` | ~271-291px | 不溢出 | 不溢出 | **误报** |

**没有一个弹窗是无界的。** 这与初稿隐含的「内容随数据量增长」相反。每个列表和长文本区都已有内层封顶（`max-h-[400px]` / `max-h-72` / `max-h-64` / `max-h-40`），后端错误串也在 `models_api.rs:134` 的 `truncate_for_error(_, 500)` 处按字符截断。所以最坏高度全部可算出确定上界。

**`batch-verify-dialog` 是唯一确定溢出的，且它打开瞬间就必然触发最坏情况。** `dashboard.tsx:559-564` 先把所有选中项初始化成 `pending` 再开弹窗，单条结果项 38px，选中 ≥10 条列表就打满 `max-h-[400px]`。批量验活选 10 条以上是常规操作，不是边缘分支。

**`models-refresh-result-dialog` 和 `balance-dialog` 应从清单删除。** 最坏高度只占 667px 视口的 40%–68%，距溢出还有 190–375px 余量。前者被 `max-h-72` 封死，后者三个分支（loading / error / balance）互斥且无列表，加数据也长不上去。

另外 `credential-models-dialog` 的 `lastError` 值得单独说。它渲染时无 `max-h` 无行数截断，一度被判为无界向量，但源头封住了：`models_api.rs:72` 与 `:83` 用 `truncate_for_error(&body_text, 500)` 截到 500 字符，落到 `token_manager.rs:2917` 的串最长约 510 字符。而实测证据（`docs/builderid-403-repro-evidence-2026-08-25.txt`，6024 字节）里最长的 body 只有 87 字符，整个文件无任何超 200 字符的行。按 `text-xs` ASCII 字宽约 6.2px 算，实测长度在 375px 下只占 2 行 32px，理论上限也只 10 行 160px。所以它真实场景约 632px，不溢出。仍建议给它和 `credential-test-dialog` 的 `error` 各加 `max-h-32 overflow-y-auto`，理由是 500 字符错误文本在窄屏展开十行本身难读，不是布局会破。

### HIGH-2 的改法：`max-h` 与 `overflow-y-auto` 必须同批落地

初稿说「基类加 `max-h-[90vh]`，`overflow-y-auto` 按弹窗逐个加」，并把验收定为「基类加 max-h 之后 footer 可点」。**这样拆会让结果比改之前更差。**

基类是 `grid`，`overflow` 默认 `visible`。只加 `max-h` 时，被钳制的是边框盒，grid 行仍按内容 auto 撑开：

```
视口 667px，内容自然高 812px
改前：容器顶 -72.5，底 739.5 → 上下各溢出 72.5px
只加 max-h-[90vh]（600px）：
  边框盒钳到 600 → translate-y(-50%) 按 600 算 → 容器顶 +33.5
  grid 行仍撑到 812 → 内容底 845.5
  超出视口 178.5px（改前只超 72.5px）→ footer 更远离视口
```

`translate-y-[-50%]` 参照的是钳制之后的边框盒，所以容器顶从 -72.5 变成 +33.5，整块内容往下平移。

结论：`max-h-[90vh]` 与 `overflow-y-auto`（或 `flex flex-col` + `min-h-0`）必须在同一批改动里落地，不能拆成两步分别验证。

`overflow-y-auto` 是否进基类仍需分情形。已补 `max-h` 的 6 个里只有 2 个（`settings-panel`、`public-api-panel`）靠它滚，另外 4 个是 `flex flex-col` 配内层滚动区，基类再加一层会出双滚动条。可行的做法是基类同时加 `max-h-[90vh] overflow-y-auto`，然后给那 4 个 `flex flex-col` 的显式加 `overflow-y-visible` 抵消；或者基类只加 `max-h`，5 个未补的逐个补 `overflow-y-auto` 并作为同一批提交。后者改动面更小，但要靠测试保证不漏。

同时基类是 `w-full` 无横向留白，320px 下弹窗贴满两边且 `sm:rounded-lg` 视觉上不生效，建议一并改为 `w-[calc(100%-2rem)]`。

### MEDIUM

| 位置 | 问题 | 改法 |
| --- | --- | --- |
| M1 `kam-import-dialog.tsx:304` | `flex gap-4` 挂 5 个统计项（成功/profile/重复/失败/跳过），无 `flex-wrap`。503px 以下每个 span 被压到自身折行，375px 处每项高 60px、320px 处 80px，行内文字挤成竖条。同仓库 `batch-import-dialog.tsx:354` 是 `flex flex-wrap gap-4`，此处属漏写 | 加 `flex-wrap` |
| M5 `batch-verify-dialog.tsx:104`、`kam-import-dialog.tsx:277` | 错误信息与 `item.path` 文件路径含长不可断 token，父级 `max-h-[300px] overflow-y-auto`（`kam-import-dialog.tsx:292`）使横向溢出计算为 `auto`，列表内部冒出横向滚动条。`models-refresh-result-dialog.tsx:47` 已是 `break-words whitespace-pre-wrap` 正确写法 | 加 `break-words` |
| M6 `settings-panel.tsx:352`、`:208`、`add-credential-dialog.tsx:172`、`:291` | 硬编码 `grid-cols-2`。`turn 间空闲超时（秒，0=不启用）`（`settings-panel.tsx:372`）在 `text-xs` 下自然宽 190.2px，改前列宽要到 471px 视口才够，375px 与 320px 都折 2 行，不是 320px 专有。同文件 `:238` 已用 `grid-cols-1 gap-2 sm:grid-cols-3` | 改 `grid-cols-1 sm:grid-cols-2` |
| M7 `balance-dialog.tsx:85` | `grid grid-cols-2` 内含 `toLocaleString('zh-CN')` 日期。508px 到 526px 以下两格都折 2 行，360px 以下折 3 行，阈值随日期串长度浮动。640px 处还有一段反直觉区间：`sm:max-w-md` 生效后弹窗从 `calc(100%-2rem)` 收窄到 448px，列宽从 205px 掉回 191px，重新折 2 行。改列数救不了这段，本次未动 | 需连 `sm:max-w-md` 一起定 |

以下三项复核后降级，保留记录以免重复排查：

| 位置 | 复核结论 |
| --- | --- |
| M2 `public-api-panel.tsx:232-233` | 父行 `:230` 已有 `flex-wrap`。85 字符超长主机名实测不溢出，`documentElement.scrollWidth` 在 320px 下仍为 320，无横向滚动条。初稿说的「长主机名横向溢出弹窗」未复现。实际只是 `w-44` 在 320px 占掉半行偏挤，属 LOW |
| M4 `public-api-panel.tsx:139-147` | 实测 grid 的 `scrollWidth == clientWidth`，鉴权头正常折 2-3 行，未溢出轨道。断言不成立 |
| `credential-card.tsx:412` proxyUrl | **误报，见下** |

**proxyUrl 那条要改写。** 初稿说「不是溢出而是直接截断且无 `title`，尾部端口号读不到」。实际 className 是 `font-medium`，祖先链确有 `overflow-hidden`（`Card:221-222`、`CardHeader:225`），`dashboard.tsx:948` 也有 `[&>*]:min-w-0`。但实测计算值是 `word-break: normal / white-space: normal / overflow: visible`，这是普通可换行文本而非 `truncate`，祖先的 `overflow-hidden` 本身不产生 ellipsis。

375px 下单列卡片实际宽 343px，`col-span-2` 行内容宽 293px，长度扫描：

| 值 | 渲染宽 | 溢出 |
| --- | --- | --- |
| `http://127.0.0.1:7890` | 171px | 无 |
| `http://user:pass@127.0.0.1:7890` | 264px | 无 |
| 38 字符长主机名 | 317px | 无（折行） |

83 字符极端值折成 3 行，仍不溢出不截断。只有 320px + 38 字符长主机名这一组才真被裁掉 14.52px。

而且 `user:pass` 嵌在 URL 里这个形态基本不存在：代理认证走独立字段（`types/api.ts:154-155` 的 `proxyUsername`、`src/http_client.rs:75` 的 `proxy.basic_auth(username, password)`），UI 输入也是分开的（`add-credential-dialog.tsx:294,302`）。典型值是 `settings-panel.tsx:204` 占位符那种 `http://127.0.0.1:7890`。

`title` 缺失属实，但既然是无损换行而非截断，补 `title` 收益很小。建议这条从 MEDIUM 删除。

### LOW

- `credential-card.tsx:300` `grid grid-cols-2 gap-4`：实测列宽 138.5px（375px）/ 111px（320px），中文标签单行不折，仅偏挤
- `settings-panel.tsx:335` `grid grid-cols-2 items-center`：实测 `活跃连接：12（只读）` 在 158.5px 内单行放得下，未互相挤压。初稿的「互相挤压」未复现
- `dashboard.tsx:704` `<Server className="h-5 w-5" />` 未加 `shrink-0`：压缩是真的（实测 20px → 12.92px，约 65%），但**归因错了，应并入 HIGH-1**。lucide 把 `width/height` 设成 HTML 属性 `24`，`w-5` 是 CSS `width:1.25rem`；SVG 有内在宽高比，`flex-basis: auto` + `min-width: auto` 下 flex 算法允许压到 CSS 宽度以下。三组对照实测：现状 12.92px 且顶栏溢出；只补 HIGH-1 的 `flex-wrap`、不加 `shrink-0`，SVG 回到 20.00px 且不溢出；只加 `shrink-0` 不修 HIGH-1，SVG 是 20px 但顶栏仍溢出。所以 `shrink-0` 只掩盖症状，修了 HIGH-1 这项自动消失。可作为回归护栏保留，但不该独立列项。第二轮独立复现确认：压缩只在 453px 以下发生，768px 与 1920px 下 SVG 本来就是 20px；加 `flex-wrap` 后 320px 到 1920px 全档位恒为 20px

### 已正确处理，供对照

`credential-filter-bar.tsx:77`、`pagination-bar.tsx:23`、`credential-card.tsx:446`、`credential-models-dialog.tsx:155` 都有 `flex-wrap`；`dashboard.tsx:948` 的 `[&>*]:min-w-0` 与 `credential-card.tsx` 的 `min-w-0` / `truncate` / `shrink-0` 链条完整；`login-page.tsx` 无问题。全仓库无 `<table>`，所以「表格缺 `overflow-x-auto`」这类不适用。

## 4. 测试策略：只能断言 className

当前配置下 className 断言是唯一可行手段，仓库里已有先例。`credential-card.test.tsx:161-171` 是全仓库唯一的样式类断言，守的正好是本次这类缺陷：

```ts
expect(nameEl.className).toContain('truncate')
expect(idEl.className).toContain('shrink-0')
expect(idEl.className).not.toContain('truncate')
```

（`toHaveClass` / `classList` / `toHaveStyle` / `getComputedStyle` / `toMatchSnapshot` 在别处均无命中。）

为什么不能断言真实布局：

- jsdom 不做布局计算，`offsetWidth` 与 `getBoundingClientRect()` 恒为 0，换行、挤压、溢出在原理上无法观测
- `vite.config.ts:25-29` 的 test 块只有 `environment: 'jsdom'` / `setupFiles` / `globals`，没有 `css: true`，Tailwind 样式表不进测试环境，`getComputedStyle` 对 `flex-wrap` 只返回 UA 默认值
- `package.json` 无 Playwright、无 `@vitest/browser`、无 Storybook；`src/test/setup.ts` 未 stub `matchMedia` / `ResizeObserver`，连响应式分支都模拟不了

要断言真实布局需引入 Playwright（设 viewport 后取 `boundingBox()`，或比对 `scrollWidth > clientWidth`）或开 vitest browser mode，两者都意味着新依赖与新 CI 环节。

我倾向不上浏览器。这类缺陷的判据是静态的，针对已知风险点写 className 断言即可，成本最低且契合现有约定：断言标题行两层容器各含 `flex-wrap`（第 2 节，优先级最高）、顶栏容器含 `flex-wrap`、`DialogContent` 基类同时含 `max-h-[90vh]` 与 `overflow-y-auto`（两者缺一则改动无效，见 HIGH-2）、M5 两处含 `break-words`。

（初稿这里还列了「`proxyUrl` 节点含 `break-all` 且带 `title`」，复核判定该项为误报，已从清单移除。）

复核本身用的是另一条路：headless Chrome 加载构建产物 CSS，设视口后取 `scrollWidth` / `clientWidth` / `getComputedStyle`。这条路只适合一次性核查，不建议进 CI，因为它依赖 `dist/` 已构建，实测耗时也远高于 className 断言。

两轮复核暴露了两个测量陷阱，后续再做这类审计要避开：

**视口档位选窄了会漏掉恒定缺陷。** 第一轮只测 320/375/414 三档，全部溢出，于是把标题行竖排归为窄屏问题。补到 6000px 才发现它与宽度无关。凡是怀疑「窄屏问题」的项，都要往上测到 `.container` 封顶（1536px）以上一档，确认宽视口下是否恢复。

**`scrollWidth == clientWidth` 不等于「没有压缩」。** 2.1 的收纳方案在 1536px 下测出需求恰好等于上限 1472px，看着刚好放下，实际是 `h2` 被压到 46px 双字竖排换来的。flex 先压缩可压项再报告尺寸，所以判断是否真放得下要同时看目标元素的 `getBoundingClientRect()`，不能只看容器溢出量。

复现片段也要逐个属性核对源码。顶栏最小需求笔算 470px、第一轮复现 423px、按真实 className 重现 453px，差异全来自抄错一个尺寸变体（`size="sm"` 的 `h-9 px-3` 抄成了 default 的 `h-10 px-4`）。

还可以加一层静态兜底，对 `src/**/*.tsx` 做正则检查，命中「同一 className 串里有 `justify-between` 却无 `flex-wrap`」即失败。这能一次罩住整类问题，省去逐组件补测试。代价是会有误判，确实不该换行的两栏布局需要豁免名单。这条兜底值不值得留取决于豁免项是否够少，实现时先跑一遍看命中数再定。

## 5. 定时刷新的两项功能优化

这两项不是布局问题，是布局审计交付后用户追加提出的，一并记在这里。它们改的是 `runAutoRefresh`（`dashboard.tsx:473`）与开关状态，与前四节的布局改动互不依赖，可以分开实施。

先说清现状，避免误解：定时刷新**已经**只刷当前页。`runAutoRefresh` 取的 id 来自 `currentCredentials.filter(c => !c.disabled)`，是「当前页 + 启用」双重收窄，UI 的 title 也如实写了。所以「只针对当前页」不是待办项。

### 5.1 串行改为限并发

现状是串行 `for`，每个凭据一次 `await getCredentialBalance(id, true)`。

先说清一点：**串行是设计选择而非疏漏。** `runAutoRefresh` 里两处注释写明了理由，「防重入：上一轮未跑完就跳过本轮，不排队堆叠」和「手动批量操作优先，避免与其争抢上游」。所以这项改动是在已知取舍上调参，不是补漏。

初稿举的动机场景（当前页 12 条、间隔 30s，一轮跑不完被跳过）选得不好。反推触发所需的单次请求耗时：

| 每页 | 间隔 30s | 间隔 120s（默认） |
| --- | --- | --- |
| 12（默认） | > 2.5s | **> 10s** |
| 24 | > 1.25s | > 5s |
| 48 | > 0.62s | > 2.5s |
| 100 | **> 0.3s** | > 1.2s |

默认组合（12 条 / 120s）要单次超 10s 才会跳过，而 30s 不是默认值，需用户主动调低。真正容易踩到的是 **100 条 / 30s**，0.3s 就跳过。动机场景应换成这一组。

可行，但有几个约束要先摆出来：

**这会是仓库里第一次对同类批量循环用并发。** 五处批量操作（验活、刷新 Token、恢复异常、批量删除、批量余额）全是串行 for。唯一的 `Promise.all` 在 `settings-panel.tsx:75`，那是五个不同接口的并行读取，不是同一接口的批量循环，不构成先例。所以这一改动会引入新的并发约定，值得单独评审而不是顺手做掉。

**后端不设限流。** `src/admin/` 下没有任何 semaphore 或 rate limit，并发上限完全由前端自律。`get_balance`（`service.rs:315`）的 `balance_cache.lock()` 用块作用域包住，在 `fetch_balance().await` 之前就释放了，不存在持锁跨 await。

**但「后端并发安全」这句只对 `balance_cache` 成立，初稿漏了另一把锁。** `src/kiro/token_manager.rs:839`：

```rust
/// Token 刷新锁，确保同一时间只有一个刷新操作
refresh_lock: TokioMutex<()>,
```

这是**全局单锁**，不是 per-credential。`get_usage_limits_for` 在 token 已过期或 10 分钟内将过期（`is_token_expiring_soon`）时会拿它，而锁内包着 `refresh_token(...).await`（网络请求）和 `persist_credentials()`（磁盘写）。

锁内确有 double-check，但它比对的是**同一凭据自己**的 token 状态。不同凭据各自需要刷新时，每个都会真刷，于是这些刷新在这把全局锁上完全串行。

所以并发上限 3 的实际收益是区间而非定值：

| token 新鲜度 | 收益 |
| --- | --- |
| 全部新鲜（无需刷新） | 余额请求彼此独立，接近 3x |
| 全部临近过期 | 刷新段串行，限并发对这段无效，退到约 2x |

写进任务时应表述为「2x~3x，取决于 token 新鲜度分布」，不要写成线性加速。

**上游才是真约束。** `force=true` 会跳过缓存直接打上游，并发数就是同时打向上游的请求数。定时刷新是周期性的，并发放大后单位时间的上游请求量会显著上升，这是限流或封号风险所在，而不是本地性能问题。

建议的实现形态：

- 并发上限取 3，作为常量 `AUTO_REFRESH_CONCURRENCY` 放在文件顶部现有那组常量旁。取 3 是保守值，理由是定时刷新按周期反复执行，与一次性手动批量的风险量级不同；实测后可调，但不建议做成用户可配（多一个用户能调高的上游压力旋钮，收益不明）
- 用固定数量的 worker 从共享游标取任务，不用 `Promise.all` 切片分批。分批会在每批末尾等最慢的那个，慢请求仍会拖住整批
- `failCount` 从闭包内累加改为收集每个任务的结果再汇总，避免并发写同一个计数器
- `loadingBalanceIds` 的增删已经是函数式 `setState`，并发下本身安全，不用改
- 防重入的 `autoRefreshRunningRef` 与让位判断（`queryingInfo || verifying || batchRefreshing`）都保留，语义不变
- 翻页时正在进行的那一轮仍会刷完旧页 id。要一并收紧的话，在取下一个任务前比对当前页快照，页变了就停止取新任务。这算独立的小改动，可以不做，但如果做了要注意别把已发出的请求结果丢弃，那些数据是有效的

验证上，`getCredentialBalance` 的 mock 加上可控延迟后，断言「同一时刻在飞的请求数不超过 3」和「全部 id 都被请求过一次」。前者需要在 mock 里记录进入与退出的时间点。

### 5.2 开关状态持久化

现状是 `useState(false)`（`dashboard.tsx:69`），刷新页面就回到关闭。每页条数已经持久化到 localStorage，同一个面板里两个偏好一个记一个不记，不一致。

这里有个取舍要明确：**不持久化可能是刻意的。** 定时刷新会周期性打上游，如果用户开着它然后关掉标签页，下次进来自动恢复运行，用户未必知道后台在持续消耗配额。

复核查了代码，没找到答案：`lib/storage.ts` 只有 `getApiKey`/`setApiKey` 与 `getPerPage`/`setPerPage` 三组，无任何 autoRefresh 相关键；`dashboard.tsx:69-71` 三个 `useState` 也没有注释说明。既无证据表明是刻意保护，也无证据表明是遗漏，需要维护者拍板。

倾向做，但要连带三个保护（下面要点里的首屏不跑、恢复提示、非法值回落），这样能把上述风险降到用户可感知，而不是靠不持久化来规避。

如果确认要做，形态照搬 `storage.ts` 的现有模式（`getPerPage` 那种「读取 + 校验 + 回落默认」）：

```ts
const AUTO_REFRESH_STORAGE_KEY = 'credentialsAutoRefresh'

getAutoRefresh: (): { enabled: boolean; interval: number } => { … }
setAutoRefresh: (enabled: boolean, interval: number) => { … }
```

要点：

- 间隔一并持久化，只存开关不存间隔的话，恢复后跑的是默认 120s 而非用户设定值，比不持久化更让人困惑
- 读回时必须校验：间隔要过 `Number.isInteger` 且不小于 `AUTO_REFRESH_MIN_SECS`（10），不合法就回落默认。localStorage 的值可能被手改或是旧版本遗留，`getPerPage` 用 `PER_PAGE_OPTIONS.includes` 兜的是同一类问题
- 间隔不必限定在 `AUTO_REFRESH_PRESETS` 内，因为 UI 本来就允许自定义输入
- 恢复为「开启」时，建议首屏不立刻跑一轮，等第一个间隔到点再跑。挂载即打上游会让人措手不及
- 若采纳持久化，考虑在开关旁提示恢复状态（比如首次恢复时给一次 toast），让用户知道它在跑

验证上，预置 localStorage 再挂载 Dashboard，断言开关为开、间隔显示为存的值；另断言非法值（如 `interval: 3`，低于下限）回落到默认且不抛错。`dashboard.test.tsx` 里「下次进入页面沿用已存的每页条数」那条用例是同一写法，可直接参照。

## 6. 实施顺序建议

初稿按「后果严重度」排序，第一轮复核改为按「成本收益比」排。第二轮把第 2 节从第 3 步提到第 1 步，因为它是唯一在所有桌面分辨率下都成立的缺陷，且不等任何前置。

原先的「第 0 步（阻塞项）」下移。定最小支持宽度仍是必要的，但它只挡第 4、6 步，不该挡整份方案。

第 1、3、4、6 步已由 OpenSpec change `fix-admin-ui-layout-wrapping` 落地，第 6 步里的 M7 没做。第 2、5、7 步未动。

1. ~~**第 2 节：标题行两层容器加 `flex-wrap`**~~ **已落地** → 实测 1024px 到 1920px 下「凭据管理」横排 80px 单行、全档无横向滚动条；改前 1440px 处竖排 20px 宽 4 行且横向溢出。这里的「两层」原本漏了一层：外层 `dashboard.tsx:777` 与右侧操作区都加了，装标题的左侧容器 `:779` 没加，线上验收才发现。勾选「全选本页」后左侧子项从 2 个涨到 3 个，`nowrap` 下 320 到 337px 横向溢出 17px、560px 以下标题压成 20px 宽竖排 4 行、容器高从 36px 涨到 166px。未勾选时全档正常，所以只做静态断言而不点勾选就测不出来。折行放开后又冒出第二个问题：`h2` 与同排按钮不等高，`items-center` 换行后在行内各自居中，375px 下标题比按钮低 4px。加 `leading-9` 把 `h2` 拉到 36px 对齐。维护者随后要求给整行加 `rounded-md border p-3`。框线样式抄 `credential-filter-bar.tsx:77`，范围从「凭据管理」到「添加凭据」，这一整排都是同一组操作
2. **5.1 限并发** → verify: mock 加延迟后断言在飞请求数不超过 3、全部 id 各被请求一次。同样与屏幕宽度无关。实施时按 5.1 的修正记好 `refresh_lock` 约束，预期收益写 2x~3x 区间。作为仓库首个同类批量并发改动，值得单独一轮评审。已拆成独立 change 待排
3. ~~**HIGH-2，只为 `batch-verify-dialog` 而改，但改基类**~~ **已落地** → 实测 667px 高视口下弹窗高度封在 600px（=0.9×视口高），标题与 footer 都在视口内；改前高 1334px、上下双向被裁且不可滚动。基类的 `max-h-[90vh]` 与 `overflow-y-auto` 同批落地，`w-full` 同时改成 `w-[calc(100%-2rem)]`；4 个用 `flex flex-col` 的弹窗加 `overflow-y-visible` 抵消，逐个核过 12 处 `twMerge` 结果无双滚动条

此处插入阻塞项：定下 Admin UI 的最小支持宽度。定 768px 则第 4、6 步作废；定 375px 则只做第 4 步；定 320px 则全做。落地时按 375px 处理，因为修正后的档位表显示 M1 / M6 在 375px 都触发。

4. ~~**HIGH-1 顶栏 + M5 `break-words`**~~ **已落地** → 实测 320px 到 1920px 全档无横向滚动条，`min-h-14` 在 640px 及以上保持 56px、640px 以下长到 72px、320px 处 120px。M5 三处补了 `break-words`。顶栏 SVG 没加 `shrink-0`，按 LOW 一节的三组对照，`flex-wrap` 落地后压缩本就不再发生
5. **2.1 收纳低频操作进溢出菜单** → verify: 1280px 到 1920px 全档位右侧单行、行高 74px。前置是维护者确认候选名单（`Kiro Account Manager 导入`、`批量导入`、`在线授权`、`刷新全部模型`、`清除已禁用`）。它与第 1 步是叠加关系，单独用仍竖排，所以必须排在第 1 步之后。已拆成独立 change 待排
6. ~~**M1 / M6**~~ **已落地**，**M7 未做** → M1 统计行加 `flex-wrap` 后每项恒 20px 高，320px 处折 2 行也不再压成竖条；M6 改 `grid-cols-1 sm:grid-cols-2` 后长标签全档单行。M7 卡在 640px 那段 `sm:max-w-md` 收窄上，改列数救不了，得连弹窗宽度一起定，超出本次范围
7. 决定是否加静态正则兜底 → verify: 先看首轮命中数与豁免项数量
8. **从清单删除或改写为「已正确处理」**：proxyUrl（无损换行非截断）、顶栏 SVG 独立项（并入 HIGH-1）、M2、M4、`models-refresh-result-dialog`、L1、L2。这些复核后是误报或已被其他项覆盖。`balance-dialog` 从这份删除名单里移出，它的 M7 是真缺陷，只是本次没改

定时刷新的 5.2 单独排，它等的是另一个拍板：

- **5.2 开关持久化** → verify: 预置 localStorage 挂载后开关为开、间隔为存值；非法间隔（如 3，低于下限 10）回落默认且不抛错。前置是维护者确认「不持久化」是遗漏而非刻意保护，复核没能从代码判定（见 5.2）。形态有 `storage.getPerPage` 的现成模式可照搬，`dashboard.test.tsx` 里「下次进入页面沿用已存的每页条数」那条用例是同一写法

关于 OpenSpec change：第 3、5 步是纯 className 调整，不碰 Admin API 与凭据管理契约；5.1 与 5.2 改了行为契约（并发模型、跨会话状态），这两项倾向要按 `AGENTS.md` 建 change。最终由维护者定。
