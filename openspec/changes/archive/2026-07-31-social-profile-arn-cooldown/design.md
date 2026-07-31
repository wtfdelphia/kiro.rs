## Context

分析基线：kiro-rs `e775835` + `kam-external-idp-import-compat` 已实现改动（未提交）。
本文档的行号均基于该基线。完整实验数据与三轮审核记录见
`docs/social-profile-arn-force-refresh-storm.md`。

## 当前实现

### 触发链

`ListAvailableProfiles` 的位置是关键：它在 `resolve_profile_arn` 内**无条件先于**
`decide_profile_action` 执行（`profile.rs:230-246`）。因此任何只作用于 decide 的抑制
都省不掉 list 那次往返。

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
        │   :432 把 ProfileArnUnavailable 吸收为 Ok(None)
        ▼
  resolve_profile_arn             (profile.rs:195)
        │
        ├─ trusted_profile_arn 命中 → 直接返回，不发请求
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

### 放大因子（按实际触发路径分层）

| 层 | 解析次数量级 | 依据 |
| --- | --- | --- |
| 普通对话 / MCP | 每请求 1 次，作用于**被选中的那一条**凭据 | `acquire_context` 只返回单条（`token_manager.rs:1075`）；失败重试或凭据切换时才多于 1 |
| 启动预热 | O(N)，每凭据 1 次 | `spawn_warmup_models(2)`（`main.rs:164`） |
| 批量余额 / 批量模型刷新 / 批量验活 | O(N) | Admin 逐凭据调用 |

周期性放大源是 `dashboard.tsx:485-490` 的定时余额刷新：逐个调
`getCredentialBalance(id, true)`，`force=true` 绕过 300 秒余额缓存
（`admin/service.rs:25`）→ 每凭据一次解析。但它**默认关闭**（`dashboard.tsx:53`）、
默认间隔 120 秒（`:32`）。

`use-credentials.ts:21` 的 30 秒轮询**不是**放大因子：它调 `getCredentials` →
`get_all_credentials`（`admin/service.rs:82`），只读 `snapshot()` 内存状态，不发上游请求。

## 目标设计

### 方案选型

| 方案 | 做法 | 取舍 |
| --- | --- | --- |
| **A：每凭据解析冷却窗口**（选中） | `CredentialEntry` 加内存字段记录上次解析结果与时刻；冷却期内跳过 list 与强刷，直接软放行 | 改动小、语义清晰、可离线测试 |
| B：进程内一次性标记 | 每凭据只尝试一次，之后永不重试 | 最简单，但凭据换了 token 后也不再试，会永久放弃可能拿到的 ARN |
| C：把 Social 纳入 `refresh_routes_to_idc` 软放行 | 彻底不强刷 | 违反上一个 change 的既有结论（Social refresh 可能返回 ARN），会丢功能 |
| D：给 `resolve_profile_arn` 加只读刷新变体 | 新增不落盘、不轮换的轻量刷新 | 需复制刷新逻辑，且仍每请求一次往返，没解决根本问题 |

**选 A。** B 过于激进，C 丢功能，D 治标。

### 冷却状态

在 `CredentialEntry` 增加**内存字段**（不进 `KiroCredentials`，不落盘）：

```rust
profile_arn_cooldown: Option<ProfileArnCooldown>,

struct ProfileArnCooldown {
    /// 冷却起点：本次解析**完成**的时刻
    since: std::time::Instant,
    /// 冷却原因，决定窗口时长
    kind: CooldownKind,
    /// 写入时的凭据版本号
    version: u64,
}

enum CooldownKind {
    /// 解析走完但上游确实没有可用 ARN（list miss + 刷新成功却无 ARN）
    NoArn,
    /// 瞬时故障（网络错误 / 429 / 5xx）导致解析未能完成
    TransientFailure,
}
```

用 `Instant` 而非 `SystemTime`：只关心相对时长，不受系统时钟调整影响，也不需要序列化。

### 凭据版本号而非 refreshToken 哈希（关键设计点）

