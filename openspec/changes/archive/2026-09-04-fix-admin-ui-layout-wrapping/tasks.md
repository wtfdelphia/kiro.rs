## 1. 凭据管理标题行折行

- [x] 1.1 `dashboard.tsx` 标题行外层容器（`flex items-center justify-between`）加 `flex-wrap gap-y-2`
- [x] 1.2 同处右侧控件容器（`flex gap-2`）加 `flex-wrap justify-end`
- [x] 1.3 `dashboard.test.tsx` 断言两层容器各含 `flex-wrap`

## 2. 顶栏折行

- [x] 2.1 `dashboard.tsx` 顶栏容器加 `flex-wrap gap-y-2`，`h-14` 改 `min-h-14`
- [x] 2.2 顶栏右侧控件容器加 `flex-wrap justify-end`
- [x] 2.3 `dashboard.test.tsx` 断言顶栏外层含 `flex-wrap` 与 `min-h-14`、右侧容器含 `flex-wrap`。「不含 `h-14`」必须写成 `expect(el.className.split(' ')).not.toContain('h-14')`，不能用 `not.toContain('h-14')` 或 `/\bh-14\b/`：两者对 `min-h-14` 都判为命中，会把正确实现判成失败

## 3. 弹窗高度上限与滚动通路

- [x] 3.1 `ui/dialog.tsx` 的 `DialogContent` 基类加 `max-h-[90vh] overflow-y-auto`，`w-full` 改 `w-[calc(100%-2rem)]`
- [x] 3.2 给 4 个 `flex flex-col` 弹窗加 `overflow-y-visible` 抵消基类滚动：`add-credential-dialog.tsx:111`、`online-auth-dialog.tsx:236`、`kam-import-dialog.tsx:240`、`batch-import-dialog.tsx:313`
- [x] 3.3 逐个核对 12 处 `<DialogContent>` 经 `twMerge` 后的最终 className，确认无双滚动条、无宽度回退。清单：`settings-panel:183`、`public-api-panel:91`、`add-credential:111`、`online-auth:236`、`kam-import:240`、`batch-import:313`、`batch-verify:40`、`models-refresh-result:27`、`credential-models:94`、`credential-test:131`、`balance:31`、`credential-card:577`
- [x] 3.4 新增 `ui/dialog.test.tsx`，断言基类同时含 `max-h-[90vh]` 与 `overflow-y-auto`（缺一则改动无效），并断言含 `w-[calc(100%-2rem)]`
- [x] 3.5 断言 4 个 `flex flex-col` 弹窗含 `overflow-y-visible`

## 4. 长文本换行

- [x] 4.1 `batch-verify-dialog.tsx:104` 错误信息节点加 `break-words`
- [x] 4.2 `kam-import-dialog.tsx:277` 的预览行（含 `item.path` 与 `item.error`）加 `break-words`
- [x] 4.3 `kam-import-dialog.tsx:346` 结果区的 `result.error` 节点加 `break-words`（父级同为 `overflow-y-auto`，与 4.2 同源）
- [x] 4.4 三处各补 className 断言

## 5. M1 统计行与 M6 超时网格折行

- [x] 5.1 `kam-import-dialog.tsx:304` 统计行 `flex gap-4 text-sm` 加 `flex-wrap`，对齐 `batch-import-dialog.tsx:354` 的同款写法
- [x] 5.2 `settings-panel.tsx:352` 的 `grid grid-cols-2 gap-2` 改 `grid grid-cols-1 gap-2 sm:grid-cols-2`，对齐同文件 `:238` 的写法
- [x] 5.3 不改 `settings-panel.tsx:208`、`add-credential-dialog.tsx:172`、`:291`：这些格子里装的是 `Input`（基类 `w-full`，无文本节点，不折行），320px 下列宽 131px 已容得下最长 placeholder 的 126px
- [x] 5.4 不改 `settings-panel.tsx:335`：第二列是 `text-xs` 说明文本「活跃连接：N（只读）」，自然宽 123.3px，320px 下列宽 131px，实测 320px 到 640px 全档单行。第一列 `Select` 的最长选项「passthrough（预留）」143.1px 会在触发器内截断，属组件既有行为，与列数无关
- [x] 5.5 补 className 断言：统计行含 `flex-wrap`，超时网格含 `grid-cols-1` 与 `sm:grid-cols-2`

## 6. 验收

