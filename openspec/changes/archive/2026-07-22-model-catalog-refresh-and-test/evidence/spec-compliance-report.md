# Spec Compliance Report: model-catalog-refresh-and-test

日期：2026-07-22（复审）  
审查 skill：spec-compliance-check  
变更：openspec/changes/model-catalog-refresh-and-test/  
分支：dev（实现未提交）

## 总体状态

**WARN**

proposal / design / specs 范围内的能力已实现且 tasks 20/20 勾选完成；本会话相关单测与 `openspec validate --all` 通过。  
非阻塞剩余：全量 `cargo test` 仍有 8 个 pre-existing converter 旧别名失败；ListAvailableModels / test 成功路径无 mock HTTP；catalog 仅内存。

**无 CRITICAL**（无密钥入 diff、无 Docker/SSE/Cargo deps 越界、validate 通过）。

## 六维审查表

| 维度 | 状态 | 结论 |
| --- | --- | --- |
| Scope | PASS | tracked: admin/*, anthropic handlers+mod, kiro catalog/token_manager/provider/mod, main, README；untracked 应提交: available_models.rs、models_api.rs、docs 设计稿、openspec change。未改 Cargo.toml/Docker/SSE/admin-ui/credentials schema。 |
| Design | PASS | D1 双层缓存；D2 冷启动乐观；D3 host us-east-1；D4 /v1/models 只读缓存；D5 真实最小 generate test；D6 supports_opus；D7 模块落点。失败 refresh 仅 last_error、不覆盖 model_ids。4-6/4.6 别名匹配为合规后优化，符合设计风险缓解。 |
| Scenarios | PASS | 见 Scenario 矩阵；mock HTTP 成功路径仍缺但有解析/路由/选号/错误映射单测覆盖核心行为。 |
| Project Rules | PASS | OpenSpec 工件齐全；git status 无 config/credentials/.codegraph；未隐瞒全量测试红灯；surgical。 |
| Verification | WARN | 相关模块本会话重跑全绿；全量 8 converter fail 仍在（非本 change 映射表改动）。 |
| README/AGENTS Sync | PASS | Admin 新 endpoints + /v1/models 缓存优先说明已写；main 启动日志已补；AGENTS 无需改；main specs 归档时再同步。 |

## Scenario 对照

### model-catalog

| Scenario | 证据 | 状态 |
| --- | --- | --- |
| 成功拉取 / 解析 | models_api + parse fixture 单测 | PASS |
| 上游失败保留旧缓存 | refresh Err 只写 last_error + `test_refresh_failure_preserves_existing_model_cache` | PASS |
| 单凭据刷新 + 全局合并 | refresh + `test_global_catalog_merge_from_per_credential` | PASS |
| 删除清理 | `test_delete_credential_clears_model_catalog` | PASS |
| Admin 刷新/查看/404 | router + service 404 单测 | PASS |
| GET /v1/models 非空+thinking | `models_from_catalog_*` | PASS |
| 空缓存静态 fallback | `static_fallback_models_has_core_ids` + README | PASS |
| 添加/启用异步 refresh | add / set_disabled / reset_and_enable spawn | PASS |

### model-aware-routing

| Scenario | 证据 | 状态 |
| --- | --- | --- |
| 有缓存不含模型跳过 | `test_select_next_credential_filters_by_model_set` | PASS |
| 冷启动/空 set 乐观 | cold_start / empty_set 单测 | PASS |
| Free+opus 拒绝 | opus_subscription 单测 | PASS |
| balanced 候选集 | `test_select_next_credential_balanced_among_model_capable` | PASS |
| dash/dot modelId | `set_contains_model_accepts_dash_version` + accepts_dash_model_id | PASS |

### credential-model-test

| Scenario | 证据 | 状态 |
| --- | --- | --- |
| 非法 model 400 / 无 secret 字段 | `test_credential_rejects_unmapped_model` | PASS |
| 502/错误响应无 secret 字段名 | `upstream_error_response_has_no_secret_fields` + status map | PASS |
| 默认/指定模型成功 generate | 实现路径完整 | WARN（无 mock/E2E） |
| Token 失败专用路径 | 归类复用 classify_balance_error | WARN（无专用单测） |

## 范围与 diff

**git diff --stat（tracked）：** 13 files, **+1109 / -16**  
含 README、main、admin/*、anthropic/*、kiro token_manager/provider/mod。

**应纳入提交（untracked）：**  
- src/kiro/model/available_models.rs  
- src/kiro/models_api.rs  
- docs/model-refresh-and-test-optimization-design.md  
- openspec/changes/model-catalog-refresh-and-test/**

**禁止提交：** models_list.txt、tmp_*、tmp_patches/**、真实凭据、.codegraph

## 本会话验证（真实执行）

| 命令 | 结果 |
| --- | --- |
| cargo test select_next_credential | 6 passed |
| cargo test refresh_failure_preserves | ok |
| cargo test available_models | 5 passed |
| cargo test test_credential_rejects | ok |
| cargo test models_from_catalog | 2 passed |
| openspec validate --all | 6 passed, 0 failed |
| git status --short | 无密钥/无 .codegraph |

历史全量：247 passed / 8 failed（converter 旧别名 `claude-sonnet-4` 等）。

## 发现项

### WARN-1（保留）：无 mock HTTP 集成
不扩 Cargo.toml；List 非 200 与 test 成功路径靠代码+局部单测。

### WARN-2（保留）：全量 converter 8 fail
pre-existing；本 change 仅 `pub use map_model`，不修映射表。

### INFO：UI 6.2 SKIP
API-first，符合 design。

### INFO：提交卫生
工作区仍有 tmp_* / models_list.txt，commit 时必须排除。

## 证据路径

- openspec/changes/model-catalog-refresh-and-test/{proposal,design,tasks}.md  
- specs/{model-catalog,model-aware-routing,credential-model-test}/spec.md  
- evidence/bridge-plan.md  
- evidence/spec-compliance-report.md（本文件）  
- 实现：src/kiro/{models_api,model/available_models,token_manager}.rs、src/admin/*、src/anthropic/handlers.rs  

## 剩余风险

1. 冷启动乐观窗口选错号（设计接受）。  
2. test 消耗上游配额（仅 Admin）。  
3. 进程重启 catalog 清空。  
4. 误提交 tmp/密钥。  
5. 全量测试红灯被误读为本回归。  

## 门禁结论

| 问题 | 答案 |
| --- | --- |
| 规格是否落地？ | **是** |
| 可否 commit？ | **是**（精选路径） |
| 可否 archive？ | **可**（建议 PR 注明 converter 8 fail + UI SKIP） |
| CRITICAL 未处理？ | **无** |

总体：**WARN**（可接受剩余风险；无阻塞项）

---
复审：Codex / spec-compliance-check  
