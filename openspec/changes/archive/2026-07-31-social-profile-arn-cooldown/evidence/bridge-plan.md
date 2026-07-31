# Bridge Plan — social-profile-arn-cooldown

> 产出时间：2026-07-31
> 分支：`dev`（主分支为 `master`）
> 基线：`e775835` + `kam-external-idp-import-compat` 已实现改动（未提交）
> 状态：`openspec status --change social-profile-arn-cooldown --json` → `isComplete: true`，
> 四件工件（proposal / design / specs / tasks）均 `done`，**非 blocked**

## 1. 范围与非目标

### 范围

| 文件 | 改动 |
| --- | --- |
| `src/kiro/token_manager.rs` | `CredentialEntry` 加 `credentials_version`、`profile_arn_cooldown`、`profile_arn_resolving`；新增存取方法与 RAII guard；6 处 `entry.credentials =` 递增版本号 |
| `src/kiro/profile.rs` | `resolve_profile_arn` 在 list **之前**加冷却检查与资格抢占；按结局分类写/清冷却；补 4 条日志；新增测试 |
| `openspec/changes/.../specs/profile-arn-resolution/spec.md` | 已产出（1 MODIFIED + 5 ADDED Requirement） |

### 非目标（本计划逐条复核，均确认为「不碰」）

- 不改 `decide_profile_action`（冷却在其上层）
- 不改 `force_refresh_token_for` 语义与签名（仅递增版本号一行）
- 不改 IdC / API Key 决策
- 不改刷新硬失败的错误文本与传播路径
- 不引入配置项（两个时长为编译期常量）
- 不落盘冷却状态
- 不改 `refresh_lock` 粒度
- 不改 `ListAvailableProfiles` 的重试与退避
- 不修 `force_refresh_token_for` 取锁前克隆凭据的既有缺陷（仅加注释留痕）
- 不重定义 `refresh_routes_to_idc` 语义

### 关键设计决策（三项，均已在文档审核中定论）

1. **冷却检查点在 `ListAvailableProfiles` 之前**，而非 `decide_profile_action` 内部。
   依据：list 在 `profile.rs:230` 无条件先于决策执行，只挡强刷省不掉那次 TLS 握手。
   代价：`decide_profile_action` 完全不动，其 10 个既有测试即回归证明。
2. **指纹用凭据版本号，不用 refreshToken 哈希。**
   依据：`token_manager.rs:332-334` 确认 Social 刷新会轮换 refreshToken，
   而这次强刷本身就是轮换源 → 哈希方案使冷却在最常见路径上永不生效，且静默。
3. **并发未抢到标记者直接软放行，不等待。**
   依据：`ensure_profile_arn_for_request:432` 已把 `ProfileArnUnavailable` 吸收为 `Ok(None)`，
   无 ARN 继续本来就是合法路径。

## 2. 高风险项

风险类型（AGENTS.md 矩阵）：**Token / 多凭据 + OpenSpec**。

| # | 风险 | 后果 | 控制 |
| --- | --- | --- | --- |
| H1 | 版本号时序写错（记录刷新**前**的版本） | 冷却永不生效，方案零收益，且日志看起来正常 | tasks 6.4 专项测试；实现时先写测试 |
| H2 | 并发标记泄漏 | 该凭据本进程内**永久**不再解析 ARN，比原缺陷更糟 | RAII guard（tasks 5.4）；三种退出路径测试 |
| H3 | 冷却误吞硬错误 | refreshToken 失效的凭据不再被计失败/禁用，故障静默 | tasks 6.3/6.5 逐行分类；`invalid_grant` 不写冷却 |
| H4 | 违反生效 spec 的无条件 MUST | 归档时一致性审查不通过 | 已产出 spec delta（本 change 的前置条件） |
| H5 | 锁误用（把冷却读写放进 `refresh_lock` 或跨 await） | 死锁或状态竞争 | 见 §4 的锁边界证据；tasks 1.4 |

## 3. CodeGraph 证据

