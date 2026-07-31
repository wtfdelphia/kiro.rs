# Social 凭据 profileArn 解析导致每请求强制刷新 Token

> 状态：分析与优化方案（未实现；已经三轮独立审核并据此修订方案）
> 日期：2026-07-31
> 分析基线：kiro-rs `e775835` + `kam-external-idp-import-compat` 已实现改动（未提交）
> 分析方法：源码精读 + CodeGraph 调用链 + **隔离实例受控实验**（独立端口、独立凭据文件、真实上游请求）
> 现象来源：用户观察到 `正在刷新 Social Token...` 与 `凭据 #N Token 已强制刷新` 在余额查询、
> 模型测试、真实 API 请求时都频繁打印
>
> 修订说明：初版方案（每凭据冷却窗口）经审核发现会违反生效 spec 中一条无条件 MUST，
> 且存在哈希时序、失败分类、并发去重三处会使冷却失效或改变错误语义的缺口。
> 第 6 节起为修订后方案，第 7 节为新增的 spec 冲突处理。

## 1. 结论

**无可信 `profileArn` 的 Social 凭据，每一次 profileArn 解析都会发起一次
`ListAvailableProfiles` 加一次强制 Token 刷新，且几乎必然无收益。**
实测强刷为一比一关系：3 次对话 → 3 次强刷，3 次余额查询 → 3 次强刷，
2 次模型刷新 → 2 次强刷。

每次解析的实测与推算代价：

- **0.87–1.23 秒**上游 OAuth 往返，直接进入请求延迟（对话总耗时 4.4–5.0 秒中约占 20–25%）
- **外加一次 `ListAvailableProfiles` 往返**，且该调用在决策之前无条件发出（见 3.1）。
  它每次新建 reqwest client、带 `Connection: close`（`profile.rs:365-378`），
  每次都是全新 TLS 握手；被判为瞬态的失败还会重试 3 次并退避 200ms + 400ms
  （`profile.rs:291-308`）。本次实验未单独计时该段，故 2.3 节的数字**只是解析总开销的下界**。
- 一次 `credentials.json` 全量重写（多凭据格式且已配置回写路径时；见 3.5）
- 持有 `refresh_lock`（`TokioMutex`），与所有其他刷新路径共用，并发刷新在此串行化
- 一次 refreshToken 轮换风险（Social 响应带新 token 时会替换）

这不是 KAM 导入引入的缺陷，而是 `2026-07-30-profile-arn-refresh-fallback-order`
那个 change 只修了 IdC 一半、留下 Social 另一半的直接后果。

## 2. 实证数据

### 2.1 实验设计

隔离实例（独立端口 18995、独立 `credentials.json`、只含 1 条 Social 凭据），
统计 `凭据 #N Token 已强制刷新` 的出现次数。该日志字符串在全仓库唯一，
只由 `force_refresh_token_for`（`token_manager.rs:2375`）产生，可作为精确计数器。

对照组（**短路控制组**）：同一凭据注入一个**非占位**的 `profileArn` 后重启（端口 18996）。

### 2.1.1 本次实验缺失的可复现信息

以下项当时未记录，第三方无法据此原样重跑，实现后重跑 8.2 节实验时必须补齐：

- `ListAvailableProfiles` 的实际上游状态码与响应体。**这一项决定 `ListOutcome`
  落在 `Failed` 还是 `Empty`**（`profile.rs:230-244`），也决定本问题是否只对特定账号成立。
- 该凭据的 `authMethod` 与 `provider` 取值（决定它确实走 Social 分支而非被推断为其他形态）。
- 复现命令原文（各场景的 curl / Admin 端点调用）。

缺这三项不影响第 1 节结论——`decide_profile_action` 对 `Failed`/`Empty`/`Placeholder`
三种 miss 的处理完全相同（`profile.rs:163-176`），故落哪一种都得到 `ForceRefresh`。
但它影响「问题普适性」的判断，也影响改动后的对照可比性。

### 2.2 计数结果

| 场景 | 操作次数 | 新增强刷次数 | 比例 |
| --- | --- | --- | --- |
| 启动（模型缓存预热 `spawn_warmup_models`） | 1 | **1** | 1:1 |
| 真实对话 `POST /v1/messages` | 3 | **3** | 1:1 |
| 余额查询 `GET /credentials/1/balance?force=true` | 3 | **3** | 1:1 |
| 模型刷新 `POST /credentials/1/models/refresh` | 2 | **2** | 1:1 |
| Admin 连通性测试 `POST /credentials/1/test` | 1 | **1** | 1:1 |
| **合计** | 10 | **10** | — |

**短路控制组（注入非占位 profileArn）：启动后强刷 0 次。**

这条证明的是「本地 ARN 缓存命中会短路 list 与 refresh」，据此排除「强刷由别的原因触发」。
它**不能**证明注入的 ARN 真实有效——`trusted_profile_arn`（`profile.rs:112-119`）
只按「非空 且 非已知占位」判定，任意满足该条件的字符串都会短路。
故不宜称为「决定性锁定根因」。若要验证真实可信 ARN 的行为，需额外确认携带该 ARN
的上游请求仍成功（否则会走 `provider.rs:535-538` 的去 ARN 重试路径）。

### 2.3 单次强刷延迟

从 `正在刷新 Social Token...` 到 `凭据 #N Token 已强制刷新` 的时间差：

```
第 1 次：1234 ms
第 2 次：1054 ms
第 3 次： 866 ms
```

对话请求实测总耗时 4.36–5.02 秒。即**每个对话请求里约 1 秒是这次白做的 OAuth 往返**。

### 2.4 失败原因完全静默

10 次强刷期间，日志中与 profile 相关的行数为 **0**。核实原因：
`src/kiro/profile.rs` 全文**没有任何 `tracing::` 语句**。

后果：用户只看到「正在刷新 Social Token / 已强制刷新」反复出现，
无从知道这是 profileArn 解析在兜底，也无从知道 `ListAvailableProfiles` 为何失败。
这是本问题「看起来莫名其妙」的直接原因。

## 3. 机制分析

### 3.1 触发链

注意 `ListAvailableProfiles` 的位置：它在 `resolve_profile_arn` 内**无条件先于**
`decide_profile_action` 执行（`profile.rs:230-246`）。因此任何只作用于 decide 的抑制
都省不掉 list 那次往返，这直接决定了 6.3 节冷却点的位置选择。

