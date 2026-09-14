# 调研记录：tray-icon 0.25.0 的 ksni 后端（推翻 change 2 结论）

日期：2026-09-15
方法：crates.io API + crates.io 源码包下载（`/tmp/trayicon-src/`）

## 事实

1. `tray-icon` 0.25.0 发版于 2026-09-11（crates.io API
   `updated_at` / `created_at`），累计下载约 2873 万。
2. features（crates.io API 逐版本核对 0.25.0）：
   `ksni = ["dep:ksni", "muda-snapshot"]`。change 2 调研时（2026-09-11，
   0.24.2）逐个核对 0.20–0.24.2 均无此 feature；0.25.0 同日发版补上。
3. 后端门控（源码包 `src/platform_impl/mod.rs`）：Linux/BSD 上
   `feature = "ksni"` 即选 `ksni/mod.rs`；`libappindicator` 且非
   `ksni` 才选 `gtk/mod.rs`。
4. ksni 实现（`src/platform_impl/ksni/mod.rs`）：`spawn()` 跑
   `ksni::blocking` D-Bus 服务（独立线程），菜单另起
   `tray-icon-menu-watcher` 线程。创建与事件读写对调用线程无事件
   循环要求（对比 gtk 后端要求同线程有 gtk 事件循环）。
5. 事件通道不变：`TrayIconEvent::receiver()` / `MenuEvent::receiver()`
   全局 `crossbeam_channel`（`src/lib.rs:732` 与 muda
   `menu_event.rs:22`）。
6. ksni 0.3.6（0.25.0 的依赖）：`assume_sni_available(true)` 时
   无 `org.kde.StatusNotifierWatcher` 走 `Tray::watcher_offline` 软
   错误而非 `spawn()` 失败（`src/blocking.rs:73`、`src/service.rs:86`）。
   默认 `false` 时 `spawn()` 返回 `Err`。

## 结论

change 6 直接采用 `tray-icon 0.25 + ksni feature`，不引 GTK3 依赖
树，不走 change 2 记录的方向 a（GTK 线程）与方向 b（裸用 `ksni`
crate 自画菜单）：方向 b 的菜单协议集成由上游官方完成。

## 与 change 2 记录的关系

`openspec/changes/desktop-app-shell/evidence/tray-icon-research.md`
的「必须在三个方向里重选」结论在其调研时点成立，本记录为 0.25.0
发版后的重选结论。
