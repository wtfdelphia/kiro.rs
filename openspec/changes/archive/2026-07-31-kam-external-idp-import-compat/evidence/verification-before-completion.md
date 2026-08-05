# Verification Before Completion: kam-external-idp-import-compat

> 日期：2026-07-30
> 原则：只记录本会话**实际运行过**的命令与结果。未运行项写明原因与剩余风险。

## 1. 实际运行的命令与结果

全部命令在本会话内于 `D:\MyProgram\wtf\wkspace\github\kiro.rs` 执行，
平台 Windows 11，shell 为 Git Bash。

### 后端测试

| 命令 | 结果 |
| --- | --- |
| `cargo test kiro::model::credentials` | ok. **68 passed**; 0 failed |
| `cargo test kiro::external_idp` | ok. **26 passed**; 0 failed |
| `cargo test kiro::kam_adapter` | ok. **34 passed**; 0 failed |
| `cargo test kiro::token_manager` | ok. **61 passed**; 0 failed |
| `cargo test kiro::profile` | ok. **24 passed**; 0 failed |
| `cargo test admin` | ok. **40 passed**; 0 failed |
| `cargo test common::atomic_file` | ok. **6 passed**; 0 failed |
| `cargo test`（全量） | ok. **693 passed**; 0 failed; 0 ignored |
| `cargo build` | Finished，**0 error**（13 warning，均为改动前已存在的 dead_code / unused_import） |
| `cargo tree -i url` | `url v2.5.7` 下列出 `kiro-rs`，确认为直接依赖 |

改动前基线为 652 passed（首次全量运行）。净增 41 个测试。

### 前端

| 命令 | 结果 |
| --- | --- |
| `pnpm --dir admin-ui install --frozen-lockfile` | Already up to date（lockfile 已同步，CI 不会失败） |
| `pnpm --dir admin-ui test` | **11 passed**（1 test file） |
| `pnpm --dir admin-ui exec tsc -b` | 无错误 |
| `pnpm --dir admin-ui build` | ✓ built in 2.44s，1777 modules transformed |

### 规格与卫生

| 命令 | 结果 |
| --- | --- |
| `openspec validate --all` | **19 passed, 0 failed** |
| `openspec validate kam-external-idp-import-compat --strict` | valid |
| `openspec show kam-external-idp-import-compat --json` | `deltaCount: 23` |
| `git status --short` | 28 条（21 modified + 7 untracked），无凭据/缓存 |
| `git check-ignore -v credentials.example.external.json` | 命中 `.gitignore:11:!/credentials.example.*.json`（未被忽略） |
| `git check-ignore -v credentials.json` | 命中 `.gitignore:9:/credentials.*`（仍被忽略） |

### 关键平台行为验证

`write_atomic_replaces_existing`（`src/common/atomic_file.rs`）在 **Windows 上实际
运行通过**。这条不是靠假设：design.md 明确要求验证 `std::fs::rename` 对同目录
已存在文件的覆盖行为，tasks 9.1 写了「不得假设」。

## 2. 未运行的验证与剩余风险

### 2.1 未做真实账号在线验活

**未运行任何真实 Kiro / Microsoft 账号的端到端验证。**

原因：AGENTS.md 验证纪律禁止使用真实凭据；本会话也不具备可用的测试租户。

已覆盖的替代验证：

- external 刷新的 **form 构造**由纯函数测试断言（公共客户端不含 `client_secret` 键、
  `scope` 空时不含 `scope` 键、三个必填字段恒存在）
- endpoint 校验在**出站前**拦下的两条路径有集成测试
  （`test_external_without_endpoint_fails_before_network`、
  `test_external_rejects_non_whitelisted_endpoint_before_network`）
- 响应解析类型 `ExternalRefreshResponse` 同时支持 `expires_in` 与 `expires_at`

**剩余风险**：若 Microsoft token 端点实际返回的字段名与
`ExternalRefreshResponse` 不匹配，刷新会在 `response.json()` 阶段失败。
影响可控——错误可见、不静默、不影响其他凭据，且该凭据会按既有失败计数逻辑处理。
请求形状依据 KAM 已验证实现（`kam/auth/providers/external_idp.rs:139-153`）推导，
但本项目独立实现、独立测试，未与上游对接确认。

**建议的首次真实验证步骤**（交给具备测试租户的操作者）：
用一个 Entra ID 测试账号走 Admin KAM 导入，观察是否返回 `created`；
若失败，错误信息会指出是 endpoint 被拒、`invalid_grant`、还是反序列化失败。

### 2.2 未在真实凭据文件上验证迁移写回

