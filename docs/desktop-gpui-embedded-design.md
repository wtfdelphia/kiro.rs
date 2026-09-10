# kiro-rs 桌面版设计方案 v2.1（路线 B：GPUI 内嵌一体）

前置文档：`docs/gpui-kit-desktop-feasibility-analysis.md`
目标分支：`dev-desktop`（基线与 `dev` 一致，`42ec15a`）
日期：2026-09-10（v2.1 深度审核修订）
性质：设计文档，未改动任何代码

修订说明：

- v2 在 v1 基础上把托盘常驻、SQLite + 钥匙串、打包流水线纳入本期
- v2.1 是对 v2 的深度审核修订。修正两处高危设计错误：退出路径的 200 毫秒上限（5.2）、workspace 与现有告警门禁的冲突（3.1）。修正八处中等问题：存储 trait 与真实调用链的接线、keyring 回退的预发布依赖、持久化面审计失实、SQLite 并发方案的冻结风险、change 顺序成环、单实例锁缺失、内嵌服务器 adminApiKey 策略缺失、CI Linux 系统依赖缺失。另有四处措辞与代码示例修正

## 一、范围重审：四个问题的结论

| 问题 | v1 结论 | v2 结论 | 关键证据 |
| --- | --- | --- | --- |
| 系统托盘与常驻 | 二期 | 本期做 | `tray-icon` 0.24.2（Tauri 生态，2800 万下载，2026-07-27 发版，MIT OR Apache-2.0）成熟可用；GPUI 的 `on_window_should_close` 关闭拦截在 gpui-kit 依赖的 `gpui-pre` 0.3.1 至 0.3.4 全系列存在（lock 文件解析到 0.3.3，docs.rs 逐版本验证） |
| SQLite 持久化与钥匙串 | 文件 + 原子写 | 本期做 | 现有持久化是四个散落 JSON（仅凭据走原子写，其余为裸 `fs::write`），无事务、明文 secret 落盘；`rusqlite` 0.40.2（1 亿下载，`bundled` feature 免系统依赖）+ `keyring` 4.2.0（2400 万下载）是成熟组合 |
| 打包、签名、公证 | 独立变更 | 本期做流水线，签名按证书门控 | `cargo-packager` 0.11.8（Crabnebula，2026-09-09 仍在推送）支持三平台产物、签名与公证配置、更新器；签名依赖证书密钥，无证书时产出未签名包 |
| gpui-shell / gpui-wry | 不做 | 不做（维持） | shell 自述「Milestone M0：可行性基线，非稳定接口」；wry 的 Linux GTK 路径上游注释仍为「doesn't work yet」。收益评估见第十一节 |

## 二、总体架构

```text
┌────────────────────────── kiro-desktop 进程 ──────────────────────────┐
│                                                                       │
│  主线程（GPUI）                     后台线程组（tokio runtime）         │
│  ┌──────────────────────┐          ┌────────────────────────────┐    │
│  │ Root                 │          │ kiro 核心（lib 目标）        │    │
│  │ ├ Sidebar + 四视图    │          │ MultiTokenManager           │    │
│  │ └ AppTitleBar        │          │ KiroProvider                │    │
│  └────────┬─────────────┘          │ 后台预热 / 批量刷新          │    │
│           │ AdminService           │ （可选）axum serve           │    │
│           │ 进程内直调              └──────────────┬─────────────┘    │
│           ▼                                       ▼                  │
│     Arc<MultiTokenManager> ◄──────── Arc<KiroProvider>               │
│           │                                       │                  │
│  ┌────────┴───────────────────────────────────────┴─────────────┐    │
│  │ 存储层（本期新增）                                             │    │
│  │ SQLite（rusqlite bundled）：凭据元数据/配置/统计/余额缓存      │    │
│  │ 系统钥匙串（keyring）：refresh_token / api_key / secret       │    │
│  └───────────────────────────────────────────────────────────────┘    │
│                                                                       │
│  系统托盘（tray-icon）：主线程创建，事件经 receiver().try_recv() 轮询  │
│  常驻：关窗拦截 → 窗口销毁，进程存活；托盘点击 → 重建窗口              │
│  退出：两阶段（先 tokio 侧收敛，再调 quit；见第五节）                  │
│                                                                       │
│  futures::channel（跨运行时事件流）                                    │
│  tokio 侧 mpsc::Sender ──────────► GPUI 侧 cx.spawn 消费              │
└───────────────────────────────────────────────────────────────────────┘
```

两条关键判断保留：`AdminService` 同步方法在 GPUI 线程直调安全（内部 `parking_lot::Mutex`，无网络等待）；axum 监听整体放 tokio 侧。

## 三、crate 布局：独立 workspace，加 `lib.rs`

现有 `kiro-rs` crate 同时作为 lib 和 bin 发布；`desktop/` 作为独立 workspace，以 path 依赖根 crate：

```text
kiro.rs/
├── Cargo.toml        # 现有单 crate，新增 [lib] 目标，其余不动
├── src/
│   ├── lib.rs          # 新增：声明全部模块，导出装配面 + 存储 trait
│   ├── main.rs         # 保留：CLI 服务入口，改为调用 lib 的装配函数
│   ├── storage/        # 新增：CredentialStore/ConfigStore trait + JsonFileStore
│   ├── kiro/ model/ common/ http_client.rs token.rs   # 不变
│   ├── anthropic/ openai/ admin/ admin_ui/ public_api/ # 不变，可见性按需放开
│   ├── debug.rs        # 孤儿文件，不在模块树内，不随本次变更处理
│   └── test.rs         # 同上
└── desktop/
    ├── Cargo.toml      # 独立 workspace（[workspace] 空段 + members）
    │                   # path 依赖 ../（kiro-rs lib）+ gpui-kit + tray-icon
    │                   # + rusqlite + keyring
    └── src/
        ├── main.rs     # gpui_kit::application() 入口 + 单实例锁
        ├── runtime.rs  # tokio runtime 与桥接层
        ├── bootstrap.rs# 复用 lib 装配
        ├── storage/    # SqliteStore schema、迁移、keyring 集成
        ├── quit.rs     # 两阶段退出协调（本期新增，见第五节）
        ├── tray.rs     # 托盘与常驻
        ├── views/      # dashboard / credentials / settings / server
        └── bridge.rs   # 事件流定义
```

