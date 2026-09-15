## 1. 基线记录

- [x] 1.1 记录实现前冒烟基线：`cargo run` 起服务（临时 config/凭据），`curl /v1/models` 存响应体与启动日志片段到变更目录（或会话记录），实现后复录比对
- [x] 1.2 记录当前告警基线：`cargo check --release --all-targets 2>&1 | grep -c "^warning"` 存结果

## 2. 存储接缝

- [x] 2.1 新增 `src/storage/mod.rs`：定义 `CredentialStore`、`ConfigStore` trait 与 `JsonCredentialStore` / `JsonConfigStore` 实现，`JsonCredentialStore::load` 复刻 `CredentialsConfig::load_detailed` 的错误语义
- [x] 2.2 `JsonCredentialStore::persist` 收编 `persist_credentials` 内的序列化写盘与 `block_in_place` 分支；`load_stats`/`save_stats` 的路径派生改经 `cache_dir()`
- [x] 2.3 `ConfigStore` 接入：`MultiTokenManager::save_config` 改调 `ConfigStore::save`，`Config::save` 保留供测试；`AdminService::new_with_runtime` 增加可选 cache_dir 参数，余额缓存路径派生规则不变
- [x] 2.4 `MultiTokenManager` 新增 `with_stores` 构造入口，现有 `new` 内部包 `JsonCredentialStore`；`credentials_path` 字段委托给 store
- [x] 2.5 存储层单元测试：`JsonCredentialStore` 往返一致性、`load` 错误语义、`persist` 原子性（复用现有 `write_atomic`）

## 3. lib 抽取与装配

- [x] 3.1 新增 `src/lib.rs`：声明全部模块并按设计文档 §3.3 最小面 `pub use` 导出
- [x] 3.2 新增 `src/bootstrap.rs`：`Bootstrapped` / `BootOptions` / `bootstrap()` / `build_routes()`，把 `main.rs` 装配链平移过来，五处 `process::exit(1)` 改 `Err`
- [x] 3.3 `src/main.rs` 瘦身：解析参数 → `bootstrap` → `build_routes` → `axum::serve`，信号处理与 `drain_backstop` 保留
- [x] 3.4 `Cargo.toml` 确认 `[lib]` 目标生效（双目标构建），`main.rs` 内对 crate 内部类型的引用改为经 lib 路径

## 4. 回归验证

- [x] 4.1 `cargo check --release --all-targets`：告警数等于 1.2 的基线（零新增）
- [x] 4.2 `cargo test` 全量通过
- [x] 4.3 冒烟复录：起服务 + `curl /v1/models`，响应体与启动日志与 1.1 基线一致；五条 `exit(1)` 改错路径各触发一次验证退出码与错误信息
- [x] 4.4 `openspec validate desktop-lib-extraction` 通过；`git status --short` 无 `.codegraph/` 或凭据文件误入