```
七个入口
  ├─ provider.rs:178   MCP / WebSearch 调用
  ├─ provider.rs:298   MCP bearer-invalid 错误恢复
  ├─ provider.rs:393   generateAssistantResponse（每次对话）
  ├─ provider.rs:556   generate bearer-invalid 错误恢复
  ├─ token_manager.rs:1931  get_usage_limits_for（查余额前）
  ├─ token_manager.rs:2602  refresh_models_for（刷新模型缓存前，含启动预热）
  └─ admin/service.rs:881   run_minimal_generate（Admin 连通性测试）
        │
        ▼
  ensure_profile_arn_for_request  (profile.rs:398)
        │
        ▼
  resolve_profile_arn             (profile.rs:195)
        │
        ├─ trusted_profile_arn 命中 → 直接返回，不发请求   ← 短路控制组走这里
        │
        ├─ ListAvailableProfiles（含重试）── 无条件执行，不受任何抑制
        │     └─ 失败 / 空列表 / 仅占位 → ListOutcome::{Failed,Empty,Placeholder}
        │
        ▼
  decide_profile_action           (profile.rs:158)
        │
        ├─ is_api_key_credential            → Unsupported
        ├─ ListOutcome::Resolved(arn)        → Use(arn)
        ├─ refresh_routes_to_idc(cred)       → SoftUnavailable   ← IdC 在此软放行
        ├─ refresh_token.is_some()           → ForceRefresh      ← Social 落这里
        └─ 否则                              → Fail
```

### 3.2 关键代码

`src/kiro/profile.rs:158-177`：

```rust
fn decide_profile_action(credentials: &KiroCredentials, list: ListOutcome) -> ResolveAction {
    if credentials.is_api_key_credential() { return ResolveAction::Unsupported; }
    if let ListOutcome::Resolved(arn) = list { return ResolveAction::Use(arn); }

    // Refresh cannot produce an ARN for these; proceed without one instead.
    if crate::kiro::token_manager::refresh_routes_to_idc(credentials) {
        return ResolveAction::SoftUnavailable;   // ← IdC 受保护
    }

    if credentials.refresh_token.is_some() {
        return ResolveAction::ForceRefresh;      // ← Social 每次都到这里
    }

    ResolveAction::Fail
}
```

`src/kiro/profile.rs:253-272`：强刷成功后若仍无可信 ARN，返回
`ProfileArnUnavailable`——**与 `SoftUnavailable` 分支的最终结果完全相同**。
也就是说这次强刷除了消耗 1 秒和一次写盘，没有改变任何后续行为。

### 3.3 为什么 Social 没有被上一个 change 保护

`2026-07-30-profile-arn-refresh-fallback-order` 明确把 Social 排除在外，理由写在
该 change 的 Non-Goals：

> **不改 Social 凭据的强刷行为。** Social refresh 可能返回真实 `profileArn`，
> 强刷是有效尝试。其「反复重试」问题需要负缓存/冷却，属独立 change。

这个判断本身是对的——`refresh_social_token`（`token_manager.rs:336-338`）确实会写
`profile_arn`：

```rust
if let Some(profile_arn) = data.profile_arn {
    new_credentials.profile_arn = Some(profile_arn);
}
```

`RefreshResponse.profile_arn`（`model/token_refresh.rs:18`）也确实是可选字段。
所以 Social 的强刷**在首次**是有意义的尝试。

**问题在于第 2 次及以后。** 该 change 明确把「负缓存/冷却」留给独立 change，
但那个独立 change 从未创建。本文档就是补这一块。

### 3.4 无任何抑制机制

核实 `CredentialEntry`（`token_manager.rs:524-541`）的全部字段：

```
id, credentials, failure_count, refresh_failure_count,
disabled, disabled_reason, success_count, last_used_at
```

**没有任何字段记录「上次 profileArn 解析失败」。** 因此每个请求都从零开始尝试，
无从知道前一秒刚失败过。

### 3.5 代价的具体来源

| 代价 | 证据 |
| --- | --- |
| 强刷延迟 0.87–1.23 s/次 | 实测（2.3 节） |
| 外加 list 往返 | `profile.rs:230` 无条件执行，每次新建 client + `Connection: close`；未单独计时 |
| 写盘（**有条件**） | `force_refresh_token_for` 无条件调 `persist_credentials()`（`token_manager.rs:2371`），但该函数仅在**多凭据格式且已配置 `credentials_path`** 时真正写盘，否则直接 `Ok(false)`（`:1301-1312`）。本次实验的隔离实例满足该条件，故 10 次强刷 = 10 次全量重写；单凭据格式下无写放大。 |
| 刷新路径串行化 | `refresh_lock`（`TokioMutex`，`:701`）被 `force_refresh_token_for`（`:2354`）、`try_ensure_token`（`:1227`）、`get_usage_limits_for`（`:1882`）、模型刷新（`:2488`、`:2557`）共用。**这与保护 `entries` 的同步 `Mutex`（`:697`）是两把不同的锁**——后者只在极短临界区内读写条目状态，不跨 await。 |
| refreshToken 轮换风险 | Social 响应带 `refreshToken` 时会替换（`:332-334`） |
| 上游限流风险 | 高频 OAuth 调用；`refresh_social_token` 有 429 分支说明上游会限流 |

### 3.6 放大因子

按实际触发路径分层，避免高估：

| 层 | 解析次数量级 | 依据 |
| --- | --- | --- |
| 普通对话 / MCP | 每请求 1 次，作用于**被选中的那一条**凭据 | `acquire_context` 只返回单条（`token_manager.rs:1075`）；失败重试或凭据切换时才多于 1 |
| 启动预热 | O(N)，每凭据 1 次 | `spawn_warmup_models(2)`（`main.rs:164`） |
| 批量余额 / 批量模型刷新 / 批量验活 | O(N) | Admin 逐凭据调用 |

**已修正的错误表述**：初版把 `use-credentials.ts:21` 的 30 秒轮询列为放大因子，这不成立。
该 query 调 `getCredentials` → `get_all_credentials`（`admin/service.rs:82`），
只读 `snapshot()` 内存状态，不发任何上游请求，不进入 profile 解析。