### 3.1 为什么 desktop 必须独立 workspace（v2.1 修订）

仓库根 `Cargo.toml` 没有 `[workspace]` 段，是单 crate 项目。如果 `desktop/` 加进根 workspace，会立即打穿现有 CI：

- `.github/workflows/warning-gate.yaml` 在 `ubuntu-latest` 上跑 `cargo check --release --all-targets --locked`，workspace 会连带编译 desktop 及其全部依赖
- GPUI 的 Linux 构建需要 `libxkbcommon-x11-dev`、`libfontconfig-dev`、`libwayland-dev`、`libx11-xcb-dev`、`libasound2-dev` 等系统库（Zed `script/linux` 清单），门禁环境没有，直接编译失败
- 三条调用方流水线（build、build-dev-release、docker-build）全部挂在门禁之后，desktop 进根 workspace 等于把桌面构建的成败绑到服务端发布路径上

独立 workspace 的取舍：

- 根侧告警门禁、发布流水线零改动，服务端用户不受影响
- desktop 自带一套等价的告警判定（`cargo check --release --all-targets` + `-D warnings` + `--locked`），写进自己的 CI，不借用根门禁
- 代价：`Cargo.lock` 两份；根 crate 的版本与 desktop 的 path 依赖靠 review 保证一致
- 后续若要让根门禁覆盖 desktop，前置条件是门禁环境安装 GPUI 系统依赖，列为独立改进项，不阻塞本期

### 3.2 admin-ui/dist 的编译期依赖

`src/admin_ui` 用 rust-embed 在编译期嵌入 `admin-ui/dist`（该目录被 gitignore，缺失时编译直接失败）。desktop 依赖根 lib，所以 desktop 的任何构建（本地与 CI）都要先满足其一：跑过 `pnpm build`，或建一个占位 `admin-ui/dist`。warning-gate 现在用的就是占位目录方案，desktop CI 沿用。

### 3.3 `lib.rs` 导出的最小面

| 导出 | 桌面端用途 |
| --- | --- |
| `model::config::Config`、`model::arg` | 配置加载与路径解析 |
| `kiro::model::credentials::{CredentialsConfig, KiroCredentials}` | 凭据模型 |
| `kiro::token_manager::MultiTokenManager` | 核心句柄 |
| `kiro::provider::KiroProvider`、`kiro::endpoint::{IdeEndpoint, KiroEndpoint}` | provider 构建 |
| `admin::{AdminService, AdminState, create_admin_router}` | 管理面直调与可选 HTTP |
| `anthropic::create_router_with_provider_and_auth`、`openai::create_openai_routes`、`admin_ui::mount_admin_ui` | 可选 axum 监听的路由装配 |
| `http_client::ProxyConfig`、`token::{init_config, CountTokensConfig}`、`public_api` | 装配所需 |
| `storage::{CredentialStore, ConfigStore, JsonFileStore}`（本期新增） | 存储接缝，见第七节 |
| `bootstrap`、`build_routes`（本期新增） | 装配复用，见 4.3 |

### 3.4 两个不参与编译的孤儿文件

`src/debug.rs` 和 `src/test.rs` 不在任何模块树里（`rg "mod debug|mod test"` 全仓无命中），当前不参与编译。lib 化对它们无影响，本变更不处理。

## 四、运行时模型

### 4.1 双运行时布局

GPUI 的 `application().run()` 占用并阻塞主线程，tokio 运行时在进入 `run` 之前建好：

```rust
// desktop/src/main.rs 骨架（示意，未标注全部类型）
fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("kiro-tokio")
        .build()
        .expect("tokio runtime 创建失败");
    let handle = rt.handle().clone();

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        // run 的回调返回 ()，错误只能记录或终止进程，不能用 ?
        gpui_kit::init(cx);
        cx.set_app_identity("dev.kiro-rs.desktop", "kiro-rs");
        let tray = kiro_desktop::tray::init(cx);
        let bootstrap = match kiro_desktop::bootstrap::load(&handle) {
            Ok(b) => b,
            Err(e) => { /* 错误视图或直接退出，见 9.4 */ }
        };
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), move |window, cx| {
                let view = cx.new(|cx| AppView::new(bootstrap, tray, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            }).expect("窗口创建失败");
        }).detach();
    });
    // run() 返回即应用已退出；两阶段退出见第五节
}
```

`rt` 的所有权留在 `main`，`run()` 返回后 drop。注意顺序：两阶段退出保证退出前服务器与存储已收敛，`run()` 返回时没有需要等待的在途工作（第五节）。

### 4.2 跨运行时桥接

边界规则：

- 一切核心调用经 `handle.spawn` 进入 tokio 侧，绝不在 GPUI 任务里直接 await reqwest future
- 回传用 `futures::channel`（不绑定执行器）
- 持续事件（服务器状态、凭据后台刷新完成）用 `mpsc`，事件只传「发生了什么」，UI 收到后调 `AdminService` 同步快照方法重拉数据

```rust
pub enum CoreEvent {
    ServerStatus(ServerStatus),
    ServerDrained,             // 两阶段退出的收敛确认（本期新增）
    CredentialsChanged,
    ModelsRefreshed(u64),
}
```

### 4.3 装配复用：`bootstrap` 模块

`main.rs` 现有装配链抽成 lib 里的分步函数（跨模块重构，走 OpenSpec）：

