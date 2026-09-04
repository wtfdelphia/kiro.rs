# 线上验收记录（172.20.66.24，2026-08-27）

对应 tasks 7.6 与 `docs/admin-ui-credential-list-display-and-query-optimization-design.md` 第 8.4 节。

环境：新二进制 18:03 上线，pm2 `kiro-rs` pid 2566330，监听 `172.20.66.24:18990`。启动日志中 10 个凭据的 Token 与模型缓存全部刷新成功，error log 为空。线上 10 个凭据，`available=7`，禁用 3 个（id 3/4/5）。

## 接口侧（已核对）

| 项 | 判据 | 实测 | 结果 |
| --- | --- | --- | --- |
| 8 | `?perPage=4` 返回 4 条，`pageInfo` 为 `{page:1, perPage:4, filteredTotal:10, totalPages:3, hasPrev:false, hasNext:true}` | 完全一致，ids `[1,2,3,4]` | 通过 |
| 10 | `?perPage=4&page=99` 得 200、空数组、`hasNext:false` | 200，`rows=0`，`page` 回显 99，`totalPages:3`，`hasPrev:true`、`hasNext:false` | 通过 |
| 11 | `?perPage=500` 的 `pageInfo.perPage == 100` | 100 | 通过 |
| 12 | 原判据是「单页时省略 `Link` 头」；评审否决整个 `Link` 方案后，改为「任何请求都不写该头」 | 多页请求 `?perPage=4&page=2` 的 `Link` 头也是 `None` | 通过 |
| 5 | Profile 筛选「无 Profile ARN」命中 10 条 | `?hasProfileArn=false` 得 10 条，`?hasProfileArn=true` 得 0 条 | 通过 |
| 6 | 六类筛选各自生效，组合为 AND | `disabled=false` 7 条 / `disabled=true` 3 条（ids 3,4,5）/ `authMethod=social` 1 条 / `authMethod=idc` 8 条 / `priorityMin=0&priorityMax=0` 5 条；`disabled=false&perPage=4` 两页分别 4 条与 3 条，`filteredTotal:7` | 通过 |
| 7 | 筛选时 `total` / `available` 保持全量口径，筛选提示用 `filteredTotal` | 所有筛选查询的 `total=10`、`available=7` 不变，只有 `filteredTotal` 随筛选变化 | 通过 |
| 14 | 带筛选翻页时筛选参数保持不变 | `disabled=false` 翻到第 2 页仍只回未禁用项（ids 8,9,10） | 通过 |

补充核对（规范里有、8.4 未列的边界）：

- `page=0` 与 `page=-1` 按 1 处理，返回第一页
- `perPage=0` 与 `perPage=-5` 回落默认 12
- `page=abc`、`page=1.5`、`disabled=yes` 返回 400（类型错误不属于 clamp 承诺范围）
- `subscriptionTitle=__unknown__` 得 0 条，与线上 10/10 都有订阅等级吻合
- `(priority, id)` 全序为 `[(0,1),(0,2),(0,3),(0,4),(0,5),(1,6),(1,7),(2,8),(2,9),(2,10)]`，稳定
- `facets` 返回 `subscriptionTitles: [KIRO FREE, KIRO PRO, KIRO PRO+]`、`authMethods: [api_key, idc, social]`，与实际分布一致

## 首屏字段完整性（第 1/2/3 项，接口侧）

不带任何参数请求一次的结果：订阅等级 10/10 有值，email 9/10，id 10/10，`profileArn` 0/10，余额缓存 7/10（3 个已禁用项无缓存）。认证方式分布 `idc` 8 / `social` 1 / `api_key` 1，订阅等级分布 `KIRO PRO+` 4 / `KIRO PRO` 3 / `KIRO FREE` 3。

## 渲染侧（已核对）

第 2/4/13/15 项是渲染行为，用 Playwright 驱动 Chromium 打开 `http://172.20.66.24:18990/admin` 核对，13 条断言全过。

| 项 | 判据 | 实测 |
| --- | --- | --- |
| 2 | 余额显示缓存值并带新鲜度标注 | 缓存标注 7 处（如「缓存已过期（25 分钟前）」），余额数值 7 处（`1360.26 / 2000.00`、`49.93 / 50.00`、`1999.61 / 2000.00`），与接口侧余额缓存 7/10 吻合 |
| 4 | 徽章区不同时出现 `IdC` 与 `BuilderId` | 10 张卡里 8 张带 `IdC`，同时带 `BuilderId` 的 0 张 |
| 4 | endpoint 等于默认端点时徽章消失 | 带端点徽章的卡 0 张 |
| 13 | `perPage=4` 时页码条呈现 `1 2 3` | 按钮序列 `上一页 / 1 / 2 / 3 / 下一页` |
| 13 | 第 3 页得 2 条 | `#9`、`#10` |
| 13 | `aria-current` 只标当前页 | 1 个，文本为 `3` |
| 13 | 刷新后 `page` 与 `perPage` 从查询串恢复 | URL `?page=3&perPage=4`，当前页 3，卡片 2 张 |
| 13 | perPage 落 localStorage | 选 UI 可选值「24 条/页」后 `credentialsPerPage` = `24` |
| 15 | Tab 依次聚焦上一页、各页码、下一页 | `Page 1 → Page 2 → Page 3 → 下一页 → 每页条数下拉` |
| 15 | 省略号被跳过 | 两个省略号都带 `aria-hidden="true"`，Tab 从 `Page 10` 落到 `Page 11 → Page 12 → Page 20 → 下一页` |

十张卡的徽章实况：`#1 [当前, IdC]`、`#2 [IdC]`、`#3/#4/#5 [已禁用, IdC]`、`#6 [API Key]`、`#7 [IdC]`、`#8 [Social, Github]`、`#9/#10 [IdC]`。

第 13 项刷新后 `localStorage.credentialsPerPage` 为空。4 不在 UI 可选值 `[12, 24, 48, 100]` 内，`admin-ui/src/lib/storage.ts` 的 `getPerPage` 会把它回落成默认值，属于既定的 clamp 行为，所以另跑了一条走可选值 24 的断言确认写库路径正常。

第 15 项的省略号线上触发不了：10 个凭据按 `perPage=4` 只有 3 页，页码条不省略。改用 `page.route` 拦截响应、只把前端收到的 `pageInfo.totalPages` 改成 20，等省略号真实渲染出来再核对焦点落点，线上数据没动。

对应的渲染逻辑在 `admin-ui/src/components/pagination-bar.test.tsx` 与 `dashboard.test.tsx` 里有单测覆盖。