```
codegraph status
→ Files 134 / Nodes 2,711 / Edges 7,162 / rust 76 files（索引可用）

codegraph callers "resolve_profile_arn"
→ 1: ensure_profile_arn_for_request (src/kiro/profile.rs:398)

codegraph callers "force_refresh_token_for"
→ 2: resolve_profile_arn (src/kiro/profile.rs:195)
     force_refresh_token (src/admin/service.rs:721)

codegraph impact "resolve_profile_arn"
→ 3 affected symbols，全部在 src/kiro/profile.rs 内
```

**结论：**

- `force_refresh_token_for` 恰好两个调用者，印证「Admin 按钮 + profileArn 兜底」
  这一双重用途，也印证不改其语义的必要性（`admin/service.rs:721` 的
  `force_refresh_token` 直接透传，只做错误分类）。
- `impact` 显示改动闭合在 `profile.rs` 内，与「公开签名不变、七个调用点零改动」一致。

### CodeGraph 的一处漏报（必须由 rg 补盲）

```
codegraph callers "ensure_profile_arn_for_request"
→ No callers found      ← 与文档所述「七个入口」矛盾
```

原因：调用点多为 `crate::kiro::profile::ensure_profile_arn_for_request(...)`
全限定路径或 `profile::` 模块前缀形式，索引未建立该边。
**这正是 AGENTS.md 要求「CodeGraph 不替代 rg」的实例。** 已用 rg 补齐（§4）。

## 4. rg / 源码补盲

### 4.1 七个入口（rg 确认，文档记录准确）

```
rg -n 'ensure_profile_arn_for_request' src/
src/kiro/provider.rs:178        MCP / WebSearch
src/kiro/provider.rs:298        MCP bearer-invalid 恢复
src/kiro/provider.rs:393        generateAssistantResponse
src/kiro/provider.rs:556        generate bearer-invalid 恢复
src/kiro/token_manager.rs:1931  get_usage_limits_for
src/kiro/token_manager.rs:2602  refresh_models_for
src/admin/service.rs:881        run_minimal_generate
src/kiro/profile.rs:398         定义处
```

七处调用点行号与 proposal/design 记录**逐一相符**。

### 4.2 六处 `entry.credentials =`（tasks 1.3 的前置核对，已完成）

```
rg -n 'entry\.credentials = ' src/kiro/token_manager.rs
1253  try_ensure_token 内刷新
1899  get_usage_limits_for 内刷新
2190  upsert（validated_cred）
2365  force_refresh_token_for
2497  模型刷新路径
2573  模型刷新路径
```

**恰好 6 行，行号与 design 记录逐一相符。** tasks 1.3 的停止条件未触发。

### 4.3 `CredentialEntry` 构造点（CodeGraph 未提示，rg 补出）

```
rg -n 'CredentialEntry \{' src/
524   struct 定义
814   构造点 1（初始加载）
2243  构造点 2（entries.push，新增凭据）
```

**影响 tasks 4.1**：三个新字段必须在**两处**构造点初始化，不止一处。
已在下方任务映射中标注。

### 4.4 `profile.rs` 零日志（确认）

```
rg -n 'tracing::' src/kiro/profile.rs
→ No matches found
```

与文档 2.4 节一致。tasks 7.1 的验收「从 0 条变为 4 条」成立。

### 4.5 两把锁的边界（确认 design 的说法）

```
token_manager.rs:697  entries: Mutex<Vec<CredentialEntry>>        ← 同步锁（parking_lot 风格 .lock() 无 await）
token_manager.rs:701  refresh_lock: TokioMutex<()>                ← 异步锁
```

`report_refresh_token_invalid`（`:1646-1668`）与 `report_failure`（`:1476`）
是既有的「同步锁内短临界区」范式，冷却存取方法应照此实现。
`force_refresh_token_for` 中 `entries.lock()`（`:2363`）与 `refresh_lock`（`:2354`）
并存但不嵌套跨 await，验证了 design 的锁划分可行。

### 4.6 `persist_credentials` 的条件写盘（确认）