```rust
pub struct Bootstrapped {
    pub config: Config,
    pub token_manager: Arc<MultiTokenManager>,
    pub kiro_provider: Arc<KiroProvider>,
    pub endpoint_names: Vec<String>,
    pub proxy_config: Option<ProxyConfig>,
    pub is_multiple_format: bool,
}

pub fn bootstrap(stores: &Stores, config_path: &str, credentials_path: &str)
    -> anyhow::Result<Bootstrapped>;

pub fn build_routes(b: &Bootstrapped, api_key: &str, admin_key: Option<&str>)
    -> (axum::Router, AppState);
```

`Stores` 是 `(Arc<dyn CredentialStore>, Arc<dyn ConfigStore>)` 的组合，桌面端传 SQLite 实现，CLI 传 JSON 文件实现。`build_routes` 的 `admin_key` 参数决定 Admin 路由是否挂载（与现有 `main.rs` 的空 key 判定逻辑一致），见 5.3。

CLI 调这两个函数后 `axum::serve`，行为与现在等价；桌面端调同样的函数，区别在服务器是否启动与存储后端。

## 五、内嵌服务器的生命周期与退出

### 5.1 状态机

```text
Stopped ──start──► Starting ──bind 成功──► Running ──stop──► Stopping ──drain 完──► Stopped
                       │
                       └──bind 失败──► Failed(错误信息) ──start──►（可重试）
```

```rust
pub struct ServerControl {
    handle: tokio::runtime::Handle,
    state: Arc<RwLock<ServerStatus>>,
}

impl ServerControl {
    /// 启动：handle.spawn 内 bind + axum::serve，
    /// graceful_shutdown future 由 UI 触发的 oneshot 驱动
    /// （替换 main.rs 中信号驱动的 shutdown_signal），
    /// 内部仍广播 ws_shutdown 并复用 drain 兜底（10 秒上限）
    pub fn start(&self, routes: Router, addr: &str);
    /// 只发停止信号，立即返回；收敛完成经 CoreEvent::ServerDrained 通知
    pub fn request_stop(&self);
    pub fn status(&self) -> ServerStatus;
}
```

默认行为：启动应用时按配置 `host:port` 自动尝试启动，失败进 `Failed` 态；端口冲突检测就是 bind 失败本身，不做预检。

### 5.2 两阶段退出（v2.1 修订）

v2 设计在 `on_app_quit` 里停服务器并等待最多 10 秒，这个方案不成立。GPUI 的 `App::shutdown()` 对 `on_app_quit` 回调只等 `SHUTDOWN_TIMEOUT`，该常量是 200 毫秒（Zed `crates/gpui/src/app.rs:75`），超时记一条 `timed out waiting on app_will_quit` 后继续退出。Cmd+Q、托盘「退出」、窗口全部关闭（若零窗口即退出）都走这条路径，服务器会被直接掐掉。

修正为两阶段：

```text
阶段 1（退出协调，不限 200ms，发生在 quit 之前）
  退出请求（托盘菜单 / Cmd+Q action / 关窗且关闭常驻）
    → quit.rs 拦截，置退出中状态，托盘置「正在退出」
    → ServerControl::request_stop()
    → GPUI 侧 cx.spawn 等待 CoreEvent::ServerDrained，
      上限 = drain 兜底 10 秒 + 2 秒余量
    → 存储层最终落盘（统计防抖未刷的部分）
阶段 2（真正的 quit，所有工作必须在 200ms 内完成）
    → cx.quit()，on_app_quit 只做极轻收尾（如关闭日志文件）
```

实现要点：

- gpui-kit story 把 `cmd-q`/`alt-f4` 绑到 `Quit` action；桌面应用覆写这两个键绑定到自己的 `RequestQuit` action，统一进阶段 1，不允许任何路径直达 `cx.quit()`
- `ServerDrained` 等不到（超时）时仍进阶段 2，并在日志记录未收敛的在途请求数，行为与现 `main.rs` 的 drain 兜底语义一致
- 服务器本来就没启动时，阶段 1 只剩存储落盘，直接进阶段 2

### 5.3 内嵌服务器的 adminApiKey 策略（v2.1 新增）

服务器运行时，Admin HTTP 路由的行为沿用现有语义：

- 配置了非空 `adminApiKey`：Admin API 与内嵌 `/admin` UI 挂载，行为与 CLI 完全一致
- 未配置或为空字符串：不挂载，与现 `main.rs` 的「空字符串视为未配置」判定一致
- 桌面端管理面走进程内 `AdminService`，不受此影响；即使 Admin HTTP 关闭，管理功能完整
- 凭据导入导出（7.5）是桌面端能力，不经过 Admin HTTP

## 六、系统托盘与常驻（本期新增）

### 6.1 技术选型依据

调研结论（2026-09-10）：

- gpui-kit 与 Zed 上游的 `gpui_platform` 均无托盘实现（浅克隆全仓 `rg -il "tray"` 无有效命中）
- Zed 上游的托盘 PR `zed-industries/zed#44047`（gpui: Add Tray support）于 2025-12-12 关闭，GitHub API 确认 `merged: false`，短期内不会进上游
- 事实标准是 `tray-icon`（tauri-apps）：0.24.2，累计下载约 2800 万，2026-07-27 发版，许可 MIT OR Apache-2.0，支持 Windows / macOS / Linux（AppIndicator 或 KSNI）

选型：`tray-icon = "0.24"`，Linux 用 `ksni` feature（StatusNotifierItem D-Bus 后端，自带工作线程），关掉默认的 `libappindicator`，避免引入 GTK3 依赖树。菜单用 `tray-icon` 自带的 muda 集成。

### 6.2 线程与事件模型

`tray-icon` 的平台约束（其 README 明示）：macOS 必须在主线程创建且主线程有事件循环；Windows 与 Linux AppIndicator 后端要求同线程有事件循环，KSNI 自管理工作线程。

GPUI 的 `run()` 在主线程跑事件循环，满足 macOS 约束；Windows 上 GPUI 跑原生消息循环，同样满足。事件读取用全局接收器轮询（`tray-icon` 内部是 `crossbeam_channel::unbounded`，`receiver()` 返回全局 `Receiver`，`src/lib.rs:678-704`）：

