# OpenSpec Verify Report: profile-arn-refresh-fallback-order

日期：2026-07-29
审查类型：归档前工件与证据核验（`openspec-verify-change`）
总体状态：**PASS**

> 本次工作区同时存在三个 active change，三者文件归属互不重叠。本报告只核验本 change
> 的工件、tasks 与 evidence；`src/openai/**` 与 `admin-ui/**` 归属另两个 change。

## 三维结论

| 维度 | 状态 | 结论 |
| --- | --- | --- |
| Completeness | **PASS** | `openspec status --json` 四类工件全部 `status: "done"`，`isComplete: true`。tasks **31/31 勾选，0 未勾**。evidence：`bridge-plan.md`（327 行）、`spec-compliance-report.md`。2 个 Requirement 各有 Scenario（共 10 个），无空 Requirement。 |
| Correctness | **PASS** | design 决策表 8 格与 `decide_profile_action` 逐格一致；10 个 Scenario 全部有实现或离线测试对应；`refresh_routes_to_idc` 提取经逐字比对确认为纯重构；删除的死分支经独立复核确为不可达。 |
| Coherence | **PASS** | proposal Non-Goals 与 tasks 6.2 的遗留记录作用域不冲突（前者 Social，后者亦 Social，与 spec Scenario 限定的 IdC 不重叠）。README `:396` 描述仍成立。本轮已回填 tasks 验证输出（见修正 1）。 |

## 工件完整性（openspec status）

```
$ openspec status --change profile-arn-refresh-fallback-order --json
"isComplete": true
artifacts: proposal(done) / design(done) / specs(done) / tasks(done)
specs 实存：specs/profile-arn-resolution/spec.md
nextSteps: All planning artifacts are complete
```

```
$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)
  ✓ change/profile-arn-refresh-fallback-order
  ✓ spec/profile-arn-resolution
```

## tasks 勾选项的证据支撑

| 任务组 | 声称 | 本会话实跑核验 |
| --- | --- | --- |
| 1.2（谓词等价性核对） | 「已执行，结论：不等价」——`BuilderId` + `authMethod:social` 会误判 | ✅ 独立复核成立；这是改用 `refresh_routes_to_idc` 而非 `looks_like_idc \|\| provider==BuilderId` 的正当理由 |
| 1.3（死分支论证） | 命中需 clientId+clientSecret，此类凭据必持 refreshToken 才能过 `validate_refresh_token` | ✅ 读 `token_manager.rs:72-92` 复核：要求非空且长度 ≥100，论证成立，删除非行为回归 |
| 2.0（提取 `refresh_routes_to_idc`） | 纯重构，分流逐位不变 | ✅ 与 `git show HEAD` 逐字比对：`unwrap_or_else` 回落、三个 `eq_ignore_ascii_case` 全部原样搬移 |
| 2.3（纯函数） | 无 `.await`、不接收 `&MultiTokenManager` | ✅ `profile.rs:158-177` 核实 |
| 2.4（公开签名未变） | 两个入口签名与改前一致 | ✅ 与 `git show HEAD` 逐字对比相同 |
| 3.3（ForceRefresh 副作用保留） | 与改前 `:186-204` 逐条一致 | ✅ 对照 `HEAD:profile.rs:186-204` 逐行核实 |
| 4.1-4.6（新增测试） | 7 项 `test_decide_*` | ✅ `--list` 逐一确认存在；`profile.rs` 测试数 12 → 19 |
| 4.7（无网络无凭据） | 测试体无 HTTP client、无 `MultiTokenManager` | ✅ grep 零命中；凭据用 `KiroCredentials::default()` |
| 5.1（原 12 测试未改断言） | `git diff` 中原测试块无改动 | ✅ diff 中测试块无 `-` 行命中 `assert` / `fn test_` |
| 5.2 / 5.3 / 5.4 / 5.6 | 原写「→ 验证：粘贴输出」未回填 | ⚠ 修正 1（已回填真实输出） |
| 5.5（7 处调用点未改） | 仅两文件改动，`provider.rs` 未动 | ✅ `git diff --name-only \| grep -c provider.rs` = 0 |

两条回归锁定测试经确认存在并通过：
`test_decide_builder_provider_with_social_auth_force_refreshes`（Bridge Plan 发现的回归已规避）、
`test_decide_missing_auth_method_with_client_creds_is_soft`（缺陷修得干净）。

## Requirement / Scenario 完整性

| Requirement | Scenario 数 | 对应 |
| --- | --- | --- |
| 请求前必须解析 profileArn（MODIFIED） | 8 | 全覆盖 |
| profileArn 解析决策必须可在离线条件下验证（MODIFIED） | 2 | 全覆盖 |

无空 Requirement。design 决策表 8 格与实现的逐格对照见 `spec-compliance-report.md`。

**曾疑似冲突已排除**：spec `:37` 的 Scenario「每请求不得重复无效强刷」限定 **IdC** 凭据，
而 tasks 6.2 与 proposal Non-Goals 记的「Social 凭据每请求重复强刷仍存在」限定 **Social**。
两者作用域不重叠，无矛盾——这一点在核验中特意查过，因为它是最容易被读成自相矛盾的地方。

## proposal / design 与实际改动的一致性

Non-Goals 8 条全部核实成立：

