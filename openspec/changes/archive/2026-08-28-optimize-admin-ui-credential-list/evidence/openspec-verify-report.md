# 归档前验证报告

change：`optimize-admin-ui-credential-list`　日期：2026-08-26

初次结论（2026-08-26 上午）：不建议归档。两条阻塞项——后端 `paginate` 有整数溢出缺陷（线上已复现，违反本 change 自己的 spec），tasks 6.19 的 verify 条件没有任何证据。另有 78 个 Scenario 中 1 个无覆盖、10 个断言不足，一处文档与 spec 相互矛盾。

复核结论（2026-08-26 下午）：可以归档。两条阻塞项与文档矛盾已在本 change 内修完，Scenario 覆盖从 67/10/1 提到 76/3/0（总数由 78 增至 79），剩下 3 个 PARTIAL 与三处 spec 侧空洞记为已知缺口（各条处置见下文相应段落）。

下文保留初次结论的原始判断，处置结果追加在各节末尾。

## 实跑命令与结果

| 命令 | 结果 |
| --- | --- |
| `openspec status --change optimize-admin-ui-credential-list --json` | `isPlanningComplete: true`、`isComplete: true`，proposal / specs / design / tasks 四类工件齐全 |
| `openspec validate --all` | 26 passed, 0 failed |
| `cargo check --release --all-targets` | 无告警 |
| `cargo test` | 初次 863 passed；修复后 865 passed, 0 failed |
| `cd admin-ui && pnpm test` | 初次 12 文件 145 passed；补测后 12 文件 148 passed |
| `cd admin-ui && pnpm build` | `tsc -b && vite build` 通过，产物 480.74 kB |

tasks.md 初次核对时 55 项全部勾选；本轮修补补记为第 8 组 7 项，现共 62 项，未勾选 0 项。

另跑了 `cargo test admin`（82 passed）核对后端 admin 模块单独可过。

修复后重跑全部六条命令，结果同上表右列：`cargo check --release --all-targets` 零告警，`openspec validate --all` 仍 26 passed。新增用例共 5 条，后端 2 条（溢出回归、列表请求零磁盘写零 token 刷新）、前端 3 条（他处删除计为失败、perPage 读回、清除后归零）。

另加跑 `cargo test --release pagination`（13 passed）。release 关闭 overflow-checks，与线上语义一致，溢出修复在这种构建下同样返回空数组而非环绕后的有效数据。

第三轮（归档前最终复核）把六条命令又跑了一遍，结果与上表一致：validate 26 passed、`cargo check` 零告警、`cargo test` 865 passed、`pnpm test` 12 文件 148 passed、`pnpm build` 产物 480.74 kB、`openspec list` 显示本 change 为唯一活跃项且状态 Complete。

## Completeness

工件齐全，spec 的每个 Requirement 都带 Scenario，`openspec validate` 通过。

一项缺口：tasks 6.19 的 verify 末句「手工验证执行后全量已禁用凭据均被清除」在 change 的任何证据文件中都没有记录。`evidence-online-2026-08-27.md` 里搜不到「清除」相关的实测。该项已勾选但这条 verify 条件未被满足。

补这条证据不该去线上真跑，那会删掉线上 4 条已禁用凭据，不可逆。`dashboard.test.tsx` 里已有一条「确认文案与实际删除请求数都等于全量数」，覆盖了同一意图，把 verify 条件改指向它就够了；真要走手工路径，也得在受控环境做完再补记。

已处置：tasks 6.19 的 verify 末句改为指向新增的自动化用例「清除已禁用：执行完毕后系统内不再存在已禁用凭据」。该用例把已禁用项分散在第 1、2、3 页，执行后断言服务端侧已禁用计数归零、总数从 30 减到 27、入口按钮转为禁用且可访问名不再带计数，覆盖了原手工条件要验的后置状态。手工复核条件随之取消。

### tasks 勾选项的真实支撑

62 项全部勾选，逐条核对「声称的改动是否在工作区」与「verify 条件是否真被满足」，分 FALSE_CHECK（勾了但改动不存在）、UNVERIFIED（改动存在但 verify 证据找不到）、DRIFT（改动与 verify 都在，但 tasks 写的路径或符号名与实际不符）三类。