```rust
// desktop/src/tray.rs：GPUI 侧常驻消费循环
cx.spawn(async move |cx| {
    loop {
        cx.background_executor().timer(Duration::from_millis(200)).await;
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            // 左键单击 → 显示/重建主窗口；菜单事件 → 分发
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            // 显示主窗口 / 启停服务器 / 退出（进阶段 1）
        }
    }
}).detach();
```

200ms 轮询对托盘交互的延迟可接受，且零额外依赖。若后续验证 `tray-icon` 的 `set_event_handler` 回调模式与 GPUI 主循环兼容，可改为推送式。

### 6.3 常驻模型：关窗拦截 + 窗口重建

GPUI 的 `PlatformWindow` trait 共 94 个方法，没有窗口级隐藏接口（逐一检查，只有软键盘、字符面板等无关项）。所以常驻不做「隐藏窗口」，做「销毁 + 重建」：

1. 窗口注册 `on_window_should_close`（签名 `Fn(&mut Window, &mut App) -> bool`，Zed `window.rs:6439`）
2. 回调内读用户设置 `closeToTray`（默认开，存 SQLite）：
   - 开：返回 `false` 取消原生关闭，随后调 `window.remove_window()` 销毁窗口（gpui-kit 的 `lifecycle` 测试即此写法），进程与服务器继续运行
   - 关：返回 `true`，触发两阶段退出（5.2）
3. 托盘单击或菜单「显示主窗口」：`cx.open_window` 重建，视图状态从 SQLite 与核心快照恢复
4. 托盘菜单「退出」、Cmd+Q / Alt+F4：进阶段 1

托盘菜单结构：

```text
kiro-rs  ·  服务器: 运行中 (127.0.0.1:3000)   ← 标题项，只读
☑ 启动时自启服务器
显示主窗口
启动服务器 / 停止服务器                          ← 按状态二选一
─────────
退出
```

托盘图标随服务器状态切换（运行中/已停止/失败/退出中四态），图标资源走 `gpui-kit-assets` 之外的自有 assets（托盘位图不能用 SVG，需准备 PNG/ICO/icns 三套）。

### 6.4 平台细节（v2.1 新增）

- macOS Dock 图标：GPUI 没有 activation policy API（上游无 `set_activation_policy`），常驻时 Dock 图标不会消失，这是已知体验缺口，不在本期解决
- 开机自启：三平台各写各的原生入口，设置项存 SQLite，实现放 change 6。Windows 写注册表 `Run` 键；macOS 写 `~/Library/LaunchAgents` 下的 plist；Linux 写 `~/.config/autostart` 下的 desktop entry。三者都是文件操作，无需系统权限
- 日志：`tracing_subscriber` 输出到 `<data_dir>/kiro-rs/kiro-desktop.log`（带轮转上限），保留终端输出。常驻应用没有终端可看，文件日志是排障唯一入口

### 6.5 待 spike 验证的两个前提

- 零窗口时 GPUI 应用循环是否继续运行（Zed 在 macOS 上如此，Linux/Windows 行为需在钉定的 `gpui-pre` 上实测；若自动退出，用 `on_window_should_close` 返回 `false` 兜底，常驻语义不变）
- Windows 上 `tray-icon` 事件在 GPUI 消息循环内能否送达（KSNI 只影响 Linux，Windows 走隐藏窗口消息）

两项都放进 change 2（app-shell）的验收清单，失败不阻塞其余 change，托盘降级为「仅退出入口」也能用。

## 七、SQLite 存储与系统钥匙串（本期新增）

### 7.1 现有持久化面审计（v2.1 修正）

| 数据 | 现状 | 写入方式 | 位置 |
| --- | --- | --- | --- |
| 凭据（含刷新后的 token 回写） | `credentials.json`，仅多凭据格式回写 | `write_atomic`（全仓唯一一处原子写） | `token_manager.rs:1570`（`persist_credentials`） |
| 配置（含负载均衡模式） | `config.json` | 裸 `fs::write`，非原子 | `model/config.rs:417`（`save`）、`token_manager.rs:3005` |
| 使用统计 | `kiro_stats.json`，防抖写 | 裸 `std::fs::write` | `token_manager.rs:1626,1686` |
| 余额缓存 | `kiro_balance_cache.json`，TTL 300 秒 | 裸 `std::fs::write` | `admin/service.rs:73,1073` |
| 模型目录 | 仅内存，重启丢失 | 无 | `token_manager.rs:851` |
| profileArn 冷却 | 仅内存，有测试明示不持久化 | 无 | `token_manager.rs:4307` |

问题：四个文件只有凭据是原子写，配置、统计、余额缓存在写入中途崩溃都会留下半个文件；跨文件更新无事务；明文 secret 直接落盘。

### 7.2 存储接缝与调用链接线（v2.1 修订）

在 lib 里新增最小 trait，CLI 与桌面各挂一个实现：

```rust
// src/storage/mod.rs（lib 内新增）
pub trait CredentialStore: Send + Sync {
    fn load(&self) -> anyhow::Result<CredentialsConfig>;
    fn persist(&self, credentials: &[KiroCredentials]) -> anyhow::Result<()>;
}

pub trait ConfigStore: Send + Sync {
    fn load(&self) -> anyhow::Result<Config>;
    fn save(&self, config: &Config) -> anyhow::Result<()>;
}
```

三个现有调用点全部改道经过 trait，不留旁路：

| 调用点 | 现状 | 改道后 |
| --- | --- | --- |
| `persist_credentials`（`token_manager.rs:1570`，13 处调用） | 直接序列化写文件 | 调 `CredentialStore::persist` |
| `save_config`（`token_manager.rs:1059`）→ `Config::save` | 直接写文件路径 | 调 `ConfigStore::save` |
| `load_stats` / `save_stats`（`token_manager.rs:1630,1662`）与余额缓存读写（`admin/service.rs`） | 各自读写 JSON | 走 `CredentialStore` 的扩展方法或独立 `StatsStore`，schema 见 7.3 |