| Non-Goal | 核实 |
| --- | --- |
| 不改 Social 强刷行为 | `test_decide_social_still_force_refreshes` 三态均为 `ForceRefresh` |
| 不引入负缓存或冷却计时器 | 无新增状态字段，`decide_profile_action` 为无状态纯函数 |
| 不改 `force_refresh_token_for` 自身语义 | 该函数未出现在 diff 中 |
| 不改 `refresh_token` 分流结果 | 提取经逐字比对为纯重构 |
| 不改 `refresh_lock` 粒度 | 未出现在 diff |
| 不改 `persist_credentials` 落盘策略 | 未出现在 diff |
| 不改另外四个调用点 | `git diff --name-only` 仅两文件 |
| 不改 `provider.rs:543` bearer-invalid 分支 | `provider.rs` 零改动 |

## evidence 核验

| 证据 | 状态 | 核验 |
| --- | --- | --- |
| `bridge-plan.md` | ✅ 327 行 | Skills 门禁「开始实现前」产物；其发现的 `BuilderId+social` 回归风险已由锁定测试规避 |
| `spec-compliance-report.md` | ✅ | 本轮上游门禁，状态 PASS |
| 敏感数据 | ✅ 零命中 | 错误文案（`:266`/`:268`/`:275`/`:279`）只插入失败原因描述，不含 token / refreshToken / clientSecret 值；`validate_refresh_token` 截断提示只打印长度 |

`profile.rs:22`/`:25` 的两个 ARN 常量经 `git show HEAD` 确认为改动前既有的 AWS 侧公开默认
profile，非本次引入、非用户凭据。

## 本轮修正（消除工件间不一致）

### 修正 1：tasks 5.2 / 5.3 / 5.4 / 5.6 回填真实验证输出

原文四条均写「→ 验证：粘贴输出」但未回填结果，与同批 change
（`admin-ui-balance-refresh-fixes` 的「验证状态」章节）的记录粒度不一致。
命令本身经本会话独立复跑全部通过，**不存在虚报**，属记录格式缺口。已回填：

- 5.2 → `cargo test --bin kiro-rs` 570 passed（`kiro::profile` 19 / `kiro::token_manager` 52）
- 5.3 → 10 warnings，并注明零增量的 `git stash` 基线对比方法
- 5.4 → `Totals: 20 passed, 0 failed`
- 5.6 → 属本 change 的三个条目 + `git check-ignore -v` 的三项忽略确认

## 失败项

无。

## 剩余风险（可接受，与 compliance 报告一致）

1. **真实上游对无 profileArn 的 IdC generate 是否返回 200** 未做联网复现。
   依据为既有 spec 已固化的「BuilderId 无可信缓存」软放行场景——该路径改动前即在生产运行，
   本 change 只是扩大了进入该路径的凭据形态。这是本 change 最主要的未验证面。
2. **Social 凭据「每请求重复强刷」仍存在**（proposal Non-Goals 已声明），需负缓存/冷却，属独立 change。
3. **`refresh_lock` 粒度未改**，锁竞争随调用量下降自然缓解。

## 验证记录（本会话真实运行）

```
$ openspec status --change profile-arn-refresh-fallback-order --json
"isComplete": true；proposal/design/specs/tasks 全部 done

$ openspec validate --all --strict
Totals: 20 passed, 0 failed (20 items)

$ cargo test --bin kiro-rs
test result: ok. 570 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test --bin kiro-rs kiro::profile
test result: ok. 19 passed; 0 failed        # 改前 12，新增 7 项 test_decide_*

$ cargo test --bin kiro-rs kiro::token_manager
test result: ok. 52 passed; 0 failed        # 提取后零回归

$ cargo build
warning: `kiro-rs` (bin "kiro-rs") generated 10 warnings   # 基线对比零增量

$ grep -c "^- \[x\]" tasks.md → 31 ；grep -c "^- \[ \]" tasks.md → 0
```

## 证据路径

- Bridge：`openspec/changes/profile-arn-refresh-fallback-order/evidence/bridge-plan.md`
- Compliance：`openspec/changes/profile-arn-refresh-fallback-order/evidence/spec-compliance-report.md`
- 本报告：`openspec/changes/profile-arn-refresh-fallback-order/evidence/openspec-verify-report.md`
- Completion：待 `verification-before-completion` 产出
- 历史基线：`openspec/changes/archive/2026-07-21-profile-arn-resolution/`

## 结论

**PASS，可归档。** 四类工件完整（`isComplete: true`），tasks 31/31 且每个勾选项都有
实跑结果或 diff 支撑，10 个 Scenario 全覆盖，validate 20/20。
本轮回填了 4 条验证输出（原为格式缺口，非虚报）。
最需要留痕的两点已独立复核：`refresh_routes_to_idc` 提取逐字等价（纯重构成立），
删除的死分支经 `validate_refresh_token` 实现验证确为不可达（非行为回归）。
无失败项，无凭据入仓风险，警告零增量。

建议下一步：`verification-before-completion`，随后 `openspec-archive-change`。
归档时可考虑把「IdC 不为取 ARN 而强刷」这一决策同步进长期 `openspec/specs/profile-arn-resolution/spec.md`
（`openspec-sync-specs` 会处理）。
