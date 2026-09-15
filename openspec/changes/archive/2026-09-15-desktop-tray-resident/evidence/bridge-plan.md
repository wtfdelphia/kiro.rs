# Bridge Plan: desktop-tray-resident

门禁：`openspec-superpowers-bridge`。

- change：`desktop-tray-resident`
- 分支：`dev-desktop`（HEAD `6048893`，工作区干净）
- 状态：`openspec status --change desktop-tray-resident --json`
  返回 isComplete（工件齐）、非 blocked

## 一、范围 / 非目标

范围：系统托盘（tray-icon 0.25 ksni 后端、四态图标、五项菜单）、
关窗常驻（最小化 + 唤回，`close_to_tray` 默认开）、开机自启
（三平台入口，macOS / Windows 仅编译验证）、设置视图「行为」分区、
preferences 表（schema v2）。

非目标：真实桌面下的托盘交互验证（Xvfb 无 SNI 宿主，只验证降级
路径）；占位窗口「彻底藏进托盘」（GPUI 无窗口隐藏接口，design D2
已论证）；图标位图资源引入（change 7 打包时处理）；托盘常驻与
`launch_at_login` 之外的会话管理。

## 二、关键设计决策（design.md 摘要）

- D1 托盘后端：`tray-icon = "0.25"` + `default-features = false`
  + `features = ["ksni"]`。0.25.0（2026-09-11 发版）新增纯 D-Bus
  StatusNotifierItem 后端，自管工作线程，无 GTK 依赖。推翻 change 2
  调研结论，证据：`evidence/tray-icon-0.25-ksni.md`
- D2 常驻模型：最小化 + 唤回，不销毁窗口（change 2 spike 证明
  Linux 零窗口即退出）。`gpui-pre` 0.3.4 有 `minimize_window()`
  （window.rs:6212）与 `activate_window()`（window.rs:6202）
- D3 图标四态运行时生成 22x22 RGBA；菜单整体重建（`set_menu`），
  不逐项更新
- D4 schema v2 新增 `preferences` 表，三键（`close_to_tray` 默认开、
  `launch_at_login` 默认关、`auto_start_server` 默认开），缺省不写库
- D5 开机自启：Linux 写 `~/.config/autostart/dev.kiro-rs.desktop`；
  macOS plist / Windows 注册表仅编译验证；内容生成抽纯函数可单测
- D6 事件桥不扩枚举；托盘「退出」等四入口统一收敛 `quit::begin_phase1`

## 三、风险与对策

| 风险 | 对策 |
| --- | --- |
| ksni `spawn()` 在无 SNI 宿主时返回 `Err` | `TrayIcon::new` 失败降级记日志，返回 `None`，其余路径不受影响；Xvfb 冒烟走降级路径属预期 |
| `on_window_should_close` 返回 false 后最小化在某些 WM 下视觉异常 | 最小化走标准 `WM_CHANGE_STATE`，唤回走 `_NET_ACTIVE_WINDOW`；冒烟用 xlib 断言窗口状态 |
| muda 菜单快照与 ksni watcher 线程的刷新时序 | 整体重建（`set_menu`）而非逐项更新（design D3） |
| 托盘轮询循环泄漏或阻塞主线程 | 200ms `cx.background_executor().timer` 轮询，`try_recv` 非阻塞；退出时随 `TrayIcon::drop` 结束 |
| `launch_at_login` 写平台入口失败 | 写入口成功才落库；失败记日志并通知 |

## 四、CodeGraph 证据与补盲

- `codegraph status`：索引覆盖根仓 204 文件；`codegraph query
  "quit begin_phase1"` 无结果，证实桌面独立 workspace 不在索引内。
  本 change 全部代码影响在 `desktop/`，补盲以 `rg` 与源码精读为准
- `rg` 补盲结论：
  - `desktop/Cargo.lock` 无 muda / tray-icon 存量，新增依赖不产生
    版本协调风险
  - 关窗拦截现状：`main.rs:258-270`（change 5 留注释「change 6 会把
    这里改成常驻时最小化」）
  - 自动启动现状：`main.rs:173-177`（`server.start(&addr)` 无条件）
  - 迁移模式：`store/schema.rs` 的 `SCHEMA_VERSION` + `current < N`
    递增写法可直接照抄到 v2
  - 设置视图分区模式：`ui/settings.rs` 的 `section()` + `row()` +
    `Switch::new(...).checked(...).on_click(cx.listener(...))`，
    headless 测试经 a11y 标签断言分区在场（现有六分区测试可照抄扩展）
  - 存储单写连接模式：`SqliteStore.conn: Mutex<Connection>`，
    preferences 读写沿用同锁