`Config::save` 需要 `config_path` 才能写文件（`config.rs:417` 无路径即报错）。改道方案：`Config` 保留现有方法给 CLI，`save_config` 的实现体换成 `ConfigStore`；`AdminService` 的五处设置更新（`service.rs:1278,1324,1396,1430,1654`）与 `persist_load_balancing_mode`（`token_manager.rs:3005`）都经 `save_config` 统一走新路径，不需要逐处修改。

`MultiTokenManager::new` 增加一个接受 `Arc<dyn CredentialStore>` 的构造入口，现有签名保留（内部包一层 `JsonFileStore`），全部现有测试不动。这属于跨模块变更，走 OpenSpec。

### 7.3 SQLite schema

`rusqlite` 0.40.2，`bundled` feature（自带 SQLite 源码，免系统库，体积增量约 1MB）。数据库文件：`<data_dir>/kiro-rs/kiro.db`，WAL 模式。

```sql
CREATE TABLE schema_version (version INTEGER NOT NULL);

CREATE TABLE credentials (
    id            INTEGER PRIMARY KEY,
    auth_method   TEXT NOT NULL,
    profile       TEXT,                -- 展示名
    priority      INTEGER NOT NULL DEFAULT 100,
    disabled      INTEGER NOT NULL DEFAULT 0,
    endpoint      TEXT,
    api_region    TEXT, auth_region TEXT,
    fields        TEXT NOT NULL,       -- 非敏感字段的 JSON
    created_at    TEXT NOT NULL, updated_at TEXT NOT NULL
);

CREATE TABLE secrets (
    credential_id INTEGER PRIMARY KEY REFERENCES credentials(id),
    keyring_ref   TEXT NOT NULL        -- 钥匙串条目 key，如 "cred-3:refresh_token"
);

-- 整表 JSON 单行：Config 字段有 20 多个且持续增长，
-- key-value 逐字段映射的维护成本高于收益
CREATE TABLE config (id INTEGER PRIMARY KEY CHECK (id = 1), doc TEXT NOT NULL);

CREATE TABLE stats (
    credential_id   INTEGER PRIMARY KEY REFERENCES credentials(id),
    success_count   INTEGER NOT NULL DEFAULT 0,
    failure_count   INTEGER NOT NULL DEFAULT 0,
    last_used_at    TEXT
);

CREATE TABLE balance_cache (
    credential_id   INTEGER PRIMARY KEY REFERENCES credentials(id),
    snapshot        TEXT NOT NULL,
    fetched_at      INTEGER NOT NULL   -- 配合 300 秒 TTL
);

CREATE TABLE model_catalog (
    credential_id   INTEGER NOT NULL REFERENCES credentials(id),
    model_id        TEXT NOT NULL,
    info            TEXT NOT NULL,
    refreshed_at    INTEGER NOT NULL,
    PRIMARY KEY (credential_id, model_id)
);
```

两个取舍：

- 模型目录从「仅内存」改为落盘：桌面应用常驻且频繁重启窗口，重启后 `/v1/models` 不该退回空目录。CLI 行为不变（仍走内存）
- 冷却继续不持久化：它带 `credentials_version` 绑定，跨进程恢复语义复杂，收益低，维持现状

并发写方案（v2.1 修正）：`parking_lot::Mutex` 保护单连接 + WAL + `busy_timeout`。不用 `tokio::sync::Mutex`：GPUI 线程在 `cx.background_spawn` 里同步调存储，若持锁方是 tokio 任务，跨运行时等锁会拖住整个后台线程池。写事务保持短小；读密集的余额缓存走独立只读连接，WAL 下不阻塞写。

### 7.4 系统钥匙串（v2.1 修订）

`keyring` 4.2.0（v1 默认后端：macOS Keychain、Windows Credential Manager、Linux Secret Service）。secret 字段指 `refresh_token`、`kiro_api_key`、`client_secret`（`credentials.rs:23,27,52`），以及配置里的 `apiKey` / `adminApiKey` / 代理密码。

规则：

- 服务名固定 `dev.kiro-rs.desktop`，条目 key 为 `cred-<id>:<field>`
- SQLite 只存 `keyring_ref`，不存明文；凭据加载时按 ref 从钥匙串取值，失败（条目丢失/解锁拒绝）该凭据标为 `InvalidConfig` 禁用，与现有 `authMethod=api_key` 缺字段的处置一致
- 写入走「先钥匙串后 SQLite」，钥匙串写失败则整体失败不落库，避免产生指向空钥匙串的凭据
- 无 Secret Service 的 Linux 环境（headless、极简桌面）：回退为自实现的加密文件（口令派生密钥，0600 权限，存 `<data_dir>/kiro-rs/secrets.enc`），界面明示当前是文件回退而非系统钥匙串

回退方案的取舍（v2.1 修正）：v2 引用的 `db-keystore` 是 0.6.0-pre.2 预发布 crate，依赖 turso 0.8.0-pre 系，且 keyring 官方 README 明确不建议通过 `cli` feature 整体引入。自实现加密文件的范围可控（一个加密器 + 一个文件），比引入预发布依赖链更稳。

### 7.5 JSON 的去留：导入导出格式

`credentials.json` / `config.json` 不再是桌面端的存储，降为交换格式：

- 首启迁移：检测到 app data 目录或 cwd 存在 JSON 文件，提示导入进 SQLite，导入成功后原文件备份为 `*.bak`（沿用现有迁移的备份习惯）
- 导出：设置面板提供「导出为 JSON」，产物与现有 `credentials.example.*.json` 格式兼容，可直接给 CLI 用
- secret 导出时明文写入导出文件，界面弹显式确认对话框并提示风险（这是现有 JSON 格式的本征属性，不是新增风险）

## 八、UI 结构

数据源标注含存储相关项：