结果：62 项全部通过，FALSE_CHECK 0、UNVERIFIED 0、DRIFT 0。tasks 写的路径与符号名与工作树字面一致，含 1.4 的 `self.balance_cache.lock().clone()`、3.1 的 `sort_by_key(|c| (c.priority, c.id))`、8.1 的 `checked_mul` + `map_or(usize::MAX, …)`；`src/admin/` 下已无只按 `priority` 排序的残留，`dashboard.tsx` 里已无 `.slice()` 假分页。8.5 声称的修补都已落地：`pagination_stable_across_duplicate_priority` 确认改为逐页调 `query_credentials`，跨页共享 `seen` 集合做 `assert!(seen.insert(id))`，等价于两两不相交。四处 id 反查全部改读 `selected.values()` 的快照。

核对顺带发现两处措辞滞后，都已修：

- tasks 5.1 仍写着「`page` 回显 clamp 之后的实际取值，不回显请求原值」，正是 8.3 判定为误导并在 spec 与 `types.rs` 注释里改掉的那句。实现与测试follow的是修正后的语义，所以不是验证缺口，但同一文档集留着被自己否决的旧措辞属于 Coherence 缺口。已改为与 spec 一致的两段式表述，并注明按 8.3 修正。
- 8.7 的 docs 声明按小节枚举 `Link` 头段落时漏了三处（6.5 节、第 7 节风险表、第 9 节能力清单）。声明的兜底句「全文凡提到 `Link` 头的段落都留作决策记录」已覆盖它们，没有残留矛盾，但枚举不全会让读者以为这三处不在作废范围。已补进枚举。

### Scenario 测试覆盖

三份 spec 共 78 个 Scenario（query 33、view 37、balance-ops 8）。逐条核对到具体测试函数：COVERED 67、PARTIAL 10、MISSING 1。

唯一没有覆盖的是 query spec 的「已选凭据被他处删除」。`dashboard.test.tsx` 里 `api.deleteCredential` 的 mock 始终 resolve success，全文没有一处 reject 或失败断言（grep `mockRejected|rejects|reject(` 零命中）。已有的「删除凭据后对应实时值与选中项一并移除」测的是本人发起删除，不是他处删除后再发起批量操作。Scenario 要求的「如实计为失败或跳过，不与跨页未加载混为一谈」这条路径没跑过。

10 个 PARTIAL 的断言差距，按值得补的程度排：

- 「列表请求不触发上游调用」（query）与「列表内联快照不触发上游」（balance-ops）共用 `credentials_status_does_not_touch_upstream_or_refresh_ttl` 一个测试，只断言了 `cached_at` 两次调用间不变、数值一致，没有 spy 证明零上游 HTTP 调用，也没断言无磁盘写。这是 spec 里的 MUST NOT，靠间接指标兜着。
- 「每页条数持久化」（view）只验了写入路径（`localStorage.credentialsPerPage='24'`），没有任何测试预置 localStorage 再挂载 Dashboard，「下次进入页面沿用」这条读回路径无自动化覆盖。线上 evidence 同样只验了写入。
- 「全量清除的执行范围覆盖全集」（view）断言了删除请求数与 id 集合等于全量，但后置条件「执行完毕后系统内不再存在已禁用凭据」没有重新查询确认归零。这一条和上面 6.19 缺手工证据是同一个空洞的两面：自动化没验归零，手工也没记录。
- 「凭据删除后清理实时值」（balance-ops）的测试名与断言不符：只断言「已选 N 条」文案消失，没断言余额数值从界面移除；而且用例里的 #13 是已禁用凭据，批量余额查询会跳过它，它本来就没有实时值。
- 「无匹配结果」（query）的 `filter_no_match_returns_empty_list` 断言了空数组与 total/available 的全量口径，漏了 Scenario 三项里的第三项 `pageInfo.filteredTotal == 0`。
- 「优先级重复时翻页不重不漏」（query）断言 id 全序，但两两不相交与并集等于全集是对本地 `ids.chunks(3)` 切出的数组做的，没真的逐页发 `page=1..n`。Scenario 明说「依次请求全部页」。
- 「键盘可完整操作」（view）断言了 Tab 从 Page 8 落到 9/10/11 且省略号 `aria-hidden`，但上一页/下一页按钮不在被断言的 Tab 序列内，「可用回车触发」也从未断言（测试只用 `user.click`）。完整 Tab 序列由线上 evidence 第 15 项人工核过。
- 「加载中保留上一次结果」（view）断言上一页 12 张卡仍在，没断言加载指示（实现是 `aria-busy={isFetching}` 加 `opacity-60`，测试没碰）。
- 「订阅等级来自列表响应」（view）断言了 `Pro` 渲染，没断言「不需要额外请求」。风险低，卡片本身不发余额请求。

