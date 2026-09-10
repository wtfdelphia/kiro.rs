# Design: desktop-lib-extraction

整体桌面架构见 `docs/desktop-gpui-embedded-design.md`（v2.1）。本文只覆盖本变更
（lib 抽取 + 存储接缝）的实现层决策，以及后续实现前必须钉死的取舍。

## Context

- `src/main.rs` 是当前唯一目标，`main()` 内联约 200 行装配链：参数与日志 →
  加载 `Config` → 加载 `CredentialsConfig`（含导入格式迁移）→ `KIRO_API_KEY`
  注入 → 代理配置 → 端点注册表 → `MultiTokenManager` + 预热 → `KiroProvider` →
  `token::init_config` → anthropic/openai/admin 路由 → `axum::serve` + 优雅关闭。
- 持久化有三条直接写文件的线（凭据 `write_atomic`、config/stats/余额缓存裸
  `fs::write`），且 `AdminService::new_with_runtime` 内部自算余额缓存路径，
  桌面端没有替换点。
- 全仓测试用 `tower` oneshot 直接打 Router，不依赖 `main()`；`warning-gate.yaml`
  以 `cargo check --release --all-targets --locked` 为准。

## Goals / Non-Goals

Goals:

- 桌面二进制可依赖本仓 lib 完成「加载配置凭据 → 构建核心 → 构建路由」全流程。
- 三条持久化线收口到两个 trait，CLI 侧实现体（`JsonCredentialStore` / `JsonConfigStore`）保持现有字节级
  行为。
- 零新增编译告警；CLI 全量测试与冒烟行为不变。

Non-Goals:

- 不引入 SQLite、keyring、rusqlite（change 3）。
- 不改配置/凭据文件格式与路径默认值。
- 不动 `src/debug.rs` / `src/test.rs` 孤儿文件。
- 不拆独立 `kiro-core` crate（v2.1 已否决，见设计文档 §3.1）。

## Decisions

### D1：lib/bin 双目标，不拆仓库

`src/lib.rs` 声明全部模块，`src/main.rs` 保留为 bin，Cargo 自动推断双目标。
备选是拆 `kiro-core` 独立 crate：否决理由见设计文档 §3.1（桌面进程本来就要带
axum 监听，拆 crate 省不了依赖，多一份版本与门禁负担）。

### D2：`bootstrap` 独立模块，`serve` 留在 main

装配链抽到 `src/bootstrap.rs`（lib 模块），导出：

```rust
pub struct Bootstrapped {
    pub config: Config,
    pub token_manager: Arc<MultiTokenManager>,
    pub kiro_provider: Arc<KiroProvider>,
    pub endpoint_names: Vec<String>,
    pub proxy_config: Option<ProxyConfig>,
    pub is_multiple_format: bool,
}

pub struct BootOptions {
    pub credential_store: Arc<dyn CredentialStore>,
    pub config_store: Arc<dyn ConfigStore>,
}

/// 装配到「核心就绪」为止：不构建路由、不 bind、不 serve
pub fn bootstrap(opts: &BootOptions) -> anyhow::Result<Bootstrapped>;

/// 全量路由（anthropic + openai + admin + admin_ui），admin_key 为空串/None
/// 时不挂 admin 路由，与现 main.rs 判定一致
pub fn build_routes(b: &Bootstrapped) -> (axum::Router, AppState);
```

`main()` 瘦身为：解析参数 → `bootstrap` → `build_routes` → `axum::serve` +
`shutdown_signal` + `drain_backstop`（信号处理是 CLI 运行时语义，留在 bin）。
`AppState` 是路由构建的产物，所以不放在 `Bootstrapped` 里，而是由
`build_routes` 连同 `Router` 一起返还：`main()` 的优雅关闭需要
`app_state.ws_shutdown`，admin 装配（在 `build_routes` 内）需要
`app_state.auth` / `ws_settings` / `ws_admission`。

`BootOptions` 不携带路径字段：路径等后端细节封装在 store 实现内部
（`JsonCredentialStore::new(path)` / `JsonConfigStore::new(path)`），
桌面端的 SQLite store 根本没有「路径」概念，这里不重复暴露。
备选是把 serve 也抽进 lib：否决，桌面端的服务器生命周期是 `ServerControl`
状态机（设计文档 §5），与 CLI 的信号驱动不共用代码，强行抽出来会出现两套
shutdown 语义的分支耦合。

`bootstrap()` 返回失败而非 `process::exit(1)`：现 main 里五处 `exit(1)`（配置、
凭据、端点校验 ×2、token_manager）全部改成 `Err`，`main()` 统一打印后退出。
退出码语义不变（非零）。

### D3：存储 trait 的最小面