**用 refreshToken 哈希做指纹会使方案完全失效。** Social 刷新响应带 `refreshToken` 时
会轮换它（`token_manager.rs:332-334`），而这次强刷本身就是轮换源：

```
强刷 → refreshToken A→B → 若记录 hash(A)
下个请求 → 当前是 B → hash(B) ≠ hash(A) → 判定「凭据变了，值得再试」→ 再强刷
```

冷却在最常见路径上永不生效。

**采用凭据版本号**：`CredentialEntry` 内存自增计数，在所有 `entry.credentials = …`
赋值点递增。全仓库共 6 处：`token_manager.rs:1253`、`:1899`、`:2190`（upsert）、
`:2365`（强刷）、`:2497`、`:2573`。冷却记录里存「写入时的版本号」，判定时比对当前版本。

选它的理由：把「凭据是否变了」变成由写入方声明的事实，而不是让冷却逻辑从 token 值
反推变化来源——后者无法区分「别人改了凭据」与「我自己刚刚刷新过」。
版本号还顺带覆盖 upsert、过期刷新等所有变更路径，不必逐条枚举失效条件。

代价是要触碰 6 个赋值点。

### 冷却时长

| 原因 | 窗口 | 依据 |
| --- | --- | --- |
| `NoArn` | 15 分钟 | 上游确实没有 ARN 时，其可用性变化（订阅变更、profile 首次创建）是人工操作量级。15 分钟把「每请求一次」降到「最多每 15 分钟一次」，又不让刚变可用的账号等太久 |
| `TransientFailure` | 30 秒 | 瞬时故障应快速恢复，不能因一次网络抖动压制 15 分钟 |

两个值都是编译期常量，不做配置化——它们是实现细节，暴露成配置项会增加无意义的调参面。

### 冷却检查点必须在 list 之前

把冷却分支放在 `decide_profile_action` 内部**省不掉** `ListAvailableProfiles`——
它在 `profile.rs:230` 无条件先于决策执行。而一次 list 是完整 TLS 握手 + 可能的 3 次
重试与 600ms 退避，量级与强刷相当。

因此冷却检查在 `resolve_profile_arn` 内、list 调用**之前**：

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
冷却是 `resolve_profile_arn` 这一层的调度决策，不是决策函数的输入维度。

**权衡**：一起冷却 list 意味着「上游 ARN 刚变可用」时，list 也要等到窗口过期才重试。
接受的理由：`NoArn` 冷却只在 list **已经** miss 且刷新也拿不到 ARN 后才写入，
即已有证据表明该凭据当前无 ARN；而 ARN 变可用是人工操作量级。
若将来实测发现 list 的恢复速度确有价值，可拆成两个独立窗口（list 短、refresh 长），
但在没有数据前不做这个复杂化。

### 并发去重

冷却的「检查—动作」不是原子的：N 个并发请求可同时读到「未冷却」，全部发起 list + 强刷，
并在 `refresh_lock` 上排队。这条路径会完全绕过冷却，使改动后的最坏情况与改动前无异。

加剧这一点的是既有缺陷：`force_refresh_token_for` 在**取锁之前**克隆凭据
（`token_manager.rs:2344-2351`，取锁在 `:2354`），因此排队中的后续任务持有的是已被
前一次刷新轮换掉的旧 refreshToken，仍会拿它去请求上游。任何「检查冷却 → 决定强刷」
的方案若不做去重，都会被这条路径绕过。

**做法**：给「为取 ARN 而解析」加每凭据的进行中标记，与冷却字段同一把 `entries` 锁保护：

```rust
/// true 表示已有请求正在为该凭据解析 ARN
profile_arn_resolving: bool,
```

- 抢到标记者：走完整解析，结束时清标记并写冷却。
- 未抢到者：**直接软放行**（`Err(ProfileArnUnavailable)` → `Ok(None)`），不等待。