真正的周期性放大源是 `dashboard.tsx:485-490` 的定时余额刷新：它逐个调
`getCredentialBalance(id, true)`，`force=true` 绕过 300 秒余额缓存
（`admin/service.rs:25` `BALANCE_CACHE_TTL_SECS`）→ 每凭据一次 profileArn 解析。
但它**默认关闭**（`dashboard.tsx:53` `useState(false)`）、默认间隔 120 秒（`:32`）。

准确表述：

- 凭据列表的 30 秒轮询**不会**触发强刷；
- 用户主动开启定时余额刷新后，会按配置周期对当前页启用凭据发起解析；
- 手动批量验活、批量余额刷新、批量模型刷新同样放大调用量。

## 4. 根因

1. **决策函数缺少「已知失败」维度。** `decide_profile_action` 只看
   （凭据类型 × list 结果），不看「这个凭据最近是否已经试过且失败」。
   同样的输入必然得到同样的 `ForceRefresh`，无法收敛。

2. **`ForceRefresh` 与 `SoftUnavailable` 的最终结果相同却重复付出代价。**
   强刷成功但无可信 ARN 时（`profile.rs:262-263`）返回 `ProfileArnUnavailable`，
   与直接软放行等价。

3. **`force_refresh_token_for` 语义过重。** 它是为 Admin「强制刷新」按钮设计的
   （无条件刷新 + 落盘），被 profileArn 解析借用后，把「取 ARN」这个只读意图
   变成了带副作用的写操作。

4. **可观测性缺失。** `profile.rs` 零日志，用户看到的是 token_manager 的强刷日志，
   与真实原因（profile 解析兜底）之间没有任何线索连接。

5. **解析无并发去重。** N 个并发请求命中同一条无 ARN 凭据时，各自独立走完整条链：
   N 次 list + N 次强刷，全部在 `refresh_lock` 上排队。
   加剧这一点的是既有缺陷：`force_refresh_token_for` 在**取锁之前**克隆凭据
   （`token_manager.rs:2344-2351`，取锁在 `:2354`），因此排队中的后续任务持有的是
   已被前一次刷新轮换掉的旧 refreshToken，仍会拿它去请求上游。
   任何「检查冷却 → 决定强刷」的方案若不做去重，都会被这条路径绕过。

## 5. 优化目标

功能目标：

- 无可信 ARN 的 Social 凭据，**每凭据在冷却窗口内最多解析一次**（含 list 与强刷），
  而非每请求一次。
- 首次尝试**保留**（Social refresh 可能返回 ARN，不能一刀切取消）。
- 凭据确实发生变化（重新导入、Admin 手动刷新、过期刷新轮换）时**失效冷却**，允许再试。
- **并发解析只允许一个请求取得刷新资格**，其余等待其结果或直接软放行，不各自发起往返。

不变量：

- IdC 与 API Key 的现有决策**逐位不变**。
- Admin「强制刷新」按钮的无条件语义**不变**。
- **刷新硬失败的错误语义不变**：仍上抛同时含 list 与 refresh 两处原因的错误，
  仍按既有策略计入失败/禁用（这是生效 spec 的 MUST，见第 7 节）。

工程目标：

- 冷却状态**不落盘**——它是运行时优化，不是需要持久化的事实。
- 日志能让运维一眼看出「这次强刷是 profileArn 解析在兜底」以及「list 为何失败」。
- 先更新 `profile-arn-resolution` spec，再改代码（第 7 节）。

## 6. 推荐方案

### 6.1 方案选型

| 方案 | 做法 | 取舍 |
| --- | --- | --- |
| **A：每凭据解析冷却窗口**（推荐） | `CredentialEntry` 加一个内存字段记录上次解析结果与时刻；冷却期内跳过 list 与强刷，直接软放行 | 改动小、语义清晰、可离线测试；需回答「冷却多久、何时失效、失败如何分类」 |
| B：进程内一次性标记 | 每凭据只尝试一次，之后永不重试 | 最简单，但凭据换了 token 后也不再试，会永久放弃可能拿到的 ARN |
| C：把 Social 也纳入 `refresh_routes_to_idc` 软放行 | 彻底不强刷 | 违反上一个 change 的既有结论（Social refresh 可能返回 ARN），会丢功能 |
| D：给 `resolve_profile_arn` 加只读刷新变体 | 新增不落盘、不轮换的「轻量刷新」 | 需要复制刷新逻辑，且仍然每请求一次往返，没解决根本问题 |

**选 A。** B 过于激进，C 丢功能，D 治标。

### 6.2 冷却状态设计（已按审核修订）

在 `CredentialEntry` 增加**内存字段**（不进 `KiroCredentials`，不落盘）：

```
profile_arn_cooldown: Option<ProfileArnCooldown>

struct ProfileArnCooldown {
    /// 冷却起点：本次解析**完成**的时刻
    since: std::time::Instant,
    /// 冷却原因，决定窗口时长与到期后的行为
    kind: CooldownKind,
    /// 凭据指纹：见下方「指纹时序」
    fingerprint: u64,
}

enum CooldownKind {
    /// 解析走完但上游确实没有可用 ARN（list miss + 刷新成功却无 ARN）
    NoArn,
    /// 瞬时故障（网络错误 / 429 / 5xx）导致解析未能完成
    TransientFailure,
}
```

用 `Instant` 而非 `SystemTime`：只关心相对时长，不受系统时钟调整影响，也不需要序列化。

**指纹时序（修订要点，初版在此处会导致方案完全失效）**

初版写「该次尝试**时**凭据 refreshToken 的哈希」，这是错的。Social 刷新响应带
`refreshToken` 时会轮换它（`token_manager.rs:332-334`），而这次强刷本身就是轮换源：

```
强刷 → refreshToken A→B → 若记录 hash(A)
下个请求 → 当前是 B → hash(B) ≠ hash(A) → 判定「凭据变了，值得再试」→ 再强刷
```

冷却在最常见路径上永不生效，方案归零。

修正方案，二者取一：

- **下限做法**：记录强刷**完成后**的 refreshToken 哈希。这要求在
  `force_refresh_token_for` 返回后**重新从 manager 读取最新凭据**
  （`token_manager.rs:2365` 已把新凭据写回 entry），**不能**沿用传入
  `resolve_profile_arn` 的 `&KiroCredentials` 旧快照——该快照此时已过期。
