# Spec Compliance Report: model-resolution-identity-dark-ui

日期：2026-07-24
分支：dev
审查范围：当前工作区 diff（17 tracked + untracked docs/openspec/.claude）
本会话真实运行的验证命令见 Verification 维度。

## 六维表

| 维度 | 状态 | 结论 |
| --- | --- | --- |
| Scope | PASS | 全部 tracked 改动落在 proposal Impact 列出的文件集合内；未触碰 Cargo.toml/Cargo.lock；token_manager/provider 仅被读取核查未改动（属声明范围子集）。新增 `admin-ui/src/components/ui/select.tsx` 为 D8 主题化 Select，属范围内。 |
| Design | PASS | D1 三层管线、D2 auto→defaultChatModel、D3 catalog 透传须命中、D4 OpenAI 别名、D5 列表分层、D6 错误语义、D7 client-identity 独立资源、D8 主题化 Select 复用 `@radix-ui/react-dropdown-menu`（未新增依赖）、D9 路由用 resolved id 均落地。 |
| Scenarios | PASS | 各能力 Scenario 均有实现或单测证据（见下表）。 |
| Project Rules | PASS | OpenSpec 工件齐全并 --strict 通过；错误不含密钥（error.rs 单测）；示例仅 `*.example.json`；无真实凭据。 |
| Verification | PASS | 报告仅列本会话真实运行命令；唯一 SKIPPED（live smoke, tasks 6.4）在 verification-before-completion.md 明示原因与剩余风险。 |
| README/AGENTS Sync | PASS | README、config.example.json 已同步 modelResolution 与 client-identity；AGENTS/tooling 无纪律变化，无需更新。 |

## Scenario 对应证据（抽样核实）

### model-resolution（src/anthropic/converter.rs）
- thinking 后缀剥离：`resolve_model` L267-276；单测 `test_resolve_model_thinking_suffix` L1253。
- 未知不可透传拒绝：`ResolveError::Unmapped/NotInCatalog` L134-158；L340/L343。
- gpt-4o 大小写不敏感→claude-sonnet-4.5：`builtin_compat_aliases` L169-177；`test_resolve_model_openai_aliases` L1203。
- auto→defaultChatModel：L282-295；`test_resolve_model_auto` L1221。
- catalog 命中透传：L319-339；`test_resolve_model_catalog_passthrough` L1234（含 disabled/未命中拒绝断言）。
- resolvedModelId + resolveKind：`ResolvedModel`/`ResolveKind` L100-132。

### model-catalog / credential-model-test
- test 走 resolve：service.rs L687；响应含 `resolved_model`/`resolve_kind` L714-715。
- unmapped 非“凭据无效”前缀：`AdminServiceError::ModelUnmapped` error.rs L37；单测 `model_unmapped_message_has_no_credential_prefix` L100，`*_has_no_secret_fields` L112/L122。
- 默认模型 claude-sonnet-4.6：service.rs L661。

### model-aware-routing
- 路由用 resolved upstream id：convert 写入 `conversationState...modelId=resolved.model_id`（converter.rs L477）；provider `extract_model_from_request` 读取该 id（provider.rs L656）→ `acquire_context`/`select_next_credential`（token_manager.rs L885/L956）。冷启动乐观放行单测 `test_select_next_credential_cold_start_optimistic` L3401。

### admin-runtime-settings（client-identity）
- 读写热更单测 `client_identity_get_and_update`、空值拒绝 `client_identity_rejects_empty`（均通过）。

### admin-ui-dark-theme / model-ops
- 主题化 Select 用 `bg-background`/`bg-popover`/`text-popover-foreground` token，无硬编码纯白（select.tsx L42/L57）。
- 无业务原生 `<select>` 残留（grep 确认）。
- credential-test-dialog 仅展示 testable、优先 sonnet、消费 resolveTo/resolvedModel 元数据（L57-71、L151、L190）。
- index.css 增加 `color-scheme: light/dark`。

## Verification（本会话真实运行）

| 命令 | 结果 |
| --- | --- |
| `cargo test`（全量） | 283 passed, 0 failed（仅既有 dead_code warnings） |
| `cargo test resolve` | 7 passed（含 auto/别名/透传/thinking/Claude 回归 + convert alias + auto 本地解析） |
| `cargo test client_identity` | 2 passed（读写热更 + 空值拒绝） |
| `openspec validate model-resolution-identity-dark-ui --strict` | valid |
| `git check-ignore config.json credentials.json .codegraph/` | 三者均被忽略 |
| grep `<select`（admin-ui/src） | 无业务原生 select 残留 |

pnpm build 结果引用自 verification-before-completion.md（本次未重跑）。

## 发现项

- **[INFO] 未跟踪的 `.claude/` 目录**：`.claude/settings.local.json` 已被 .gitignore（第16行）忽略；`.claude/skills/` 未忽略但非本 change 目标，属工具目录。提交时应仅暂存本 change 相关文件，勿 `git add .` 误纳 `.claude/skills`。
- **[INFO] tasks 6.4 live smoke 未执行**：已在 verification-before-completion.md Skipped 表明示（部署新二进制后需跑 test 默认/auto/gpt-5.6-sol/claude-sonnet-4.6）。属可接受剩余风险。

## 证据路径

- openspec/changes/model-resolution-identity-dark-ui/evidence/verification-before-completion.md
- openspec/changes/model-resolution-identity-dark-ui/evidence/bridge-plan.md
- openspec/changes/model-resolution-identity-dark-ui/evidence/dark-select-style-check.png

## 剩余风险

- catalog 透传仅保证本地解析与进入 generate 路径；上游对 gpt-5.6-sol 等的实际支持依赖账号/端点。
- 热更的版本字符串通过非空/长度校验，但上游仍可能拒绝非法版本。
- dark 验收基于静态预览 + 合成 Select 样式检查；部署后建议真实 Admin 流程点检一遍。

## 总体状态：PASS

必需工件齐全、范围未越界、Scenario 均有实现/单测、真实验证通过、无密钥入库风险；仅存已明示的可接受剩余风险（live smoke SKIPPED）。提交前注意仅暂存本 change 文件，避免 `.claude/skills` 等误入。
