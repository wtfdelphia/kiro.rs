# Proposal: desktop-sqlite-storage

桌面版（路线 B，`docs/desktop-gpui-embedded-design.md` v2.1）的第三步：把
change 1 引入的存储接缝（`CredentialStore` / `ConfigStore`）落地为 SQLite
后端 `SqliteStore`，secret 字段走系统钥匙串（`keyring`）并在无 Secret
Service 的环境回退到自实现加密文件；JSON 文件降级为导入导出交换格式。
不新增任何桌面 UI 视图，不动 CLI 行为。

## Why

- change 2 的桌面骨架仍用 `JsonCredentialStore` / `JsonConfigStore` 读写
  JSON。§7.1 的审计表明四个持久化文件里只有凭据是原子写，配置、统计、
  余额缓存中途崩溃都会留半个文件；跨文件更新无事务；明文 secret 直接
  落盘。桌面常驻应用对「写一半崩溃」的暴露面远大于 CLI。
- §7.2 已在 change 1 把三条调用线收口到 trait，但桌面端还没有真正可替换
  的存储后端，`MultiTokenManager::with_stores` 的注入点空等一个实现。
- 没有 SQLite + 钥匙串，change 4-6 的凭据视图、设置面板、常驻都没有
  可靠的数据源，顺序上必须先落地存储。

## What Changes

- 新增 `desktop/crates/kiro-desktop/src/store/`（桌面 workspace 内）：
  `SqliteStore` 同时实现 `kiro_rs::storage::{CredentialStore, ConfigStore}`，
  桌面 `main.rs` 装配时经 `BootOptions` 注入，替换 change 2 的 JSON store。
- SQLite schema 与迁移（§7.3）：`schema_version` / `credentials` / `secrets`
  / `config` / `stats` / `balance_cache` / `model_catalog` 七表，WAL 模式 +
  `busy_timeout`，迁移幂等。`rusqlite` 用 `bundled` feature。
- 并发（§7.3）：`parking_lot::Mutex` 保护单写连接 + 独立只读连接读余额
  缓存；不用 `tokio::sync::Mutex`（GPUI 线程在 `background_spawn` 里同步
  调存储，跨运行时等锁会拖住后台线程池）。
 - 钥匙串（§7.4）：凭据级 secret 字段（`refresh_token` / `client_secret` /
  `kiro_api_key` / `proxy_password`）经 `keyring` 4.x v1 后端存系统钥匙串，
  服务名 `dev.kiro-rs.desktop`，条目 key `cred-<id>:<field>`；SQLite 只存
  `keyring_ref`。配置级 secret（`apiKey` / `adminApiKey` /
  `countTokensApiKey` / 代理密码）本期留在 `config.doc` 整表 JSON 里
  （见 design D4），拆分推迟到后续 change。
- 无 Secret Service 环境（headless / 极简桌面）回退为自实现加密文件
  `<data_dir>/kiro-rs/secrets.enc`（口令派生密钥、0600 权限），不引入
  预发布的 `db-keystore`。
- JSON 导入导出（§7.5）：首启检测到 JSON 文件提示导入进 SQLite（导入成功
  备份为 `*.bak`）；设置面板导出与现有 `credentials.example.*.json` 格式
  兼容。本期只做存储层的导入导出函数与格式往返测试，导出入口的按钮放
  change 5。
 - 模型目录落盘（§7.3 取舍）：从仅内存改为写 `model_catalog` 表，桌面重启
  窗口后 `/v1/models` 不退回空目录；为此根仓给 `MultiTokenManager` 补两个
  公开访问器（`seed_model_cache`、`credential_model_catalog`），CLI 行为
  不变（仍走内存）。

## Capabilities

本变更新增桌面端的存储后端，不改变 kiro-rs 服务端任何需求级行为，也不
改变 JSON 文件格式本身。

### New Capabilities

无（存储后端，无可被规格化的需求行为；格式细节见 design）。

### Modified Capabilities

无。

## Impact

  代码：新增在 `desktop/crates/kiro-desktop/src/store/`；`main.rs` 改装配
  分支（显式 `--config` / `--credentials` 时保留 JSON store，否则换
  `SqliteStore`）。根仓只改 `src/kiro/token_manager.rs`：`test_seed_model_cache`
  去掉 `#[cfg(test)]` 更名 `seed_model_cache`、新增 `credential_model_catalog`
  只读访问器，两处测试调用点随更名更新，无行为变化。
- 依赖：`desktop/` 内新增 `rusqlite`（bundled）、`keyring`（v1）、
  `parking_lot`、加密库（`ring` 已在桌面锁里，优先复用；否则 `aes-gcm` +
  `argon2`）。根仓依赖零变化。
- 构建：`rusqlite` bundled 自带 SQLite 源码，不依赖系统库；`keyring` 的
  Linux v1 后端走 zbus（纯 Rust），无 GTK 依赖，与 change 2 的托盘问题不同。
- 测试：迁移幂等、钥匙串不可用回退、导入导出往返一致性、并发写不损坏，
  全部在桌面 workspace 内；CLI 全量测试须保持全绿（验证根仓零影响）。
- 后续：change 4-6 的视图与服务都以 `SqliteStore` 为数据源；导出的按钮入口
  在 change 5 设置面板接上。