```
token_manager.rs:1301-1312
  !self.is_multiple_format          → Ok(false)
  credentials_path == None          → Ok(false)
```

与 proposal「写盘仅在多凭据格式且已配置路径时发生」一致。
Success Criteria 中「写盘 10→1」的限定条件成立。

### 4.7 `invalid_grant` 的分类与禁用路径

```
token_manager.rs:307-315   Social：400 + "invalid_grant" + "Invalid refresh token provided" → RefreshTokenInvalidError
token_manager.rs:403-411   IdC 同形态
token_manager.rs:225-228   external_idp：400 + invalid_grant（无第二条件）
token_manager.rs:1156-1158 acquire_context 内 downcast → report_refresh_token_invalid
token_manager.rs:1646-1668 立即禁用，DisabledReason::InvalidRefreshToken
```

**注意 Social 的判据是两个条件的合取**：仅 `invalid_grant` 而无
`Invalid refresh token provided` 会落入 `:317-324` 的通用 `bail!`，
被本 change 分类为 `TransientFailure`（400 不在 429/5xx 之列，但也不是
`RefreshTokenInvalidError`）。**这是 tasks 6.3 实现时必须明确的边界**：
分类依据应是 `e.downcast_ref::<RefreshTokenInvalidError>().is_some()`
（而非解析错误文本），凡非该类型的刷新失败一律写 `TransientFailure`。
已在下方任务映射中作为 6.3 的补充验收点。

### 4.8 配置 / Docker / CI / example 补盲（CodeGraph 不覆盖）

本 change **不引入配置项**，因此需确认无相关面被牵动：

- `credentials.example.*.json`：冷却状态不落盘 → 示例文件**无需改动**
- `Dockerfile` / `docker-compose` / `.github/workflows`：无新增依赖、无新增环境变量
  → **无需改动**
- `admin-ui/`：`CredentialEntrySnapshot` 不新增字段（tasks 4.4 断言）
  → 前端类型与 `pnpm build` **不在影响面内**

## 5. 两处需修正的既有工件（本次核查发现）

### 5.1 design 对 `provider.rs:402-414` 的引用需收窄（P2，措辞层）

design 的结局分类表写「刷新硬失败当前返回硬错误，经 `provider.rs:402-414`
计入 `report_failure`」。核查确认该路径存在（`:405` 确为 `report_failure`），
**但它只是七个入口之一**。其余六处对硬错误的处理并不相同：

| 入口 | 硬错误处理 |
| --- | --- |
| `provider.rs:393`（generate） | `report_failure` + 换凭据（`:403-414`） |
| `provider.rs:178`（MCP） | 仅 `warn!`，继续（`:186`） |
| `provider.rs:298` / `:556`（bearer 恢复） | `.ok().flatten()` **吞掉错误** |
| `token_manager.rs:1931`（余额） | 仅 `warn!`，继续裸查 |
| `token_manager.rs:2602`（模型） | 仅 `warn!`，继续 |
| `admin/service.rs:881`（test） | 仅 `warn!`，继续 |

**对本 change 的影响：无。** 「不改错误语义」这一约束在所有七处都因
「错误对象逐字不变」而自动满足；design 的论证只是举了影响最大的那一处。
但「refreshToken 失效的凭据不再被计失败」这个风险描述应精确为
**仅对 generate 路径成立**——另外六处本来就不计失败。
**处置**：不改 design（结论不变，且该措辞不会误导实现），在此留痕。

### 5.2 spec delta 中「MUST 按既有策略计入失败与禁用判定」措辞过强（P1，需修正）

我在 `specs/profile-arn-resolution/spec.md` 的
`Scenario: 瞬时失败仍上抛硬错误` 中写了：

```
- AND MUST 按既有策略计入失败与禁用判定
```

按 §5.1 的核查，这条对七个入口中的**六个不成立**——它们只 `warn!` 或直接吞错误，
不计入任何失败。该措辞会把一条事实上不存在的行为写成 MUST，
使 spec 与代码不一致（且实现者可能误以为需要新增计失败逻辑，属范围外改动）。

