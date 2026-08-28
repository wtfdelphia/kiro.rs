## 0. 前置：前端渲染测试基建

实现时发现的缺口：`admin-ui` 原本只有纯 `.ts` 逻辑测试，无 jsdom 也无 `@testing-library/react`，而下列 verify 条件要求 DOM 层断言——1.8（三态文案）、2.2（`title` 属性）、6.4（翻页时上一页内容仍在）、6.7（边界按钮禁用态而非移除）、6.8（`aria-current` / 省略号不可聚焦 / `aria-live`）、6.10（勾选三态）、6.11（已选计数文案）、6.13（四处反查的交互路径）、6.19（按钮可用性）。不补基建则这些条件只能靠人工检查。

- [x] 0.1 新增 devDependency `jsdom` / `@testing-library/react` / `@testing-library/jest-dom` / `@testing-library/user-event`，`vite.config.ts` 配 `test.environment: 'jsdom'` 与 `setupFiles`，新增 `src/test/setup.ts` 挂 jest-dom 匹配器并在 `afterEach` 做 `cleanup` → verify: `pnpm test` 既有 31 条用例全绿且 setup 阶段生效，`pnpm build` 类型检查通过

## 1. 阶段一：列表契约暴露既有数据（P4，非 breaking，可独立发布）

- [x] 1.1 在 `src/kiro/token_manager.rs` 的 `CredentialEntrySnapshot` 增加 `subscription_title: Option<String>`，并在 `snapshot()` 中填充 → verify: 新增单测断言快照带出该字段，`cargo test` 通过
- [x] 1.2 在 `src/admin/types.rs` 新增 `CredentialBalanceSnapshot`（`subscriptionTitle` / `currentUsage` / `usageLimit` / `remaining` / `usagePercentage` / `nextResetAt` / `cachedAt` / `ageSecs` / `stale`），`cachedAt` 与 `nextResetAt` 同为 Unix 秒（`f64`），`nextResetAt` 用 `skip_serializing_if` → verify: `cargo check --release --all-targets` 零新增告警，单测断言 `cachedAt` 序列化为数字而非字符串
- [x] 1.3 在 `CredentialStatusItem` 增加 `subscription_title` 与 `balance`，`balance` 缓存未命中时省略字段 → verify: 单测断言无缓存凭据的 JSON 不含 `balance` 键
- [x] 1.4 在 `src/admin/service.rs` 构造列表项时一次性 `self.balance_cache.lock().clone()` 取整张余额表，按 id 查表生成快照，`stale` 由 `age_secs > BALANCE_CACHE_TTL_SECS` 判定 → verify: 单测断言 TTL 内 `stale=false`、超 TTL `stale=true`
- [x] 1.5 单测确认列表接口不触发上游：构造无网络的 service，请求列表不产生额度查询、不刷新缓存 TTL → verify: 对应 spec scenario「列表请求不触发上游调用」的用例通过
- [x] 1.6 前端 `admin-ui/src/types/api.ts` 补 `subscriptionTitle` 与 `CredentialBalanceSnapshot` 类型 → verify: `pnpm build` 类型检查通过
- [x] 1.7 前端实现 `BalanceView` 三态视图模型（`live` / `cached` / `none`），抽为可单测的纯函数 → verify: `pnpm test` 覆盖三态各一条用例
- [x] 1.8 `credential-card.tsx` 按三态渲染订阅等级与用量：缓存态标注相对时间、`stale` 加过期提示、无数据显示「未查询」而非「未知」 → verify: `pnpm test` 断言三态文案，且实时结果覆盖缓存后新鲜度标注消失
- [x] 1.9 实现订阅等级三级取值链（实时查询 → 顶层 `subscriptionTitle` → 快照内订阅等级），抽为纯函数 → verify: `pnpm test` 覆盖顶层优先于快照、仅快照有值时回落、三者全缺显示「未知等级」三条用例

## 2. 阶段二：卡片展示收敛（P1 / P2，纯前端，非 breaking）