集成测试用临时目录覆盖了备份存在、原子替换、失败保留三条路径
（`test_migrate_backs_up_and_writes_native_format`、
`test_migrate_failure_preserves_original_file`、
`write_atomic_preserves_original_on_failure`），但未在含真实凭据的
`credentials.json` 上运行。

**剩余风险**：低。迁移前必备份，失败不覆盖原文件，且失败不阻止启动。
最坏情况是迁移未生效、文件保持 KAM 格式，下次启动重试。

### 2.3 未运行 `cargo build --release`

只运行了 `cargo build`（debug）。release 构建需先 `pnpm build` 产出
`admin-ui/dist`（`rust-embed` 嵌入静态资源），本会话已运行 `pnpm build`，
但未跑完整 release 编译。

**剩余风险**：低。release 与 debug 的差异是优化与 LTO，不改变类型检查结果；
`cargo build` 已通过意味着编译期无问题。

### 2.4 未运行 admin-ui 的组件级渲染测试

vitest 覆盖的是从 `kam-import-dialog.tsx` 导出的三个纯函数
（`parseJsonDocument`、`describePreviewItem`、`describeContainer`）。
React 组件的渲染与交互未测——本会话新引入 vitest，未装 `@testing-library/react`
与 jsdom 环境。

**剩余风险**：中。UI 的逐条渲染逻辑（`handleImport` 中把服务端结果映射为
`VerificationResult`）没有自动化覆盖，只有 `tsc -b` 的类型保证。
建议后续 change 补组件测试；本次不扩范围。

### 2.5 未测试 Docker 构建

`Dockerfile` / `docker-compose.yml` 未改动，故未运行容器构建。CI workflow
（`build.yaml`、`build-dev-release.yaml`）也未改动，但新增的 vitest 依赖已通过
`--frozen-lockfile` 本地验证，CI 的 install 步骤不会失败。

## 3. 未隐藏的失败与偏离

本会话过程中出现过 3 次测试失败，均已修复且记录在此：

| 失败 | 原因 | 处理 |
| --- | --- | --- |
| `test_external_without_endpoint_fails_before_network` 等 2 个 | fixture 的 `refreshToken` 用了 `"rt"`（2 字符），被 `validate_refresh_token` 的截断检查先拦下 | 改 fixture 为 150 字符占位值。**非产品缺陷** |
| `rejects_social_without_refresh_token` | `has_value` 只排除 `null`，`"  "` 算存在，与 `str_field`（trim 后过滤）语义不一致 | 修 `has_value` 也排除空白字符串。**产品缺陷，已修** |
| `test_external_credential_uses_cached_real_arn` | 我的测试假设错误——`decide_profile_action` 不检查缓存，缓存命中由上层 `resolve_profile_arn` 的 `trusted_profile_arn` 处理 | 改测试断言 list 返回可信 ARN 的情形，并注明职责边界。**测试假设错误，非产品缺陷** |

另有 1 次实现期返工：占位文案的 `\n` 转义在 bash heredoc 中被反复破坏，
第 4 次改用独立 Python 脚本文件写入才成功；脚本已删除。

## 4. 边界与纪律确认

- **KAM 仓库只读**：`git -C ../kiro-account-manager status` 的 16 个改动文件
  mtime 为 11:33，早于本会话实现阶段（19:0x 起），确认本次未修改
- **既有测试未被迁就修改**：唯一改动是 4 处全字段初始化追加
  `..Default::default()`（新增字段导致 `E0063` 编译错误，不改任何断言）
- **两个 region 哨兵测试保持通过**：`test_api_call_uses_effective_api_region`、
  `test_api_call_uses_credential_api_region`——bridge-plan 8.1 的核心结论
- **无新增运行时配置项**：`config.example.json` 与 `src/model/config.rs` 无改动
- **Social / IdC 刷新函数体无删改**：`git diff` 验证
- **`profile.rs` 只有 `#[cfg(test)]` 区域的新增**：`git diff` 无删除行
- **无真实凭据入库**：`git status` 无 `config.json` / `credentials.json` /
  `.codegraph/`；新增文件的 `ksk_` 与 JWT 模式检索为空

## 5. 结论

可以交付。83/83 tasks 完成，693 个后端测试 + 11 个前端测试全绿，
`openspec validate --all` 19/19 通过，Non-Goals 边界全部守住，
bridge-plan 的 10 条实现期停止条件均未触发。

**必须向使用者声明的一点：本变更未经真实 Microsoft Entra ID / Azure AD 账号
在线验活。** external_idp 的刷新链路在离线条件下逐层验证（endpoint 校验、
form 构造、分派选择、字段持久化），但首次真实刷新的成败取决于 Microsoft
响应形状是否与实现假设一致。建议由具备测试租户者做一次真实导入验证。
