# Spec Compliance Report: profile-arn-refresh-fallback-order

日期：2026-07-29
审查类型：实现后 / 提交前合规（`spec-compliance-check`）
总体状态：**PASS**

> 说明：本次工作区同时存在三个 active change（本 change、`codex-responses-lite-tool-passthrough`、
> `admin-ui-balance-refresh-fixes`）。三者文件归属互不重叠，本报告只审查
> `src/kiro/profile.rs` 与 `src/kiro/token_manager.rs`；`src/openai/**` 与 `admin-ui/**`
> 的改动归属另两个 change，不计入本 change 的越界。

## 六维表

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | **PASS** | `git diff --name-only` 中属本 change 的仅 `src/kiro/profile.rs`、`src/kiro/token_manager.rs`，与 proposal Impact 一致。`provider.rs` 零改动（tasks 5.5 声称的 7 处调用点未动，已用 `git diff --name-only \| grep -c provider.rs` = 0 核实）。`infer_provider` / `looks_like_idc` 函数体未被修改（diff 中无对应 `+/-` 行）。`Cargo.toml` / `Cargo.lock` 零改动。 |
| Design | **PASS** | design.md「调整后的决策表」8 行与 `decide_profile_action`（`src/kiro/profile.rs:158-177`）逐格对应，详见下方对照表。`Unsupported` 与缓存命中仍在 list **之前**判定（`profile.rs:201-225`），符合 design 注解。`ForceRefresh` 副作用与改前代码逐行一致。 |
| Scenarios | **PASS** | 10 个 Scenario 全部有实现或离线测试对应，详见覆盖表。`decide_profile_action` 新增 7 项测试（`profile.rs` 测试数 12 → 19）。 |
| Project Rules | **PASS** | 属 AGENTS.md OpenSpec 条件「Token 刷新、多凭据」，已建 change 且 `evidence/bridge-plan.md`（327 行）齐备。高风险矩阵要求的 `cargo test` 已实跑。无真实凭据，`git status --short` 无 `config.json` / `credentials.*` / `.codegraph/`。 |
| Verification | **PASS** | 本会话独立复跑全部命令并粘贴结果（见「验证记录」）。tasks 5.2/5.3/5.4/5.6 原文写「→ 验证：粘贴输出」但未回填实际输出，属**记录格式缺口**（WARN-1），非验证缺失——本报告已补齐真实输出。 |
| README/AGENTS Sync | **PASS** | 不改启动、构建、部署、测试入口与 API 端点。`README.md:396` 的「固定表 / ListAvailableProfiles / refresh fallback」顺序描述仍成立（顺序未变，仅 IdC 分支不再落入 refresh），无需改写。`spec/requirements.md` 无 profileArn 层事实。 |

## 决策表对照

design.md:153-162 的真值表逐格核对（实现见 `src/kiro/profile.rs:158-177`）：

| design 决策表条目 | 实现 | 一致? |
| --- | --- | --- |
| api_key × 任意 → `Unsupported` | `:159-161` `is_api_key_credential()` 提前返回；另在 list 前 `:215-217` 已拦 | ✅ |
| 不支持 profile 且 provider 未知 × 任意 → `Unsupported` | list 前置判定 `:223-225`（`!supports_profiles && provider.is_none()`），按 design 注解不进决策函数 | ✅ |
| 任意支持型 × `Resolved(arn)` → `Use(arn)` | `:163-165` | ✅ |
| `refresh_routes_to_idc` == true × `Failed` → `SoftUnavailable` | `:168-170`（三态共用同一分支） | ✅ |
| 同上 × `Empty` → `SoftUnavailable` | `:168-170` | ✅ |
| 同上 × `Placeholder` → `SoftUnavailable` | `:168-170` | ✅ |
| Social 且有 refreshToken × 三态 → `ForceRefresh` | `:172-174` | ✅ |
| Social 且无 refreshToken × 三态 → `Fail` | `:176` | ✅ |

分派顺序（api_key → Resolved → IdC → refreshToken → Fail）使「IdC 三态」在
「有 refreshToken 则强刷」之前短路，正是本 change 的核心行为变更。

`ListOutcome` 归一（`profile.rs:230-244`）四态齐全：`Resolved`（非空且非占位）/
`Placeholder`（非空但占位）/ `Empty`（空）/ `Failed`（Err），与决策表输入维度对齐。

## Requirement / Scenario 对照

### profile-arn-resolution（MODIFIED）