**修正**：改为约束「错误对象不变」而非「下游必须计失败」，
把下游行为的不变性表述为「与引入前相同」。已在本计划产出后立即修正。

## 6. 任务到执行步骤映射

| tasks | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1–1.4 | 已在本计划完成（§1、§4.2、§4.5） | 6 处赋值点已核对为恰好 6 行；两把锁已区分 | 若赋值点数量 ≠ 6 → 已确认为 6，未触发 |
| 2.1 | 抽 `resolve_profile_arn_inner`，list 阶段以闭包/参数注入；公开函数保持原签名 | `git diff` 显示 `pub async fn resolve_profile_arn` 与 `ensure_profile_arn_for_request` 签名逐字不变 | **若无法在不改公开签名前提下注入 → 停下汇报**（三条验收项依赖它） |
| 3.1–3.3 | 定义 `CooldownKind` / `ProfileArnCooldown` / 两常量 / `is_cooling(kind, elapsed)` | 6 行状态机表单测；常量不出现在任何 serde 结构 | 无 |
| 4.1 | 新增 `credentials_version`，**在 `:814` 与 `:2243` 两处构造点**初始化 | `cargo build`；rg 确认无第三处构造点 | 若出现新构造点 → 补齐后继续 |
| 4.2 | 6 处赋值点后递增版本号 | 表驱动单测覆盖 6 条，逐处断言 +1 | 无 |
| 4.3–4.4 | 版本号读取方法；断言不落盘、不进 snapshot | serde roundtrip 测试 | 无 |
| 5.1–5.5 | 冷却与标记字段 + 存取方法 + RAII guard | 5.3 断言两次调用只有一次取得资格；5.5 覆盖 entry 已删除 | 无 |
| 5.6 | 确认 `force_refresh_token_for` 仅多出递增一行 | `git diff` 该函数 | 若需在其内清冷却 → 说明版本号方案未落实，停下 |
| 6.1–6.2 | 冷却检查插入点：`trusted_profile_arn` 之后、api_key 判定之后、list **之前** | 断言 list 未被调用（核心验收项）；trusted 命中时不查冷却 | 无 |
| 6.3 | 结局分类写/清冷却；**分类依据为 `downcast_ref::<RefreshTokenInvalidError>()`，非错误文本**（§4.7） | 7 条对应单测；补一条：Social 400 无 `Invalid refresh token provided` → `TransientFailure` | 无 |
| 6.4 | 冷却在强刷**之后**写入，取当时的新版本号 | 专项测试：强刷轮换 refreshToken 后紧随请求仍命中冷却 | **最易写错处**，测试未通过前不推进 |
| 6.5 | 错误文本逐字不变 | 断言仍匹配 `no available Kiro profile (list: ...; refresh: ...)` | 无 |
| 6.6 | guard 覆盖全部退出路径 | 并发测试 + 错误退出后标记已清 | 无 |
| 6.7–6.8 | 确认 `decide_profile_action` 与七个调用点零改动 | `git diff` 不含 `provider.rs`、`admin/service.rs`；既有 10 测试未改且全绿 | 若必须改 decide → 说明设计偏离，停下 |
| 7.1–7.5 | 4 条日志 | `rg 'tracing::' src/kiro/profile.rs` 由 0 → 4；无机密 | 无 |
| 8.1–8.2 | 既有缺陷注释留痕；确认 `refresh_routes_to_idc` 未改 | 注释存在且逻辑未动 | 无 |
| 9.1–9.9 | 验证与收尾 | 见 §7 | 见 §8 |

## 7. 必跑验证

```powershell
cargo test kiro::profile          # 含 decide_profile_action 既有 10 测试
cargo test kiro::token_manager    # 含 6 处版本号逐处断言
cargo test                        # 全仓库
openspec validate --all           # 已通过（20/20），改动 spec 后需重跑
git status --short                # 确认无凭据/缓存入库
```

