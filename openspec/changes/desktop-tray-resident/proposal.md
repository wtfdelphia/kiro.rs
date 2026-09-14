# Proposal: desktop-tray-resident

桌面版（路线 B，`docs/desktop-gpui-embedded-design.md` §12 里程碑 6）：
系统托盘、关窗常驻、开机自启。完成后桌面应用具备常驻后台的运行形态：
关窗不退出、托盘唤回、状态可见、随系统启动。

## Why

- 桌面代理的价值形态就是常驻：用户关掉管理窗口不等于要停掉代理。
  目前关窗即触发两阶段退出，代理随之停止，与「桌面版 = 代理本体 +
  管理界面」的定位不符。
- 设计文档 §6 在 change 2 时留了两个未验证前提（零窗口存活、
  托盘事件送达），本 change 用实测结论定案并落地。
- 上游事实变化：`tray-icon` 0.25.0（2026-09-11 发版）新增 `ksni`
  feature，Linux 有了纯 D-Bus 后端，不需要 GTK3 依赖树。change 2
  调研记录的「必须在三个方向里重选」由此收敛：选 ksni。

## What Changes

- 桌面 `tray.rs`（新增）：`tray-icon = "0.25"`，`default-features = false`
  + `ksni` feature。托盘图标随服务器状态切四态（运行中/已停止/失败/
  退出中，运行时生成 RGBA，不引图片资源）；菜单：标题状态行、
  显示主窗口、启动/停止服务器、开机自启勾选、退出。事件经
  `TrayIconEvent::receiver()` / `MenuEvent::receiver()` 轮询
  （200ms，设计文档 §6.2 模型不变；ksni 后端自管 D-Bus 工作线程，
  无事件循环约束）。
- 常驻模型（修订设计文档 §6.3）：change 2 实测 Linux 零窗口即退出，
  「销毁 + 重建」不成立。改为「最小化 + 唤回」：关窗拦截读
  `close_to_tray` 设置，开则 `minimize_window()` 并保留窗口，进程
  与服务器继续运行；托盘单击或「显示主窗口」经 `activate_window()`
  唤回。`close_to_tray` 关时关窗走两阶段退出。Cmd+Q / Alt+F4 /
  托盘「退出」始终走两阶段退出。
- 桌面 `store`：schema v2 新增 `preferences` 表（`key` / `value`），
  存 `close_to_tray`（默认开）、`launch_at_login`（默认关）、
  `auto_start_server`（默认开）。
- 桌面 `autostart.rs`（新增）：开机自启三平台入口。Linux 写
  `~/.config/autostart/` 下的 desktop entry；macOS 写
  `~/Library/LaunchAgents/` 下的 plist；Windows 写注册表 `Run` 键
  （`winreg`，target 级依赖）。设置开关即写即删。
- 桌面 `ui/settings.rs`：新增「行为」分区，三个开关对应上述设置。
- 装配接线：`auto_start_server` 关时装配成功后不自动启动服务器
  （当前行为是无条件自动启动）。
- 降级：无 SNI 宿主（`org.kde.StatusNotifierWatcher` 缺席）时
  `TrayIcon::new` 报错，记日志后继续运行，常驻与退出路径不受影响。

## Capabilities

桌面端进程治理与系统集成，服务端代码零改动。

## Out of Scope

- macOS Dock 图标隐藏（GPUI 无 activation policy API，设计文档已记）
- 精修图标资源（随 change 7 打包进 `desktop/assets/`）
- Windows / macOS 实测（本机仅 Linux，这两个平台只到编译验证）

## Risks

- muda 菜单勾选态刷新依赖快照机制，若 `set_checked` 不触发托盘刷新
  则改整体重建菜单（实现时验证）
- Xvfb 无窗口管理器，最小化/唤回的平台效果有限，验证以「关窗后
  进程与窗口存活、托盘事件驱动唤回动作」为准，真实桌面效果留给
  后续手测
- 无 SNI 宿主的环境（部分极简桌面）托盘不可见，只能靠日志与
  两阶段退出兜底