#### Requirement: 请求前必须解析 profileArn

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 可信缓存命中 | `profile.rs:201-203` `trusted_profile_arn` 前置短路；既有测试 `test_cache_contract_nonempty_profile_arn` | PASS |
| 固定占位 ARN 不可信 | `:205-213` 命中占位则 `clear_profile_arn`；既有测试 `test_placeholder_arn_not_trusted` | PASS |
| BuilderId 无可信缓存 | `refresh_routes_to_idc` → `SoftUnavailable` → `ProfileArnUnavailable` → `:432` 转 `Ok(None)`；测试 `test_decide_idc_never_force_refreshes` | PASS |
| IdC 账号不得为取 ARN 而强制刷新 token | `:168-170` 在 refreshToken 分支之前短路；测试 `test_decide_idc_never_force_refreshes` | PASS |
| 每请求不得重复无效强刷 | 决策为纯函数且无状态，IdC 每次都走 `SoftUnavailable`，路径上无 `force_refresh_token_for` 调用；测试同上。**限定 IdC**，Social 的重复强刷属 Non-Goals（与 tasks 6.2 无矛盾） | PASS |
| Enterprise 动态列表 | `:230-233` `Resolved` → `:252-255` `Use(arn)` + `set_profile_arn(.., provider)`；测试 `test_decide_resolved_arn_is_used` | PASS |
| refresh fallback | `:257-271` `ForceRefresh` 分支，与改前 `HEAD:profile.rs:186-204` 逐行一致 | PASS |
| Social 强刷行为不得回归 | 测试 `test_decide_social_still_force_refreshes`（三态各一）+ `test_decide_builder_provider_with_social_auth_force_refreshes`；刷新成功仍无可信 ARN → `:263` `ProfileArnUnavailable`；刷新失败 → `:265-270` bail 含 list 与 refresh 双原因 | PASS |

#### Requirement: profileArn 解析决策必须可在离线条件下验证

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 决策与副作用分离 | `decide_profile_action(&KiroCredentials, ListOutcome) -> ResolveAction`（`:158`）不含 `.await`、不接收 `&MultiTokenManager`；7 项测试无 HTTP client 构造（已 grep 确认）；`resolve_profile_arn`（`:195-200`）与 `ensure_profile_arn_for_request`（`:398-403`）签名与 `HEAD` 逐字相同 | PASS |
| 账号类型与 list 结果的组合可断言 | 7 项 `test_decide_*` 覆盖 api_key / IdC / BuilderId+social / 缺 authMethod+clientCreds / Social 有无 refreshToken × Resolved/Empty/Placeholder/Failed；`kiro::profile` 19 passed | PASS |

## 发现项

### WARN-1：tasks.md 验证项未回填实际输出（LOW）

- **事实**：`tasks.md:83-96` 的 5.2 / 5.3 / 5.4 / 5.6 写「→ 验证：粘贴输出」，但条目下方无实际命令输出，
  与同批 change（如 `admin-ui-balance-refresh-fixes` 的「验证状态」章节）的记录粒度不一致。
- **影响**：仅记录完整性问题。命令本身经本会话独立复跑全部通过，不存在虚报。
- **建议**：归档前把「验证记录」章节的真实输出回填至 tasks.md，或在 tasks 中引用本报告。

### INFO-1：`refresh_routes_to_idc` 提取为逐字等价（已确认非风险）

- **事实**：`token_manager.rs:117-131` 的新函数与 `HEAD` 中 `refresh_token` 内联逻辑逐字相同：
  `auth_method` 的 `unwrap_or_else`（clientId+clientSecret → `"idc"`，否则 `"social"`）、
  三个 `eq_ignore_ascii_case("idc"/"builder-id"/"iam")` 全部原样搬移。`refresh_token:148` 改为调用它。
- **判定**：分流去向逐位不变，符合 proposal Non-Goals「提取是纯重构」。`kiro::token_manager` 52 passed 零回归。

### INFO-2：tasks 1.3 的死分支论证成立

- **事实**：删除的 `HEAD:profile.rs:207-215`（`looks_like_idc || provider==BuilderId`）确为不可达。
  论证链已独立复核：命中需 clientId+clientSecret → 该形态经 `refresh_routes_to_idc` 判为 IdC →
  但更早的 `validate_refresh_token`（`token_manager.rs:72-92`）要求 refreshToken 非空且长度 ≥100，
  故此类凭据必先在 `:186`（旧编号）的 refreshToken 分支返回。