- [x] 2.1 卡片标题改为「身份名 + `#id`」共存，身份名按 `email` → `nickname` → `userId` 兜底，全缺时显示「未获取身份」 → verify: `pnpm test` 覆盖四条兜底分支
- [x] 2.2 长身份名截断并保留完整值于 `title` 属性，`#id` 不参与截断 → verify: `pnpm test` 断言 `title` 属性存在且 `#id` 完整
- [x] 2.3 实现 `isProviderRedundant`，当 provider 可由 `authMethod` 推导时省略 provider 徽章；`external_idp` 恒判定为不可推导 → verify: `pnpm test` 覆盖 idc+BuilderID 省略、social+Google 保留、external_idp+Azure 保留三条用例
- [x] 2.4 `credential-card.tsx` 的 `authMethod` 徽章文案补齐 `external_idp`，四个取值均不落裸值兜底 → verify: `pnpm test` 断言四个取值各有可读文案
- [x] 2.5 接入 `GET /api/admin/settings/endpoint` 取默认 endpoint，仅在凭据 endpoint 与之不同时渲染 endpoint 徽章；该请求失败时恒渲染 → verify: `pnpm test` 覆盖相同省略、不同渲染、请求失败保守渲染三条用例。「不同」这一分支在当前单端点部署下不会真实出现，用构造入参在纯函数层测，不做端到端验证

## 3. 阶段三前置：排序全序化（breaking，与第 4-8 组同批发布）

- [x] 3.1 把 `src/admin/service.rs` 的 `sort_by_key(|c| c.priority)` 改为 `sort_by_key(|c| (c.priority, c.id))` → verify: 新增单测 `pagination_stable_across_duplicate_priority`，构造同优先级凭据逐页取回，断言各页 id 集合两两不相交且并集等于全集
- [x] 3.2 新增单测断言同一页重复请求返回顺序完全一致 → verify: 对应 spec scenario「排序与请求次数无关」的用例通过

## 4. 阶段三：后端筛选（P3）

- [x] 4.1 在 `src/admin/types.rs` 新增 `CredentialsQuery`，含 8 个筛选字段与 `page` / `per_page`，`page` 与 `per_page` 用 `Option<i64>` 以便负值进入 clamp 而不在提取阶段 400，`#[serde(rename_all = "camelCase")]` + `Default` → verify: `cargo check --release --all-targets` 零新增告警
- [x] 4.2 实现 `matches_query`：`subscriptionTitle` 精确且大小写不敏感、`__unknown__` 哨兵匹配空值、`email` 与 `id` 子串大小写不敏感、优先级闭区间、`disabled` / `authMethod` / `hasProfileArn` 精确，`authMethod` 覆盖 `social` / `idc` / `external_idp` / `api_key` 四个取值 → verify: spec 中筛选相关的 8 条 scenario 各有对应单测通过
- [x] 4.3 `get_all_credentials` 改造为 `query_credentials(&CredentialsQuery)` + 薄封装，内部严格按 `filter → sort → paginate` 顺序 → verify: `cargo test` 通过
- [x] 4.4 核对 `src/admin/service.rs:1956-2000` 两个既有列表单测在默认分页下的行为：两者都用 `manager_with_one()`，只有 1 条凭据，`perPage=12` 完全容纳，`total == 1` 与 `credentials[0].model_count` 照旧成立，预期不需要改动 → verify: `cargo test` 全绿；若实际出现失败，先确认是分页逻辑写错还是断言口径过窄，不要直接改断言迁就实现
- [x] 4.5 `src/admin/handlers.rs` 接收 `Query<CredentialsQuery>` 并传入 service → verify: 集成层请求带筛选参数后返回筛选结果

## 5. 阶段三：后端分页（P5，BREAKING）

