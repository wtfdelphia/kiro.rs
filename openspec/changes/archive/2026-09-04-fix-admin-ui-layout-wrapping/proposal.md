## Why

Admin UI 的「凭据管理」标题行在任何视口宽度下都是坏的。右侧最多 12 个控件全带 `whitespace-nowrap`，整行需求 2207px，而 `.container` 在 1536px 封顶、扣掉 `px-8` 后内容宽上限只有 1472px，恒缺 735px。压缩全落在唯一可压项（中文 `h2`）上，于是「凭据管理」四个字被压成 20px 宽、112px 高的竖排。实测 320px 到 6000px 每一档都是竖排，1920px 桌面同样如此，加宽视口不能消除。

同批还有四项同源缺陷：顶栏两侧都无 `flex-wrap`，375px 下横向溢出 62px，登出与设置按钮被推出视口不可点；共享 `DialogContent` 基类既无高度上限也无滚动容器，`batch-verify-dialog` 在 667px 高视口下打开即溢出约 40px，标题与 footer 都点不到；三处错误信息与文件路径缺 `break-words`，父级 `overflow-y-auto` 使列表内部冒出横向滚动条；`kam-import-dialog` 的统计行与 `settings-panel` 的超时参数网格在 375px 到 414px 之间折行错位。

审计与实测过程见 `docs/admin-ui-responsive-layout-optimization-design.md`。

### 第一轮落地后的复测：折行只止血，没收住根因

第一轮改动上线后（部署 172.20.66.24:18990，产物哈希与本地构建一致），用真实凭据逐视口复测「全选本页」后的标题行几何：竖排消失，但排列仍然乱。选中态右组 12 个控件仍折成 2 到 3 行，整行高 152px（375px 下 460px），批量操作与全局操作交错混排；左组（标题 + 全选按钮 + 已选徽章，511px）与右组放不进同一行，被 `justify-between` 顶成独占一行，右侧大片留白。根因没动：选中态右侧需求约 1914px，加左组恒超容器上限 1472px。`flex-wrap` 解决「不可读」，不解决「12 个控件挤在标题行」。

这正是设计文档 2.1 预判并推迟的收纳方案。复测推翻了推迟的前提：只加 `flex-wrap` 时，选中态在任何桌面宽度都占 2 到 3 行，标题区吃掉 118 到 152px 垂直空间。用户反馈的「菜单排列还是存在问题」指向的就是这个。

## What Changes

- 「凭据管理」标题行两层容器加 `flex-wrap`，右侧控件区允许折行。实测 1440px 与 1920px 下 `h2` 恢复横排 80px、无横向滚动条，已选中态行高从 166px 降到 118px
- 顶栏外层加 `flex-wrap gap-y-2`、右侧容器加 `flex-wrap justify-end`、`h-14` 改 `min-h-14`（换行后高度要能长）
- `DialogContent` 基类加 `max-h-[90vh]`，并同批补齐滚动容器。两者必须一起落地：基类是 `grid` 且 `overflow: visible`，只加 `max-h` 时边框盒被钳制而 grid 行仍按内容撑开，`translate-y-[-50%]` 参照钳制后的边框盒，footer 会比改前更远离视口（178.5px vs 72.5px）
- `DialogContent` 基类 `w-full` 改 `w-[calc(100%-2rem)]`，让 320px 下弹窗不贴满两边、`sm:rounded-lg` 视觉生效
- `batch-verify-dialog` 与 `kam-import-dialog` 的两处错误信息、`item.path` 文件路径共三处加 `break-words`
- `kam-import-dialog` 的 5 项统计行加 `flex-wrap`（设计文档 M1）。同仓库 `batch-import-dialog.tsx:354` 同款统计行已有 `flex-wrap`，此处属漏写
- `settings-panel` 的 WebSocket 超时参数网格改 `grid-cols-1 sm:grid-cols-2`（设计文档 M6）
- 补 className 断言测试。jsdom 不做布局计算，真实几何无法观测，判据只能是静态类名
- 第二轮：5 个低频操作（KAM 导入、批量导入、在线授权、刷新全部模型、清除已禁用）收进「更多操作」溢出菜单，新增 `ui/dropdown-menu.tsx` 薄封装。依赖 `@radix-ui/react-dropdown-menu` 已在 `package.json`，`ui/select.tsx` 是同款先例
- 第二轮：批量操作（批量验活、批量刷新 Token、恢复异常、批量删除）与批量余额/订阅拆出独立工具栏行。行内有凭据时常驻，未选中时只有批量余额/订阅，选中后追加四项批量操作。标题行只留全局操作

