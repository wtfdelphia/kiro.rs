# Tasks: desktop-review-followups

## 1. 单实例锁

- [x] 1.1 `lock.rs`：拿锁后先 `set_len(0)` 再写 PID，截断失败按
      `Err(Some(e))` 上抛
- [x] 1.2 新增测试：预写长内容后拿锁，断言内容恰好等于当前 PID

## 2. 加密文件权限

- [x] 2.1 `store/crypt.rs`：Unix 分支 `write_file_0600` 改
      `OpenOptions::mode(0o600)` 创建，保留 `set_permissions` 兜底；
      非 Unix 走 `File::create`
- [x] 2.2 现有 0600 权限测试保持绿

## 3. 首启导入可见性

- [x] 3.1 `store/json_io.rs`：逐条成功导入记 `info!` 来源路径；
      来源在数据目录之外时记 `warn!`
- [x] 3.2 `desktop/README.md`：补充探测目录（数据目录 + cwd）、
      `.bak` 改名与 `KIRO_RS_DATA_DIR` 覆盖说明

## 4. 删除前自动禁用

- [x] 4.1 `ui/credentials/dialogs.rs`：`open_delete_confirm` 的
      `on_ok` 改为先 `set_disabled(id, true)` 再 `delete_credential`
- [x] 4.2 对话框文案注明「将先禁用再删除」
- [x] 4.3 补服务层测试：启用中的凭据可经「先禁用再删除」路径删除

## 5. 验证

- [x] 5.1 桌面门禁 `cargo check --release --all-targets --locked`
      （`RUSTFLAGS="-D warnings"`）零告警
- [x] 5.2 桌面 `cargo test --release` 全绿
- [x] 5.3 根仓 `cargo check --release --all-targets` 零告警回归
- [x] 5.4 `openspec validate desktop-review-followups` 通过
- [x] 5.5 Xvfb 冒烟：临时数据目录启动正常，锁文件内容为当前 PID
