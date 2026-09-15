# Design: desktop-review-followups

四个独立修复，逐项给出现状、目标实现与取舍。

## D1 单实例锁：先截断再写 PID

现状：`OpenOptions::new().create(true).write(true)` 打开后加
`try_lock`，成功即 `write!(f, "{}", pid)`。文件不会被截断，上次运行
留下的字节仍在。

修法：

- 不在 `open` 时加 `.truncate(true)`：那会在另一个进程持锁时也把
  它的 PID 抹掉，破坏「锁文件可读出现有实例进程号」的排查用途。
- 拿到锁之后（确认自己是唯一持有者）再 `file.set_len(0)`，然后写
  PID。`set_len` 失败按 `Err(Some(e))` 上抛（与写失败不同：写失败
  现在就是 `let _ =` 忽略，截断失败属于拿锁过程的 IO 错误）。

测试：构造一个预写长内容的锁文件，拿锁后断言文件内容恰好等于当前
PID，无残留。

## D2 write_file_0600：创建即 0600

现状：`fs::File::create`（默认 0644，受 umask 影响）写完后
`set_permissions`。两次调用之间文件对同机其他用户可读。

修法：Unix 分支用 `std::os::unix::fs::OpenOptionsExt::mode(0o600)`：

```rust
let mut f = fs::OpenOptions::new()
    .create(true)
    .write(true)
    .truncate(true)
    .mode(0o600)
    .open(path)?;
```

`mode` 只在创建时生效，文件已存在时保留原权限，随后 `set_permissions`
兜底把已有文件也收紧到 0600。非 Unix 保持 `File::create`（桌面端
本期目标平台是 Linux，非 Unix 分支只是不让编译断）。

密钥文件 `secrets.key` 与密文 `secrets.enc` 都经此函数落盘，一处修复
覆盖两处。

## D3 首启导入可见性

行为不改：首启且库为空时，探测数据目录与 cwd 的 `config.json` /
`credentials.json`，导入成功后源文件改名 `.bak`。

补充：

- 每次成功导入单独 `tracing::info!("已导入{凭据|配置} {}", path)`，
  不再只有末尾汇总。
- 来源路径不在数据目录之下（即 cwd 命中）时额外 `tracing::warn!`
  提示「来源在数据目录之外，已备份为 .bak」。
- README「常用命令」后已有的一段导入说明改写为明确列出两个探测目录、
  改名行为与数据目录覆盖方式。

## D4 删除凭据前自动禁用

服务端约束：`token_manager::delete_credential` 要求
`entry.disabled == true`，否则 `bail!`。

修法：删除确认对话框的 `on_ok` 里，把派往 tokio 的 future 改为

```rust
service.set_disabled(id, true).map_err(|e| e.to_string())?;
service.delete_credential(id).map_err(|e| e.to_string())
```

两个调用都是同步的，已经在 `exec` 的 tokio 侧执行，没有线程问题。
`AdminService::set_disabled(id, true)` 在禁用的是当前凭据时会
`switch_to_next`，这正是删除启用中凭据想要的结果；`disabled=false`
分支才有的 `spawn_refresh_models_arc` 不会触发。

取舍：`set_disabled(true)` 会把 `disabled_reason` 置为 `Manual` 并
持久化。若随后的 `delete_credential` 失败（持久化失败等小概率），
凭据停留在手动禁用态。用户从错误通知可知原因，表格开关可重新启用；
相比「删不掉还要求用户手动禁用一轮」，失败残留禁用态是更小的恶。
对话框描述文案补一句「启用中的凭据将先禁用再删除」。

## 验证策略

- 桌面门禁：`desktop/` 下 `RUSTFLAGS="-D warnings" cargo check
  --release --all-targets --locked` 零告警。
- 桌面测试：`cargo test --release`（含新增测试）。
- 根仓回归：根目录 `cargo check --release --all-targets` 零告警
  （本次未动根源码，确认不回归）。
- 冒烟：Xvfb + 临时 `KIRO_RS_DATA_DIR` 启动一次，确认锁文件内容
  与启动链路正常。

## 回滚

四个修复互相独立，单提交回滚即可。
