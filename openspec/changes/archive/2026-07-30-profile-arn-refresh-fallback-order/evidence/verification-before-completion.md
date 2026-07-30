# Verification Before Completion: profile-arn-refresh-fallback-order

日期：2026-07-29
门禁：最终回复 / 归档前验证（`verification-before-completion`）
结论：**通过（可归档）** — 全部关键验证在本会话真实运行，无隐藏失败

> 本次工作区同时存在三个 active change，三者文件归属互不重叠。
> 本报告的验证覆盖整个工作区（测试与构建无法按 change 切分），
> 但范围判定与文档同步只针对本 change 的 `src/kiro/profile.rs` 与 `src/kiro/token_manager.rs`。

## Verification 列表

全部命令在本会话真实执行，输出为实际粘贴。AGENTS.md 高风险矩阵对
「Token / 多凭据」要求 `token_manager` 相关测试与 example 配置完整性：

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `cargo test --bin kiro-rs` | `570 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` | ✅ 通过 |
| `cargo test --bin kiro-rs kiro::profile` | `19 passed; 0 failed` | ✅ 改前 12 项，新增 7 项 `test_decide_*` |
| `cargo test --bin kiro-rs kiro::token_manager` | `52 passed; 0 failed` | ✅ 提取后零回归 |
| `cargo build` | `10 warnings` / `Finished dev profile` | ✅ 通过，警告零增量 |
| `openspec validate --all --strict` | `Totals: 20 passed, 0 failed (20 items)` | ✅ 通过 |
| `openspec status --change <name> --json` | `isComplete: true`，proposal/design/specs/tasks 全 `done` | ✅ 工件完整 |
| `openspec list` | 本 change `✓ Complete`（31/31 tasks） | ✅ 无未勾选任务 |
| `git status --short` | 16 条，全部为预期文件 | ✅ 无敏感文件 |
| `git check-ignore -v` | `config.json`(.gitignore:2) / `credentials.*`(:9) / `.codegraph/`(:14) 均被忽略 | ✅ |

警告零增量的验证方法（非推断）：用 `git stash push --include-untracked` 取 `HEAD` 基线后
`cargo build`，得 10 条；恢复工作区后再 build，同为 10 条。10 条均为既有 dead_code /
unused_import。基线检查后工作区已完整还原（`git stash list` 为空）。

关键正确性核实（本会话独立复核，非采信 tasks 自述）：

```
$ git diff -- src/kiro/token_manager.rs
# 与 HEAD 逐字比对：refresh_routes_to_idc 的 unwrap_or_else 回落
# （clientId+clientSecret → "idc"，否则 "social"）与三个
# eq_ignore_ascii_case("idc"/"builder-id"/"iam") 全部原样搬移
# → 纯重构成立，分流去向逐位不变

$ grep -n -A20 "fn validate_refresh_token" src/kiro/token_manager.rs
# :72-92 要求 refreshToken 非空且长度 ≥100
# → tasks 1.3 的死分支论证成立：删除 HEAD:profile.rs:207-215 非行为回归

$ git diff --name-only | grep -c "provider.rs"
0
# → tasks 5.5 声称的 7 处调用点未改动成立

$ git show HEAD:src/kiro/profile.rs | grep "pub async fn resolve_profile_arn" -A6
# 与当前签名逐字相同 → tasks 2.4 公开签名未变成立

$ git diff -- src/kiro/profile.rs | grep -E "^-" | grep -E "assert|fn test_"
（空 —— 原 12 项测试断言零删改，无「改测试迁就实现」）

$ git diff --stat -- Cargo.toml Cargo.lock
（空 —— 未引入依赖）
```

`decide_profile_action`（`profile.rs:158-177`）确认为纯函数：无 `.await`、
不接收 `&MultiTokenManager`。新增 7 项测试无 HTTP client 构造、无网络访问，
凭据用 `KiroCredentials::default()` 填假值。

## SKIPPED（未运行的验证）

| 项 | SKIPPED 原因 | 剩余风险 |
| --- | --- | --- |
| 真实上游对无 profileArn 的 IdC `generate` 是否返回 200 | 需联网并持有真实 IdC 凭据；本会话未做联网复现，且不应在验证中使用真实凭据 | **中，本 change 最主要的未验证面。** 依据为既有 spec 已固化的「BuilderId 无可信缓存」软放行场景——该路径改动前即在生产运行，本 change 只是扩大了进入该路径的凭据形态（从 `looks_like_idc \|\| provider==BuilderId` 扩到 `refresh_routes_to_idc`）。若上游实际拒绝无 ARN 的 IdC 请求，表现为该类凭据请求失败而非静默错误 |
| `example` 配置完整性（高风险矩阵项） | 本 change 未新增或修改任何配置项，`config.example.json` 无需变更 | 低 |
| Social 凭据「每请求重复强刷」的修复验证 | 该问题本 change 明确不修（proposal Non-Goals），需负缓存/冷却机制 | 低。属独立 change 范围，行为与改动前一致 |
| 本 change 单独的回归基线 | 三个 change 的改动同时在工作区，无法按 change 切分测试 | 低。`kiro::profile` 19 / `kiro::token_manager` 52 已定位到本 change 影响面；`anthropic::` 103 与 `openai::*` 零失败佐证无跨模块污染 |
| 多凭据并发下的锁竞争实测 | `refresh_lock` 粒度未改（Non-Goals）；锁竞争随强刷调用量下降自然缓解，非本 change 引入 | 低 |