```text
Root
└─ AppView
   ├─ AppTitleBar（服务器状态徽标 + 退出中状态）
   ├─ Sidebar（icon 折叠模式）：总览 / 凭据 / 设置 / 服务
   └─ 主内容区
```

| 视图 | 数据源 | 组件 |
| --- | --- | --- |
| 总览 | `get_credential_facets`、`query_credentials`、`get_global_models_catalog`、`get_public_api` | `badge`、`description_list`、状态卡 |
| 凭据 | `query_credentials`（筛选/分页）、`get_balance`、`test_credential`、`refresh_models`、`set_disabled`、`set_priority`、`delete_credential`、`add_credential`、`import_credentials_batch`、`import_kam_document`、Builder ID / IAM SSO 登录 | `DataTable`、`dialog`、`form`、`input`、`select` |
| 设置 | `get/update_proxy_settings`、`endpoint`、`auth`、`websocket`、`load_balancing_mode`，加桌面专属：`closeToTray`、开机自启、JSON 导入导出 | `form`、`switch`、`select`、`input` |
| 服务 | `ServerControl` + `Config` 的 host/port | `button`、`input`、`notification` |

状态同步模式：视图持快照，写操作成功后重拉，后台任务经 `CoreEvent` 通知。

## 九、配置、凭据路径与进程治理

### 9.1 数据目录

桌面端数据目录统一为 `<data_dir>/kiro-rs/`（Linux `$XDG_DATA_HOME`、macOS `~/Library/Application Support`、Windows `%APPDATA%`），内含：

| 文件 | 内容 |
| --- | --- |
| `kiro.db` / `-wal` / `-shm` | SQLite 主存储与 WAL 附属 |
| `kiro-desktop.log` | 日志（带轮转上限） |
| `kiro-desktop.lock` | 单实例锁，见 9.2 |
| `secrets.enc`（可选） | 钥匙串不可用时的回退存储 |
| `config.json`（仅导入时出现） | 迁移源，导入后改名 `.bak` |

### 9.2 单实例锁（v2.1 新增）

常驻应用容易误启第二个实例，两个实例会抢端口、抢 SQLite 写、抢 keyring 条目。启动时在数据目录创建 `kiro-desktop.lock` 并加文件锁（Linux/macOS 用 flock，Windows 用 `LockFileEx`）：

- 拿锁成功：写入 PID，正常启动；退出时解锁删除
- 拿锁失败：弹窗提示已有实例在运行，提供「显示现有窗口」（后续可做跨进程唤回）或直接退出
- 本期只做「提示并退出」，跨进程唤回列为二期

### 9.3 路径解析顺序

命令行 `--config` / `--credentials`（指定时以 JSON 文件为存储，等价 CLI 行为，便于调试）→ app data 目录 SQLite → 首启生成。`KIRO_API_KEY` 环境变量的处理与 CLI 一致（最高优先级临时凭据，不落库）。

### 9.4 启动失败路径（v2.1 新增）

单实例锁失败、SQLite 损坏、钥匙串条目缺失的启动期处置各有出口：锁冲突弹窗；SQLite 打开失败时备份损坏文件后重建空库（凭据需重新导入，界面明示）；个别凭据的钥匙串缺失只禁用该凭据，不影响启动（7.4）。

## 十、打包、签名、公证流水线（本期新增）

### 10.1 工具选型

| 工具 | 版本 | 状态 | 结论 |
| --- | --- | --- | --- |
| `cargo-packager` | 0.11.8 | Crabnebula（Tauri 团队）维护，2026-09-09 有推送，477 星 | 选用 |
| `tauri-bundler` | 2.9.4 | 158 万下载，但与 Tauri 应用结构绑定 | 不选 |
| `cargo-bundle` | 0.11.0 | 能力停留在打 bundle，无更新器与签名集成 | 不选 |

`cargo-packager` 与框架无关（只打已构建的二进制），配置 `packager.toml`，支持 macOS `.app`/`.dmg`、Linux `.deb`/`.AppImage`、Windows `.msi`/`.nsis`，自带可选的更新器服务端协议。其源码含完整签名配置面：macOS `signing_certificate` / `signing_certificate_password` / `MacOsNotarizationCredentials`（`config/mod.rs:662-742`）、Windows 证书 thumbprint 与摘要算法。

### 10.2 流水线范围

新增 `.github/workflows/desktop-release.yml`：

1. 矩阵构建：`macos-latest` / `windows-latest` / `ubuntu-latest`。Linux 腿需先安装系统依赖（`libxkbcommon-x11-dev`、`libfontconfig-dev`、`libwayland-dev`、`libx11-xcb-dev`、`libasound2-dev` 等，清单以 Zed `script/linux` 为准），再在 `desktop/` 目录下 `cargo build --release`，最后 `cargo packager`
2. warning-gate 对齐：构建前在 `desktop/` workspace 跑 `cargo check --release --all-targets --locked` + `RUSTFLAGS=-D warnings`，门禁红则不出产物；另建 `admin-ui/dist` 占位目录（3.2）
3. 签名门控：
   - macOS：`MACOS_CERTIFICATE` / `MACOS_CERTIFICATE_PASSWORD` / `APPLE_ID` 等 secrets 存在时走内置签名 + 公证；缺失时产出未签名 `.dmg` 并在 release 说明标注
   - Windows：`WINDOWS_CERTIFICATE` 存在时签名，缺失时未签名
   - Linux：不签名，产物附 sha256
4. 产物上传 release，命名含平台与架构

证书密钥属于仓库敏感配置，本期目标是「流水线就绪，证书到位即生效」，不采购证书。这条按 `AGENTS.md` 高风险矩阵属于发布/CI 变更，走 OpenSpec 与流水线审查（绿路径与红路径都要有 run 证据）。

现有三条服务端流水线与 `warning-gate.yaml` 不受影响：desktop 是独立 workspace，根门禁的 `--all-targets` 不会触达它（3.1）。

### 10.3 桌面专属资源