另有三处 spec 侧空洞，因为没写成 Scenario 所以不计入上面的统计：`hasProfileArn` 维度列在筛选表格里却没有对应 Scenario（测试反倒有，`filter_has_profile_arn_both_directions`）；facets 的「MUST NOT 发起上游请求或磁盘 I/O」既无 Scenario 也无断言；「响应 MUST NOT 依赖 `Link` 响应头」无 Scenario 无断言，只有线上 evidence 第 12 项人工核过多页请求的 `Link` 头为空。

#### 处置结果

Scenario 总数由 78 增至 79（query spec 新增「page 极大值不因整数溢出返回数据」，该条自带回归测试），覆盖数从 COVERED 67 / PARTIAL 10 / MISSING 1 变为 COVERED 76 / PARTIAL 3 / MISSING 0。MISSING 那一条和排在前面的 PARTIAL 都补了断言（「零上游调用」一条对应 query 与 balance-ops 两个 Scenario，所以下面 4 个条目抵掉 5 个 PARTIAL）：

- 他处删除：新增「批量删除：已选凭据被他处删除时如实计为失败，不与已跳过混淆」。选中两条已禁用凭据后从服务端 store 里抽走一条，让 `deleteCredential` 对不存在的 id 走 reject。断言两条都发出了请求、`toast.warning` 文案含「成功 1 个，失败 1 个」且不含「已跳过」，失败与跳过是两个独立计数。这也是 `dashboard.test.tsx` 里第一处删除失败路径。
- 零上游调用：新增 `credentials_status_writes_no_disk_and_refreshes_no_token`。原有的 `credentials_status_does_not_touch_upstream_or_refresh_ttl` 判别力偏弱，成因是 seed 的缓存年龄 120s 本就在 `BALANCE_CACHE_TTL_SECS = 300` 之内，真走 `get_balance` 也会命中缓存而不改写 `cached_at`，`before == after` 因此两种情况都成立。这条 Scenario 的判别力实际由新增用例的双信号承担，另有一层结构性保证：`query_credentials` 是同步 `fn`，无法 await 上游。用「文件字节未变 + `expires_at` 未变」两个可观测信号替代对 HTTP 层打桩——凭据构造成已过期，真走刷新路径就会改写 token 并回写文件。写完基线后特意把文件覆盖为非 JSON 哨兵串，否则内容相同的二次回写查不出来。这条断言的判别力验证过：临时插一行 `set_priority` 探针后测试转为 FAILED，删掉探针恢复通过。
- perPage 读回：新增「下次进入页面沿用已存的每页条数」。挂载前预置 `localStorage.credentialsPerPage = '24'`，断言首屏渲染 24 张卡、发出的每一个请求都不带默认值 12、按钮文案显示「24 条/页」。
- 全量清除归零：见上面 6.19 的处置说明。
- `filteredTotal == 0`：`filter_no_match_returns_empty_list` 补齐 `page_info.filtered_total` 与 `total_pages` 两条断言，与既有的 total/available 全量口径断言并存。

另外两条虽在 PARTIAL 里但顺手改掉了：`pagination_stable_across_duplicate_priority` 从对本地 `chunks(3)` 断言改为真的逐页发 `page=1..4`，断言各页 id 集合两两不相交、并集等于全集、拼接顺序与全序一致；「凭据删除后清理实时值」原来测试名与断言不符，改成让 #13 先保持启用拿到实时值、再从 UI 开关禁用它以满足删除前置条件，最后断言余额数值从界面消失。

剩下 3 个 PARTIAL 记为已知缺口，都不阻塞：键盘操作的完整 Tab 序列与回车触发（线上 evidence 第 15 项人工核过）、加载指示 `aria-busy` 无断言、订阅等级「不需要额外请求」无断言。三处 spec 侧空洞同样保留，其中 `hasProfileArn` 只是漏写 Scenario、测试本就存在。

## Correctness

本节引用的行号是发现问题时的状态。处置本身改动了 `service.rs`、`types.rs` 与 spec，这些行号已经漂移，按符号名和引文定位更可靠。

### 缺陷：`page` 极大值触发整数溢出，越界页返回有效数据

`src/admin/service.rs` 的 `paginate`，修复前的偏移量计算（行号按发现时的状态，下方处置结果里的修复已让它漂移）：

```rust
let offset = ((page - 1) * per_page) as usize;
```

`page` 来自用户输入且只 clamp 下界（同函数里的 `query.page.unwrap_or(1).max(1)`），无上界。`per_page` 已 clamp 到 `[1,100]`，但 `(page - 1) * per_page` 在 `i64` 上可溢出。