- **推荐做法**：改用**凭据版本号**（`CredentialEntry` 内存自增计数），
  在所有 `entry.credentials = …` 赋值点递增。全仓库共 6 处：
  `token_manager.rs:1253`、`:1899`、`:2190`（upsert）、`:2365`（强刷）、`:2497`、`:2573`。
  冷却记录里存「写入时的版本号」，判定时比对当前版本。

推荐版本号：它把「凭据是否变了」变成由写入方声明的事实，而不是让冷却逻辑
从 token 值反推变化来源——后者无法区分「别人改了凭据」与「我自己刚刚刷新过」，
这正是初版缺陷的根源。版本号还顺带覆盖了 upsert、过期刷新等所有变更路径，
不必逐条枚举失效条件。

代价是要触碰 6 个赋值点。若判定该代价过高，退回下限做法也可行，
但「必须读强刷后的凭据」这条时序必须写进 tasks 的验收项，并有测试锁定。

**冷却时长**

| 原因 | 窗口 | 依据 |
| --- | --- | --- |
| `NoArn` | 15 分钟 | 上游确实没有 ARN 时，其可用性变化（订阅变更、profile 首次创建）是人工操作量级，不会秒级发生。15 分钟把「每请求一次」降到「最多每 15 分钟一次」，又不会让刚变可用的账号等太久。 |
| `TransientFailure` | 30 秒 | 瞬时故障应快速恢复，不能因一次网络抖动压制 15 分钟 |

初版给 15 分钟的理由是「Social access token 有效期小时级」——这个论据不成立，
token 有效期与 ARN 可用性变化频率无关。上表已换成 ARN 可用性本身的变化量级。

两个值都是编译期常量，不做配置化——它们是实现细节，暴露成配置项会增加无意义的调参面。

### 6.3 冷却检查点必须在 list 之前（修订要点）

初版把冷却分支放在 `decide_profile_action` 内部。这省不掉 `ListAvailableProfiles`——
它在 `profile.rs:230` 无条件先于决策执行（见 3.1）。而一次 list 是完整的
TLS 握手 + 可能的 3 次重试与 600ms 退避，量级与强刷相当。

**修订**：冷却检查提到 `resolve_profile_arn` 内、list 调用**之前**：

```
resolve_profile_arn:
  1. trusted_profile_arn 命中            → 返回 ARN（不变）
  2. 清理已知占位 ARN                     → 不变
  3. api_key / 不支持 profile             → Unsupported（不变）
  4. 读冷却状态                            ← 新增
       命中且未到期 → 直接 Err(ProfileArnUnavailable)，不发 list、不强刷
  5. ListAvailableProfiles（含重试）       → 不变
  6. decide_profile_action                → 不变（签名不变）
  7. 按结果写/清冷却                        ← 新增
```

这样 `decide_profile_action` **完全不需要改动**，保持既有纯函数与全部现有测试不变。
冷却是 `resolve_profile_arn` 这一层的调度决策，不是决策函数的输入维度——
这比初版方案改动更小，也避免了「冷却状态该排在哪个分支之间」那个问题
（初版 6.3 讨论的排序问题在此方案下不存在）。

**权衡**：一起冷却 list 意味着「上游 ARN 刚变可用」时，list 也要等到窗口过期才会重试。
接受这个代价的理由是：`NoArn` 冷却只在 list **已经** miss 且刷新也拿不到 ARN 后才写入，
即已有证据表明该凭据当前无 ARN；而 ARN 变可用是人工操作量级（见 6.2 时长表）。
若将来实测发现 list 的恢复速度确有价值，可拆成两个独立窗口（list 短、refresh 长），
但在没有数据前不做这个复杂化。

### 6.4 并发去重（新增，初版缺失）

冷却的「检查—动作」不是原子的：N 个并发请求可同时读到「未冷却」，
全部发起 list + 强刷，并在 `refresh_lock` 上排队。这条路径会完全绕过冷却，
使改动后的最坏情况与改动前无异（见根因 5）。

**做法**：给「为取 ARN 而解析」加每凭据的进行中标记，与冷却字段同一把 `entries` 锁保护：

```
CredentialEntry {
    /// Some(_) 表示已有请求正在为该凭据解析 ARN
    profile_arn_resolving: bool,
}
```

- 抢到标记者：走完整解析，结束时清标记并写冷却。
- 未抢到者：**直接软放行**（`Err(ProfileArnUnavailable)` → `Ok(None)`），不等待。

选「不等待」而非「等待结果」：等待需要 `Notify`/`watch` 一类同步原语并处理
超时与取消，复杂度远高于收益——本请求以无 ARN 继续本来就是既有的合法路径
（`ensure_profile_arn_for_request:432` 已吸收为 `Ok(None)`），且并发首请求的
后续请求很快会命中新写入的冷却或已解析出的 ARN。

标记必须在**所有**退出路径上清除，包括错误返回与 panic 安全（用 RAII guard，
或在 `resolve_profile_arn` 内以 `match` 收敛单一出口后清除）。
标记泄漏会导致该凭据永久不再解析 ARN，比原缺陷更糟——这一点需要专门的测试覆盖。

### 6.5 结果分类与冷却写入规则（修订要点）

初版只说「走完强刷且未拿到 ARN 后记录」，未区分四种结局。区分是必需的，
因为其中一种若误入冷却会**改变错误传播语义**：刷新硬失败当前返回硬错误
（`profile.rs:265-270`），经 `provider.rs:402-414` 计入 `report_failure`；
若改为软放行，一个 refreshToken 已失效的凭据将不再被计失败、不再被禁用。

