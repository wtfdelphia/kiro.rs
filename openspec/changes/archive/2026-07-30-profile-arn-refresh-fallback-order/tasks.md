## 1. 实现前核对

- [x] 1.1 读 `AGENTS.md` 与本 change 的 proposal / design / specs
  → 验证：能陈述本次改动的高风险类型（Token/多凭据）与验证命令（`cargo test`）
- [x] 1.2 核对 `looks_like_idc`（`src/kiro/profile.rs:54`）与 `refresh_token` 的
  分流条件（`src/kiro/token_manager.rs:128-143`）是否等价
  → **已执行，结论：不等价。** `provider: "BuilderId"` + `authMethod: "social"`
  会命中 `provider==BuilderId` 但刷新实际走 Social 端点。故判定谓词改为提取
  `refresh_routes_to_idc`（见 2.0），不使用 `looks_like_idc || provider==BuilderId`
- [x] 1.3 确认 `profile.rs:207-215` 分支对 IdC 确为不可达（非仅低频）
  → **已执行**：命中该分支需 clientId+clientSecret，此类凭据必须持 refreshToken
  才能通过 `validate_refresh_token`（`token_manager.rs:72`），故必然先在
  `profile.rs:186` return

## 2. 抽出决策边界

- [x] 2.0 在 `src/kiro/token_manager.rs` 提取
  `pub(crate) fn refresh_routes_to_idc(&KiroCredentials) -> bool`，
  内容为 `refresh_token`（`:128-143`）现有的 `auth_method` 推断 + `{idc, builder-id, iam}`
  判定；`refresh_token` 改为调用它做分流
  → 验证：`cargo test` 通过（分流结果逐位不变，纯重构）；
  `refresh_token` 内不再内联该推断逻辑
- [x] 2.1 在 `src/kiro/profile.rs` 新增私有 `enum ListOutcome`
  （`Resolved(String)` / `Placeholder` / `Empty` / `Failed`）
  → 验证：`cargo build` 通过
- [x] 2.2 新增私有 `enum ResolveAction`
  （`Use(String)` / `Unsupported` / `SoftUnavailable` / `ForceRefresh` / `Fail`）
  → 验证：`cargo build` 通过
- [x] 2.3 新增纯函数 `decide_profile_action(credentials, list) -> ResolveAction`，
  按 design「调整后的决策表」实现，账号类型维度使用 `refresh_routes_to_idc`
  → 验证：函数不含任何 `.await`、不接收 `&MultiTokenManager`；
  未修改 `infer_provider` 与 `looks_like_idc`
- [x] 2.4 确认 `resolve_profile_arn` 与 `ensure_profile_arn_for_request` 公开签名未变
  → 验证：`rg "fn resolve_profile_arn|fn ensure_profile_arn_for_request" src/` 签名与改前一致

## 3. 前移账号类型判定

- [x] 3.1 `resolve_profile_arn` 改为：缓存/Unsupported 前置判定保持原位
  （`profile.rs:143-167`）→ 发 list → 归一为 `ListOutcome` → 调 `decide_profile_action`
  → 按动作执行副作用
  → 验证：`cargo build` 通过；函数内不再有「先强刷后判账号类型」的顺序
- [x] 3.2 删除原 `profile.rs:207-215` 的死分支（其语义已并入 `decide_profile_action`）
  → 验证：文件内无重复的 `looks_like_idc || provider==BuilderId` 判定
- [x] 3.3 保留 `ForceRefresh` 动作下的既有副作用：`force_refresh_token_for` →
  `profile_arn_of` 命中非占位则返回、命中占位则 `clear_profile_arn`、
  刷新失败则 bail 且错误文案同时含 list 与 refresh 原因
  → 验证：对照 `profile.rs:186-204` 改前代码，Social 路径逐条行为一致
- [x] 3.4 确认 `set_profile_arn` 仅在 `Use(arn)` 动作下调用，且仍传 `provider`
  → 验证：`Use` 分支保留 `provider.clone()` 参数
- [x] 3.5 清理本次改动产生的孤儿 import / 未用函数（仅限自己造成的）
  → 验证：`cargo build` 无 unused 警告增量

## 4. 离线测试

- [x] 4.1 新增测试：IdC × `ListOutcome::Failed` → `SoftUnavailable`
  → 验证：`cargo test decide_profile_action` 通过且断言不为 `ForceRefresh`
