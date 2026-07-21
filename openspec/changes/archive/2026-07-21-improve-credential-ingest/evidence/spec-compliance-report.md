# Spec Compliance Report: improve-credential-ingest

Date: 2026-07-21
Change: improve-credential-ingest
Skill: spec-compliance-check
Reviewer: Codex (fresh compliance pass)

## Overall Status: **PASS**

结论：实现覆盖 proposal/design/tasks 与三份 delta specs 的 MUST 场景；未触碰 non-goals；相关模块验证与 openspec validate --all 通过。存在若干非阻塞 WARN（实网 E2E、未鉴权 HTTP 单测、全量 suite 中无关 converter 失败）。

## Six Dimensions

| Dimension | Status | Notes |
| --- | --- | --- |
| Scope | PASS | 改动限于 credentials 模型、token_manager ingest、user_info、online_auth、admin API/UI、example/README、OpenSpec/docs。未改 anthropic/SSE、LB 选号、Cargo.toml。usage_limits.rs 仅增 UserInfo 反序列化字段以支撑 GetUserInfo，属本 change 范围。 |
| Design | PASS | 统一 ingest_credential；裸 POST 默认 reject、import/batch 默认 upsert；GetUserInfo best-effort；在线授权完成后 ingest_online_tokens；会话 TTL 15min；batch concurrency 默认 1。与 design D1-D7 一致。 |
| Scenarios | PASS | 见下方 Scenario 映射表。MUST 均有实现；少数 SHOULD/安全场景以代码路径+中间件保证，无独立 HTTP 401 用例（WARN）。 |
| Project Rules | PASS | OpenSpec 工件齐全；无真实密钥/credentials.json/.codegraph；示例仅 example；验证只记真实命令；surgical 范围符合 AGENTS。 |
| Verification | PASS | 相关：online_auth 6、token_manager 43、user_info 1、admin:: 3、pnpm build、openspec validate --all 均 PASS。全量 cargo test 8 个 converter 失败明示 WARN 且 out-of-scope。 |
| README/AGENTS Sync | PASS | README 字段说明已更新；AGENTS 纪律未变无需改；主 specs 同步延后 archive（openspec-sync-specs）。 |

## Scenario Coverage Matrix

### credential-ingest

| Scenario | Evidence |
| --- | --- |
| OAuth refresh 失败不落盘 | MultiTokenManager::ingest_credential refresh 失败 early return；token_manager 测试覆盖 |
| API Key 跳过 refresh | ingest 路径按 authMethod/kiroApiKey 分支 |
| 旧文件缺字段可加载 | credentials 字段 Option + serde default |
| 列表暴露身份字段 | status item 含 user_id/nickname/start_url |
| GetUserInfo 成功写回 / 失败不阻断 | user_info::get_user_info + ingest best-effort |
| 默认 reject 重复 refreshToken hash | OnConflict::Reject 默认；hash 去重 |
| userId upsert 更新并复活 | ingest match userId + clear disabled |
| 无 userId 禁止 silent upsert | upsert 仅在有 userId 时 merge |
| API Key 重复默认不覆盖 | apiKey hash reject |
| 旧客户端最小 body | POST /credentials -> ingest，默认 reject |
| 响应附加 action | AddCredentialResponse.action |
| import 成功返回身份 | POST /credentials/import + response email/userId |
| 批量混合结果 / 默认串行 / 失败不写盘 | import_credentials_batch summary+results，concurrency 默认 1 |
| 状态接口脱敏 | 既有 status 不返回 refreshToken/clientSecret |

### credential-import

| Scenario | Evidence |
| --- | --- |
| KAM userId/nickname 映射 | kam-import-dialog.tsx normalize + batch |
| 平铺/嵌套 | KAM normalize 路径保留 |
| 同 userId 重导 upsert | import/batch 默认 onConflict=upsert |
| UI 调用 batch | BatchImportDialog / KamImportDialog -> importCredentialsBatch |
| 导入后 profile/身份可见 | status hasProfileArn + userId/email |
| verified vs verified_warn | batch UI 用 warning -> verified_warn |
| batch 条目级警告不整批失败 | per-item warning，默认不 stopOnError |

