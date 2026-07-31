## Why

无可信 `profileArn` 的 Social 凭据，**每一次** profileArn 解析都会发起一次
`ListAvailableProfiles` 加一次强制 Token 刷新，且几乎必然无收益。

分析基线：kiro-rs `e775835` + `kam-external-idp-import-compat` 已实现改动（未提交）。
完整分析、受控实验与三轮审核记录见 `docs/social-profile-arn-force-refresh-storm.md`。

### 实测：强刷与操作是一比一关系

隔离实例（独立端口 18995、独立 `credentials.json`、只含 1 条 Social 凭据），
以全仓库唯一的日志字符串 `凭据 #N Token 已强制刷新`（`token_manager.rs:2375`）作精确计数器：

| 场景 | 操作次数 | 新增强刷次数 |
| --- | --- | --- |
| 启动（`spawn_warmup_models`） | 1 | **1** |
| 真实对话 `POST /v1/messages` | 3 | **3** |
| 余额查询 `GET /credentials/1/balance?force=true` | 3 | **3** |
| 模型刷新 `POST /credentials/1/models/refresh` | 2 | **2** |
| Admin 连通性测试 `POST /credentials/1/test` | 1 | **1** |
| **合计** | 10 | **10** |

短路控制组（同一凭据注入非占位 `profileArn` 后重启）：强刷 **0** 次。
这条证明「本地 ARN 缓存命中会短路 list 与 refresh」，据此排除「强刷由别的原因触发」；
它**不能**证明注入的 ARN 真实有效——`trusted_profile_arn`（`profile.rs:112-119`）
只按「非空 且 非已知占位」判定。

单次强刷延迟（`正在刷新 Social Token...` → `凭据 #N Token 已强制刷新` 的时间差）：
1234 ms / 1054 ms / 866 ms。对话请求总耗时 4.36–5.02 s，即**每个对话请求里约 1 秒
是这次白做的 OAuth 往返**。

### 每次解析的代价

- **0.87–1.23 s** 上游 OAuth 往返，直接进入请求延迟
- **外加一次 `ListAvailableProfiles` 往返**：它在决策之前**无条件**发出
  （`profile.rs:230-246`），每次新建 reqwest client、带 `Connection: close`
  （`:365-378`），每次全新 TLS 握手；被判瞬态的失败还会重试 3 次并退避
  200ms + 400ms（`:285-310`）。本次实验未单独计时该段，故上面的延迟数字**只是下界**。
- 一次 `credentials.json` 全量重写（**仅**多凭据格式且已配置 `credentials_path` 时；
  `token_manager.rs:1301-1312` 否则直接 `Ok(false)`）
- 持有 `refresh_lock`（`TokioMutex`，`:701`），与所有其他刷新路径共用
- 一次 refreshToken 轮换风险（Social 响应带新 token 时会替换，`:332-334`）

### 为什么强刷几乎必然无收益

强刷成功后若仍无可信 ARN，返回 `ProfileArnUnavailable`（`profile.rs:262-263`），
与 `SoftUnavailable` 分支**结果完全相同**。也就是说这次强刷除了消耗 1 秒和一次写盘，
没有改变任何后续行为。10 次强刷中 **0 次**拿到 ARN。

### 这是上一个 change 的已知遗留

`2026-07-30-profile-arn-refresh-fallback-order` 只修了 IdC 一半，其 Non-Goals 明确写：

> **不改 Social 凭据的强刷行为。** Social refresh 可能返回真实 `profileArn`，
> 强刷是有效尝试。其「反复重试」问题需要负缓存/冷却，属独立 change。

那个独立 change 从未创建。本 change 补这一块。

### 根因

1. **决策函数缺少「已知失败」维度。** `decide_profile_action`（`profile.rs:158-177`）
   只看（凭据类型 × list 结果），同样的输入必然得到同样的 `ForceRefresh`，无法收敛。
2. **`CredentialEntry`（`token_manager.rs:524-541`）8 个字段中没有任何一项记录
   「上次 profileArn 解析失败」**，每个请求都从零开始。
3. **`force_refresh_token_for` 语义过重**：为 Admin「强制刷新」按钮设计（无条件刷新 + 落盘），
   被 profileArn 解析借用后，把「取 ARN」这个只读意图变成带副作用的写操作。
