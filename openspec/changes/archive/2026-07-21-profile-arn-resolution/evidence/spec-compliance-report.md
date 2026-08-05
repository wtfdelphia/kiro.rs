# Spec Compliance Report: profile-arn-resolution

**Date:** 2026-07-20 (Asia/Shanghai)
**Skill:** spec-compliance-check
**Overall status:** **WARN**

## 六维表

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | **PASS** | 改动限于 kiro/admin/admin-ui/examples/README/openspec；未触 OpenAI、多 endpoint、负载均衡、Docker/CI；未改 Cargo.toml/Cargo.lock |
| Design | **PASS** | resolve 顺序 cache→fixed→list→refresh→persist；请求前 ensure；403 先 resolve 再 force refresh；provider + KAM 默认 BuilderId 对齐 design |
| Scenarios | **WARN** | 主路径已实现；list/refresh 缺 mock 集成测；KAM 验活「仅余额成功」未提示 profile 未解析 |
| Project Rules | **PASS** | OpenSpec + bridge 齐全；无真实凭据；有实测验证记录 |
| Verification | **WARN** | 相关 97 tests 通过；openspec validate 通过；pnpm build 通过；全量 converter 8 失败为范围外既有问题 |
| README/AGENTS Sync | **PASS** | README 已补 provider/KAM；AGENTS 无需改；长期 spec/ 可归档时 sync |

## 发现项

### CRITICAL

无。

### WARN

1. **导入验活 UX 缺口（credential-import / 仅余额成功）**
   - 规格要求不得只把 usage 成功当成完全健康。
   - **已缓解（2026-07-20 优化）**：KAM 使用 verified vs verified_warn；toast 区分缺 profile；凭据卡片展示 Profile 已就绪/未解析与 provider。
   - 后端 get_usage_limits_for 仍 best-effort resolve，对话路径也会 resolve。

2. **ListAvailableProfiles / refresh-fallback 缺 HTTP mock 单测**
   - profile.rs 覆盖固定表/infer/supports/transient；list/refresh 网络路径无 mock。

3. **全量 cargo test 既有失败**
   - anthropic converter 8 例 UnsupportedModel(claude-sonnet-4)，与本 change 无关。

4. **可选回滚开关未实现**
   - design 中可选 env/config 关闭解析未做；默认始终 resolve（可接受 YAGNI）。

### PASS 证据（Scenario 映射）

| Scenario | 证据 |
| --- | --- |
| 缓存命中 | ensure_profile_arn_for_request 先读已有 profile_arn |
| BuilderId 固定 ARN | get_fixed_profile_arn + 测试 test_fixed_builder_id / infer BuilderId |
| ListAvailableProfiles | list_available_profiles(_with_retry) |
| refresh fallback | resolve 内 force_refresh 后再读/再 fixed |
| usage 前 resolve | get_usage_limits_for → ensure |
| 403 无 profile | provider.rs bearer invalid 分支 |
| provider 持久化 | credentials 字段 + roundtrip 测试；snapshot.provider |
| KAM 接收字段 | AddCredentialRequest + kam-import；add 不再硬编码 profile_arn:None |
| 导入后 resolve | add 后 get_usage_limits_for |

## 改动范围

- Modified: README, admin-ui, examples, src/admin/*, src/kiro/{mod,credentials,provider,token_manager}.rs
- Added: src/kiro/profile.rs, openspec/changes/profile-arn-resolution/**
- Not touched: Cargo.toml/lock, Docker, CI, 真实 credentials

## 验证证据路径

- evidence/bridge-plan.md
- evidence/apply-session-verification.md
- evidence/spec-compliance-report.md (本文件)

## 实测命令

| 命令 | 结果 |
| --- | --- |
| cargo test (kiro profile/credentials/token_manager/endpoint) | 97 passed |
| openspec validate profile-arn-resolution / --all | passed |
| admin-ui pnpm build | success |
| git status --short | 无 config.json / credentials.json / .codegraph |

## 剩余风险

- 未做真实 IdC 端到端 curl（有意避免密钥入库）。
- Enterprise list 与多区域 REST base 未生产验证。
- 导入 UX 可能显示「验活成功」而 hasProfileArn 仍为 false（对话时会再 resolve）。

## 结论

**WARN — 可进入 openspec-verify-change / 归档前审查。**
无 CRITICAL。建议归档前处理 WARN#1 或在 design 中明确降级为已知限制。
