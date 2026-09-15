# Design: desktop-credentials-view

整体见 `docs/desktop-gpui-embedded-design.md` §8。本文只覆盖本变更（凭据
数据视图 + 操作 + 对话框 + 在线登录）的实现层决策。

## Context

- change 2 的四个视图是占位面板；change 3 的 `SqliteStore` 已装配，凭据
  落库安全，但桌面端没有任何操作入口。
- `AdminService`（`src/admin/service.rs`）封装全部凭据业务：查询/增删/
  启停/测试/余额/模型刷新/在线登录。admin-ui 的 React 前端消费的就是它。
- `AdminService::new_with_runtime` 与全部所需方法已是 `pub`，`Bootstrapped`
  字段（`token_manager` / `endpoint_names` / `kiro_provider`）足够构造
  服务实例。根仓零改动。
- gpui-kit 0.6 组件面核实：`DataTable` + `TableDelegate`（`src/table/`）、
  `Dialog` / `AlertDialog`（`window.open_dialog`）、`Input` / `Textarea`
  （`InputState`）、`Select`、`Switch`、`Notification`
  （`window.push_notification`）均可用。

## Goals / Non-Goals

Goals:

- 「凭据」视图替换占位：`DataTable` 列表 + 筛选 + 分页。
- 行操作全通：启停、优先级、删除、测试、余额、刷新模型。
- 添加凭据（social / api_key / idc 表单）、批量导入（JSON 数组）、
  KAM 文档导入、删除确认对话框。
- Builder ID 登录（浏览器 + 轮询）与 IAM SSO 登录（startUrl → 浏览器 →
  粘贴回调）。
- headless 测试：表格渲染、写操作走真实 `AdminService` + tempdir
  `SqliteStore`。

Non-Goals:

- 总览/设置/服务视图仍是占位（总览数据面板与凭据视图独立，放后续）。
- 不做凭据编辑（修改已有凭据的字段）：服务端没有 patch 语义，删除重建
  是当前 admin-ui 的同款做法。
- 不碰网络测试：测试/余额/SSO 的上游交互留手动全流程（设计文档 §12）。

## Decisions

### D1：模块布局

```text
desktop/crates/kiro-desktop/src/
├── core/
│   └── mod.rs          # CoreHandle（service + tokio handle + exec 助手）
├── ui/
│   ├── mod.rs
│   ├── root.rs         # AppView：持有 Option<CoreHandle>，下发视图
│   ├── views.rs        # 占位视图（总览/设置/服务保留）
│   └── credentials/
│       ├── mod.rs      # CredentialsView：筛选栏 + 表格 + 工具栏
│       ├── table.rs    # CredentialsDelegate（TableDelegate 实现）
│       └── dialogs.rs  # 添加/批量导入/KAM/删除确认/BuilderID/IamSso
```

### D2：`CoreHandle` 与事件通道

装配产物从 `Bootstrapped` 延伸：`main.rs` 在 bootstrap 成功后用
`new_with_runtime(token_manager, endpoint_names, None, Some(kiro_provider))`
构造桌面专用 `AdminService`（不带 admin_api_key 门槛：门槛只管 HTTP 路由，
本地界面不受限），连同 tokio handle 打包成 `CoreHandle` 塞进
`CoreEvent::Bootstrapped`。

```rust
#[derive(Clone)]
pub struct CoreHandle {
    pub service: Arc<AdminService>,
    pub rt: tokio::runtime::Handle,
}

impl CoreHandle {
    /// 把 future 派到 tokio 侧，oneshot 回传结果
    pub fn exec<R: Send + 'static>(
        &self,
        fut: impl Future<Output = R> + Send + 'static,
    ) -> oneshot::Receiver<R>;
}
```

`CoreEvent` 去掉 `PartialEq` / `Eq`（`Arc<AdminService>` 不可比较），保留
`Debug` + `Clone`；`bridge.rs` 的往返测试改为逐字段断言。`AppView` 收到
`Bootstrapped` 后把 handle 存入自身，再创建 `CredentialsView` 实体；装配
完成前凭据视图显示加载态。

### D3：同步 / 异步调用分界

**同步方法在 GPUI 线程直调的前提是不触发 `tokio::spawn`。**
`set_disabled` / `set_priority` 内部会 `spawn_refresh_models_arc`
（`service.rs:292,310`），在 GPUI 线程直调会 panic（无 runtime 上下文）。
因此：

- **GPUI 直调**（纯内存快照读）：`query_credentials`、`get_credential_facets`
- **派 tokio**（经 `CoreHandle::exec` + oneshot）：`set_disabled`、
  `set_priority`、`delete_credential`、`get_balance`、`test_credential`、
  `add_credential`、`import_credentials_batch`、`import_kam_document`、
  `refresh_models`、四条在线登录方法

`delete_credential` 虽是同步方法，内部走 `persist_credentials`（SQLite
写）+ `save_stats`，为与其余写操作路径一致也派 tokio。

操作结果统一收敛为通知 + 表格重拉（视图持快照、写后重拉，§8 既定模式），
不解析原始响应体。进行中状态：对话框确认按钮禁用直到 oneshot 返回。