debug 构建会 panic。把这两行算术复刻出来单独编译，`page = i64::MAX` 在 overflow-checks 开启下报 `attempt to multiply with overflow`。`Cargo.toml:11-13` 的 `[profile.release]` 只设了 `lto` 和 `strip`，overflow-checks 走 release 的默认关闭，所以线上进程不会崩。

release 下的后果更麻烦：越界页会返回有效数据。溢出后取 `as usize` 相当于对 2^64 取模，选 `page = 4611686018427387905`（即 `2^62 + 1`）能让 `(page-1) * per_page` 归零，绕开紧随其后的 `offset >= filtered_total` 判断。线上实测（172.20.66.24:18990）：

```
GET /api/admin/credentials?page=4611686018427387905&perPage=12
→ 200，返回 10 条完整数据（ids 1-10）
   pageInfo = {page: 4611686018427387905, perPage: 12, filteredTotal: 10,
               totalPages: 1, hasPrev: true, hasNext: false}
```

同一 page 值在 `perPage=100` 下返回 10 条、叠加 `disabled=true` 筛选后返回 4 条（ids 1,3,4,5），说明与筛选正交。

这违反 spec `admin-credential-list-query` 的两处约定：「页码越界返回空数组」（第 101-104 行）要求空数组，实际返回满页；「分页导航由 pageInfo 单一提供」（第 135 行）要求 `hasPrev`/`hasNext`/`totalPages` 足以推出导航，而此处 `totalPages=1` 与返回非空数据自相矛盾，客户端据此无法纠正状态。

影响面：内嵌 Admin UI 不受实际影响，`dashboard.tsx:126-127` 拿到响应后按 `page > totalPages` 回退末页，能自我纠正。受影响的是直接调用 API 的外部消费者，以及任何 debug 构建。`admin-ui/src/lib/credential-query-url.ts:64-67` 的 `searchParamsToPage` 也只校验下界，不阻止这种 page 进入请求。

现有测试无一覆盖极大 `page`：`service.rs` 的 12 个 `pagination_*` 用例里最大取值是 99（`pagination_page_beyond_last_returns_empty`）。

修法不复杂：给 `page` 加上界，或者把乘法换成 `saturating_mul` / `checked_mul`。这行代码是本 change 引入的，建议归档前修掉，并补一条 `page = i64::MAX` 的回归用例。

#### 处置结果

已修。乘法换成 `checked_mul`，溢出时把偏移量取 `usize::MAX` 让它走清空分支，而不是 wrap 回小偏移量：

```rust
let offset = (page - 1)
    .checked_mul(per_page)
    .map_or(usize::MAX, |v| v as usize);
```

没有给 `page` 加上界，保持「越界不算错误、原样回显」的 GitHub 契约不变。

回归用例 `pagination_extreme_page_returns_empty_without_overflow` 同时跑 `i64::MAX` 和 `2^62 + 1`：前者是 debug 下 panic 的那个值，后者是乘积对 2^64 取模归零的那个值，两条路径都断言返回空数组、`page` 原样回显、`totalPages` 为 3、`hasNext=false`。修复前 debug 构建下前者直接 panic。

spec 侧同步补了一条 Scenario「page 极大值不因整数溢出返回数据」和一条 SHALL 约定，把「MUST NOT 因整数环绕返回有效数据」写进契约，防止后续重构再踩。

两种构建都验过：`cargo test`（debug，overflow-checks 开）865 passed，`cargo test --release pagination`（overflow-checks 关，即线上语义）13 passed。未在线上复验，线上跑的是修复前的进程，要验得先重新部署，超出本轮范围。

### 措辞误导，行为无误

spec 第 77 行「`pageInfo.page` SHALL 回显 clamp 之后的值」与第 104 行 Scenario「`pageInfo.page` 为 99」读起来像冲突，实际不冲突。`service.rs:1886` 是 `.max(1)`，只有下界，99 从未被 clamp，回显原值就是回显 clamp 后的值。三处证据一致：`pagination_page_beyond_last_returns_empty` 断言 `(page, total_pages) == (99, 3)`、`paginate` 的 doc 注释写明「`page` 超过末页不算错误」、线上 evidence 第 10 项实测回显 99。

要改的是措辞，两处：spec 第 77 行写成「`page` 小于 1 时回显 clamp 后的 1，越界大值原样回显」更准确；`src/admin/types.rs:65` 的注释「回显 clamp 之后的实际取值而非请求原值」更误导，对 `page=99` 它恰恰就是请求原值。

#### 处置结果