- 环境事实：系统无 openbox/fluxbox 等窗口管理器，Xvfb 冒烟时
  `minimize_window` 发出的 `WM_CHANGE_STATE` 没有 WM 响应，视觉
 最小化无法断言；冒烟改断言「窗口未销毁 + 进程存活 + 服务器存活」，
  唤回动作断言「`activate_window` 调用路径执行（日志）」与后续真实
  桌面手测

## 五、任务到执行步骤

| 任务 | 执行步骤 | 验证 |
| --- | --- | --- |
| 1.1 schema v2 | `store/schema.rs`：`SCHEMA_VERSION = 2`，`current < 2` 时建 `preferences` 表 | 现有三迁移测试 + 新增 |
| 1.2 preferences 读写 | `store/mod.rs`：`get_preference(key) -> anyhow::Result<bool>`（缺省兜底）/ `set_preference(key, bool)` | 单测缺省、写入读出 |
| 1.3 迁移幂等 | 沿用 `migrate_is_idempotent` 模式，v1→v2 重开不丢数据 | `cargo test --release` |
| 2.1 依赖 | `desktop/crates/kiro-desktop/Cargo.toml`：tray-icon 0.25 ksni；`cargo update` 落锁 | `cargo check` 零告警 |
| 2.2 托盘模块 | 新建 `src/tray.rs`：`TrayState`（图标四态 + 菜单构建）、`init` 返回 `Option<TrayIcon>` | 图标单测 + 降级单测 |
| 2.3 事件轮询 | 主事件循环旁挂 200ms 轮询：`TrayIconEvent` 单击唤回、`MenuEvent` 分发 | 无 SNI 环境手动启动验证降级日志；`dbus-run-session` 真实注册测试 |
| 2.4 句柄挂接 | `CoreHandle` 挂 `Arc<Mutex<Option<TrayIcon>>>`；`Bootstrapped` 后创建；`ServerStatus` 驱动重建 | 冒烟日志 |
| 2.5 托盘单测 | 图标尺寸/格式；`dbus-run-session` 下注册成功与失败降级 | `cargo test --release` |
| 3.1 关窗拦截 | `main.rs:258`：退出中放行；`close_to_tray` 开则 `minimize_window()` + false；关则进阶段 1 | Xvfb 冒烟 + 单测偏好读取 |
| 3.2 唤回 | `activate_window()`，托盘单击与「显示主窗口」共用 | 冒烟日志 |
| 3.3 自动启动开关 | `main.rs:173` 前读 `auto_start_server`，关则跳过 | 冒烟 |
| 4.1/4.2 开机自启 | 新建 `src/autostart.rs`：纯函数生成内容 + 三平台写删（cfg 门控） | 单测内容字段 |
| 5.1 行为分区 | `ui/settings.rs`：三 Switch，直读 preferences，自启开关联动平台入口 | headless 测试 |
| 5.2 headless | 扩展现有分区测试到七个分区标题 | `cargo test --release` |
| 6.1 门禁 | 桌面 `RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked` + `cargo test --release`；根仓 `cargo check --release --all-targets` 确认（预期零改动） | 零告警 |
| 6.2 validate | `openspec validate desktop-tray-resident` | 通过 |
| 6.3 冒烟 | Xvfb：启动 → 关窗（常驻）→ 进程/窗口/服务器存活 → 唤回动作 → 两阶段退出；降级日志可证 | 脚本证据留档 |
| 6.4 文档回填 | `docs/desktop-gpui-embedded-design.md` §6 | humanizer-zh 过检 |

## 六、必跑验证

1. `cd desktop && PKG_CONFIG_PATH=... LIBRARY_PATH=... RUSTFLAGS="-D
   warnings" cargo check --release --all-targets --locked`（零告警）
2. `cargo test --release`（桌面 workspace）
3. 根仓 `cargo check --release --all-targets`（零根仓改动确认）
4. `openspec validate desktop-tray-resident`
5. Xvfb 冒烟脚本（复用 `/tmp/smoke5/inject.py`、`ocr.py`）
6. 提交前 `git status --short`，确认无 `.codegraph/` 与凭据混入

## 七、README / spec 同步判断

- `desktop/README.md`：里程碑表第 6 行勾选 + 托盘/常驻能力一句话，
  任务 6.4 一并处理（影响启动行为描述）
- 根 `README.md`：桌面小节已指向 `desktop/README.md`，无需改
- `docs/desktop-gpui-embedded-design.md` §6：回填 0.25 ksni 与
  最小化常驻的落定结论（修订原设计选型）
- 主 specs：`skip_specs: true`，无同步

## 八、停止条件

- 托盘创建失败导致进程退出而非降级（必须 `None` 降级）
- 关窗拦截在 `close_to_tray` 关时不能走两阶段退出（回归 change 5
  语义即停）
- 桌面门禁出现新增告警且无法归因
- 冒烟中服务器在常驻期间停止服务（常驻语义被破坏）