| 解析结局 | 当前行为 | 写冷却？ | 修订后行为 |
| --- | --- | --- | --- |
| list 得到可信 ARN | `Use(arn)`，落盘 | **清除**冷却 | 不变 |
| 强刷后拿到可信 ARN | 返回 ARN | **清除**冷却 | 不变 |
| 强刷成功但无可信 ARN | `Err(ProfileArnUnavailable)` → `Ok(None)` | **写 `NoArn`（15 min）** | 不变（软放行） |
| 强刷失败：网络 / 429 / 5xx | `bail!` 硬错误（含 list + refresh 两处原因） | **写 `TransientFailure`（30 s）** | **仍 `bail!` 同样的硬错误**——只抑制后续 30 秒内的重复往返，不改本次错误 |
| 强刷失败：`invalid_grant`（`RefreshTokenInvalidError`） | `bail!` 硬错误 | **不写** | 不变，继续走既有禁用策略（`report_refresh_token_invalid`，`:1646`） |
| `persist_credentials` 失败 | 仅 `warn!`，不影响返回值（`:2371-2373`） | 按上面各行照常 | 不变 |

关键约束：**写冷却与「本次返回什么」是两件独立的事。**
`TransientFailure` 那一行必须同时满足两点——本次仍上抛含两处原因的硬错误
（生效 spec 的 MUST，见第 7 节），且后续 30 秒内的解析直接软放行不再往返。
这不矛盾：冷却抑制的是**后续请求的往返**，不是**本次请求的错误**。

`invalid_grant` 不写冷却，因为该凭据会被立即禁用（`:1156-1158`），
禁用后不再被 `acquire_context` 选中，冷却无意义；且若运维手动重新启用，
应当立刻重试而非等 30 秒。

### 6.6 冷却失效条件

在版本号方案（6.2 推荐）下，失效条件收敛为两条：

1. 窗口到期（`NoArn` 15 分钟 / `TransientFailure` 30 秒）
2. 凭据版本号变化——自动覆盖重新导入、upsert、过期刷新、Admin 手动强刷、
   以及本次强刷自身的轮换

**Admin 手动强刷的处理（修订要点）**

初版 6.4 要求「Admin 手动强刷时清除冷却」，同时 6.6 又写「不改
`force_refresh_token_for` 的无条件语义」——两条冲突：要在该函数内清冷却就必须改它。
更麻烦的是该函数同时是 profileArn 兜底路径的调用目标（`profile.rs:254`），
在其内部清冷却会与 6.5 的写入规则纠缠成顺序依赖。

版本号方案下这个矛盾**自然消解**：`force_refresh_token_for` 在 `:2365`
赋值 `entry.credentials` 时递增版本号，冷却因版本不匹配而自动失效，
无需任何显式「清除冷却」逻辑，该函数的语义与签名都不必改。
这是选版本号而非哈希的第二个理由。

### 6.7 可观测性改进

`profile.rs` 当前零日志，需补最小必要的四条：

| 位置 | 级别 | 内容 |
| --- | --- | --- |
| list 失败 / 空 / 仅占位 | `debug` | 凭据 id + list 阶段结果（不含 token） |
| 决定强刷前 | `info` | `凭据 #N 无可信 profileArn，尝试刷新以获取（后续 15 分钟内不再重试）` |
| 命中冷却而跳过解析 | `debug` | `凭据 #N profileArn 解析冷却中（原因/剩余时长），以无 ARN 继续` |
| 未抢到并发标记 | `debug` | `凭据 #N 已有解析在进行，本次以无 ARN 继续` |

关键是第二条：它把「强刷」这个动作与「profileArn 解析」这个原因显式连起来，
用户不再需要读代码才能理解日志。

`force_refresh_token_for` 的日志来源标注（区分 Admin 手动 vs profileArn 兜底）
在版本号方案下**不再必需**——第二条 `info` 日志已提供了因果线索，
且 6.6 已说明该函数无需改动。列为可选增强。

### 6.8 不做的事

- **不改 `force_refresh_token_for` 的语义与签名**（Admin 按钮依赖其无条件行为；
  版本号方案下也不需要在其中清冷却）
- **不改 `decide_profile_action`**（冷却在其上层，既有纯函数与全部现有测试不变）
- **不改 IdC / API Key 的现有决策**
- **不改刷新硬失败的错误内容与传播路径**（含两处原因的 `bail!` 逐字保留）
- **不引入配置项**（两个冷却时长为编译期常量）
- **不落盘冷却状态**（重启后重新尝试一次可接受，且避免「持久化的负缓存如何失效」整类问题）
- **不改 `refresh_lock` 粒度**（锁竞争会随调用量下降自然缓解）
- **不改 `ListAvailableProfiles` 自身的重试与退避策略**（只在冷却期内跳过整个调用）
- **不修 `force_refresh_token_for` 取锁前克隆凭据这个既有缺陷**（根因 5 第二段）。
  它独立于本问题、影响所有刷新路径，应作为单独 change。本方案的并发去重
  使其在 profileArn 路径上不再被触发，但 Admin 批量强刷等路径仍存在。
  **该遗留必须在实现时以注释或 tasks 备注留痕，不可静默跳过。**

## 7. 与生效 spec 的冲突（新增：必须先处理）

**本方案违反一条已生效的无条件 MUST。** 初版文档完全未提及这一点，
第 6.6 节只写了「不动 spec 中 IdC 相关的既有场景」，精确避开了 IdC，
却没发现 Social 那条场景正挡在路上。

`openspec/specs/profile-arn-resolution/spec.md:63-69`：

```
#### Scenario: Social 强刷行为不得回归

- GIVEN 凭据为 Social 形态且持有 refreshToken，无可信缓存
- WHEN ListAvailableProfiles 失败、返回空列表或仅返回占位 ARN
- THEN 系统 MUST 仍执行强制刷新以尝试取得 profileArn
- AND 刷新成功但仍无可信 ARN 时返回 soft-unavailable
- AND 刷新失败时 MUST 保留同时包含 list 与 refresh 两处失败原因的错误信息
```

第 3 行是**无条件** MUST，不是「首次 MUST」。方案 A 的全部价值就在于让第 2 次
及以后不再强刷，因此直接违反它。相应地，初版 8.1 节把「Social 首次强刷」
列为回归项也是错的——spec 要求的是每次。

同一份 spec 的 `每请求不得重复无效强刷` 场景（`:53-59`）只覆盖 IdC，
不能用来支持 Social 的冷却。

### 7.1 必需的 spec delta

必须先建立独立 OpenSpec change 并提供
`openspec/changes/<change>/specs/profile-arn-resolution/spec.md`，
把上述场景改为带冷却限定的措辞，并新增冷却相关场景。delta 需表达：

