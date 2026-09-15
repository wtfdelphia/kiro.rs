# Design: desktop-app-shell

整体桌面架构见 `docs/desktop-gpui-embedded-design.md`（v2.1）。本文只覆盖本
变更（应用骨架）的实现层决策，以及 spike 验证的口径。

## Context

- change 1 已入库（`9323be7`）：`kiro-rs` 是 lib + bin 双目标，装配链在
  `kiro_rs::bootstrap::{bootstrap, build_routes}`，存储接缝是
  `kiro_rs::storage::{CredentialStore, ConfigStore}` + JSON 文件实现。
- 本机（Ubuntu 22.04，无 root，无 `DISPLAY`）GPUI Linux 构建所需的部分
  `-dev` 包缺失。已验证的替代路径：`apt-get download` 下载 `.deb` 解包为
  本地目录，把 `.pc` 文件的 `prefix` 改写为绝对路径后并入
  `PKG_CONFIG_PATH`（不用 `PKG_CONFIG_SYSROOT_DIR`，避免把系统库解析也
  指进 sysroot）。运行时库（`libasound.so.2`、`libxkbcommon.so.0` 等）系统
  已有。
- 显示环境：本机无 `DISPLAY`，有 `Xvfb`/`xvfb-run` 与 Vulkan ICD（含
  `lvp` 软渲染）。应用「可启动」的验收改为：Xvfb 下进程存活 + 日志出现
  窗口创建记录；不要求人工看到画面。

## Goals / Non-Goals

Goals:

- `desktop/` 独立 workspace 可编译（`cargo check --release --all-targets
  --locked`，`-D warnings` 等价口径零告警）。
- 应用可启动：Xvfb 下打开主窗口，sidebar 四视图可切换（headless 或日志
  证据），`bootstrap` 成功装配核心（`MultiTokenManager` 句柄可查询凭据数）。
- 单实例锁、数据目录、文件日志、跨运行时事件桥各有一条最小可运行路径与
  对应测试。
- §6.5 两个 spike 结论落进证据目录。

Non-Goals:

- 不引入 SQLite / keyring / rusqlite（change 3）。
- 不实现托盘与常驻（change 6）；spike 只做「事件能否送达」的探测，不做
  菜单与四态图标。
- 不实现任何视图数据交互（change 4/5）；四个视图是带标题的占位面板。
- 不做两阶段退出（change 5）；本期退出即 `cx.quit()`，直接退出语义。
- 不写桌面侧 CI 工作流（change 7 的三平台流水线统一建）；本期只在
  `desktop/README.md` 固化本地构建方法。
- 不改根仓任何源码、流水线、门禁。

## Decisions

### D1：workspace 布局

```text
desktop/
├── Cargo.toml          # [workspace] 根，成员 ["crates/*"]
├── Cargo.lock          # 独立锁文件，入库
├── README.md           # 构建方法（含无 root sysroot 说明）
└── crates/
    └── kiro-desktop/
        ├── Cargo.toml  # bin crate，path 依赖 ../../../ 的 kiro-rs lib
        └── src/
            ├── main.rs     # 双运行时 + run()
            ├── bridge.rs   # CoreEvent + 通道句柄
            ├── paths.rs    # 数据目录解析
            ├── lock.rs     # 单实例锁
            ├── logging.rs  # 文件 + 终端双写
            └── ui/
                ├── mod.rs
                ├── root.rs     # Root 布局
                ├── titlebar.rs # 服务器状态徽标占位
                ├── sidebar.rs  # 四视图切换
                └── views.rs    # 四个占位视图
```

备选是把骨架直接做成根仓内的示例或独立仓：否决。设计文档 §3.1 已论证
独立 workspace 是唯一不打穿根门禁的布局；独立仓则割裂 path 依赖与提交
原子性（一个 change 一个提交）。

### D2：双运行时与事件桥

`main()`：`tokio::runtime::Builder::new_multi_thread().enable_all()` →
`rt.handle().clone()` → `gpui_kit::application().run(move |cx| { ... })`。
`rt` 所有权留在 `main`，`run()` 返回后 drop（§4.1）。

桥的最小面（§4.2 的裁剪版，只保留骨架需要的）：

