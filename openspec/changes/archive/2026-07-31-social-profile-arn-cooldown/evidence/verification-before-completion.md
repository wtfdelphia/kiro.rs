# Verification Before Completion — social-profile-arn-cooldown

> 产出时间：2026-07-31
> 分支：`dev`（主分支 `master`），基线 `857c76f`
> 范围：实现阶段完成核验（**未** commit、**未** push、**未** archive）

## Verification（仅本会话真实运行）

| # | 命令 | 结果 | 结论 |
| --- | --- | --- | --- |
| 1 | `cargo build` | Finished，0 error | 新增字段、方法、guard、注入形态均编译通过 |
| 2 | `cargo test kiro::profile` | **38 passed / 0 failed** | 含 `decide_profile_action` 既有 10 个测试（未修改）与 12 个新增解析调度测试 |
| 3 | `cargo test kiro::token_manager` | **78 passed / 0 failed** | 含冷却状态机 5 项、版本号 3 项、存取与 guard 8 项 |
| 4 | `cargo test` | **724 passed / 0 failed** | 全仓库全绿，无新增阻塞性 warning |
| 5 | `openspec validate --all` | **20 passed / 0 failed** | 含 `change/social-profile-arn-cooldown` |
| 6 | `git status --short` | 2 个 `M` + 2 个 `??` | 无机密、无本地缓存进入候选（详见下） |
| 7 | `git diff --name-only` | `src/kiro/profile.rs`、`src/kiro/token_manager.rs` | 与 proposal Scope 完全一致；七个调用点零改动 |
| 8 | `rg -c 'tracing::' src/kiro/profile.rs` | **4**（原为 0） | 四条日志到位，无 token 值 |
| 9 | `rg -c 'entry\.credentials = '` + 后 2 行含 `bump_credentials_version` | **6 / 6** | 全部赋值点已递增版本号 |

### git status 明细（无敏感文件）

```
 M src/kiro/profile.rs
 M src/kiro/token_manager.rs
?? docs/social-profile-arn-force-refresh-storm.md
?? openspec/changes/social-profile-arn-cooldown/
```

`git status --porcelain | rg -i 'credentials\.json|config\.json|\.codegraph|\.env'`
→ **无匹配**。`config.json`、`credentials.*`、`.codegraph/` 均被 `.gitignore` 覆盖
（Bridge Plan §9 已用 `git check-ignore -v` 确认）。

## 端到端实验（tasks 9.5，已补做）

用户在 `127.0.0.1:18990` 运行含本改动的 `kiro-rs.exe`（16:53 构建）后补做。
完整记录见 `evidence/e2e-experiment.md`。

| 项 | 结果 |
| --- | --- |
| 8 个成功操作（3 余额 + 2 模型 + 3 对话）的强刷次数 | **0**（原基线 8 次，1:1） |
| 对话延迟 | **2.03–3.56 s**（原基线 4.36–5.02 s） |
| **反证**：Admin 手动强刷使冷却失效后 | 下一次余额查询**重新出现强刷**（2794 ms），随后两次立即恢复抑制（1549 → 757 ms） |
| IdC `#1` 三次余额查询 | 强刷 **0** 次，无回归 |
| `hasProfileArn` 终态 | 三条凭据均 false（Social 强刷仍 0 次拿到 ARN） |

计数器用 Admin 快照的 `(expiresAt, refreshTokenHash)` 指纹而非日志字符串
（该实例日志只走 stdout、未落盘）。等价且更精确，且不复制真实凭据、
不打印 hash 全文（只取前 12 位）。

反证实验是关键：单看「0 次强刷」无法区分「冷却生效」与「解析压根没走到该凭据」，
步骤 2 重新出现强刷才把归因闭合。

### 仍缺的子项