- [x] 5.1 新增 `PageInfo`（`page` / `perPage` / `filteredTotal` / `totalPages` / `hasPrev` / `hasNext`），加入 `CredentialsStatusResponse`；`page` 小于 1 时回显 clamp 后的 1，越过末页时 clamp 不生效、原样回显请求值（措辞按 8.3 修正，原写法「不回显请求原值」对越界页是错的） → verify: `cargo check --release --all-targets` 零新增告警；spec 中「分页导航由 pageInfo 单一提供」的 3 条 scenario 各有对应单测通过
- [x] 5.2 实现分页与 clamp：`page` 缺省 1，为 0 或负值时 clamp 为 1；`perPage` 缺省 12、为 0 或负值时取默认、超 100 时 clamp 为 100；越界页返回空数组且状态码 200，`hasPrev` 仍为 true → verify: spec 中分页相关的 7 条 scenario（零凭据归 5.3）各有对应单测通过，含 `page=-1` 回显 `pageInfo.page=1`
- [x] 5.3 单测断言零凭据时 `totalPages=0`、`hasPrev` 与 `hasNext` 均为 false → verify: 对应 scenario 用例通过
- [x] 5.4 单测断言 `total` / `available` 不受筛选影响，`filteredTotal` 为筛选后切页前条数；`available` 维持「未禁用凭据数」既有口径，不因额度耗尽减少 → verify: 对应 scenario「筛选不改变 total 与 available」与「available 不受额度耗尽影响」的用例通过
- [x] 5.5 单测断言分页不改动单个列表项的字段契约：切页后每项仍带 `modelCount`、`modelsUpdatedAt`、`subscriptionTitle`、`balance`（命中缓存时） → verify: 对应 scenario 用例通过，`admin-ui-model-ops` 既有约束不被收窄
- [x] 5.6 新增 `GET /api/admin/credentials/facets`，返回全集去重的 `subscriptionTitles` 与 `authMethods`（含 `external_idp`），纯内存无 I/O → verify: spec 中 facets 2 条 scenario 用例通过
- [x] 5.7 在 `src/admin/router.rs` 注册 facets 路由 → verify: 路由可达，返回 200

## 6. 阶段三：前端筛选与分页