```rust
pub enum CoreEvent {
    Bootstrapped { credential_count: usize }, // 装配完成（tokio 侧 spawn 后回报）
    BootstrapFailed(String),
}

pub struct CoreHandle {
    pub runtime: tokio::runtime::Handle,
    pub events: futures::channel::mpsc::UnboundedReceiver<CoreEvent>,
}
```

GPUI 侧用一个后台轮询任务消费 `events`（本期 200ms 定时器，与 §6.2 托盘
轮询同模式；change 5 的 `ServerStatus`/`ServerDrained` 事件届时扩枚举）。

### D3：存储与装配

本期存储固定走 JSON store（§9.3 调试路径）：

- `--config <path>` / `--credentials <path>`：显式指定，等价 CLI 行为
- 未指定：解析 `<data_dir>/kiro-rs/` 下的 `config.json` /
  `credentials.json`（首启不存在则由 `Config::load` 语义返回默认配置、
  凭据为空，与 CLI 一致）
- 装配用 `kiro_rs::bootstrap::bootstrap(&BootOptions { credential_store,
  config_store })`，失败走 §9.4：日志记录 + 错误视图（本期用只读文本
  面板展示错误，不弹窗）
- SQLite 在 change 3 才替换这里的 store，骨架不预留 SQLite 分支

### D4：单实例锁

`flock(LOCK_EX | LOCK_NB)`（本期只支持 Linux，`#[cfg(unix)]`；Windows
分支留 `todo` 注释，change 7 补三平台）。拿锁成功写 PID；失败打印提示并
以退出码 1 退出（§9.2 本期口径：提示并退出，不做唤回）。锁文件不删除
（flock 随进程释放，删除有竞态；`README` 注明可手工清理）。

### D5：视图骨架

`Root` = 顶栏 + 左栏 + 内容区（gpui-kit 的 `sidebar` story 布局参照
`gpui-kit.com` 文档）。四视图用 `EmptyView` 派生：各自一个标题 + 一句
「将在 change N 实现」的占位文案，不带任何数据组件。视图切换状态放
`Root` 的 `EntityState`（`current_view: usize`），不引入路由库。

### D6：验证口径

- 编译：`cd desktop && cargo check --release --all-targets --locked`，
  附加 `RUSTFLAGS="-D warnings"`，等价根门禁的逐 flag 判定（§3.1）
- 单测：`paths`（数据目录解析、`KIRO_RS_DATA_DIR` 覆盖）、`lock`
  （同进程二次拿锁失败、跨进程失败）、`bridge`（事件往返）走
  `cargo test --release`；UI 布局不做 headless 测试（gpui-kit headless
  测试模式属 change 4 的引入项，本期只保证可编译）
- 启动：`xvfb-run -a cargo run --release -- --config <tmp> --credentials
  <tmp>`，存活 5 秒视为通过，日志须含窗口创建与 `Bootstrapped` 事件
- 根侧回归：`cargo check --release --all-targets`（根仓）告警数不变；
  `git status` 确认根仓源码零改动

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| `gpui-pre` 在本机编不过或运行即崩（年轻快照，下载量仅万级） | 实现前先做 `/tmp` 编译 spike；跑不起来则止损，change 2 降级为「仅编译通过 + 记录阻塞证据」，是否继续桌面路线回到可行性文档重议 |
| 无显示环境下窗口创建失败，验收标准无法满足 | 验收口径改为「Xvfb 下进程存活 + 日志证据」（D6）；Vulkan 软渲染（`lvp` ICD）兜底 |
| 无 root 环境缺 `-dev` 包，CI 或他人机器复现不了 | sysroot 方法写进 `desktop/README.md`，含精确包清单与 `.pc` 改写步骤；有 root 环境一行 `apt install` 等价 |
| `desktop/` 的 path 依赖把根仓编译产物拖进桌面构建（根仓的 `rust-embed` 需要 `admin-ui/dist`） | 复用根侧既有方案：构建前确保 `admin-ui/dist` 存在（已有产物或占位目录），`README` 注明 |
| 独立 `Cargo.lock` 与根锁版本漂移 | 本期接受（§3.1 已论证，靠 review）；`desktop/README.md` 注明升级根依赖时复查 |
| GPUI 版本钉死在 `gpui-kit` 0.6.1 解析的 `gpui-pre` 0.3.x，Zed 上游快速演进 | `Cargo.lock` 钉版；升级作为独立改进项 |
