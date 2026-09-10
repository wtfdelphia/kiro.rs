# Proposal: desktop-lib-extraction

为桌面版（路线 B，`docs/desktop-gpui-embedded-design.md` v2.1）铺路：把现有单目标
`kiro-rs` crate 变成 lib + bin 双目标，装配链抽成可复用函数，并落地存储接缝
（`CredentialStore` / `ConfigStore` trait + `JsonCredentialStore` / `JsonConfigStore` 实现），为后续桌面端
换用 SQLite 后端留出替换点。CLI 服务端行为逐字节不变。

## Why

- 桌面进程需要直接依赖本仓的核心装配（`MultiTokenManager` + `KiroProvider` +
  路由构建），但 `src/main.rs` 是唯一目标，装配链 200 多行内联在 `main()` 里，
  无法被第二个二进制复用。
- 存储读写散落在三处（`persist_credentials` / `save_config` / stats+余额缓存），
  直接写文件，桌面端无法替换为 SQLite。设计文档 §7.2 要求这三条调用线全部改道
  经过 trait，不留旁路；`JsonCredentialStore` / `JsonConfigStore` 是 CLI 的实现体，保证行为不变。
- 设计文档 §12 把存储 trait 与 JSON 文件 store 放在 change 1（而非 change 3），
  是为了让 change 2（app-shell）启动时就能用 JSON 文件 store 读配置与凭据，
  解开「shell 要读配置、存储在 change 3」的顺序环。

## What Changes

- 新增 `src/lib.rs`：声明全部模块，导出桌面端需要的最小装配面（`Config`、
  `MultiTokenManager`、`KiroProvider`、endpoint、`AdminService`、路由构建函数、
  `ProxyConfig`、`token` 初始化、`public_api`）与存储 trait。
- 新增 `src/bootstrap.rs`（或并入 `lib.rs`）：把 `main.rs` 的装配链抽成
  `bootstrap()` 与 `build_routes()` 两个函数。
- `src/main.rs` 改为调用上述两个函数后 `axum::serve`，优雅关闭与信号处理保留。
- 新增 `src/storage/mod.rs`：定义 `CredentialStore` / `ConfigStore` trait，
  提供 `JsonCredentialStore` / `JsonConfigStore`（复用现有 `common::atomic_file` 文件写逻辑）。
- 三条调用线改道经过 trait：
  - `MultiTokenManager::persist_credentials` → `CredentialStore::persist`
  - `MultiTokenManager::save_config` / `Config::save` → `ConfigStore::save`
  - `load_stats` / `save_stats` / 余额缓存读写 → 经存储门面
- `MultiTokenManager::new` 保留现有签名（内部包 `JsonCredentialStore`），新增接受
  `Arc<dyn CredentialStore>` 的构造入口。
- 孤儿文件 `src/debug.rs` / `src/test.rs` 维持不参与编译，不在本变更处理。

非破坏性：不改变任何对外 API、配置文件格式、凭据文件格式或运行时行为。

## Capabilities

本变更是纯结构重构加内部接缝引入，不改变任何需求级行为（CLI 读写同样的文件、
同样的格式、同样的路径），因此不新增或修改任何 capability。

### New Capabilities

无。

### Modified Capabilities

无。

## Impact

- 代码：`src/lib.rs`（新增）、`src/bootstrap.rs`（新增）、`src/main.rs`（改调
  lib）、`src/storage/`（新增）、`src/kiro/token_manager.rs`（存储调用改道）、
  `src/model/config.rs`（`save` 经 `ConfigStore`）、`src/admin/service.rs`（余额
  缓存经存储门面）。
- 依赖：无新增外部依赖（JSON 文件 store 复用现有 `common::atomic_file`）。
- 构建：`Cargo.toml` 增加 `[lib]` 目标；`warning-gate.yaml` 的 `--all-targets`
  会同时覆盖 lib 与 bin，需确认零新增告警。
- 后续：本变更是桌面版全部后续 change（app-shell / sqlite-storage / views /
  tray / packaging）的前置。