两处都改了。spec 第 77 行拆成两条，一条讲下界 clamp 与越界原样回显，一条讲极大值不环绕；`types.rs:65` 的注释改为「小于 1 时回显 clamp 后的 1；越过末页时 clamp 不生效，原样回显请求值」。`paginate` 的 doc 注释也补了一句说明偏移量用 `checked_mul` 兜溢出。

## Coherence

本节的 docs 行号是发现问题时的状态。处置往文首加了 3 行声明，节内提到的行号都已后移 3 行，按小节名定位更可靠。

`docs/admin-ui-credential-list-display-and-query-optimization-design.md` 里 `Link` 响应头出现 19 处，其中多处仍把它当作要实现的设计：第 42 行 G5 目标含 `Link` 头、第 212 行「分页元数据**必须**同时出现在响应体与响应头」、第 524-535 行给出完整的头格式与 `handler 返回 (HeaderMap, Json<T>)` 的实现要求、第 684 行列出 `link_header_matches_page_info` 测试项、第 785 行的实施步骤 8 写「后端分页 + PageInfo + Link 头」。

否决声明只有一处，在第 730 行，且作用域被限定为 8.4 节的第 9、12、14 项验收项。

而 change 的 `design.md`（「`Link` 头不采用」那条决策）与 `specs/admin-credential-list-query/spec.md`（「响应 MUST NOT 依赖 `Link` 响应头传递导航信息」）都明确否决，实现也没写。同一份文档集对同一事实给出相反陈述，后续读者按 docs 第 212 行理解会得出「实现漏了 Link 头」的错误结论。

在 docs 顶部或 6.x 分页决策处补一条全局否决声明就能收口：说明第 212、489、524-535、684、785 行的 `Link` 头方案已在 change 评审中作废，事实源是 change 的 `design.md` 与 `spec.md`。

#### 处置结果

已在 docs 文首元信息块追加一条「实现期修订（2026-08-26）」，声明全文凡提到 `Link` 头的段落都留作决策记录、不再是待实现项，并按小节名逐一点出（G5 目标、第 5 节的「必须同时出现在响应体与响应头」、GitHub 契约借鉴表、头格式与 `handler 返回 (HeaderMap, Json<T>)` 的实现要求、`link_header_*` 测试项、8.4 节验收第 9/12/14 项、实施步骤 8），事实源指向 change 的 `design.md` 与 `spec.md`。

第一版声明写的是行号清单，但追加这段本身就把后续内容推后 3 行，行号当场失效。改成按小节名定位，文档再改也不会指错地方。原先那条局部声明保持不动，一条管 8.4 节的验收项，一条管全篇，不冲突。

没有逐行删改那些段落，避免破坏文档作为设计过程记录的完整性。读者从文首就能知道该忽略什么。

## 证据清单

| 路径 | 内容 |
| --- | --- |
| `openspec/changes/optimize-admin-ui-credential-list/evidence-online-2026-08-27.md` | 8.4 节线上核实记录，接口侧 8 项 + 渲染侧 4 项。放在 change 根目录而非 `evidence/` 子目录，tasks 7.6 按这个路径引用 |
| `openspec/changes/optimize-admin-ui-credential-list/evidence/openspec-verify-report.md` | 本文件 |

change 目录里没有单独的 Bridge / Compliance / Completion 文件。本 change 走 `spec-driven` schema（`.openspec.yaml` 里声明），该 schema 要求的工件就是 proposal / specs / design / tasks 四类，`openspec status` 全部判定 `done`、`isComplete: true`。验证记录由 tasks 各条的 verify 条件加上面两份证据承担，不是缺口。

## 剩余观察项（不阻塞）

`README.md` 的本次 diff 里删掉了一行与本 change 无关的内容：`Sonnet 5 的 thinking 行为与已知限制见 docs/claude-sonnet-5.md`。该文件确实不存在（`ls` 确认），是死链接，删掉本身合理，但它超出本 change 范围，属于顺手改动。是否保留由维护者定。

线上 `available` 从 7 降到 6（id 1 变为 disabled），发生在 8.4 核对之后，与本 change 无关，只影响再次核对时的基数对照。

`credentials_status_omits_balance_without_cache` 用 `assert!(!json.contains("balance"))` 判断字段缺省，是子串匹配而非键匹配。当前 `CredentialStatusItem` 的字段集里没有别的名字含 `balance` 子串（`types.rs` 里的 `fetch_balance` 与 `BalanceResponse.balance` 属于其他结构体，不进这份序列化），断言成立。后续给该结构体加名字含 `balance` 的字段会让这条断言失去判别力。
