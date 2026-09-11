# 调研记录：tray-icon 的 Linux 后端（推翻设计文档 §6.1 的 ksni 假设）

日期：2026-09-11

## 结论

设计文档 §6.1 写「Linux 用 `ksni` feature（StatusNotifierItem D-Bus 后端），
关掉默认的 `libappindicator`，避免引入 GTK3 依赖树」。**这个方案在当前
版本不成立**：

- `tray-icon` 0.24.2（2026-07-27，docs.rs 页面原文）：「Platforms supported:
  Windows, macOS, Linux (**gtk Only**)」，且「On Windows and Linux, an event
  loop must be running on the thread... on Linux, a **gtk event loop**」
- crates.io API 逐个核对 0.20.0 / 0.21.3 / 0.22.2 / 0.23.0 / 0.23.1 /
  0.24.0-0.24.2 的 features：全部只有 `common-controls-v6`、`libxdo`、
  `serde`（0.24 起多一个 `gtk`，指向 `muda/gtk` + `dep:libappindicator`），
  **没有任何版本有 `ksni` feature**
- Linux 依赖（官方文档）：`libgtk-3-dev`、`libxdo-dev`、
  `libappindicator-gtk3`（或 ayatana）

## 对后续 change 的影响

1. change 6（托盘常驻）必须引入 GTK3 依赖树（`default = ["libxdo", "gtk"]`），
   Linux 上 `tray-icon` 没有不依赖 GTK 的官方后端
2. 事件模型也要重新设计：`tray-icon` 要求「同线程有 gtk 事件循环」，而
   GPUI 主线程跑的是自己的循环。可选路径（change 6 立项时钉死）：
   a. 独立 gtk 线程 + `gtk::init` + 事件转发回 GPUI（README 的
      `EventLoopProxy` 模式），需验证与 GPUI 共存的可行性
   b. 不用 `tray-icon`，直接用 `ksni` crate 实现 StatusNotifierItem
      （纯 D-Bus，无 GTK），自己画菜单（ksni 自带 SNI 菜单协议支持）
   c. 接受 GTK3，独立线程跑 `gtk::main_iteration` 轮询
3. 设计文档 §6.1 的选型段与 §6.2 的事件读取方式（全局 `receiver()` 轮询）
   需在 change 6 立项时按实测结果回填

## 证据来源

- `https://docs.rs/tray-icon/0.24.2/tray_icon/`（2026-09-11 抓取，存档
  `/tmp/tray-docs2.html`，本文件摘录关键句）
- `https://crates.io/api/v1/crates/tray-icon/<version>`（features 字段逐版本）
- 设计文档原始调研引用的 `src/lib.rs:678-704`（receiver 轮询）属于较早版本，
  API 形态需以 0.24.2 为准重新确认