```rust
// src/storage/mod.rs
pub trait CredentialStore: Send + Sync {
    /// 语义与 CredentialsConfig::load_detailed 一致：后端源缺失/为空返回
    /// 空多凭据配置，不是错误；返回 LoadedCredentials 携带迁移标记
    fn load(&self) -> anyhow::Result<LoadedCredentials>;
    fn persist(&self, credentials: &[KiroCredentials]) -> anyhow::Result<()>;
    /// 缓存目录（统计等派生路径的锚点），无后端源时返回 None
    fn cache_dir(&self) -> Option<PathBuf>;
    /// 若加载结果需格式迁移，写回原生格式并返回备份位置；默认不操作
    fn migrate_to_native(&self, _config: &CredentialsConfig) -> anyhow::Result<Option<PathBuf>>;
}

pub trait ConfigStore: Send + Sync {
    fn load(&self) -> anyhow::Result<Config>;
    fn save(&self, config: &Config) -> anyhow::Result<()>;
}

/// CLI 默认实现：凭据与配置拆成两个独立 store（各自持路径）
pub struct JsonCredentialStore { credentials_path: PathBuf }
pub struct JsonConfigStore { config_path: PathBuf }
```

三条调用线的接线点：

| 现状 | 改道后 |
| --- | --- |
| `persist_credentials` 内序列化 + `write_atomic` | 序列化保留在 `MultiTokenManager`，写盘调 `CredentialStore::persist`；`block_in_place` 分支逻辑平移进 `JsonCredentialStore` |
| `save_config` → `Config::save` | `save_config` 改调 `ConfigStore::save`；`Config::save` 保留，`config.rs:482` 的 roundtrip 测试直接用它 |
| `persist_load_balancing_mode`（codegraph callers 补查确认：不经过 `save_config`，直接 `Config::load` + `Config::save`，且有「路径未知仅进程内生效」分支） | 改经 `ConfigStore::load` + `ConfigStore::save`，保留重新加载语义与路径未知时的 warn 分支 |
| `load_stats` / `save_stats` 直接 `fs::write` | 路径派生改经 `CredentialStore::cache_dir()`（`join("kiro_stats.json")`），写盘逻辑留在 `token_manager` |
| `AdminService` 自算余额缓存路径（`service.rs:71-73`） | 不改：它经 `token_manager.cache_dir()` 派生，该方法委托给 store 后路径规则不变 |

`load` 返回 `Option`：现在 `main.rs` 对「凭据路径存在但无文件」与「路径为
None」的处置不同（前者 `CredentialsConfig::load_detailed` 报错退出，后者不存在
该分支）。为保持行为，`JsonCredentialStore::load` 严格复刻 `load_detailed` 的错误语义，
trait 文档注明。

备选是把 stats/余额缓存拆成独立 `CacheStore` trait：否决，两条都挂在凭据的
`cache_dir` 下，独立 trait 只会多一个接缝没有多一个实现；change 3 的
`SqliteStore` 实现同一个 `CredentialStore` 即可。

### D4：`MultiTokenManager` 双构造入口

现有 `MultiTokenManager::new(config, credentials, proxy, credentials_path,
is_multiple_format)` 签名保留，内部包 `JsonCredentialStore`；新增
`with_stores(config, credentials, proxy, credential_store, config_store,
is_multiple_format)` 供桌面端注入。`credentials_path`
字段被 `Arc<dyn CredentialStore>` 取代，`cache_dir()` 委托给 store。现有 60+
测试构造路径不动。

### D5：可见性策略

`pub(crate)` 项一律不改。lib 导出面只加 `pub use` 再导出（`bootstrap`、
`storage`、以及桌面端需要的既有 `pub` 类型所在模块）。`anthropic` 模块内部
的 `pub(crate) use`（converter/handlers/middleware 的互用项）保持原样，它们
在同 crate 的 lib 内部仍然可见。

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| 装配链抽取引入行为偏差 | 实现前先记录冒烟基线：`cargo run` 起服务后 `curl /v1/models` 的响应体与启动日志；抽取后同法复录比对。`exit(1)` 改 `Err` 的五条路径各配一条手动验证或单测 |
| `Config::save` 与 `ConfigStore` 双路径漂移 | 本变更后 `Config::save` 只留作测试工具与 CLI 兜底，生产写路径唯一走 `ConfigStore`；change 3 实现 `SqliteStore` 时复查无旁路 |
| lib/bin 双目标产生新告警（如仅 bin 用到的 `pub` 项在 lib 视角变 dead） | 按零告警纪律用最小可见性调整解决，禁止 `#[allow]`；`cargo check --release --all-targets` 闭环 |
| `rust-embed` 的 `admin-ui/dist` 在 lib 编译期校验，CI 占位目录约定变化 | 不变：`warning-gate.yaml` 已有 `mkdir -p admin-ui/dist` 步骤，本变更不触碰 |
| 桌面端在 change 2 才出现，本变更的导出面可能多给或少给 | 导出面按设计文档 §3.3 的最小清单执行；少给时 change 2 补 `pub use` 属一行改动 |
