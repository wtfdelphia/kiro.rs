# Bridge Plan: desktop-review-followups

门禁：`openspec-superpowers-bridge`。

- change：`desktop-review-followups`
- 分支：`dev-desktop`（HEAD `c633953`，工作区干净）
- 状态：审核结论的落地修复，无新增能力面

## 一、范围 / 非目标

范围：审核 42ec15a..HEAD 发现的四项：锁文件截断、加密文件创建权限、
首启导入日志可见性与 README、删除前自动禁用。

非目标：不改导入行为本身（仍探测数据目录与 cwd、仍改名 .bak）；不改
服务端删除约束；不新增桌面能力；不动根仓源码。

## 二、关键设计决策（design.md 摘要）

- D1：截断发生在拿锁之后，绝不在 open 时 `.truncate(true)`（会抹掉
  持锁实例的 PID）
- D2：`OpenOptionsExt::mode(0o600)` 让文件出生即 0600，
  `set_permissions` 兜底收紧已有文件
- D3：逐条导入 `info!` + 数据目录外来源 `warn!`，README 写清探测路径
- D4：删除 future 内串行「先禁用再删除」；失败残留禁用态的取舍已记录

## 三、风险与对策

| 风险 | 对策 |
| --- | --- |
| 截断引入新的 IO 失败路径 | 按 `Err(Some(e))` 上抛，与现有错误语义一致 |
| `set_disabled(true)` 持久化失败导致删除中断 | 错误经通知透传；凭据停留禁用态可手动恢复 |
| cwd 探测的 `warn!` 噪音 | 只在首启且库为空时触发一次 |

## 四、验证口径

- `desktop/`：`RUSTFLAGS="-D warnings" cargo check --release
  --all-targets --locked`、`cargo test --release`
- 根仓：`cargo check --release --all-targets`（未动根源码，确认不回归）
- 冒烟：Xvfb + 临时 `KIRO_RS_DATA_DIR` 启动，核对锁文件内容

## 五、规格映射

`skip_specs: true`。四项均为实现缺陷修复与可见性补充，不改变任何已
规格化的需求行为（与前四个 change 同口径）。