- [x] 6.1 `pnpm test` 全绿，`pnpm build` 无新增告警
- [x] 6.2 `pnpm build` 后用 headless Chrome 加载构建产物核一次实测几何（一次性验收，不进 CI）。测前先 `grep -c` 确认新类已进 CSS：`w-\[calc`、`min-h-14`、`overflow-y-visible`、`gap-y-2`、`sm:grid-cols-2` 在改动前的产物里全部 0 命中，Tailwind JIT 只生成源码扫到的类，用旧产物测改后样式会得到类缺失的回落值
- [x] 6.3 实测项：1440px 与 1920px 下标题行 `h2` 横排且宽约 80px、无横向滚动条；375px 下顶栏无横向滚动条；667px 高视口选中 10 条以上打开批量验活弹窗，标题与 footer 都可达；375px 下 kam-import 统计行与 settings 超时网格折行整齐不错位
- [x] 6.4 `cargo check --release --all-targets` 零新增告警（确认前端改动未牵连 Rust 侧的静态资源嵌入）
- [x] 6.5 `git status --short` 核对，确认无 `config.json` / `credentials.json` / `.codegraph/` 混入
- [x] 6.6 更新设计文档 `docs/admin-ui-responsive-layout-optimization-design.md`：把第 6 节已完成的步骤标为已落地；修正 `:158`-`:162` 触发档位表与 `:259`、`:260`、`:437` 里「M1 / M6 / M7 只在 320px 触发」的错误结论，换成本次实测的分档数据

## 7. 线上验收补漏

- [x] 7.1 部署后核对二进制内嵌资源哈希与 `admin-ui/dist` 一致：RustEmbed 在编译期打包，只跑 `cargo check` 不产出新二进制，改前的产物里 `min-h-14` 与 `w-[calc(100%-2rem)]` 都是 0 命中
- [x] 7.2 补上 `dashboard.tsx:779` 左侧容器的 `flex-wrap`：外层 `:777` 已加而内层漏了，勾选后子项从 2 个涨到 3 个，`nowrap` 把标题压成竖排。实测 320 到 337px 横向溢出 17px、560px 以下 `h2` 竖排 4 行、内层高从 36px 涨到 166px
- [x] 7.3 补 `dashboard.test.tsx` 断言：左侧容器含 `flex-wrap`，并新增一例先点「全选本页」再断言，覆盖只在勾选后暴露的状态。回退源码验证两例都会失败
- [x] 7.4 给 `h2` 加 `leading-9`：`items-center` 只在行内按各自高度居中，`h2` 默认 28px 而同排按钮 36px，不换行时看不出，换行后错开 4px（实测 375px 下 `h2` top=487、按钮 top=483）。筛选栏换 4 行不歪是因为七个子项高度全是 36px，照它对齐即可
- [x] 7.5 给标题行 `dashboard.tsx:777` 加 `rounded-md border p-3`：从「凭据管理」到「添加凭据」是一整组操作区，框线样式取自 `credential-filter-bar.tsx:77`，有了边界才跟下方列表分得开。试过只框左侧，右半边光着不对称；框整行时右侧总有按钮，未勾选态看着也不空。实测 1440px 勾选态框高 152px 含 15 项、375px 折成 460px

## 8. 第二轮：收纳与批量工具栏行

第一轮只加了 `flex-wrap`，真实部署（172.20.66.24:18990）复测确认竖排消失但混排依旧：选中态右组 12 控件折 2 到 3 行、整行高 152px（375px 下 460px）、左组被 `justify-between` 顶成独占一行。根因是右侧需求约 1914px 恒超容器上限 1472px，折行不减少控件数。本轮按设计文档 2.1 落地收纳，并追加批量工具栏行拆分。

- [x] 8.1 新增 `ui/dropdown-menu.tsx` 薄封装（`@radix-ui/react-dropdown-menu` 为既有依赖，参照 `ui/select.tsx` 的封装形态）
- [x] 8.2 5 个低频操作（KAM 导入、批量导入、在线授权、刷新全部模型、清除已禁用）从标题行收进「更多操作」溢出菜单，保留各自可用性判据
- [x] 8.3 批量操作四项（批量验活、批量刷新 Token、恢复异常、批量删除）与批量余额/订阅拆出独立工具栏行（`role="toolbar"`），有凭据时常驻，未选中时只有批量余额/订阅
- [x] 8.4 更新既有测试：清除已禁用四项用例改从溢出菜单查询；新增批量工具栏归属断言与溢出菜单内容断言
- [x] 8.5 playwright 复测最终几何：≥1280px 选中态标题行单行 64px、批量工具栏单行；375px 无横向滚动。证据存 `evidence/toolbar-refactor/`
- [x] 8.6 部署后线上复测（2026-09-04，172.20.66.24:18990，二进制含 `index-HfGrMh8b.js`）：1280 到 2560px 全选后标题行单行 64px、工具栏行单行 54px；768 与 1024px 标题行 108px、工具栏单行；375px 无横向滚动。截图存 `evidence/toolbar-refactor/deployed/`