打包需要而仓库还没有的：应用图标（1024 PNG → 各平台格式）、托盘四态图标、macOS `Info.plist` 元数据、Windows 版本信息。随 change 7 一并进仓库，路径 `desktop/assets/`。

## 十一、gpui-shell 与 gpui-wry 的必要性评估

### 11.1 gpui-shell（JS 扩展）：本期不做

- 上游自述（`crates/shell/README.md`）：「This crate is at milestone M0: a feasibility baseline, not a stable interface」。接口随版本变动的风险由使用方承担
- 依赖代价：QuickJS JIT 运行时（仓库为其专设了 `[profile.dev.package]` 优化，可见编译分量不小）、能力授权系统的审计成本
- 收益面：kiro-rs 是单用户代理管理工具，用户目标是配凭据、看余额、起服务，没有插件生态的需求信号。面板扩展用 gpui 原生视图就能覆盖
- 结论：本期不做。重估触发条件：出现「第三方要往管理台里加自定义面板」的真实需求，且 shell 离开 M0

### 11.2 gpui-wry（webview）：本期不做

- Linux 路径未就绪：官方 `examples/webview/src/main.rs` 的 GTK 分支注释仍是「doesn't work yet / TODO: How to initialize this fixed?」
- 收益倒挂：webview 的价值是复用现有 React admin-ui，但路线 B 的界面本来就是 GPUI 原生重写，嵌 webview 等于维护两套 UI
- 依赖代价：Linux 需 webkit2gtk 系统库，打包产物体积与平台耦合上升
- 结论：本期不做。唯一可能翻盘的场景是需要嵌入上游提供的 Web 页面（如 Kiro 官方控制台），目前不存在

两者合计本期节省约 1.5-2 人周，且不背 M0 接口与 webkit 依赖两笔技术债。

## 十二、OpenSpec 变更拆分

按提交粒度纪律（一个 change 一个提交），七个 change，顺序执行：

| # | change 名 | 内容 | 验证 |
| --- | --- | --- | --- |
| 1 | `desktop-lib-extraction` | 新增 `lib.rs`，装配函数抽取，`main.rs` 改调 lib，存储 trait 与 `JsonFileStore` 落地并完成 7.2 的调用线改道 | `cargo check --release --all-targets` 零新增告警；全量 `cargo test`；CLI 冒烟（`/v1/models` + `/v1/messages` 前后比对） |
| 2 | `desktop-app-shell` | `desktop/` 独立 workspace 骨架：双运行时、bridge、单实例锁、数据目录解析、窗口 + sidebar + 空视图；含 6.5 两个常驻前提的 spike 结论 | 应用可启动显示四视图骨架；零窗口存活与托盘事件送达验证记录；根侧 `cargo check` 确认门禁腿不受影响 |
| 3 | `desktop-sqlite-storage` | `SqliteStore`、schema 与迁移、keyring 集成与加密文件回退、JSON 导入导出、存储门面并发方案 | 首启迁移测试；keyring 不可用回退测试；导入导出往返一致性测试；并发写测试；CLI 行为不变（全量测试） |
| 4 | `desktop-credentials-view` | 凭据列表 + 启停/优先级/删除/测试/余额 + 添加与批量导入对话框 | gpui headless 测试；手动全流程 |
| 5 | `desktop-settings-server-view` | 设置面板 + 服务器启停视图 + 两阶段退出（5.2） | 设置修改落 SQLite 验证；服务器启停与端口冲突手测；退出时 10 秒级在途请求收敛验证 |
| 6 | `desktop-tray-resident` | tray-icon 集成、关窗拦截、窗口重建、托盘四态图标与菜单、开机自启 | 三平台托盘交互手测；关窗常驻 → 托盘唤回全流程 |
| 7 | `desktop-embedded-packaging` | 自动启动服务器收尾、`packager.toml`、三平台 CI 流水线（含 Linux 系统依赖安装步骤）、签名门控、应用图标资源 | 三平台产物构建证据（绿路径）；签名缺失降级路径证据（红路径）；产物可安装冒烟 |

依赖关系：change 1 是全部前置；change 2 用 change 1 的 `JsonFileStore` 读取配置与凭据（解决 v2 的「shell 启动就要读配置，存储层却在 change 3」的顺序问题）；change 3 是 4/5/6 的前置（视图与服务都依赖存储层）。

每个 change 实现前按门禁走 `openspec-superpowers-bridge`，实现后 `spec-compliance-check`，归档前 `openspec-verify-change`。

## 十三、验证矩阵

| 层 | 命令/手段 | 通过标准 |
| --- | --- | --- |
| 编译门禁（根侧） | `cargo check --release --all-targets` | 告警数不高于变更基线；desktop 未进根 workspace |
| 编译门禁（desktop） | `desktop/` 内 `cargo check --release --all-targets --locked` + `-D warnings` | 零告警 |
| 核心回归 | `cargo test`（根仓全量） | 全绿，含 lib/bin 双目标 |
| 存储 | change 3 新增测试 | 迁移幂等、钥匙串缺失降级、并发写不损坏（WAL + `busy_timeout`） |
| UI 集成 | `#[gpui_kit::test]` headless | 凭据列表、对话框、表单关键交互 |
| 托盘常驻 | 三平台手测 | 关窗不退出、托盘唤回、服务器状态同步到托盘菜单 |
| 退出收敛 | 手测 + 计时 | 活跃请求下退出：阶段 1 完成于 12 秒内，退出后无残留进程 |
| 单实例 | 双开手测 | 第二个实例提示并退出 |
| 手动冒烟 | 全流程 | 启动 → 导入凭据 → 测试凭据 → 起服务器 → 外部 `curl /v1/messages` → 关窗常驻 → 唤回 → 退出无残留进程 |
| 打包 | 三平台 CI | 产物可安装；未签名降级路径有记录 |

## 十四、风险与对策

