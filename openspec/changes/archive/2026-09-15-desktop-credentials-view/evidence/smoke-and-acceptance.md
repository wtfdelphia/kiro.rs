# 冒烟与验收证据（change 4 desktop-credentials-view）

日期：2026-09-11，分支 `dev-desktop`。

## 门禁命令（本会话真实运行）

| 命令 | 结果 |
| --- | --- |
| `desktop/`：`RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked` | 0 错误 0 告警 |
| `desktop/`：`cargo test --release` | 41 passed / 0 failed |
| 根仓：`cargo check --release --all-targets` | 0 告警（基线不变） |
| 根仓：`cargo test --release --all-targets` | 874 + 1 passed / 0 failed |
| `openspec validate desktop-credentials-view` | valid |

## Xvfb 冒烟（6.3）

`KIRO_RS_DATA_DIR=<临时目录> RUST_LOG=info timeout 20 xvfb-run -a target/release/kiro-desktop`，
默认 SQLite 模式，日志链完整：

1. 单实例锁获取成功；
2. 系统钥匙串探测失败（headless 无 Secret Service）→ 自动回退加密文件存储；
3. `SQLite 存储就绪（<dir>/kiro.db），secret 后端: 加密文件回退`；
4. `已加载 0 个凭据配置`（空库首启，符合预期）；
5. `主窗口已创建 (window_id=4294967297)`；
6. `核心装配完成，凭据数: 0`（事件桥 `Bootstrapped` 注入主视图路径生效）。

进程存活至 20 秒超时被终止（exit 124），无 panic、无崩溃。
数据目录产物：`kiro.db` + `-shm`/`-wal`（WAL 模式）、`secrets.key`（加密回退密钥）、
锁文件与日志文件。

## 测试覆盖说明（6.2 / 5.2 / 5.3）

- `table_rows_columns_and_cells_match_snapshot`（`#[gpui::test]`）：
  真实 `AdminService`（`MultiTokenManager::with_stores` + tempdir `SqliteStore`
  + 内存 secret 后端）装配两条凭据，headless 窗口内 `TableState::dump`
  断言 9 列 2 行、账号列单元格与快照 email 一致。
- `exec_toggle_and_priority_reload_reflects_changes`：启停与优先级经
  `CoreHandle::exec` 派 tokio，`query_credentials` 重查断言生效。
- `exec_delete_cleans_sqlite_row_and_secret`：删除前先禁用（服务端约束），
  删除后重查列表为空、`credential_count()==0`、钥匙串条目
  `cred-<id>:refresh_token` 同步清理。
- `dialogs::tests`：`validate_add_input` 必填校验、`ksk_` 前缀、
  认证方式推断（social / idc / api_key 优先级）纯函数断言。

## 剩余风险（6.5）

本环境无真实网络凭据，以下路径只有编译与接线证据，未做上游交互实测：

1. **凭据测试 / 余额查询 / 模型刷新**：按钮经 `exec` 接到
   `test_credential` / `get_balance` / `refresh_models`，错误分支经
   `AdminServiceError::Display` 透传通知；上游行为未实测。
2. **Builder ID / IAM SSO 登录**：start → 浏览器拉起 → 轮询/回调链路已接线，
   `open::that` 失败时 URL 走日志与通知保留；真实授权流未走通。
3. **删除确认对话框**的 `AlertDialog` 交互只在 headless 编译层验证，
   未做点击路径冒烟。

以上三项的实测留到具备网络凭据的环境补做。
