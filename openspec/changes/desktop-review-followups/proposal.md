# Proposal: desktop-review-followups

修复桌面端前四个 change（lib 化、应用骨架、SQLite 存储、凭据视图）合入后
代码审核发现的四个问题。全部是局部修正，不改任何既定行为边界，根仓零改动
（除桌面 README 文案补充）。

## Why

审核范围 `42ec15a..HEAD`（5 个提交，66 个文件）发现：

1. 单实例锁写 PID 未截断旧内容。重启后新 PID 位数变短（12345 → 987）
   会残留旧字节，锁文件里出现 `98745`，排查时会误读进程号。
2. 加密文件回退的 `write_file_0600` 先用默认权限（0644）创建文件再
   `set_permissions(0o600)`，密钥与密文在落地瞬间有一个同机其他用户
   可读的窗口。
3. 首启 JSON 导入会探测数据目录和 cwd，命中后导入并把源文件改名
   `.bak`。行为本身是设计内的，但日志里没有逐条记录来源路径，cwd
   命中时也没有单独提示，README 没有说明这条路径，出问题时无从追溯。
4. 删除确认对话框直接调 `delete_credential`，而服务端要求凭据先禁用
   （`src/kiro/token_manager.rs` 的「只能删除已禁用的凭据」检查）。
   启用中的凭据点删除必然报错，用户要手动禁用再删一遍。

## What Changes

1. `lock.rs`：拿到锁之后先 `set_len(0)` 再写 PID；补「短 PID 覆盖长
   PID 后内容精确」的测试。
2. `store/crypt.rs`：Unix 下 `write_file_0600` 改用
   `OpenOptions::mode(0o600)` 创建，文件出生即 0600；保留
   `set_permissions` 兜底。非 Unix 走原路径。
3. `store/json_io.rs`：每次成功导入 `tracing::info!` 记录来源路径，
   来源在数据目录之外（cwd 命中）时额外 `tracing::warn!`；
   `desktop/README.md` 补充导入路径与 `.bak` 改名的说明。
4. `ui/credentials/dialogs.rs`：删除确认的 `on_ok` 改为先
   `set_disabled(id, true)` 再 `delete_credential(id)`，同一个
   `exec` 任务内完成；对话框文案注明「将先禁用再删除」。

## Capabilities

纯修复与可见性补充，服务端行为与需求不变。

### New Capabilities

无。

### Modified Capabilities

无。

## Impact

- 代码：`desktop/crates/kiro-desktop/` 下四个文件（`lock.rs`、
  `store/crypt.rs`、`store/json_io.rs`、`ui/credentials/dialogs.rs`）
  加 `desktop/README.md`。根仓源码零改动。
- 测试：`lock.rs` 新增短 PID 覆盖测试；`crypt.rs` 现有权限测试继续
  覆盖 0600；删除先禁用补服务层行为测试（走真实 `AdminService`）。
- 风险：删除前自动禁用会把目标凭据标记为 `DisabledReason::Manual`。
  删除成功时无感知（凭据已不存在）；删除失败时凭据停留在禁用态，
  错误通知会带出原因，用户可手动重新启用。这个取舍在 design 中记录。
- 后续：无依赖项，不阻塞 change 5（设置与服务器视图）。
