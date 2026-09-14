# Design: desktop-tray-resident

系统托盘、关窗常驻、开机自启。逐决策给出现状、目标实现与取舍。

## D1 托盘后端：tray-icon 0.25 ksni（推翻 change 2 调研结论）

现状：设计文档 §6.1 选型 `tray-icon`，但 change 2 调研（2026-09-11）
确认 0.20–0.24 全系无 `ksni` feature，Linux 只有 GTK 后端，留了三个
重选方向（a 接受 GTK3、b 直接用 `ksni` crate、c 混合）。

新事实（2026-09-15 核实，证据见 `evidence/tray-icon-0.25-ksni.md`）：

- `tray-icon` 0.25.0 于 2026-09-11 发版（crates.io API），features 新增
  `ksni = ["dep:ksni", "muda-snapshot"]`；`platform_impl/mod.rs` 的后端
  门控：Linux 上 `ksni` feature 存在即用 `ksni/mod.rs`，否则用
  `gtk/mod.rs`（两者同时开启时 ksni 优先）
- ksni 后端（`platform_impl/ksni/mod.rs`，0.25.0 源码）：
  `StatusNotifierTray::spawn()` 在独立线程跑 D-Bus 服务
  （`ksni::blocking::Handle`），菜单另起 `tray-icon-menu-watcher`
  线程盯 `MenuChangeEvent`。创建与事件读写对调用线程无事件循环要求
- 事件回传不变：`TrayIconEvent::receiver()` / `MenuEvent::receiver()`
  全局 `crossbeam_channel`，GPUI 侧 200ms 轮询（设计文档 §6.2 模型
  原样可用）
- `ksni` 0.3.6（tray-icon 0.25 的依赖，源码核实）：无
  `org.kde.StatusNotifierWatcher` 时，`assume_sni_available(false)`
  下 `spawn()` 直接返回 `Err`；置 `true` 则走 `watcher_offline` 软
  错误，宿主出现后 `watcher_online` 恢复

决策：`tray-icon = "0.25"`，`default-features = false`，
`features = ["ksni"]`。不引 GTK3、libxdo、libappindicator；macOS /
Windows 各自走 crate 原生后端（同一份代码）。

无宿主降级：`TrayIcon::new` 失败记日志，托盘功能缺席，其余路径
（常驻、退出、服务器）不受影响。不用 `assume_sni_available(true)`：
桌面应用启动时 DE 通常已就绪，真缺 SNI 的环境宁可明示降级也不挂一个
永不显示的服务。

## D2 常驻模型：最小化 + 唤回（修订设计文档 §6.3）

现状：§6.3 设计「关窗拦截 → `remove_window()` 销毁 → 托盘唤回时
`cx.open_window` 重建」。change 2 spike 实测（`evidence/spike-zero-window.md`）：
Linux 上销毁最后一个窗口后 `run()` 立即返回，「销毁 + 重建」在
Linux 不成立。spike 记录列的兜底方向 a「占位窗口」需要额外维护一个
不可见窗口的生命周期与资源。

新事实（2026-09-15 源码核实）：`gpui-pre` 0.3.4 `Window` 有
`minimize_window()`（`window.rs:6212`）与 `activate_window()`
（`window.rs:6202`）；X11 实现分别走 `WM_CHANGE_STATE` 与
`_NET_ACTIVE_WINDOW`（`gpui-pre-linux` x11/window.rs），都是标准协议
消息，不依赖窗口管理器之外的设施。

决策：常驻不销毁窗口，改最小化：

1. `on_window_should_close`：已在退出流程返回 `true`；否则读
   `close_to_tray` 设置（SQLite preferences，默认开）——开：
   `window.minimize_window()` 后返回 `false`，进程与服务器继续运行；
   关：走两阶段退出（返回 `false`，阶段 2 才真正退出，语义同 change 5）
2. 托盘单击或「显示主窗口」：`activate_window()`（`_NET_ACTIVE_WINDOW`
   会把最小化窗口恢复并聚焦）
3. 窗口始终存在，不存在重建路径；视图状态天然保留，零恢复逻辑

取舍：最小化窗口在任务栏仍占位（没有「彻底藏进托盘」的观感）。
GPUI 无窗口隐藏接口（设计文档 §6.3 已确认 94 个方法逐一检查），
占位窗口方案复杂度高且收益只有视觉，本期不做。

## D3 托盘状态与菜单

图标四态（运行中/已停止/失败/退出中）运行时生成 22x22 RGBA
（纯色圆点 + 透明底），不经 `gpui-kit-assets`：托盘协议要位图，
引图片资源是 change 7 打包的事，本期用生成图占住语义。