| 子项 | 原因 | 剩余风险 |
| --- | --- | --- |
| `ListAvailableProfiles` 的上游状态码与响应体 | 需抓包或 debug 日志，本次未捕获 | 决定 `ListOutcome` 落 `Failed` 还是 `Empty`。对本 change 结论无影响（三种 miss 处理相同，且冷却在 list 之前拦截），但影响问题普适性判断 |
| list 与 refresh 分段计时 | 需进程内埋点 | 「延迟下降来自省掉两段往返」是量级相符的推断，不是分段实测 |
| 15 分钟窗口到期后的真实行为 | 需等待 15 分钟 | 单测已覆盖（`test_cooldown_expiry_allows_new_attempt`，回拨 16 分钟） |
| 并发去重的真实实例验证 | 需并发压测 | 单测已覆盖（`test_concurrent_resolve_deduplicates`） |

## Documentation Sync

| 入口 | 需同步 | 理由 |
| --- | --- | --- |
| `README.md` | 否 | 不改启动、构建、部署、测试、API 入口；无新增配置项或环境变量 |
| `AGENTS.md` | 否 | 不改 AI 纪律、高风险矩阵、验证命令 |
| `CLAUDE.md` | 否 | 规则入口未变 |
| `spec/design.md` / `requirements.md` / `structure.md` | 否 | 模块边界与数据流不变；冷却是 `src/kiro/` 内部实现细节，无新增模块或文件 |
| `openspec/specs/profile-arn-resolution/spec.md` | **归档时** | delta 已在 `changes/.../specs/`；按 OpenSpec 流程归档才合入主规格。**注意本 change 含 MODIFIED**：把 `Social 强刷行为不得回归` 的无条件 MUST 改为带冷却限定的 `Social 首次强刷行为不得回归` |
| `docs/tooling-sources.md` | 否 | 未引入新工具或新依赖 |
| `credentials.example.*.json` | 否 | 冷却状态不落盘 |
| `admin-ui/` | 否 | `CredentialEntrySnapshot` 未新增字段（已由 `test_cooldown_state_not_persisted` 断言） |

## Residual Risk

1. **端到端实验有 4 个子项未做**（见上表）。主指标（强刷次数、延迟、因果归因、
   IdC 无回归）均已实测，缺的是分段计时与两个已有单测覆盖的场景。
2. **6 处版本号赋值点为源码级断言，非运行时断言**。5 处紧随 `refresh_token` 的网络
   调用、第 6 处（upsert）也要求 OAuth 凭据先经网络刷新，离线单测无法逐一触发。
   `test_every_credentials_assignment_bumps_version` 扫描自身源码要求恰好 6 处且
   每处 2 行内必有递增——漏改即测试失败，但不覆盖运行时行为。
   漏改后果是「冷却比预期多持续一个窗口」，非数据错误（proposal Risks 表已判可接受）。
3. **未 commit、未 push、未创建 PR、未 archive**。当前改动仅存在于工作区。
4. **两个窗口时长（15 分钟 / 30 秒）未经实测校准**。设计上是编译期常量，可实测后调整；
   应急旋钮是把常量设为 `Duration::ZERO`（等价于关闭冷却）。
5. **external_idp 只是被顺带缓解，未根治**。其正解（重定义 `refresh_routes_to_idc`
   的语义）留作后续 change，`test_external_without_arn_currently_force_refreshes`
   仍锁定现状。
6. **`force_refresh_token_for` 取锁前克隆凭据的既有缺陷未修**，仅加注释留痕。
   本 change 的并发去重使其在 profileArn 路径上不再被触发，但 Admin 批量强刷等
   路径仍存在。应作为单独 change。

## 结论

自动化验证**全部真实运行且全绿**（724 项测试、20 项 spec 校验），
端到端实验在含本改动的构建上实测通过并以反证闭合因果归因。
工作区无机密泄漏风险。

**43/43 任务完成，可归档。** 归档时需把 `changes/.../specs/profile-arn-resolution/spec.md`
的 delta 合入主规格（含 MODIFIED 场景的措辞变更）。
剩余风险见上，均为非阻塞项。