4. **可观测性缺失**：`src/kiro/profile.rs` 全文**零 `tracing::` 语句**。10 次强刷期间
   profile 相关日志行数为 0。用户只看到反复的强刷日志，无从知道原因是 profileArn 解析在兜底。
5. **解析无并发去重**：N 个并发请求命中同一条无 ARN 凭据时各走完整链路，
   N 次 list + N 次强刷，全部在 `refresh_lock` 上排队。

## What Changes

- 在 `CredentialEntry` 增加**内存**（不落盘）的 profileArn 解析冷却状态与凭据版本号。
- `resolve_profile_arn` 在 **`ListAvailableProfiles` 调用之前**检查冷却：命中则直接软放行，
  既不发 list 也不强刷。
- 按解析结局分类写入冷却：`NoArn`（15 分钟）/ `TransientFailure`（30 秒）/
  `invalid_grant` 不写。
- 加每凭据「解析进行中」标记做并发去重，未抢到者立即软放行（不等待）。
- 凭据版本号在 6 处 `entry.credentials =` 赋值点递增，版本变化即失效冷却。
- 为 `profile.rs` 补 4 条最小必要日志。
- **BREAKING（spec 层，非 API 层）**：修改 `profile-arn-resolution` 中
  `Social 强刷行为不得回归` 场景的无条件 MUST（见下文「与生效 spec 的冲突」）。
  代码层 `resolve_profile_arn` / `ensure_profile_arn_for_request` /
  `decide_profile_action` 的签名均不变，七个调用点零改动。

## 与生效 spec 的冲突（本 change 的前置理由）

`openspec/specs/profile-arn-resolution/spec.md:63-69` 的
`Scenario: Social 强刷行为不得回归` 第 3 行是**无条件** MUST：

> - THEN 系统 MUST 仍执行强制刷新以尝试取得 profileArn

本方案的全部价值在于让第 2 次及以后不再强刷，因此直接违反它。
同一份 spec 的 `每请求不得重复无效强刷`（`:43-48`）只覆盖 IdC，不能用来支持 Social 的冷却。

因此本 change **必须**提供 `specs/profile-arn-resolution/spec.md` delta，
把该场景改为带冷却限定的措辞，并新增冷却相关场景。这是前置条件，不是可选项。

## Scope

- `src/kiro/profile.rs`：`resolve_profile_arn` 在 list 前加冷却检查、加并发去重标记、
  按结局分类写/清冷却；补 4 条日志；新增测试。
- `src/kiro/token_manager.rs`：`CredentialEntry` 加冷却字段、进行中标记、版本号；
  新增读/写这些状态的方法；6 处 `entry.credentials =` 赋值点（`:1253`、`:1899`、
  `:2190`、`:2365`、`:2497`、`:2573`）递增版本号。
- `openspec/changes/social-profile-arn-cooldown/specs/profile-arn-resolution/spec.md`：
  必需 delta。

## Non-Goals

- **不改 `decide_profile_action`。** 冷却是 `resolve_profile_arn` 这一层的调度决策，
  不是决策函数的输入维度。其现有 10 个测试保持不变即为回归证明。
- **不改 `force_refresh_token_for` 的语义与签名。** Admin 按钮依赖其无条件行为；
  版本号方案下也不需要在其中清冷却。
- **不改 IdC / API Key 的现有决策**，逐位不变。
- **不改刷新硬失败的错误内容与传播路径。** 含 list + refresh 两处原因的 `bail!`
  逐字保留（这是生效 spec 的 MUST）。
- **不引入配置项。** 两个冷却时长为编译期常量。
- **不落盘冷却状态。** 重启后重新尝试一次可接受，且避免「持久化的负缓存如何失效」整类问题。
- **不改 `refresh_lock` 粒度。** 锁竞争会随调用量下降自然缓解。
- **不改 `ListAvailableProfiles` 自身的重试与退避策略**，只在冷却期内跳过整个调用。
- **不修 `force_refresh_token_for` 取锁前克隆凭据这个既有缺陷**
  （`token_manager.rs:2344-2351` 克隆、`:2354` 取锁，排队中的后续任务持有已被轮换掉的
  旧 refreshToken）。它独立于本问题、影响所有刷新路径，应作为单独 change。
  本方案的并发去重使其在 profileArn 路径上不再被触发，但 Admin 批量强刷等路径仍存在。
  **该遗留必须在实现时以代码注释留痕，不可静默跳过。**