选「不等待」而非「等待结果」：等待需要 `Notify`/`watch` 一类同步原语并处理超时与取消，
复杂度远高于收益——本请求以无 ARN 继续本来就是既有的合法路径
（`ensure_profile_arn_for_request:432` 已吸收为 `Ok(None)`），且并发首请求之后的
后续请求很快会命中新写入的冷却或已解析出的 ARN。

标记必须在**所有**退出路径上清除，包括错误返回与 panic 安全（用 RAII guard）。
标记泄漏会导致该凭据永久不再解析 ARN，比原缺陷更糟。

### 结果分类与冷却写入规则

必须区分结局，因为其中一种若误入冷却会**改变错误传播语义**：刷新硬失败当前返回硬错误
（`profile.rs:265-270`），经 `provider.rs:402-414` 计入 `report_failure`；
若改为软放行，一个 refreshToken 已失效的凭据将不再被计失败、不再被禁用。

| 解析结局 | 当前行为 | 写冷却？ | 改动后行为 |
| --- | --- | --- | --- |
| list 得到可信 ARN | `Use(arn)`，落盘 | **清除**冷却 | 不变 |
| 强刷后拿到可信 ARN | 返回 ARN | **清除**冷却 | 不变 |
| 强刷成功但无可信 ARN | `Err(ProfileArnUnavailable)` → `Ok(None)` | **写 `NoArn`（15 min）** | 不变（软放行） |
| 强刷失败：网络 / 429 / 5xx | `bail!` 硬错误（含 list + refresh 两处原因） | **写 `TransientFailure`（30 s）** | **仍 `bail!` 同样的硬错误** |
| 强刷失败：`invalid_grant`（`RefreshTokenInvalidError`） | `bail!` 硬错误 | **不写** | 不变，继续走既有禁用策略（`report_refresh_token_invalid`，`:1646`） |
| `persist_credentials` 失败 | 仅 `warn!`，不影响返回值（`:2371-2373`） | 按上面各行照常 | 不变 |
| IdC / API Key（未走强刷） | `SoftUnavailable` / `Unsupported` | **不写** | 不变 |

关键约束：**写冷却与「本次返回什么」是两件独立的事。** `TransientFailure` 那一行
必须同时满足两点——本次仍上抛含两处原因的硬错误（生效 spec 的 MUST），
且后续 30 秒内的解析直接软放行不再往返。这不矛盾：冷却抑制的是**后续请求的往返**，
不是**本次请求的错误**。

`invalid_grant` 不写冷却，因为该凭据会被立即禁用（`:1156-1158`），禁用后不再被
`acquire_context` 选中，冷却无意义；且若运维手动重新启用，应立刻重试而非等 30 秒。

### 冷却失效条件

版本号方案下收敛为两条：

1. 窗口到期（`NoArn` 15 分钟 / `TransientFailure` 30 秒）
2. 凭据版本号变化——自动覆盖重新导入、upsert、过期刷新、Admin 手动强刷，
   以及本次强刷自身的轮换

**Admin 手动强刷**：版本号方案下无需任何显式「清除冷却」逻辑——
`force_refresh_token_for` 在 `:2365` 赋值 `entry.credentials` 时递增版本号，
冷却因版本不匹配而自动失效，该函数的语义与签名都不必改。这是选版本号而非哈希的第二个理由。

### 可观测性

`profile.rs` 当前零日志，补最小必要的四条：

| 位置 | 级别 | 内容 |
| --- | --- | --- |
| list 失败 / 空 / 仅占位 | `debug` | 凭据 id + list 阶段结果（不含 token） |
| 决定强刷前 | `info` | `凭据 #N 无可信 profileArn，尝试刷新以获取（后续 15 分钟内不再重试）` |
| 命中冷却而跳过解析 | `debug` | `凭据 #N profileArn 解析冷却中（原因/剩余时长），以无 ARN 继续` |
| 未抢到并发标记 | `debug` | `凭据 #N 已有解析在进行，本次以无 ARN 继续` |

关键是第二条：它把「强刷」这个动作与「profileArn 解析」这个原因显式连起来。

