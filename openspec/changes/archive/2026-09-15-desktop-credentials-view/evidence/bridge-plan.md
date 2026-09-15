# Bridge Plan: desktop-credentials-view

门禁：`openspec-superpowers-bridge`。

- change：`desktop-credentials-view`
- 分支：`dev-desktop`（HEAD `c620b82`，工作区干净）
- 状态：`openspec validate` 通过，state 非 blocked

## 一、范围 / 非目标

范围：凭据数据视图（表格 + 筛选 + 分页 + 行操作）、添加/批量导入/
KAM 导入/删除确认对话框、Builder ID / IAM SSO 在线登录、CoreHandle
装配接线、headless 测试。

非目标：总览/设置/服务视图（占位保留）；凭据编辑（删除重建）；
网络类上游交互的自动化测试。

## 二、关键设计决策（design.md 摘要）

- D1 模块布局：`src/core/mod.rs` + `src/ui/credentials/{mod,table,dialogs}.rs`
- D2 `CoreHandle { service: Arc<AdminService>, rt }` 经
  `CoreEvent::Bootstrapped` 下发；`exec` 用 tokio spawn + oneshot
- D3 同步/异步分界白名单：只有 `query_credentials` /
  `get_credential_facets` 允许 GPUI 直调（纯读快照）；其余全部派
  tokio。`set_disabled` / `set_priority` 内部含
  `spawn_refresh_models_arc`（`service.rs:292,310`），GPUI 线程直调
  会 panic，这是白名单的直接依据
- D4 DataTable：`CredentialsDelegate` 持快照，写后重拉 +
  `TableState::refresh`
- D5 筛选走 `CredentialsQuery` 现有参数 + `get_credential_facets`
- D6/D7 对话框与在线登录均派 tokio；浏览器经 `open` crate
- D8 测试：`gpui` dev-dep（test-support）+ 真实 AdminService +
  tempdir SqliteStore

## 三、CodeGraph 证据

| 命令 | 结论 |
| --- | --- |
| `codegraph callers AdminService::new_with_runtime` | 仅 `AdminService::new` 一处（桌面将是第二个调用方） |
| `codegraph query query_credentials` | 实现在 `service.rs:132`，admin-ui 前端经 `useCredentials` 消费 |
| `rg "spawn" service.rs:277-312` | `set_disabled`/`set_priority`/`reset_and_enable` 均含 `spawn_refresh_models_arc` |

## 四、rg / 源码补盲（已核实）

| 事项 | 证据 |
| --- | --- |
| `AdminService::new_with_runtime` 签名 | `service.rs:65`：token_manager + known_endpoints + client_auth + provider，全部可从 `Bootstrapped` 取 |
| 凭据操作 API 面 | `query_credentials:132`、`set_disabled:277`、`set_priority:298`、`delete_credential:744`、`get_balance:315`、`add_credential:382`、`import_credentials_batch:486`、`import_kam_document`（handlers 侧）、`test_credential:855`、`refresh_models:794`、SSO 四方法 1662-1757 |
| 请求/响应类型 | `CredentialsQuery`（types.rs:16）、`AddCredentialRequest`（250）、`BatchImportRequest`（611）、`KamImportRequest`（types.rs）、`CredentialStatusItem`（98） |
| gpui-kit 组件 API | `TableDelegate` 必填四方法（table/delegate.rs）、`DataTable::new(Entity<TableState<D>>)`、`window.open_dialog`（window_ext.rs:141）、`AlertDialog::confirm`（alert_dialog.rs:96）、`window.push_notification`（window_ext.rs:180）、`Notification::success/error` |
| gpui headless 测试 | `#[gpui::test]` + `TestAppContext`（gpui-pre test_context.rs），gpui-kit 透传 `test-support` feature；gpui-component 自带 `tests/table_dump_range.rs` 作参考用法 |
| 浏览器拉起 | `open` 5.4.4 已在 `desktop/Cargo.lock` |
| `CoreEvent` 现状 | `bridge.rs`：derive `PartialEq, Eq`，加 `Arc<AdminService>` 后必须去掉 |

## 五、任务到执行步骤映射

| 任务 | 执行步骤 | 验证 / 停止条件 |
| --- | --- | --- |
| 1.1-1.4 装配 | `core/mod.rs`、`bridge.rs`、`main.rs`、`root.rs` | 编译零告警；bridge 测试绿 |
| 2.1-2.4 表格 | `credentials/{mod,table}.rs` + 筛选分页 + 行操作 | headless 表格测试绿 |
| 3.1-3.4 对话框 | `credentials/dialogs.rs` | 编译通过；表单校验测试绿 |
| 4.1-4.2 登录 | dialogs 内两条流程 | 编译通过（上游交互手测） |
| 5.1-5.4 测试 | dev-deps + 三类测试 | `cargo test --release` 全绿 |
| 6.1-6.5 验收 | 双门禁 + Xvfb 冒烟 + 根仓检查 | 见 design D8 与 proposal |

## 六、必跑验证

1. `desktop/`：`RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked`
2. `desktop/`：`cargo test --release`
3. 根仓：`cargo check --release --all-targets`（告警数不变，基线 0）
4. Xvfb 冒烟：装配 + 凭据视图数据加载日志链
5. `openspec validate desktop-credentials-view`
6. `git status --short`（无敏感文件）

## 七、同步判断

- `docs/tooling-sources.md`：登记 `open`（运行时）与 `gpui` dev-dep
- `desktop/README.md`：补 change 4 状态与凭据视图说明
- 根仓 / `spec/`：零改动，`skip_specs: true`

## 八、停止条件

- 工件缺失、互相矛盾或状态 blocked
- 发现未写入规格的高风险影响（尤其根仓被迫改动）
- 工作区存在会被提交的真实凭据 / token
- `AdminService` 任一所需方法实际不可达（可见性或字段不足），
  迫使根仓改动时先停下来评估
- 无法确定验证命令或剩余风险