## Documentation Sync 表

| 文档 | 是否需要同步 | 理由 |
| --- | --- | --- |
| `README.md` | **否** | 不改启动、构建、部署、测试入口与 API 端点。`:396` 描述的解析顺序「固定表 / ListAvailableProfiles / refresh fallback」仍成立——顺序未变，仅 IdC 分支不再落入 refresh 阶段，属边界收窄而非顺序调整 |
| `AGENTS.md` | **否** | 未改变 AI 协作纪律、OpenSpec 条件、高风险矩阵或验证命令 |
| `CLAUDE.md` | **否** | 规则入口未变 |
| `spec/requirements.md` | **否** | 无 profileArn 层面的长期事实条目 |
| `spec/design.md` / `spec/structure.md` | **否** | 未改变模块划分；`decide_profile_action` 与 `ListOutcome`/`ResolveAction` 均为 `profile.rs` 内私有 |
| `openspec/specs/profile-arn-resolution/spec.md` | **待归档时同步** | 本 change 的 spec 含 2 个 MODIFIED Requirement（10 Scenario），归档时由 `openspec-archive-change` / `openspec-sync-specs` 合并。**建议明确同步「IdC 不为取 ARN 而强刷」这条决策**，它是本 change 的核心行为变更 |
| `openspec/specs/credential-import/spec.md` | **否** | 未改变凭据导入或字段语义 |
| `docs/tooling-sources.md` | **否** | 未引入新工具或依赖 |
| `config.example.json` | **否** | 无新增配置项 |

## 安全核查

- `git status --short` 16 条全部为预期文件；`--untracked-files=all` 全量展开后无意外条目。
- `config.json` / `credentials.*` / `.codegraph/` 经 `git check-ignore -v` 确认被忽略。
- **错误文案不泄露凭据**：`profile.rs:266`/`:268`/`:275`/`:279` 的 `bail!` 只插入 list 与
  refresh 的失败原因描述，不含 token / refreshToken / clientSecret 字段值。
  `validate_refresh_token` 的截断提示只打印长度（`refreshToken 已被截断（长度: {} 字符）`）。
- `profile.rs:22`/`:25` 的两个 ARN 常量（`SOCIAL_SIGN_IN_PROFILE_ARN` /
  `BUILDER_ID_PROFILE_ARN`）经 `git show HEAD` 确认为**改动前既有**的 AWS 侧公开默认 profile，
  非本次引入、非用户凭据。
- 新增测试无真实凭据、无网络访问。
- 无新增 `#[allow(...)]`、无 `dbg!` / `eprintln!` / `println!` / `TODO` / `FIXME` 残留。

## Residual Risk

| 风险 | 说明 |
| --- | --- |
| **未 archive** | 本 change 尚未执行 `openspec-archive-change`；2 个 MODIFIED Requirement 未合并进长期 spec |
| **未 commit / push / PR** | 改动仍在工作区未提交状态（当前分支 `dev`，主分支 `master`）。本会话未执行任何 git 提交、推送或 PR 操作 |
| **三个 change 共存于同一工作区** | 提交时需按 change 分离 staging（本 change 对应 `src/kiro/profile.rs` + `src/kiro/token_manager.rs`），否则三个变更会混入同一 commit |
| **无真实上游 E2E** | IdC 无 ARN 的 generate 行为未联网复现（见 SKIPPED 第 1 项），是本 change 最主要的未验证面 |
| **Social 重复强刷未修** | proposal Non-Goals 已声明，需负缓存/冷却，属独立 change |
| **`refresh_lock` 粒度未改** | 锁竞争随调用量下降自然缓解，未做并发实测 |
| **工具限制** | Rust 侧无 coverage 工具；决策表覆盖度靠 Scenario 对照与 7 项组合测试人工判断，非工具度量 |

## 结论

**通过，可归档。** 本会话真实运行的 9 类验证全部通过：`cargo test` 570 passed / 0 failed
（`kiro::profile` 19、`kiro::token_manager` 52）、`cargo build` 警告零增量（基线对比法验证）、
`openspec validate` 20/20、工件 `isComplete: true`、tasks 31/31、`git status` 无敏感文件。

高风险矩阵要求的 Token/多凭据测试已实跑。两个最需要留痕的正确性判断已独立复核而非采信自述：
`refresh_routes_to_idc` 提取与 `HEAD` 逐字比对确认为纯重构；删除的死分支经
`validate_refresh_token` 实现（要求 refreshToken 长度 ≥100）验证确为不可达。

5 项 SKIPPED 已逐条写明原因与剩余风险。其中「真实上游对无 profileArn 的 IdC generate」
是本 change 最主要的未验证面，已如实标为中等风险，不写成通过。

不存在被隐藏的失败。本轮已回填 tasks 5.2/5.3/5.4/5.6 的真实输出（原为「→ 验证：粘贴输出」
的格式缺口，命令本身均已通过，非虚报）。

下一步：`openspec-archive-change`。归档时建议明确把「IdC 不为取 ARN 而强刷」同步进长期
`openspec/specs/profile-arn-resolution/spec.md`。提交时注意按 change 分离 staging。