- [x] 6.1 新增 `admin-ui/src/lib/pagination.ts` 的纯函数 `buildPageItems(current, totalPages, marginPageCount, surroundingPageCount)`，返回 `(number | 'ellipsis')[]`，折叠区间只剩一页时渲染该页码而非省略号 → verify: `pnpm test` 覆盖多页折叠、单页间隔不折叠、页数少时无省略号三条用例
- [x] 6.2 `admin-ui/src/lib/storage.ts` 增加 `getPerPage` / `setPerPage`，仿现有 `getApiKey` 模式 → verify: `pnpm test` 断言读写往返
- [x] 6.3 `admin-ui/src/api/credentials.ts` 的 `getCredentials` 接受查询对象并序列化为查询串 → verify: `pnpm test` 断言生成的 URL 参数
- [x] 6.4 `use-credentials.ts` 改为 `queryKey: ['credentials', query]` 并加 `placeholderData: (prev) => prev` → verify: `pnpm test` 断言翻页请求进行中上一页内容仍在
- [x] 6.5 `dashboard.tsx` 移除 `currentPage` / `itemsPerPage = 12` 硬编码与 `slice` 假分页，改为消费服务端 `pageInfo` → verify: `pnpm test` 断言不再对完整数组切片
- [x] 6.6 实现筛选栏七个维度控件，文本类输入 300ms 防抖，下拉可选值取自 facets 端点，改任一条件重置到第 1 页 → verify: `pnpm test` 覆盖筛选下发、防抖只发一次、可选值覆盖全集、改条件回第一页四条用例
- [x] 6.7 实现分页控件：页码条 + 上下页 + 每页条数选择器，边界按钮渲染为禁用态而非移除，每页条数变更重置到第 1 页，页码越界自动回退末页 → verify: `pnpm test` 覆盖禁用态保留、改每页条数回第一页、越界回退三条用例
- [x] 6.8 无障碍：`nav aria-label`、页码 `aria-label="Page N"`、当前页 `aria-current="page"`、省略号 `aria-hidden` 且不可聚焦、`aria-live` 播报页码变化 → verify: `pnpm test` 覆盖当前页可识别、键盘遍历跳过省略号两条用例
- [x] 6.9 筛选与分页状态同步到 URL 查询串，用 `history.replaceState`（不引入 `react-router`），页面加载时从查询串恢复 → verify: `pnpm test` 断言状态写入与恢复往返一致
- [x] 6.10 新增「全选」按钮（`dashboard.tsx` 现只有逐个 `toggleSelect` 与 `deselectAll:172`，无全选）：作用域限定为当前页返回的凭据，不隐式扩展到筛选后全集；已选凭据跨页保留，翻页不清空；勾选态三值（未选 / 部分选 / 全选），点击时按当前页是否已全选决定全选或取消本页选择 → verify: `pnpm test` 覆盖全选只覆盖当前页、翻页后已选不丢、部分选态可识别、再次点击只取消本页四条用例
- [x] 6.11 已选计数展示区分「已选 N 条」与「当前页 M 条」，N 为跨页累计 → verify: `pnpm test` 断言第 1 页选 3 条后翻到第 2 页再选 2 条时显示已选 5 条
- [x] 6.12 选中状态由 `Set<number>` 升级为 `Map<number, CredentialSelection>`，`CredentialSelection` 至少含 `id` / `disabled` / `failureCount`（批量恢复要判失败次数，不止禁用状态），在勾选那一刻从当前页数据取值存入 → verify: `pnpm test` 断言勾选后 Map 中该项带齐三个属性
- [x] 6.13 四处 id 反查全部改从选中 Map 读取，不再回 `data.credentials` 查找：`dashboard.tsx:87-90` 的 `selectedDisabledCount`、`:183-186` 批量删除的 `disabledIds`、`:240-243` 批量恢复的 `failedIds`、`:288-291` 批量刷新 Token 的 `enabledIds` → verify: `pnpm test` 为四处各写一条用例，断言在第 2 页选中符合条件的凭据后翻回第 1 页，该凭据仍被正确识别、不计入「已跳过」
- [x] 6.14 跟改所有把 `selectedIds` 当 `Set<number>` 消费的读取点，`Array.from(selectedIds)` 在 Map 上会得到 `[id, value]` 元组而非 id：至少 `dashboard.tsx:513` 批量验活的 `ids`、`:193` 的 `skippedCount`、`:742` 的已选计数展示 → verify: `pnpm build` 类型检查零错误；`pnpm test` 断言批量验活的作用范围仍是已选 id 集合
- [x] 6.15 已选凭据被他处删除时从 Map 移除，并在结果汇总中如实计为失败或跳过 → verify: `pnpm test` 断言轮询刷新后该 id 已不在系统中时不与「跨页未加载」混同
- [x] 6.16 `dashboard.tsx:97-129` 的实时余额缓存清理判据由「不在当前页」改为「已从系统删除」：翻页不清理，仅在删除或清除操作成功后按受影响 id 移除 → verify: `pnpm test` 覆盖翻页往返后实时值仍在、删除凭据后对应实时值被移除两条用例
- [x] 6.17 `disabledCredentialCount`（`dashboard.tsx:86`）由 `data.credentials.filter` 改为 `total - available`：两者都不受筛选与分页影响，全量已禁用数不必额外请求也不必后端加字段 → verify: `pnpm test` 断言当前页无已禁用但全量有时该计数仍大于 0
- [x] 6.18 新增 `admin-ui/src/api/credentials.ts` 的 `fetchAllDisabledIds()`：请求 `?disabled=true&perPage=100` 并按 `pageInfo.hasNext` 逐页取回全量已禁用凭据的 id，仅在执行清除时调用 → verify: `pnpm test` 覆盖单页取回、多页续取两条用例
- [x] 6.19 `handleClearAll`（`dashboard.tsx:327-373`）保持全量语义：删除范围改用 6.18 取回的全量 id 而非 `data.credentials.filter`（后者分页后只剩当前页，会让删除范围静默退化为当页），按钮可用性与确认文案条数改用 6.17 的计数 → verify: `pnpm test` 断言当前页无已禁用但全量有时按钮仍可用、确认文案条数等于全量数、实际发出的删除请求数等于全量数；归零后置条件由 `dashboard.test.tsx` 的「清除已禁用：执行完毕后系统内不再存在已禁用凭据」断言（已禁用项分散在三页，执行后服务端侧计数为 0、按钮转禁用且不带计数），无需手工复核

## 7. 全量校验与发布准备