1. **首次或冷却已过期时 MUST 尝试刷新**（保留 Social 强刷的既有价值）
2. **已有 `NoArn` 记录且仍在冷却窗口内时 MUST NOT 再刷新**，且 MUST NOT 再调用
   `ListAvailableProfiles`，以无 profileArn 继续
3. **list 成功时始终优先使用该 ARN**，冷却状态不得阻止使用已取得的 ARN
4. **并发解析同一凭据时，只允许一个请求取得刷新资格**；其余以无 ARN 继续
5. **刷新硬失败时 MUST 仍保留同时含 list 与 refresh 两处原因的错误信息**
   （原 spec 第 5 行**逐字保留**，冷却只抑制后续往返，不改本次错误）
6. `invalid_grant` 等永久认证错误 MUST 继续走既有禁用策略，不进入冷却

改动原场景的 GIVEN 需追加类似「且该凭据在本进程内无未过期的 ARN 解析冷却记录」。

归档时再同步主规格。AGENTS.md 的 OpenSpec 条件明确覆盖「Token 刷新、多凭据」，
本变更属该类，不可豁免。第 10 节自己强调过「必须先更新 spec，不能只改代码」——
这条纪律同样适用于本方案。

### 7.2 建议的 change 结构

```
openspec/changes/social-profile-arn-cooldown/
  proposal.md
  design.md      ← 本文档第 3–6 节的分析与设计可直接迁入
  tasks.md       ← 第 9 节测试矩阵迁入为验收项
  specs/profile-arn-resolution/spec.md   ← 7.1 的 delta（必需）
```

## 8. 影响面

| 文件 | 改动 |
| --- | --- |
| `src/kiro/profile.rs` | `resolve_profile_arn` 在 list 前加冷却检查、加并发标记、按 6.5 分类写/清冷却；补 4 条日志；新增测试。**`decide_profile_action` 不改。** |
| `src/kiro/token_manager.rs` | `CredentialEntry` 加冷却字段、进行中标记、版本号；新增读/写这些状态的方法；6 处 `entry.credentials =` 赋值点递增版本号 |
| `openspec/changes/<change>/specs/profile-arn-resolution/spec.md` | **必需** delta（见 7.1） |

`resolve_profile_arn` 与 `ensure_profile_arn_for_request` 的**公开签名保持不变**，
七个调用点零改动。`decide_profile_action` 签名亦不变（较初版方案改动更小）。

风险类型（AGENTS.md 高风险矩阵）：Token / 多凭据 + OpenSpec。
对应验证：`cargo test` 相关模块 + `openspec validate --all`。

## 9. 验证策略

### 9.1 必需自动化测试

`decide_profile_action` 不改，故初版那批「决策纯函数 × 冷却状态」组合测试**不再需要**——
其现有 10 个测试保持不变即为回归证明。测试重心移到冷却状态机与解析调度。

**冷却状态机（纯函数层）**

| 场景 | 验收点 |
| --- | --- |
| 无冷却记录 | 允许解析 |
| `NoArn` 写入后立即查（elapsed < 15 min） | 冷却中，跳过 list **与** 强刷 |
| `NoArn` 且 elapsed > 15 min | 允许解析 |
| `TransientFailure` 且 elapsed < 30 s | 冷却中 |
| `TransientFailure` 且 elapsed > 30 s | 允许解析 |
| 版本号与记录不符（任意 elapsed） | 允许解析 |

**解析调度层（`resolve_profile_arn`，需可注入 list 边界）**

| 场景 | 验收点 |
| --- | --- |
| trusted ARN 命中 × 有冷却记录 | 返回 ARN，**不查冷却、不发 list**（冷却不阻止使用已有 ARN） |
| list 返回可信 ARN × 有冷却记录 | `Use(arn)` 并**清除**冷却 |
| 冷却命中 | `Err(ProfileArnUnavailable)`，且**断言 list 未被调用**（这是 6.3 修订的核心，必须有测试锁定） |
| 强刷成功但无 ARN | 写 `NoArn`；返回软放行 |
| 强刷瞬时失败（网络/429/5xx） | 写 `TransientFailure`；**仍返回含 list + refresh 两处原因的硬错误** |
| 强刷 `invalid_grant` | **不写**冷却；仍返回硬错误 |
| IdC × list miss | `SoftUnavailable`，**不写**任何冷却（冷却只对走强刷的类型有意义） |

**并发去重**

| 场景 | 验收点 |
| --- | --- |
| 两个任务并发解析同一凭据 | 只有一个发起 list + 强刷；另一个立即软放行 |
| 解析以错误退出 | 进行中标记**已清除**（下一次请求可正常解析） |
| 解析 panic / 提前 return | 标记不泄漏（RAII guard 覆盖） |

**版本号**

| 场景 | 验收点 |
| --- | --- |
| 6 处 `entry.credentials =` 赋值 | 每处都递增版本号（逐处断言，防遗漏） |
| Social 强刷轮换 refreshToken | 版本号变化 → 但冷却是在强刷**之后**写入，记录的是**新**版本号 → 下次请求仍命中冷却（**这条直接锁定 6.2 的修订要点，是最容易写错的一处**） |
| Admin 手动强刷后 | 版本号变化使旧冷却失效；无需显式清除逻辑 |

**时间可测试性（修订要点）**

初版 6.2 选 `Instant`，8.1 又说「抽为接受 `Instant` 入参的纯函数」——这行不通：
`Instant` 无法构造任意过去时刻（只有 `checked_sub`，且不保证成功），
测「冷却已过」会很别扭。

**修订**：状态里存 `Instant`，但判定函数签名定为

```
fn is_cooling(kind: CooldownKind, elapsed: Duration) -> bool
```

由调用方算 `elapsed = since.elapsed()`，测试直接传 `Duration::from_secs(901)`。
不引入时间抽象层，也不需要可注入时钟。

### 9.2 端到端验证（复现本文档的实验）

实现后应重跑第 2 节的受控实验，**并补齐 2.1.1 列出的三项缺失信息**。验收标准：

