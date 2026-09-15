# Proposal: desktop-app-shell

桌面版（路线 B，`docs/desktop-gpui-embedded-design.md` v2.1）的第二步：在仓库内
新建 `desktop/` 独立 workspace，搭出可启动的应用骨架：双运行时布局、跨运行时
事件桥、数据目录与单实例锁、主窗口 + sidebar + 四个空视图，并完成 §6.5 两个
常驻前提的 spike 验证。不实现任何业务视图逻辑、存储层与托盘常驻。

## Why

- 设计文档 §12 的七个 change 里，change 2 是全部桌面 UI 的承重墙：双运行时
  模型（GPUI 主线程 + tokio 后台）与跨运行时桥接规则（§4.1/§4.2）一旦被后续
  change 各自实现一遍，就会出现两套事件流语义。骨架先立住，后续 change 往
  骨架里填视图。
- §6.5 的两个常驻前提（零窗口时 GPUI 应用循环是否存活、Linux 上 tray-icon 事件
  能否送达）是整个常驻模型的地基，设计文档明确要求放进 change 2 验收，结论
  决定 change 6 的实现方式。
- `gpui-pre` 是 2026-09 才发布的年轻快照（Zed `zed@6916400`），「在本项目环境
  能否编译、能否在无显示服务器上用 Xvfb 跑起来」是最高风险项，必须在骨架
  阶段排掉，不能拖到视图实现阶段。

## What Changes

- 新增 `desktop/` 独立 workspace（不进根 workspace，见设计文档 §3.1）：
  `desktop/Cargo.toml`（workspace 根）+ `desktop/crates/kiro-desktop/`
  （应用 crate），以 path 依赖 `kiro-rs` lib。
- 双运行时：`tokio::runtime::Builder::new_multi_thread` 在进 `run()` 前建好，
  GPUI `application().run()` 占用主线程。
- 跨运行时桥：`CoreEvent` 枚举 + `futures::channel`/`mpsc` 单向事件流
  （tokio 侧产生，GPUI 侧消费），边界规则按 §4.2。
- 数据目录解析（§9.1：`<data_dir>/kiro-rs/`）与单实例锁（§9.2：
  `kiro-desktop.lock` 文件锁，本期只做「拿不到锁就提示并退出」）。
- 启动装配经 change 1 的 `kiro_rs::bootstrap`，存储走
  `JsonCredentialStore` / `JsonConfigStore`（§9.3 的 `--config` /
  `--credentials` 调试路径）。
- 主窗口 + `AppTitleBar` + sidebar（icon 折叠模式）+ 总览/凭据/设置/服务四个
  空视图（占位内容，无数据交互）。
- 日志：`tracing_subscriber` 输出到 `<data_dir>/kiro-rs/kiro-desktop.log`，
  保留终端输出（§6.4）。
- spike：零窗口存活、tray-icon 事件送达（KSNI）两项各留验证记录。
- 根仓零改动：不改根 `Cargo.toml`、不改任何根侧源码与流水线。

## Capabilities

本变更新增一个独立的桌面应用目标，不改变 kiro-rs 服务端任何需求级行为。

### New Capabilities

无（应用骨架，无可被规格化的需求行为）。

### Modified Capabilities

无。

## Impact

- 代码：`desktop/`（全部新增）；根仓只新增 `openspec/changes/desktop-app-shell/`
  工件，不改任何源码。
- 依赖：`desktop/` 内引入 `gpui-kit`、`tokio`、`futures`、`anyhow`、
  `tracing`、`tracing-subscriber`、`clap`；根仓依赖零变化。
- 构建：`desktop/` 自带独立 `Cargo.lock` 与告警判定（`cargo check
  --release --all-targets --locked` + `-D warnings`），不借用根门禁；
  根侧流水线与告警门禁零改动。
- 环境：本机 GPUI Linux 构建缺 `libxkbcommon-x11-dev`、`libasound2-dev`、
  `libzstd-dev`、`libx11-xcb-dev`、`libxcb-xkb-dev`、`libvulkan-dev`，
  无 root 环境下用 `apt-get download` + 本地 sysroot（pkg-config 前缀改写）
  解决，方法固化进 `desktop/README.md`。
- 后续：change 3（SQLite 存储）起替换 JSON store；change 4-6 在本骨架上填
  视图、托盘与常驻。