- **判定**：删除非行为回归。新谓词还修掉了旧谓词的缺陷——旧代码会把
  `provider:"BuilderId"` + `authMethod:"social"` 误判为 IdC 而软放行，新代码按刷新实际去向落入 `ForceRefresh`，
  锁定测试 `test_decide_builder_provider_with_social_auth_force_refreshes` 已固化。

### INFO-3：`SoftUnavailable` 下游为「不带 profileArn 继续」

- **事实**：`:252` → `anyhow!(ProfileArnUnavailable)` → `ensure_profile_arn_for_request:432`
  `downcast_ref::<ProfileArnUnavailable>()` → `Ok(None)`，请求不带 profileArn 发出。
  `ProfileArnUnavailable` 是改动前既有类型（`HEAD` 中 6 处引用），语义沿用既有 spec 的 BuilderId 软放行。
- **判定**：符合 design 与 spec Scenario「每个请求 MUST 各自以无 profileArn 正常继续」。

## CRITICAL

无。

## 安全核查

- 错误文案（`:266`/`:268`/`:275`/`:279`）只插入 list 与 refresh 的原因描述，
  不含 token / refreshToken / clientSecret 字段值。`validate_refresh_token` 的截断提示只打印长度。
- `profile.rs:22`/`:25` 的两个 ARN 常量（`SOCIAL_SIGN_IN_PROFILE_ARN` / `BUILDER_ID_PROFILE_ARN`）
  经 `git show HEAD` 确认为改动前既有的 AWS 侧公开默认 profile，非本次引入、非用户凭据。
- 新增 7 项测试无网络访问、无 `MultiTokenManager`、凭据用 `KiroCredentials::default()` 填假值（tasks 4.7 成立）。
- `git status --short` 16 条全部为预期文件，无 `config.json` / `credentials.*` / `.codegraph/`。

## 验证记录（本会话真实运行）

```
$ cargo test --bin kiro-rs
test result: ok. 570 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test --bin kiro-rs kiro::profile
test result: ok. 19 passed; 0 failed        # 改前 12 项，新增 7 项 test_decide_*

$ cargo test --bin kiro-rs kiro::token_manager
test result: ok. 52 passed; 0 failed        # 提取后零回归

$ cargo build
warning: `kiro-rs` (bin "kiro-rs") generated 10 warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)

$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)
```

警告零增量已用 `git stash push --include-untracked` 取基线对比确认：
改动前后同为 10 条（+1 条汇总行），10 条均为既有 dead_code / unused_import，
无一条由本 change 产生。基线检查后工作区已完整还原（`git stash list` 为空）。

原有 12 项测试的断言零删改（`git diff` 中测试块无 `-` 行命中 `assert`/`fn test_`），
无「改测试迁就实现」迹象。

## 未验证项（沿用 tasks 6.1/6.2）

| 项 | 原因与剩余风险 |
| --- | --- |
| 真实上游对无 profileArn 的 IdC generate 是否返回 200 | 本 change 未做联网复现。依据为既有 spec 已固化的「BuilderId 无可信缓存」软放行场景，该路径改动前即在生产运行 |
| Social 凭据「每请求重复强刷」 | 仍存在，proposal Non-Goals 已声明，需负缓存/冷却，属独立 change |

## 证据路径

- Bridge：`openspec/changes/profile-arn-refresh-fallback-order/evidence/bridge-plan.md`
- 本报告：`openspec/changes/profile-arn-refresh-fallback-order/evidence/spec-compliance-report.md`
- OpenSpec 工件：`proposal.md` / `design.md` / `tasks.md` / `specs/profile-arn-resolution/spec.md`（tasks 26/26 勾选）

## 剩余风险（可接受）

1. 无真实上游 E2E（见上表）。
2. Social 重复强刷未修（已声明为独立 change）。
3. `refresh_lock` 粒度未改，锁竞争随调用量下降自然缓解（Non-Goals）。

## 结论

**PASS。** 决策表 8 格与实现逐格一致，10 个 Scenario 全部有实现或离线测试对应，
`refresh_routes_to_idc` 提取经逐字比对确认为纯重构，删除的死分支经独立复核确为不可达。
改动范围严格限于两个声明文件，警告零增量，无凭据入仓风险。
唯一发现项 WARN-1 为 tasks.md 记录格式缺口（验证已由本报告补齐真实输出），不阻塞。

建议下一步：`openspec-verify-change` → `verification-before-completion`；
可选先把「验证记录」的输出回填 tasks.md 再归档。