- **不重定义 `refresh_routes_to_idc` 的语义**（把「是否走 OIDC」改为「刷新端点是否可能
  返回 profileArn」，即 external 的正解）。它会牵连本 capability 的多个既有 spec 场景，
  需要单独的规格评审。本方案把 external 从「每请求一次」降到「每 15 分钟一次」，
  是缓解而非根治。

## Assumptions

- **「Social refresh 可能返回 ARN」是基于字段存在的推断，而非实测结论。**
  支持证据仅在代码层：`RefreshResponse.profile_arn` 为 `Option`
  （`model/token_refresh.rs:18`），`refresh_social_token` 确实会写回
  （`token_manager.rs:336-338`）。反面事实：本次 10 次强刷中 **0 次**拿到 ARN。
  本方案「保留首次强刷」完全建立在这个前提上；代价只有每凭据每窗口一次往返，
  而若前提成立，放弃它会丢功能。若将来积累到足够负向样本，可重新评估。
- 冷却时长的依据是 **ARN 可用性本身的变化量级**（订阅变更、profile 首次创建属人工操作量级），
  不是 token 有效期。
- 本次实验缺三项可复现信息，实现后重跑实验时必须补齐：`ListAvailableProfiles` 的实际
  上游状态码与响应体（决定 `ListOutcome` 落 `Failed` 还是 `Empty`）、该凭据的
  `authMethod` 与 `provider` 取值、各场景的复现命令原文。缺这三项不影响结论
  （`decide_profile_action` 对三种 miss 处理完全相同），但影响问题普适性判断与改动后的可比性。

## Success Criteria

重跑受控实验（并补齐上述三项信息）：

| 指标 | 当前 | 目标 |
| --- | --- | --- |
| 启动 + 3 次对话 + 3 次余额 + 2 次模型刷新的强刷次数 | **10** | **1**（首次尝试）+ 9 次命中冷却 |
| 同上的 `ListAvailableProfiles` 调用次数 | **10**（按链路推算，未实测） | **1** |
| `credentials.json` 写入次数（多凭据格式） | 10 | 1 |
| 对话请求延迟 | 4.36–5.02 s | 首次之后减少「强刷 0.9–1.2 s + 一次 list 往返」 |

延迟不给具体目标数字：本次实验未单独计时 list，无法给出可信区间。重跑时应分别计时两段。

自动化：`cargo test`、`openspec validate --all` 全绿；`decide_profile_action`
既有 10 个测试**未修改**且通过。

## Risks

| 风险 | 后果 | 控制措施 |
| --- | --- | --- |
| **指纹时序写错** | 冷却在最常见路径上永不生效，方案零收益且难察觉（日志正常，只是强刷照旧） | 用凭据版本号而非 refreshToken 哈希；tasks 有专项锁定测试 |
| **并发标记泄漏** | 该凭据在本进程内**永久**不再解析 ARN，比原缺陷更糟 | RAII guard；三条专门测试（错误退出 / panic / 提前 return） |
| **冷却误吞硬错误** | refreshToken 已失效的凭据不再被计失败、不再被禁用，故障静默 | 结局分类表逐行定义；`invalid_grant` 不写冷却；瞬时失败写冷却但**仍上抛原错误** |
| 冷却期内 ARN 真的变可用 | 最多延迟 15 分钟（期间 list 也不重试） | 版本号覆盖所有凭据变更路径；`provider.rs:571` 的 bearer-invalid 兜底强刷不经冷却路径，真出 403 时仍会重新解析 |
| `CredentialEntry` 加三个可变字段 | 并发访问需正确加锁 | 复用保护 `entries` 的同步 `Mutex`（`:697`，与 `refresh_lock` 是不同的锁）；读写都在锁内且不跨 await |
| 冷却掩盖真实故障 | 运维看不到 list 一直失败 | `debug` 日志保留 list 失败原因；`info` 日志标注冷却窗口与剩余时长 |
| 版本号需触碰 6 个赋值点 | 漏改一处则该路径的凭据变更不失效冷却 | 逐处断言；漏改后果是「冷却比预期多持续一个窗口」，不是数据错误，可接受 |
| 两个时长值不合适 | 过长则恢复慢，过短则抑制不足 | 编译期常量，可实测后调整；不配置化以免增加调参面 |

风险类型（AGENTS.md 高风险矩阵）：**Token / 多凭据 + OpenSpec**。
对应验证：`cargo test` 相关模块 + `openspec validate --all`。