| 指标 | 当前 | 目标 |
| --- | --- | --- |
| 启动 + 3 次对话 + 3 次余额 + 2 次模型刷新的强刷次数 | **10** | **1**（首次尝试）+ 9 次命中冷却 |
| 同上的 `ListAvailableProfiles` 调用次数 | **10**（未实测，按链路推算） | **1** |
| 对话请求延迟 | 4.36–5.02 s | 首次之后减少「强刷 0.9–1.2 s + 一次 list 往返」 |
| `credentials.json` 写入次数（多凭据格式） | 10 | 1 |

延迟目标不再给具体数字：初版的「减少 0.9–1.2 s」只算了强刷，
省掉 list 后的实际收益更大，但本次实验未单独计时 list，无法给出可信区间。
重跑时应分别计时 list 与 refresh 两段。

### 9.3 验证命令

```powershell
cargo test kiro::profile
cargo test kiro::token_manager
cargo test
openspec validate --all
git status --short
```

真实端到端验证只用本地临时凭据，确认 `config.json`、`credentials.json`、
`credentials.*`、`.codegraph/` 不进 Git 候选。

## 10. 风险与取舍

| 风险 | 后果 | 控制措施 |
| --- | --- | --- |
| **spec 冲突未先处理** | 代码通过 `cargo test` 但归档时 spec 一致性审查不通过，返工 | 第 7 节：先建 change 并提交 delta，再改代码。这是本方案的**前置条件**，不是可选项 |
| **指纹时序写错** | 冷却在最常见路径上永不生效，方案零收益且难以察觉（日志看起来正常，只是强刷照旧） | 6.2 改用版本号；9.1「版本号」表最后一行为专门的锁定测试 |
| **并发标记泄漏** | 该凭据在本进程内**永久**不再解析 ARN，比原缺陷更糟 | RAII guard；9.1 有三条专门测试（错误退出 / panic / 提前 return） |
| **冷却误吞硬错误** | refreshToken 已失效的凭据不再被计失败、不再被禁用，故障静默 | 6.5 表格逐行定义；`invalid_grant` 不写冷却；瞬时失败写冷却但**仍上抛原错误** |
| 冷却期内 ARN 真的变可用了 | 最多延迟 15 分钟才拿到（且期间 list 也不重试，见 6.3 权衡） | 版本号覆盖所有凭据变更路径；无 ARN 时上游对 Social 常可成功（实测无 ARN 对话成功）；`provider.rs:571` 的 bearer-invalid 兜底强刷不经冷却路径，真出 403 时仍会重新解析 |
| `CredentialEntry` 加三个可变字段 | 并发访问需正确加锁 | 复用保护 `entries` 的同步 `Mutex`（`:697`，与 `refresh_lock` 是不同的锁）；读写都在锁内完成且不跨 await |
| 冷却掩盖真实故障 | 运维看不到 list 一直失败 | 6.7 的 `debug` 日志保留 list 失败原因；`info` 日志明确标注冷却窗口与剩余时长 |
| 两个时长值不合适 | 过长则恢复慢，过短则抑制不足 | 编译期常量，可在实测后调整；不做配置化以免增加调参面 |
| 与 external_idp 的关系 | external 同样落 `ForceRefresh` 且 Microsoft 端点不返回 ARN | 冷却机制**自动覆盖 external**（走同一分支）。彻底方案见第 11 节 |
| 版本号需触碰 6 个赋值点 | 漏改一处则该路径的凭据变更不失效冷却 | 9.1 逐处断言；漏改的后果是「冷却比预期多持续一个窗口」，不是数据错误，可接受 |

## 11. 与 external_idp 的关系

`kam-external-idp-import-compat` 记录了一条已知遗留：external_idp 凭据也会为取
profileArn 而强刷，而 Microsoft token 端点**不返回** `profileArn`——比 Social 更确定无收益。

两条路径的关系：

| 凭据类型 | 刷新端点是否可能返回 ARN | 当前决策 | 本方案后 |
| --- | --- | --- | --- |
| IdC | 否（AWS OIDC 标准端点） | `SoftUnavailable` | 不变 |
| Social | **是**（Kiro 自有端点） | `ForceRefresh` 每次 | 首次 `ForceRefresh`，之后冷却 |
| external_idp | 否（Microsoft 端点） | `ForceRefresh` 每次 | 首次 `ForceRefresh`，之后冷却（**被本方案顺带缓解**） |

本方案把 external 的问题从「每请求一次」降到「每 15 分钟一次」，但没有根治——
external 的正解是纳入软放行，即把 `refresh_routes_to_idc` 的语义从
「是否走 OIDC」改为「刷新端点是否可能返回 profileArn」。

`profile.rs:727-740` 的 `test_external_without_arn_currently_force_refreshes`
以注释明确锁定了这一现状，将来的修复必须显式更新该测试。

**建议分两步**：本方案先解决 Social（用户实际遇到的问题）并顺带缓解 external；
`refresh_routes_to_idc` 的语义重定义作为后续 change，因为它会牵连
`profile-arn-resolution` 的多个既有 spec 场景，需要单独的规格评审。

若在实现本方案时发现语义重定义更简单，也可以合并——但必须先更新
`profile-arn-resolution` 的 spec，不能只改代码。

## 12. 关于「Social refresh 可能返回 ARN」这个前提

本方案「保留首次强刷」完全建立在这个前提上，因此有必要说明它的证据强度。

**支持证据（仅代码层）**：`RefreshResponse.profile_arn` 字段存在且为
`Option`（`model/token_refresh.rs:18`）；`refresh_social_token` 确实会把它写回凭据
（`token_manager.rs:336-338`）。

**反面事实**：第 2 节的 10 次强刷中，**0 次**拿到 ARN。本文档没有任何一次
Social refresh 返回 ARN 的正向观测。

这不足以否定「保留首次」——代价只有每凭据每窗口一次往返，而若该前提成立，
放弃它会丢功能（这正是方案 C 被否的理由）。但应当承认：
「Social 强刷有意义」目前是**基于字段存在的推断**，而非实测结论。
若将来积累到足够的负向样本，可以重新评估是否把 Social 也纳入软放行
（与第 11 节的 external 语义重定义合并处理）。

## 13. 源码证据

（行号基于 `e775835` + `kam-external-idp-import-compat` 已实现改动）

**触发链**