菜单（muda，设计文档 §6.3 结构）：

```text
kiro-rs · 服务器: 运行中 (127.0.0.1:8080)   ← 标题行，禁用态
显示主窗口
启动服务器 / 停止服务器                       ← 按状态二选一
☑ 开机自启
─────────
退出
```

菜单刷新策略：服务器状态变化时整体重建（`TrayIcon::set_menu`），
不逐项 `set_checked` / `set_text`——重建语义确定，避开 muda 快照
与 ksni watcher 线程的刷新时序问题（change 6 范围内菜单只有五个
项，重建成本可忽略）。

托盘对象生命周期：装配完成后创建，状态经 `CoreEvent::ServerStatus`
驱动刷新；进程退出随 `TrayIcon::drop`（shutdown D-Bus 服务 + join
watcher 线程）。

实现修正（2026-09-15）：`tray-icon` 顶层 `TrayIcon` 内部是
`Rc<RefCell<…>>`（`!Send`），而 `CoreHandle` 必须 `Send`（从 tokio
侧经事件通道送到 GPUI 侧），“挂 `CoreHandle`”编译不成立。实际挂法：
`Tray` 驻留主线程事件循环（`cx.spawn` 闭包，`boxed_local` 非 Send
语义），`Bootstrapped` 后经 `attach` 注入动作上下文（`CoreHandle` +
事件发送端 + 窗口句柄）；事件经 200ms tick 与核心事件 `select!`
交织，`try_recv` 非阻塞。动作上下文装配前到达的菜单事件被忽略。

## D4 设置项与 preferences 存储

schema v2 新增 `preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL)`。
三个键，缺省值不写库、读时兜底：

| 键 | 类型 | 默认 | 作用 |
| --- | --- | --- | --- |
| `close_to_tray` | bool | 开 | 关窗时最小化常驻；关则走两阶段退出 |
| `launch_at_login` | bool | 关 | 开机自启（写平台入口，见 D5） |
| `auto_start_server` | bool | 开 | 装配成功后自动启动内嵌服务器 |

装配接线：`auto_start_server` 关时跳过 `server.start(&addr)`（当前
无条件自动启动）。`close_to_tray` 与 `launch_at_login` 的开关入口在
设置视图新增「行为」分区（三个 Switch）；`launch_at_login` 写开关
的同时同步平台入口（成功才落库）。

## D5 开机自启（三平台入口）

纯文件操作，无需系统权限（设计文档 §6.4）：

- Linux：`~/.config/autostart/dev.kiro-rs.desktop`（desktop entry，
  `Exec` 为当前可执行文件路径，`X-GNOME-Autostart-enabled=true`）
- macOS：`~/Library/LaunchAgents/dev.kiro-rs.desktop.plist`
  （`RunAtLoad`），cfg 门控，本环境只编译验证
- Windows：注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
  的 `kiro-rs` 键，`winreg` crate（target-gated 依赖），本环境只编译
  验证

内容生成抽纯函数（给路径与程序名产出文件内容），单测覆盖；写删失败
只报通知不崩溃。

## D6 事件桥与退出

事件桥不扩枚举：托盘是状态消费者（订阅现有 `ServerStatus`）与动作
发起者（菜单事件翻译成 `start` / `request_stop` / `begin_phase1` /
唤回）。托盘「退出」与 Cmd+Q / Alt+F4 / 关窗（`close_to_tray` 关）
四入口统一收敛到 `quit::begin_phase1`，`QUITTING` 去重不变。

## 验证策略

- 单测：preferences 读写与缺省、desktop entry / plist 内容生成、
  托盘图标 RGBA 尺寸与格式、无 SNI 宿主时 `TrayIcon::new` 失败可
  降级（`dbus-run-session` 下真实 D-Bus 会话）
- headless：设置视图「行为」分区渲染（扩 `settings` 现有测试）
- Xvfb 冒烟：启动 → 关窗（`close_to_tray` 开）→ 进程存活、窗口
  存活、服务器继续服务 → 托盘单击/菜单「显示主窗口」唤回动作生效 →
  托盘「退出」或 Alt+F4 两阶段退出。Xvfb 无 SNI 宿主，托盘注册走
  降级路径（日志可证），真实桌面托盘交互留手测记录
- 门禁：桌面 `RUSTFLAGS="-D warnings" cargo check --release
  --all-targets --locked` 零告警；根仓门禁不受影响（零根仓代码改动
  时只跑确认）

## 回滚

单提交回滚。preferences 表是新增表，回滚后残留无副作用；自启入口
文件在回滚前需手动关闭设置项一次（或直接删文件），写入路径幂等。