| 风险 | 对策 |
| --- | --- |
| `gpui-pre` 快照年轻 | `desktop/Cargo.toml` 钉 `gpui-kit = "=0.6.1"`；升级独立验证 |
| 零窗口时 GPUI 应用退出（常驻前提不成立） | change 2 先行验证；兜底方案：`should_close` 返回 `false` 后不销毁窗口，改最小化（`minimize_window` 存在于 `Window`，`window.rs:6204`） |
| `tray-icon` 事件循环约束与 GPUI 冲突 | Linux 用 KSNI 后端（自管线程）；Windows/macOS 走主循环轮询；change 2 出验证记录 |
| GPUI 退出回调只等 200ms | 两阶段退出（5.2）：收敛工作全部前置到 `cx.quit()` 之前，`on_app_quit` 只做轻收尾 |
| keyring 在部分 Linux 环境不可用 | 自实现加密文件回退 + 界面明示；测试覆盖两条路径 |
| 双运行时死锁 | 单向数据流：GPUI 永不阻塞等核心，核心永不回调 GPUI，只发事件 |
| SQLite 锁竞争拖慢 UI | `parking_lot::Mutex` + 短事务 + 余额缓存只读连接（7.3） |
| 双实例抢资源 | 数据目录文件锁（9.2） |
| change 1 触碰装配链导致 CLI 回归 | 抽取前后各录一次 `/v1/models` + `/v1/messages` 冒烟比对；现有集成测试兜底 |
| desktop 依赖的系统库让根门禁失败 | 独立 workspace 隔离（3.1）；desktop 有自己的门禁腿 |
| `admin-ui/dist` 缺失导致 desktop 编译失败 | CI 建占位目录（3.2），本地文档说明 |
| 无证书导致产物无法分发 | 未签名包 + 文档说明手动信任流程；证书到位后流水线零改动生效 |
| `cargo-packager` 版本演进 | 钉 0.11.8；升级跟 release notes |
| admin-ui 行为在翻译中走样 | 以现有组件与其测试为语义基准，交互差异视为 bug |

## 十五、工作量

| change | 估算 |
| --- | --- |
| 1 `desktop-lib-extraction`（含存储 trait 改道） | 1.5-2 周 |
| 2 `desktop-app-shell`（含常驻 spike 与单实例锁） | 1-1.5 周 |
| 3 `desktop-sqlite-storage` | 1.5-2 周 |
| 4 `desktop-credentials-view` | 2-2.5 周 |
| 5 `desktop-settings-server-view`（含两阶段退出） | 1.5-2 周 |
| 6 `desktop-tray-resident`（含开机自启） | 1-1.5 周 |
| 7 `desktop-embedded-packaging` | 1.5-2 周 |

合计 10-13.5 人周。较 v2 的 9.5-12.5 周上调约 0.5-1 周，来自 change 1 纳入存储 trait 改道、6.4 平台细节与单实例锁。

## 十六、证据清单

v1 证据（装配链、`AdminService`、tokio 耦合点、drain、默认路径、分支基线）保留，此处列 v2 与 v2.1 新增：

| 结论 | 依据 |
| --- | --- |
| gpui-kit 与 GPUI 上游均无托盘 | 浅克隆 `rg -il "tray"` 仅命中无关词 |
| Zed 托盘 PR 被拒 | GitHub API：`zed-industries/zed#44047`，closed 2025-12-12，`merged: false` |
| `tray-icon` 成熟度、平台约束与事件模型 | crates.io：0.24.2，2026-07-27，约 2800 万下载；其 README 线程约束与 KSNI 说明；源码 `src/lib.rs:147`（crossbeam_channel）、`:678-704`（receiver） |
| `on_window_should_close` 签名与可用性 | Zed `crates/gpui/src/window.rs:6439`；docs.rs `gpui-pre` 0.3.1/0.3.3/0.3.4 页面均命中；gpui-kit lock 解析到 0.3.3 |
| 窗口级隐藏不存在 | `PlatformWindow` trait 94 个方法逐一检查，无 `hide`/`show`/`set_visible` |
| 兜底最小化存在 | Zed `window.rs:6194,6204`（`activate_window`、`minimize_window`） |
| 退出回调 200ms 上限 | Zed `app.rs:75`（`SHUTDOWN_TIMEOUT`）、`:975-995`（`shutdown()` 的 `block_with_timeout`） |
| 退出入口路径 | Zed `app.rs:916-924`（`platform.on_quit` → `shutdown()`） |
| 现有持久化面（含裸 `fs::write`） | `token_manager.rs:1570,1605,1626,1686,3005`、`admin/service.rs:73,1073`、`model/config.rs:417,424` |
| 模型目录/冷却不落盘 | `token_manager.rs:851`（内存态）、`:4307`（`test_cooldown_state_not_persisted`） |
| secret 字段清单 | `src/kiro/model/credentials.rs:23,27,52,121` |
| SQLite/keyring 选型 | crates.io：`rusqlite` 0.40.2（`bundled` feature）、`keyring` 4.2.0（v1 后端 = Keychain/Windows/Secret Service） |
| `db-keystore` 不采用 | crates.io：0.6.0-pre.2，依赖 `turso` 0.8.0-pre 系；keyring-rs README 不建议经 `cli` feature 整体引入 |
| `cargo-packager` 签名/公证配置面 | 仓库源码 `crates/packager/src/config/mod.rs:662-742`；crates.io 0.11.8；仓库 2026-09-09 推送，477 星 |
| GPUI Linux 系统依赖 | Zed `script/linux`：libxkbcommon-x11-dev、libfontconfig-dev、libwayland-dev、libx11-xcb-dev、libasound2-dev 等 |
| 根仓无 workspace 段、门禁现状 | 根 `Cargo.toml` 无 `[workspace]`；`.github/workflows/warning-gate.yaml`（ubuntu-latest + `--locked` + `-D warnings` + admin-ui/dist 占位） |
| shell M0 | `gpui-kit` 仓库 `crates/shell/README.md:28` |
| wry Linux 未就绪 | `gpui-kit` 仓库 `examples/webview/src/main.rs:32-33` |