`force_refresh_token_for` 的调用来源标注（区分 Admin 手动 vs profileArn 兜底）
在版本号方案下不再必需——第二条 `info` 日志已提供因果线索。列为可选增强，本 change 不做。

## 影响面

| 文件 | 改动 |
| --- | --- |
| `src/kiro/profile.rs` | `resolve_profile_arn` 在 list 前加冷却检查、加并发标记、按分类表写/清冷却；补 4 条日志；新增测试。**`decide_profile_action` 不改。** |
| `src/kiro/token_manager.rs` | `CredentialEntry` 加冷却字段、进行中标记、版本号；新增读/写这些状态的方法；6 处赋值点递增版本号 |
| `openspec/changes/social-profile-arn-cooldown/specs/profile-arn-resolution/spec.md` | **必需** delta |

`resolve_profile_arn` 与 `ensure_profile_arn_for_request` 的**公开签名保持不变**，
七个调用点零改动。`decide_profile_action` 签名亦不变。

**两把锁必须区分**：保护 `entries` 的同步 `Mutex`（`:697`）与 `refresh_lock`
（`TokioMutex`，`:701`）不是同一把。冷却与进行中标记的读写都在前者的极短临界区内完成，
**不跨 await**。

## 异常路径

| 异常 | 处理 |
| --- | --- |
| 抢到标记后解析以错误退出 | RAII guard 清标记；按分类表决定是否写冷却；错误原样上抛 |
| 抢到标记后 panic | RAII guard 在 unwind 时清标记 |
| 凭据在解析期间被删除 | 写冷却时找不到 entry → 静默跳过（不 panic、不报错），解析结果照常返回 |
| 凭据在解析期间被 upsert（版本号变化） | 冷却仍按「解析完成时读取的当前版本号」写入；下次请求比对到版本已变 → 允许重试。允许一次多余重试，比误压制安全 |
| `list` 成功但 `set_profile_arn` 失败 | 与现状一致（`let _ =` 忽略），仍返回 ARN；清除冷却 |
| 冷却记录存在但 `Instant::elapsed` 异常大（进程长时间挂起） | 判定为已过期 → 允许解析。偏保守方向，可接受 |

## 回滚

三部分改动彼此独立，可分别回滚：

1. 仅回滚代码（保留 spec delta）：`git revert` 相关提交后，`resolve_profile_arn` 恢复
   每请求解析；由于 spec delta 尚未归档进 `openspec/specs/`，不产生规格不一致。
2. 仅关闭冷却而保留其余：把两个时长常量设为 `Duration::ZERO` 即等价于关闭
   （冷却写入后立即过期），不必改结构。这是应急旋钮，不作为配置项暴露。
3. 冷却状态不落盘，回滚无需数据迁移，重启即回到干净状态。

## 验证策略

### 冷却状态机（纯函数层）

`Instant` 无法构造任意过去时刻（只有 `checked_sub`，且不保证成功），
所以判定函数签名定为接受已算好的 `elapsed`：

```rust
fn is_cooling(kind: CooldownKind, elapsed: Duration) -> bool
```

由调用方算 `elapsed = since.elapsed()`，测试直接传 `Duration::from_secs(901)`。
不引入时间抽象层，也不需要可注入时钟。

| 场景 | 验收点 |
| --- | --- |
| 无冷却记录 | 允许解析 |
| `NoArn` 且 elapsed < 15 min | 冷却中，跳过 list **与**强刷 |
| `NoArn` 且 elapsed > 15 min | 允许解析 |
| `TransientFailure` 且 elapsed < 30 s | 冷却中 |
| `TransientFailure` 且 elapsed > 30 s | 允许解析 |
| 版本号与记录不符（任意 elapsed） | 允许解析 |

### 解析调度层

`decide_profile_action` 不改，故「决策纯函数 × 冷却状态」的组合测试**不需要**——
其现有 10 个测试保持不变即为回归证明。测试重心在冷却状态机与解析调度。