- [x] 7.1 运行 `cargo check --release --all-targets` → verify: 零新增告警（AGENTS.md 准绳）
- [x] 7.2 运行 `cargo test` → verify: 全绿，含本次新增的筛选、分页、全序化用例
- [x] 7.3 运行 `cd admin-ui && pnpm test && pnpm build` → verify: 全绿且构建产物生成
- [x] 7.4 运行 `openspec validate --all` → verify: 无错误
- [x] 7.5 改写 `README.md:786` 的 `GET /api/admin/credentials` 描述：由「获取所有凭据状态」改为分页与筛选后的实际语义，并在版本说明标注为破坏性变更，说明外部消费者需按 `pageInfo` 逐页取回 → verify: README 中该行不再声称返回全部凭据，且含分页说明
- [x] 7.6 按 docs 设计文档第 8.4 节在 172.20.66.24 的 pm2 进程上核实线上项 → verify: 逐条记录结果，订阅等级与余额在首屏有值
  - 接口侧 8.4 第 5/6/7/8/10/11/12/14 项全部通过，结果记在 `evidence-online-2026-08-27.md`；订阅等级 10/10、余额缓存 7/10 在首屏有值
  - 渲染侧 8.4 第 2/4/13/15 项用 Playwright 驱动 Chromium 打开线上 `/admin` 核对，13 条断言全通过；省略号线上只有 3 页触发不了，用 route 拦截把前端收到的 `totalPages` 改成 20 后验证
- [x] 7.7 运行 `git status --short` 确认无 `config.json` / `credentials.*` / `.codegraph/` 误入 → verify: 输出中不含上述路径

## 8. 归档前验证发现的修补

第一轮 `openspec-verify-change` 判定不建议归档，两条阻塞项、一处文档矛盾、若干断言缺口。以下是同一轮内的处置，依据见 `evidence/openspec-verify-report.md` 各节的「处置结果」。

- [x] 8.1 修 `paginate` 的整数溢出：`(page - 1) * per_page` 在 `i64` 上可溢出，debug 构建 panic，release 构建下 `page = 2^62 + 1` 会让偏移量环绕归零、返回满页数据（线上已复现）。改用 `checked_mul`，溢出时取 `usize::MAX` 走清空分支，不给 `page` 加上界以保持「越界原样回显」契约 → verify: `cargo test`（debug，overflow-checks 开）与 `cargo test --release pagination`（关，线上语义）都通过
- [x] 8.2 补溢出回归用例 `pagination_extreme_page_returns_empty_without_overflow`，同时覆盖 `i64::MAX` 与 `2^62 + 1` 两个值 → verify: 断言两者都返回空数组、`page` 原样回显、`totalPages` 为 3、`hasNext=false`
- [x] 8.3 spec 补 Scenario「page 极大值不因整数溢出返回数据」与配套 SHALL 约定，并把第 77 行拆成「下界 clamp」与「越界原样回显」两条；`src/admin/types.rs` 的 `page` 字段注释同步改掉「而非请求原值」的误导表述 → verify: `openspec validate --all` 通过，注释与 `pagination_page_beyond_last_returns_empty` 的断言一致
- [x] 8.4 补 query spec 唯一无覆盖的 Scenario「已选凭据被他处删除」：新增 `dashboard.test.tsx` 用例，从服务端 store 抽走一条已选凭据让 `deleteCredential` 走 reject → verify: 断言 `toast.warning` 含「成功 1 个，失败 1 个」且不含「已跳过」，失败与跳过是独立计数
- [x] 8.5 补 5 个断言不足的 Scenario：新增 `credentials_status_writes_no_disk_and_refreshes_no_token`（哨兵文件 + `expires_at` 双信号，替代 HTTP 打桩）、「下次进入页面沿用已存的每页条数」（预置 localStorage 再挂载）、「清除已禁用：执行完毕后系统内不再存在已禁用凭据」（已禁用项分散三页，验归零后置条件）；`filter_no_match_returns_empty_list` 补 `filteredTotal == 0`；`pagination_stable_across_duplicate_priority` 从对本地 `chunks(3)` 断言改为真的逐页发请求 → verify: `cargo test` 865 passed、`pnpm test` 148 passed
- [x] 8.6 把 6.19 的手工 verify 条件改为指向 8.5 新增的归零用例，取消手工复核 → verify: tasks 6.19 末句不再要求手工验证
- [x] 8.7 在 `docs/admin-ui-credential-list-display-and-query-optimization-design.md` 文首补「实现期修订」声明，按小节名点出全部涉及 `Link` 响应头的段落（不用行号，因为追加声明本身会让行号漂移），并指明事实源是本 change 的 `design.md` 与 `spec.md` → verify: docs 与 spec 不再对同一事实给出相反陈述
