# Spec Compliance Report: admin-models-settings-optimization

日期：2026-07-24
审查类型：实现后 / 归档前合规
总体状态：**WARN**

## 六维表

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | **PASS** | 改动落在 proposal Impact（anthropic handlers、token_manager、admin/*、middleware、config、admin-ui、README/example）。profile/provider 的 Mutex 化服务于 proxy/endpoint 热更新，未触达 Non-goals（多 Key 配额、多 URL fallback、catalog DB、LB 重写、SSE 核心）。无 Docker/CI 无关改动。 |
| Design | **PASS** | 对齐 D1 过滤 unmapped、D2 后台预热不阻塞、D3 test UI 读 models、D4 force balance、D5 校验后落盘、D6 requireApiKey 默认 true + fail-closed、D7 endpoint 白名单、D8 模块落点。catalog 对外 id 保持上游 modelId（map 仅作门禁），与 design「可映射后仍用上游 id」一致。 |
| Scenarios | **WARN** | 核心 Scenario 有实现证据；settings 非法输入 400 / 未认证 401 依赖既有 Admin 错误映射与 admin_auth_middleware，**缺少专用 HTTP 单测**（task 5.5 勾选偏满）。 |
| Project Rules | **PASS** | 走 OpenSpec + bridge；验证有证据文件；`git status` 无 config/credentials/.codegraph；未提交真实密钥。 |
| Verification | **WARN** | 关键测试子集、admin-ui build、`openspec validate --all` 通过；全量 `cargo test` 已运行，263 passed / 8 failed，均在 converter 且源于既有旧模型映射用例。未做真实上游 E2E（可接受）。 |
| README/AGENTS Sync | **PASS** | README 配置表 + Admin 运行时设置段、`config.example.json` requireApiKey 已同步。AGENTS.md 无需改。长期 `spec/design.md` 未轻量补句（归档/sync 阶段可补）。 |

## Requirement / Scenario 对照

### model-catalog（MODIFIED）

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 缓存非空 | `get_models` → `global_model_catalog` → `models_from_catalog` | PASS |
| 缓存为空兼容 | 空 catalog 走 `static_fallback_models` | PASS |
| catalog 不暴露 unmapped | `map_model` None 则 skip + 单测 `models_from_catalog_skips_unmapped` | PASS |
| 静态 fallback 均可映射 | `static_fallback_models_all_mappable` | PASS |
| 启动预热不阻塞 | `spawn_warmup_models(2)` in `main`；异步 spawn | PASS |
| /v1/models 不因预热阻塞 | get_models 无 await 全量 refresh | PASS |
| Admin 全局 catalog | `GET /api/admin/models/catalog` | PASS |
| 未认证拒绝 | Admin 路由统一 `admin_auth_middleware` | PASS（间接） |

### admin-ui-model-ops（MODIFIED）

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 默认/指定模型测试 | TestDialog 可省略或提交 model | PASS |
| 从缓存列表选择模型 | Select 选项来自 `getCredentialModels` | PASS |
| 查看/刷新模型 | ModelsDialog + 卡片刷新 | PASS |
| 从查看模型发起测试 | `onTestModel` → TestDialog `initialModel` | PASS |
| 列表 modelCount | status 字段 + 卡片徽章 | PASS |
| 无缓存不崩溃 | `modelCount ?? 0` | PASS |
| 单卡 force 刷新余额 | 主按钮 `getCredentialBalance(id, true)` | PASS |
| 批量查询信息互补 | 顶栏保留，文案「批量余额/订阅」 | PASS |
| 重置失败降级 | 仅有失败计数或 disabled 时显示 | PASS |

### credential-model-test（MODIFIED）

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 默认/指定模型 test API | 后端 `test_credential` 未改形状 | PASS |
| 非法 model 400 | 既有 `test_credential_rejects_unmapped_model` | PASS |
| 缓存 modelId 可用于 test | UI 提交列表 id；后端 map_model 路径 | PASS |

### admin-runtime-settings（NEW）

| Scenario | 实现证据 | 状态 |
| --- | --- | --- |
| 读/写代理 | `settings/proxy` GET/PUT；校验 http(s)/socks5 | PASS |
| 非法 URL 400 | `InvalidCredential` → 400 | PASS（逻辑）/ WARN（无单测） |
| 清空代理 | 空 URL 清 config + runtime | PASS |
| 端点读/写/白名单 | `settings/endpoint` + known_endpoints | PASS |
| requireApiKey 默认 true | Config default + middleware | PASS |
| 关闭校验 | require=false 跳过 key 检查 | PASS（AuthRuntime 单测） |
| true+空 key fail-closed | middleware 空 key → 401 | PASS（AuthRuntime 单测） |
| auth mask / 热更新 | GET mask；PUT 写 Config + `client_auth` RwLock | PASS |
| 未认证写入拒绝 | Admin middleware | PASS（间接）/ WARN（无专用测） |
| 写失败不静默成功 | save 失败返回 InternalError | PASS（逻辑）/ WARN（无 FS 失败单测） |

## 范围与越界检查

**在范围内：**
- `src/anthropic/{handlers,middleware,router,mod}.rs`
- `src/admin/{handlers,router,service,types}.rs`
- `src/kiro/{token_manager,provider,profile}.rs`（热更新支持）
- `src/model/config.rs`、`src/main.rs`
- `admin-ui` 卡片/对话框/settings API/面板/types
- `README.md`、`config.example.json`、设计文档与 OpenSpec change 工件

**未发现越界：**
- 未改 SSE/parser/stream 核心
- 未引入多 API Key 配额面板
- 未实现多端点 URL fallback
- 无 `.codegraph/`、真实 `config.json`、`credentials.*` 进入 status

## 发现项

### WARN-1：task 5.5 验证覆盖偏弱
- **描述**：middleware 单测覆盖 AuthRuntime 热更新与 require 标志，但非完整 HTTP 四象限（oneshot）与 settings 400/401 路由级测试。
- **影响**：非阻塞；生产路径代码存在且错误映射明确。
- **建议**：归档前可补 `update_proxy_settings` 非法 URL 与 admin 401 轻量单测。

### WARN-2：既有 converter 模型映射用例失败
- **描述**：全量 `cargo test` 中 8 个 converter 用例失败：`test_map_model_sonnet` / `test_map_model_opus` 使用旧模型名，另 6 个转换/工具历史用例仍使用不受支持的 `claude-sonnet-4`。
- **影响**：非本 change 引入；本 change 未改 map 语义。
- **建议**：另 issue 修正测试或 map 兼容策略。

### INFO-1：catalog id 未强制写成 map 结果字符串
- **描述**：proposal 措辞「归一 canonical」；实现为「可映射门禁 + 保留上游 id」。
- **判定**：符合 design 执行约束，不构成 FAIL。

### INFO-2：长期 spec/design 未本轮补句
- **描述**：bridge 建议实现后轻量更新；本轮仅 README/example。
- **建议**：`openspec-sync-specs` / 归档时同步 main specs 与 `spec/design.md`。

## CRITICAL

无。

## 证据路径

- Bridge：`openspec/changes/admin-models-settings-optimization/evidence/bridge-plan.md`
- 验证：`openspec/changes/admin-models-settings-optimization/evidence/verification-before-completion.md`
- 本报告：`openspec/changes/admin-models-settings-optimization/evidence/spec-compliance-report.md`
- OpenSpec 工件：`proposal.md` / `design.md` / `tasks.md` / `specs/**/spec.md`（26/26 tasks 勾选）

## 剩余风险（可接受）

1. 代理热更后 in-flight 可能仍用旧 HTTP client（design 已说明）。
2. 配置写盘失败半更新无完整 FS mock 矩阵。
3. `requireApiKey=false` 仅 UI 二次确认。
4. 无真实上游 E2E。

## 结论

**WARN（可继续归档评审）**：规格能力已实现且主路径有测试与构建证据；非阻塞缺口集中在 settings HTTP 级单测与既有 converter 模型映射测试债。无 CRITICAL、无范围越界、无密钥入仓风险。

建议下一步：`openspec-verify-change`；可选先补 WARN-1 测试再 archive。