M1 与 M6 原本按「只在 320px 触发」排除，复测推翻了这个前提。用真实源码文本加载构建产物 CSS 逐 1px 实测，M1 统计行改前要到 503px 视口才恢复单行，375px 处每项 60px 高、320px 处 80px 高；M6 的 `turn 间空闲超时（秒，0=不启用）` 在 `text-xs` 下自然宽 190.2px，改前列宽要到 471px 视口才够。两处都落在 375px 支持范围内，各一处 className，纳入本次。

不在本次范围：定时刷新限并发与开关持久化（第 5 节）、M7（`balance-dialog` 用 `sm:max-w-md`，640px 处弹窗反而收窄到 448px、列宽掉到 191px 重新折行，改列数救不了这段，得连弹窗宽度一起定）、M6 剩余四处 `grid-cols-2`（`settings-panel:208`、`add-credential:172`、`:291` 装的是 `Input`，基类 `w-full` 不折行，320px 下列宽 131px 仍容得下最长 placeholder；`settings-panel:335` 的说明文本在 `text-xs` 下 123.3px，全档单行）。溢出菜单收纳原列为非目标，第二轮复测后纳入本次，理由见 Why 补充节。

## Capabilities

### New Capabilities

- `admin-ui-responsive-layout`: Admin UI 的响应式布局契约。约束多控件横向容器必须可折行、弹窗必须有高度上限与配套滚动容器、不可断文本必须可换行，以及最小支持视口宽度为 375px

### Modified Capabilities

无。现有 26 个 spec 描述的都是数据契约与交互语义，本次只改布局约束，不触碰任何既有 requirement。

## Impact

代码：

- `admin-ui/src/components/dashboard.tsx`：顶栏容器、「凭据管理」标题行两层容器
- `admin-ui/src/components/ui/dialog.tsx`：`DialogContent` 基类
- 5 个未补 `max-h` 的弹窗（`batch-verify-dialog`、`models-refresh-result-dialog`、`credential-models-dialog`、`credential-test-dialog`、`balance-dialog`）与 4 个用 `flex flex-col` 的弹窗（`add-credential`、`online-auth`、`kam-import`、`batch-import`），随基类改动一并核对，避免双滚动条
- `admin-ui/src/components/batch-verify-dialog.tsx`、`kam-import-dialog.tsx`：`break-words` 三处
- `admin-ui/src/components/kam-import-dialog.tsx`：统计行 `flex-wrap`
- `admin-ui/src/components/settings-panel.tsx`：WebSocket 超时参数网格
- 新增 / 扩充对应的 `*.test.tsx` className 断言

无 API、无新增依赖、无数据结构变化。`@radix-ui/react-dropdown-menu` 是既有直接依赖。纯前端改动，Rust 侧不受影响。

风险点有两个。`DialogContent` 基类一处改动覆盖全仓库 12 个 `<DialogContent>`，其中 6 个已自带 `max-h-*`，需要逐个确认新基类不与既有 className 冲突、不产生双滚动条。第二轮的工具栏行拆分改变了「清除已禁用」「批量余额/订阅」等入口的 DOM 位置与交互形态，既有测试的查询方式随之调整。
