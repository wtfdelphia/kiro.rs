# Tasks: desktop-app-shell

## 1. 前置排险

- [x] 1.1 `/tmp` 编译 spike：最小依赖 `gpui-kit` 的 crate 在本机（无 root、
      sysroot pkg-config）`cargo build` 通过，证据存档
- [x] 1.2 运行环境探测结论存档：`Xvfb` 可用性、Vulkan ICD（lvp）、缺失
      `-dev` 包清单与 sysroot 解法

## 2. workspace 骨架

- [x] 2.1 新增 `desktop/Cargo.toml`（workspace 根）与
      `desktop/crates/kiro-desktop/Cargo.toml`（path 依赖 `kiro-rs`），
      生成独立 `Cargo.lock`
- [x] 2.2 `desktop/README.md`：构建方法、无 root sysroot 步骤、`admin-ui/dist`
      前置、根侧零影响说明

## 3. 应用骨架

- [x] 3.1 `main.rs` 双运行时：tokio runtime 建好后进 `gpui_kit::application().run()`
- [x] 3.2 `paths.rs` 数据目录解析（`<data_dir>/kiro-rs/`，环境变量覆盖）+ 单测
- [x] 3.3 `lock.rs` 单实例锁（flock，拿不到即提示退出）+ 单测（同进程/跨进程）
- [x] 3.4 `logging.rs` 文件 + 终端双写（`kiro-desktop.log`）
- [x] 3.5 `bridge.rs` `CoreEvent` + 通道 + GPUI 侧轮询消费 + 单测（往返）
- [x] 3.6 启动装配经 `kiro_rs::bootstrap`（JSON store，`--config`/
      `--credentials` 优先，失败进错误视图）
- [x] 3.7 `ui/`：Root + TitleBar + sidebar + 四占位视图，可切换

## 4. spike 与验收

- [x] 4.1 §6.5 spike 一：零窗口时应用循环是否存活（结论：Linux 上销毁最后一个
      窗口后 `run()` 立即返回，零窗口不存活；常驻需占位窗口或拦截不销毁兜底，
      见 `evidence/spike-zero-window.md`）
- [x] 4.2 §6.5 spike 二：tray-icon 事件送达（结论：设计文档 §6.1 的 ksni
      方案不成立：tray-icon 0.20-0.24 全系无 ksni feature，Linux 后端是
      "gtk Only"；且无头环境装不动 GTK3 dev 树、无 StatusNotifier 守护进程，
      事件送达验证延到有显示 + GTK 环境，见 `evidence/tray-icon-research.md`）
- [x] 4.3 启动验收：`xvfb-run` 下进程存活 ≥5 秒，日志含窗口创建与
      `Bootstrapped` 事件
- [x] 4.4 编译门禁：`desktop/` 内 `cargo check --release --all-targets
      --locked` + `-D warnings` 零告警；`cargo test --release`（desktop）全绿
- [x] 4.5 根侧零影响：根仓 `cargo check --release --all-targets` 告警数不变，
      `git status` 确认根仓源码无改动；`openspec validate desktop-app-shell` 通过