端到端（tasks 9.5）：重跑受控实验，补齐三项缺失信息（list 上游状态码与响应体、
凭据 `authMethod`/`provider`、复现命令原文），分别计时 list 与 refresh。
目标：强刷 10→1、list 10→1、写盘 10→1。**只用本地临时凭据，实验后清理隔离实例。**

前端不在影响面（§4.8），`pnpm build` 本次**不需要**。

## 8. README / AGENTS / spec / openspec/specs 同步判断

| 入口 | 判断 | 理由 |
| --- | --- | --- |
| `README.md` | **无需** | 不改启动、构建、部署、测试、API 入口；无新增配置项或环境变量 |
| `AGENTS.md` | **无需** | 不改 AI 纪律、高风险矩阵或验证命令 |
| `spec/design.md` | **无需** | 模块边界与数据流不变；冷却是 `src/kiro/` 内部实现细节 |
| `spec/requirements.md` / `structure.md` | **无需** | 无新增模块、无新增文件 |
| `openspec/specs/profile-arn-resolution/spec.md` | **归档时同步**（不在实现阶段改） | 本 change 的 delta 已在 `changes/.../specs/`；按 OpenSpec 流程归档时才合入主规格 |
| `credentials.example.*.json` | **无需** | 冷却状态不落盘 |
| `admin-ui/` | **无需** | snapshot 不新增字段 |

若实现中结论发生变化，在最终报告中说明并执行。

## 9. 机密与工作区检查

```
git status --porcelain | grep -Ei 'credentials\.json|config\.json|\.codegraph|\.env'
→ 无匹配

git check-ignore -v credentials.json config.json .codegraph
→ .gitignore:9:/credentials.*   credentials.json
→ .gitignore:2:/config.json     config.json
→ .gitignore:16:.codegraph/     .codegraph
```

三者均被正确忽略，**工作区无会被提交的真实凭据、token、Cookie 或本地缓存**。

工作区存在 `kam-external-idp-import-compat` 的未提交改动（21 个 M + 9 个 ??）。
本 change 的改动与其在 `token_manager.rs` / `profile.rs` 上有文件级重叠但无逻辑冲突
（前者改凭据模型与刷新分派，后者加冷却状态）。
**实现时需注意：`git diff` 作为验收手段会同时显示两个 change 的改动**，
tasks 中所有「`git diff` 显示 X 未变」的验收项应针对具体函数体判断，不能整文件比对。

## 10. 停止条件

1. **tasks 2.1 判定 list 边界无法在不改公开签名的前提下注入** —— 三条核心验收项
   （断言 list 未被调用）失去手段，需重新设计或申请放宽 spec `:77`/`:84` 的签名约束。
2. **6 处赋值点数量与记录不符** —— 已核对为恰好 6 处，未触发；若后续 rebase 后变化，停下重核。
3. **需要在 `force_refresh_token_for` 内显式清冷却** —— 说明版本号方案未真正落实，
   与 Non-Goal 冲突，停下。
4. **需要修改 `decide_profile_action` 或任一调用点** —— 说明冷却检查点位置偏离设计，停下。
5. **发现冷却会改变任一入口的错误传播语义** —— 属 H3，停下重新分类。
6. 出现未写入规格的高风险影响（如冷却状态意外落盘、影响 Admin 响应结构）。
7. 无法确定某项验证命令或剩余风险。

## 11. 本计划的核查结论

- OpenSpec 工件齐备、互不矛盾、状态非 blocked。
- 文档与工件中的**全部行号与调用链断言均已由 rg 逐条复核为准确**（§4.1、§4.2）。
- 发现并处置两处工件问题：§5.1（P2，留痕不改）、§5.2（P1，需立即修正 spec 措辞）。
- 发现一处 CodeGraph 漏报（§3），已由 rg 补盲。
- 发现一处实现边界需明确：`invalid_grant` 分类应按错误类型而非文本（§4.7），
  已并入 tasks 6.3 验收点。
- 工作区无机密泄漏风险。