### D4：`DataTable` 数据流

`CredentialsDelegate` 持 `Vec<CredentialStatusItem>` 快照 + 查询参数。
列（宽度按信息密度排）：

| 列 | 内容 |
| --- | --- |
| # | `id` |
| 账号 | `nickname` / `email` / `masked_api_key` 依序降级 |
| 认证 | `auth_method` + `provider` |
| 订阅 | `subscription_title`（缺省「未知」） |
| 优先级 | `priority`（stepper 就地改） |
| 状态 | 禁用开关 + 连续失败数 + 禁用原因 |
| 余额 | 快照 `current_usage / usage_limit`（缺省「-」） |
| 过期 | `expires_at` 短格式 |
| 操作 | 测试 / 余额 / 刷新模型 / 删除（按钮组） |

`TableDelegate` 必填四方法（`columns_count` / `rows_count` / `column` /
`render_td`）；排序本期不接（服务端按优先级排序返回）。行数据刷新 =
替换快照 + `TableState::refresh`。

### D5：筛选栏与分页

筛选走 `CredentialsQuery` 现成参数：`auth_method` / `disabled` /
`subscription_title`（含 `__unknown__` 哨兵）/ `email` 子串。筛选值候选
取自 `get_credential_facets`（覆盖全量，不受当前页影响）。分页用
`page` / `per_page`（缺省 12），底部上一页/下一页 + 总数显示。
每次筛选/翻页变化重查（同步直调，无需防抖）。

### D6：对话框

全部经 `window.open_dialog`（gpui-component `Dialog`）：

- **添加凭据**：`Select` 选认证方式切换表单。social：refreshToken（必填）+
  可选 email / region。api_key：kiroApiKey（必填，`ksk_` 前缀校验）。
  idc：refreshToken + clientId + clientSecret + provider + region。
  提交走 `add_credential`，成功后通知 + 重拉。
- **批量导入**：`Textarea` 粘贴 JSON 数组 → `serde_json` 本地预解析（解析
  失败就地报错不发请求）→ `import_credentials_batch`；结果汇总
  （created/updated/duplicate/failed 计数）进通知。
- **KAM 导入**：`Textarea` 粘贴原始文档 → `import_kam_document`
  （dry_run=false）；容器判别在服务端，桌面不重复实现。
- **删除确认**：`AlertDialog::confirm`，文案带凭据标识；确认派
  `delete_credential`。

### D7：在线登录

- **Builder ID**：可选 region 输入 → `start_builder_id_login` → 成功后
  `open::that(verification_uri)` 拉起浏览器，对话框显示 `user_code`，
  tokio 侧按 `interval` 轮询 `poll_builder_id_login` 直到完成/过期；完成
  时服务内部已 `ingest_online_tokens` 入库，UI 只通知 + 重拉。
- **IAM SSO**：startUrl（必填）+ region → `start_iam_sso_login` →
  `open::that(authorize_url)` → 用户粘贴回调 URL →
  `complete_iam_sso_login`。回调解析失败/会话过期的错误文案透传
  `AdminServiceError` 的 Display。
- 浏览器拉起失败（`open` 返回 Err）时对话框保留完整 URL 供手动复制。

### D8：测试策略

- dev-deps：`gpui`（package `gpui-pre`，`test-support` feature，版本与
  锁内一致）+ 已有的 `tempfile`。
- 表格渲染：`#[gpui::test]` + `TestAppContext`，真实 `AdminService`
  （tempdir `SqliteStore` 预置 2 条凭据）→ `CredentialsDelegate` 行数/
  列数/单元格文本与快照一致。
- 写操作：`set_disabled` / `set_priority` / `delete` 经 `CoreHandle::exec`
  （测试内自建单线程 tokio runtime）走真实服务 → 重查断言状态；
  删除后 `SqliteStore` 里行与钥匙串条目同步消失。
- 表单：添加凭据的必填校验与 `ksk_` 前缀校验（纯函数，不依赖窗口）。
- 不碰网络：`test_credential` / `get_balance` / SSO 的上游路径不在单测
  范围。

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| GPUI 线程误调含 `tokio::spawn` 的同步方法导致 panic | D3 白名单制：只有两个纯读方法允许直调，其余全派 tokio；代码注释钉死 |
| 桌面与 change 5 内嵌服务器各持一个 `AdminService`，余额缓存两份 | 同一 `cache_dir` 锚点 + 300 秒 TTL，重复查询最坏多一次上游调用；合并实例等 change 5 服务器装配时再评估 |
| `CoreEvent` 去掉 `PartialEq` 影响既有测试 | `bridge.rs` 测试改逐字段断言，随本 change 一起改 |
| Builder ID 轮询期间用户关窗 | 轮询任务持 service Arc 独立存活，结果丢弃无副作用；会话在服务端自然过期 |
| gpui `test-support` 与锁内版本漂移 | dev-dep 版本写死与 `desktop/Cargo.lock` 一致，`--locked` 门禁把关 |
| SSO 流程手动验证成本高 | 设计文档既定取舍：上游交互留手动全流程，单测只覆盖状态机与错误分支 |