| 场景 | 验收点 |
| --- | --- |
| trusted ARN 命中 × 有冷却记录 | 返回 ARN，**不查冷却、不发 list** |
| list 返回可信 ARN × 有冷却记录 | `Use(arn)` 并**清除**冷却 |
| 冷却命中 | `Err(ProfileArnUnavailable)`，且**断言 list 未被调用** |
| 强刷成功但无 ARN | 写 `NoArn`；返回软放行 |
| 强刷瞬时失败（网络/429/5xx） | 写 `TransientFailure`；**仍返回含 list + refresh 两处原因的硬错误** |
| 强刷 `invalid_grant` | **不写**冷却；仍返回硬错误 |
| IdC × list miss | `SoftUnavailable`，**不写**任何冷却 |

「断言 list 未被调用」需要可注入的 list 边界。若为此改动 `resolve_profile_arn` 的
公开签名会违反既有 spec（`:77` 要求签名不变），因此边界注入只能通过内部私有函数
（如把现有逻辑抽为 `resolve_profile_arn_inner`，list 阶段以闭包/trait 传入，
公开函数保持原签名并传入真实实现）。这一点在 tasks 中作为独立任务先行确认可行性。

### 并发去重

| 场景 | 验收点 |
| --- | --- |
| 两个任务并发解析同一凭据 | 只有一个发起 list + 强刷；另一个立即软放行 |
| 解析以错误退出 | 进行中标记**已清除** |
| 解析 panic / 提前 return | 标记不泄漏（RAII guard 覆盖） |

### 版本号

| 场景 | 验收点 |
| --- | --- |
| 6 处 `entry.credentials =` 赋值 | 每处都递增版本号（逐处断言，防遗漏） |
| Social 强刷轮换 refreshToken | 版本号变化 → 但冷却在强刷**之后**写入，记录的是**新**版本号 → 下次请求仍命中冷却（**最容易写错的一处**） |
| Admin 手动强刷后 | 版本号变化使旧冷却失效；无需显式清除逻辑 |

### 验证命令

```powershell
cargo test kiro::profile
cargo test kiro::token_manager
cargo test
openspec validate --all
git status --short
```

端到端复现见 proposal 的 Success Criteria 表。真实验证只用本地临时凭据，
确认 `config.json`、`credentials.json`、`credentials.*`、`.codegraph/` 不进 Git 候选。

## 与 external_idp 的关系

| 凭据类型 | 刷新端点是否可能返回 ARN | 当前决策 | 本方案后 |
| --- | --- | --- | --- |
| IdC | 否（AWS OIDC 标准端点） | `SoftUnavailable` | 不变 |
| Social | **是**（Kiro 自有端点） | `ForceRefresh` 每次 | 首次 `ForceRefresh`，之后冷却 |
| external_idp | 否（Microsoft 端点） | `ForceRefresh` 每次 | 首次 `ForceRefresh`，之后冷却（**顺带缓解**） |

`kam-external-idp-import-compat` 记录了这条已知遗留：external_idp 也会为取 profileArn
而强刷，而 Microsoft token 端点**不返回** `profileArn`——比 Social 更确定无收益。

本方案把 external 从「每请求一次」降到「每 15 分钟一次」，但没有根治。
external 的正解是把 `refresh_routes_to_idc` 的语义从「是否走 OIDC」改为
「刷新端点是否可能返回 profileArn」，这作为后续 change：它会牵连
`profile-arn-resolution` 的多个既有 spec 场景，需要单独的规格评审。

`profile.rs:727-740` 的 `test_external_without_arn_currently_force_refreshes`
以注释锁定了这一现状。本 change **不修改**该测试（冷却在其上层，decide 行为不变）。

## Open Questions

- **无。** 三处曾需确认的点已在文档审核中定论：指纹用版本号（不用哈希）、
  冷却检查点在 list 之前（不在 decide 内）、并发未抢到者直接软放行（不等待）。
  若实现中发现 list 边界注入无法在不改公开签名的前提下完成，需停下确认——
  见 tasks 2.1。