- [x] 4.2 新增测试：IdC × `ListOutcome::Empty` → `SoftUnavailable`
  → 验证：同上
- [x] 4.3 新增测试：IdC × `ListOutcome::Placeholder` → `SoftUnavailable`
  → 验证：同上
- [x] 4.4 新增测试：Social（有 refreshToken）× 上述三种 → 仍为 `ForceRefresh`
  → 验证：三条断言均通过（Social 无回归）
- [x] 4.4b 新增测试：`provider: "BuilderId"` + `authMethod: "social"` × list 未得 ARN
  → `ForceRefresh`（**不得**为 `SoftUnavailable`）
  → 验证：该断言通过，证明 Bridge Plan 发现的回归已被规避
- [x] 4.4c 新增测试：`authMethod` 缺失 + clientId/clientSecret 齐全 × list 未得 ARN
  → `SoftUnavailable`（刷新实际走 OIDC，须被软放行）
  → 验证：该断言通过，证明缺陷修得干净
- [x] 4.5 新增测试：任意支持型 × `ListOutcome::Resolved(arn)` → `Use(arn)`
  → 验证：返回的 ARN 与输入一致
- [x] 4.6 新增测试：api_key 凭据 → `Unsupported`；非 IdC 且无 refreshToken ×
  list 未得 ARN → `Fail`
  → 验证：两条断言通过
- [x] 4.7 确认新增测试全部无网络访问、无真实凭据
  → 验证：测试体内无 HTTP client 构造、无 `MultiTokenManager`；凭据用
  `KiroCredentials::default()` 填充假值

## 5. 回归与门禁

- [x] 5.1 `profile.rs` 原有 12 个测试保持通过且断言未被修改
  → 验证：`cargo test --bin kiro-rs kiro::profile` 全绿（本 crate 无 lib target，
  不可用 `--lib`）；`git diff` 中原测试块无改动
- [x] 5.2 `cargo test` 全量通过（AGENTS.md 高风险矩阵：Token/多凭据）
  → 已运行：`cargo test --bin kiro-rs` → **570 passed; 0 failed; 0 ignored**
    （`kiro::profile` 19 passed，改前 12 项；`kiro::token_manager` 52 passed）
- [x] 5.3 `cargo build` 无新增警告
  → 已运行：`warning: kiro-rs (bin "kiro-rs") generated 10 warnings` / `Finished dev profile`。
    零增量已用 `git stash push --include-untracked` 取基线对比确认：改动前后同为 10 条，
    均为既有 dead_code / unused_import（基线检查后工作区已完整还原）
- [x] 5.4 `openspec validate --all` 通过
  → 已运行：`Totals: 20 passed, 0 failed (20 items)`
- [x] 5.5 确认 7 处调用点均未改动（5 个主链路 + `provider.rs:298`/`:556`
  两处 bearer-invalid 恢复分支）
  → 验证：`git diff --name-only` 仅含 `src/kiro/profile.rs`、
  `src/kiro/token_manager.rs` 与 openspec 工件；`token_manager.rs` 的 diff
  仅为 `refresh_routes_to_idc` 提取，不含刷新行为改动
- [x] 5.6 `git status --short` 检查，确认无 `config.json` / `credentials.json` /
  `.codegraph/` 等敏感文件混入
  → 已运行：属本 change 的仅 ` M src/kiro/profile.rs`、` M src/kiro/token_manager.rs`
    与 `?? openspec/changes/profile-arn-refresh-fallback-order/`（其余条目属另两个 active change）。
    `git check-ignore -v` 确认 `config.json`（.gitignore:2）/ `credentials.*`（:9）/
    `.codegraph/`（:14）均被忽略，无敏感文件混入
- [x] 5.7 确认无需同步 README / AGENTS / spec，并在最终报告说明原因
  → 验证：本次不改启动、构建、部署、测试入口与 API 端点

## 6. 遗留记录

- [x] 6.1 在最终报告中记录未验证项：真实上游对无 profileArn 的 IdC generate
  是否返回 200，本 change 未做联网复现
  → 验证：报告含该项及其依据（既有 spec 的「BuilderId 无可信缓存」场景）
- [x] 6.2 记录 Social 凭据「每请求重复强刷」问题仍存在，属独立 change 范围
  → 验证：报告含该项，且本 change 的 Non-Goals 已声明