### credential-online-auth

| Scenario | Evidence |
| --- | --- |
| BuilderId start 返回会话/码 | start_builder_id + unit test |
| poll pending | poll_builder_id Pending + test |
| poll 完成入库 + 会话失效 | service poll_builder_id_login -> ingest；session remove；test |
| IAM start 需 startUrl | start_iam_sso / service 校验 + test |
| complete 入库 + startUrl | complete_iam_sso + ingest_online_tokens |
| SSO 多行部分成功 / 全失败 | import_sso_tokens accounts+errors；全失败 UpstreamError |
| 会话过期 | expired_session_rejected test + cleanup_expired |
| 非 Admin 拒绝 | create_admin_router 全路由 admin_auth_middleware（无专用 401 单测 -> WARN） |
| 完成后走 ingest | ingest_online_tokens -> ingest_from_request |

## Scope Diff Inventory

**Modified (in scope):**
README, credentials.example.multiple.json, admin handlers/router/service/types, kiro credentials/usage_limits/token_manager/mod, admin-ui credentials API + dialogs + dashboard/types

**Untracked (in scope):**
src/kiro/online_auth.rs, src/kiro/user_info.rs, admin-ui/.../online-auth-dialog.tsx, docs/add-account-optimization-design.md, openspec/changes/improve-credential-ingest/**

**Not present (good):**
config.json, credentials.json, .codegraph/, Cargo.toml/lock, anthropic/SSE business changes

## Findings

### CRITICAL
- none

### WARN
1. Missing dedicated unauth HTTP integration test: Scenario non-Admin rejection relies on admin middleware layer; no new 401 route test. Low risk (same layer as all Admin routes).
2. Real OIDC/device code/IAM not E2E: online_auth unit tests use hooks; production HTTP via block_in_place needs sandbox manual acceptance.
3. Admin UI no browser E2E: only pnpm build.
4. Full cargo test 8 failures: anthropic::converter UnsupportedModel(claude-sonnet-4), out of scope for this change; handle before merge or isolate.

### INFO
- BuilderIdPollCompletedResponse kept as shape docs; poll completion uses serde_json Value (allow dead_code).
- online_auth has no token-content tracing; errors propagate as strings.
- Task 5.6 log redaction satisfied by code review + no secret log path; not automated log scan.

## Evidence Paths

- openspec/changes/improve-credential-ingest/evidence/bridge-plan.md
- openspec/changes/improve-credential-ingest/evidence/spec-compliance-report.md (this file)
- openspec/changes/improve-credential-ingest/evidence/openspec-verify-report.md
- openspec/changes/improve-credential-ingest/evidence/verification-before-completion.md

## Verification Snapshot (recorded this project session)

| Command | Result |
| --- | --- |
| cargo test online_auth | 6 passed |
| cargo test token_manager | 43 passed |
| cargo test user_info | 1 passed |
| cargo test admin:: | 3 passed |
| pnpm build (admin-ui) | success |
| openspec validate --all | 4 passed, 0 failed |
| cargo test (full) | 225 passed, 8 failed (converter, out-of-scope) |
| git status --short | no secrets / no .codegraph |

## Residual Risk

1. Upstream auth protocol field drift (best-effort + hooks testable; real network pending).
2. Batch high concurrency still limited by default concurrency=1; raising may rate-limit upstream.
3. Main specs not yet synced to openspec/specs (handle on archive).
4. Not committed / PR / archive yet.

## Stop Conditions Check

| Condition | Triggered? |
| --- | --- |
| Unauthorized-scope edits | No |
| Real credentials would be committed | No |
| Scenario cannot map to implementation | No |
| validate failed / key verification missing without note | No |

**Gate:** Ready for openspec-verify-change / archive prep. Before merge, isolate or fix unrelated converter full-suite failures (separate change).