- `src/kiro/profile.rs:158-177`：`decide_profile_action`，Social 落 `ForceRefresh` 的分支
- `src/kiro/profile.rs:230-246`：list 在决策**之前**无条件执行（6.3 修订的依据）
- `src/kiro/profile.rs:253-272`：`ForceRefresh` 执行体；`:262-263` 强刷成功但无可信 ARN 时
  返回 `ProfileArnUnavailable`，与 `SoftUnavailable` 结果相同；`:265-270` 刷新失败时
  `bail!` 含两处原因的硬错误
- `src/kiro/profile.rs:285-310`：list 重试 3 次 + 200/400ms 退避
- `src/kiro/profile.rs:365-378`：每次 list 新建 client、`Connection: close`
- `src/kiro/profile.rs:398`：`ensure_profile_arn_for_request`，七个入口的汇聚点；
  `:432` 把 `ProfileArnUnavailable` 吸收为 `Ok(None)`
- `src/kiro/profile.rs` 全文：**零 `tracing::` 语句**，失败静默的原因
- 七个调用点：`provider.rs:178,298,393,556`、`token_manager.rs:1931,2602`、
  `admin/service.rs:881`

**刷新与状态**

- `src/kiro/token_manager.rs:2343-2377`：`force_refresh_token_for`；
  `:2344-2351` **取锁前**克隆凭据（并发缺陷）、`:2354` 取 `refresh_lock`、
  `:2365` 写回新凭据、`:2371` `persist_credentials()`、`:2375` 「已强制刷新」日志
- `src/kiro/token_manager.rs:271`：「正在刷新 Social Token...」日志唯一来源
- `src/kiro/token_manager.rs:332-338`：Social 刷新响应写回 `refresh_token` 与 `profile_arn`
  （前者是指纹时序缺陷的成因，后者是「保留首次强刷」的唯一依据，见第 12 节）
- `src/kiro/token_manager.rs:524-541`：`CredentialEntry` 全部 8 个字段，无任何 ARN 尝试记录
- `src/kiro/token_manager.rs:697` / `:701`：保护 `entries` 的同步 `Mutex` 与
  `refresh_lock`（`TokioMutex`）是**两把不同的锁**
- 6 处 `entry.credentials =` 赋值点（版本号需覆盖）：`:1253`、`:1899`、`:2190`、
  `:2365`、`:2497`、`:2573`
- `src/kiro/token_manager.rs:1301-1312`：`persist_credentials` 仅在多凭据格式且
  已配置路径时真正写盘
- `src/kiro/token_manager.rs:1156-1158` / `:1646`：`invalid_grant` → 立即禁用
- `src/kiro/model/token_refresh.rs:18`：`RefreshResponse.profile_arn` 为 `Option`

**放大因子与规格**

- `src/main.rs:164`：`spawn_warmup_models(2)`，启动阶段的 O(N) 触发源
- `admin-ui/src/components/dashboard.tsx:485-490`：定时余额刷新（真正的周期性放大源）；
  `:32` 默认 120 秒、`:53` **默认关闭**
- `admin-ui/src/hooks/use-credentials.ts:21`：凭据列表 30 秒轮询——
  **不是**放大因子，只读 `snapshot()`（见 3.6 修正）
- `src/admin/service.rs:25`：`BALANCE_CACHE_TTL_SECS = 300`，`force=true` 绕过它
- `src/admin/service.rs:82`：`get_all_credentials` 纯内存读取，无上游调用
- `openspec/specs/profile-arn-resolution/spec.md:63-69`：**本方案违反的无条件 MUST**
  （第 7 节）
- `src/kiro/profile.rs:727-740`：external 现状锁定测试

本文档只记录分析与方案，未修改任何运行代码。实验使用的隔离实例与临时凭据文件已清理，
未修改用户运行中的实例（端口 18990）及其 `credentials.json`。

## 14. 修订记录

初版（2026-07-31）经三轮独立审核，以下为方案层的实质变更。事实层（行号、
调用链、代码结构）三轮核查均无偏差，未作修改。

| # | 问题 | 严重度 | 处理 |
| --- | --- | --- | --- |
| 1 | 方案违反 `spec.md:63` 的无条件 MUST，初版完全未提 | P0 | 新增第 7 节；影响面加入 spec delta；8.1 的「Social 首次强刷」回归项作废 |
| 2 | 指纹记录「尝试时」的 refreshToken 哈希，会被本次强刷自身的轮换立即失效，冷却归零 | P1 | 6.2 改用凭据版本号；9.1 加专项锁定测试 |
| 3 | 冷却只挡强刷，`ListAvailableProfiles` 仍每请求发 | P1 | 6.3 把检查点前移到 list 之前；`decide_profile_action` 改为不动 |
| 4 | 无并发去重，N 个并发请求全部绕过冷却 | P1 | 新增 6.4；记入根因 5 |
| 5 | 刷新失败是否进入冷却未定义，误处理会改变错误传播语义 | P1 | 新增 6.5 结果分类表，区分 `NoArn` / `TransientFailure` / `invalid_grant` |
| 6 | 「Admin 强刷清除冷却」与「不改 `force_refresh_token_for`」自相矛盾 | P1 | 6.6：版本号方案下矛盾自然消解，该函数无需改动 |
| 7 | `Instant` 无法构造过去时刻，冷却过期不可测 | P2 | 9.1：判定函数签名改为接受 `Duration` |
| 8 | 30 秒轮询被误列为放大因子 | P2 | 3.6 改为 `dashboard.tsx:485` 定时余额刷新，并注明默认关闭 |
| 9 | `N×M` 与「每次全量写盘」过度泛化 | P2 | 3.5/3.6 按触发路径分层，写盘限定为多凭据格式 |
| 10 | 对照组只证明短路条件，措辞过强 | P2 | 2.2 改称「短路控制组」，说明其不能证明 ARN 真实有效 |
| 11 | 实验缺可复现细节 | P2 | 新增 2.1.1，列出三项必补信息 |
| 12 | 15 分钟的论据（token 有效期）与结论无关 | P2 | 6.2 换成 ARN 可用性变化量级；拆出 `TransientFailure` 30 秒 |
| 13 | `refresh_lock` 与 `entries` 锁混为一谈 | P3 | 3.5/10 明确区分两把锁 |
| 14 | 「Social refresh 可能返回 ARN」缺正向实测证据 | P3 | 新增第 12 节，承认这是基于字段存在的推断 |
