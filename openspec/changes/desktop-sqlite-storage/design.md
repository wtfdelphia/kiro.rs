# Design: desktop-sqlite-storage

整体见 `docs/desktop-gpui-embedded-design.md`（v2.1）§7。本文只覆盖本变更
（SQLite 后端 + 钥匙串 + 加密文件回退 + JSON 导入导出）的实现层决策。

## Context

- change 1 已把三条持久化线收口到 `kiro_rs::storage::{CredentialStore,
  ConfigStore}`，并提供 `MultiTokenManager::with_stores` 注入点。
- change 2 的桌面骨架已能装配，但存储仍是 `JsonCredentialStore` /
  `JsonConfigStore`。
- `CredentialStore::load` 的返回是 `LoadedCredentials`（携带 `needs_migration`
  标记），`migrate_to_native` 有默认实现。SQLite 后端不需要「格式迁移」
  语义，`migrate_to_native` 保持默认（不操作）。
- rusqlite 0.40.2 / keyring 4.2.0 已在 crates.io 确认在架；keyring 4.x 默认
  `v1` feature，Linux 走 zbus Secret Service（纯 Rust，无 GTK）。

## Goals / Non-Goals

Goals:

- `SqliteStore` 实现两个 trait，桌面 `main.rs` 注入后启动装配、`/v1/models`
  与 change 2 冒烟等价。
- schema 迁移幂等（重复打开不报错、不丢数据），版本字段可查。
- 钥匙串写失败整体失败不落库；读取时条目缺失只禁用该凭据，不影响启动。
- 无 Secret Service 时自动回退加密文件，且回退路径有测试。
- JSON 导入导出往返一致（导出格式兼容 `credentials.example.*.json`）。
- 并发写不损坏（WAL + `busy_timeout` + 单写连接）。
- 模型目录落盘后，桌面重启时 `/v1/models` 能从 SQLite 读回，不退回空目录。

Non-Goals:

- 不新增任何视图 / 对话框（导入导出的函数与格式测试在本期，按钮入口放
  change 5）。
- 不改 CLI：`JsonCredentialStore` / `JsonConfigStore` 不动，`bootstrap` /
  `with_stores` 不动。根仓仅做一处最小改动（见 D7）：把模型目录的测试专用
  seed 入口公开为正式访问器，并新增一个只读访问器，供桌面端从 SQLite 回填
  内存目录。CLI 行为不变（仍走内存）。
- 不改 JSON 文件格式，不改 `credentials.json` / `config.json` 的字段定义。
- 不做 profileArn 冷却持久化（§7.3 明示维持现状）。
- 不引入 `db-keystore`（预发布，见 §7.4 取舍）。

## Decisions

### D1：`SqliteStore` 的模块布局

```text
desktop/crates/kiro-desktop/src/store/
├── mod.rs          # SqliteStore：连接持有 + trait 实现
├── schema.rs       # DDL 常量 + 迁移（幂等）
├── secrets.rs      # 钥匙串 / 加密文件回退（SecretBackend trait）
├── crypt.rs        # 加密文件的 AEAD 封装
├── json_io.rs      # JSON 导入导出（往返）
└── tests.rs        # 集成测试（迁移、并发、导入导出）
```

`SqliteStore` 持有：写连接 `parking_lot::Mutex<Connection>`、只读连接
（余额缓存）、`Arc<dyn SecretBackend>`。

### D2：trait 实现的语义对齐

`CredentialStore::load` 返回 `LoadedCredentials`：SQLite 侧把 `credentials`
表 + `secrets` 表（经钥匙串取明文）拼回 `KiroCredentials` 列表，
`needs_migration` 恒为 `false`，`migrate_to_native` 用默认实现。

`persist` 在一个写事务内：先写钥匙串（逐字段），成功后再写 `credentials`
/ `secrets` 表；钥匙串写失败则回滚不落库（§7.4）。

`cache_dir`：SQLite 后端返回 `<data_dir>`，供 `load_stats` / 余额缓存等
派生路径沿用（与 JSON 后端「凭据文件所在目录」对齐的锚点）。

`stats` / `balance_cache` 两表本期只建不用：运行时统计与余额缓存仍走
`cache_dir()` 派生的文件路径（根仓代码路径，不在本 change 改动范围），
两表留给后续 change 把文件路径收编进 SQLite。

### D3：secret 的字段拆分与回退

`SecretBackend` trait 抽象两条后端：

```rust
pub trait SecretBackend: Send + Sync {
    fn write(&self, key: &str, value: &str) -> anyhow::Result<()>;
    fn read(&self, key: &str) -> anyhow::Result<Option<String>>; // None=条目不存在
    fn delete(&self, key: &str) -> anyhow::Result<()>;
    fn is_system(&self) -> bool; // 供界面显示「系统钥匙串」还是「文件回退」
}
```

- `KeyringBackend`：`keyring::Entry::new("dev.kiro-rs.desktop", key)`，
  `set_password` / `get_password` / `delete_credential`。`NoEntry` 映射为
  `Ok(None)`，其余错误向上抛。（keyring 4.2 实测：`v1` feature 把
  `v1::*` 重导出到根，`Entry::new` 可能返回 `Error::NoDefaultStore`，
  `Entry::store_status()` 可在不建条目时探测后端初始化结果。）
- `EncryptedFileBackend`：`secrets.enc`，内存里是 `HashMap<String,String>`，
  整体序列化后 `ring` AEAD 加密（`AES_256_GCM`，12 字节随机 nonce 前置）。
  密钥为首启随机生成的 32 字节，存 `<data_dir>/secrets.key`（0600）。
  不套 pbkdf2：密钥本身就在本地文件里，口令派生只是多一层无收益的仪式。
  文件回退的安全边界是文件权限，与钥匙串的差距要在界面明示。

选择：启动时先探测钥匙串可用性（`store_status()` + 写探针条目再删），
可用用 `KeyringBackend`，否则用 `EncryptedFileBackend`，全程只选一个后端，
不做逐字段混用。

### D4：config 的整表 JSON

`config` 表单行 `doc TEXT`（§7.3），`ConfigStore::load` 反序列化、`save`
序列化。`Config::config_path` 在 SQLite 场景无文件路径，`save` 不依赖它；
`load` 返回的 `Config` 不设 `config_path`（与 `persist_load_balancing_mode`
的「路径未知仅进程内生效」回退一致）。

配置里的 `apiKey` / `adminApiKey` / `countTokensApiKey` / 代理密码本期留在
`doc` JSON 里，不拆进钥匙串：逐字段拆分会把 `Config` 的 20+ 字段一半搬进
secrets 管理，且与「整表 JSON 往返」的取舍直接冲突。拆分推迟到后续 change，
在 change 5 设置面板统一处理。本期钥匙串只覆盖凭据级四个字段：
`refresh_token` / `client_secret` / `kiro_api_key` / `proxy_password`。

### D5：并发与锁

- 写：`parking_lot::Mutex<Connection>`，所有 `persist` / `save_config` /
  stats / 余额写共用，事务保持短小。
- 读（余额缓存）：独立只读连接，WAL 下不阻塞写。
- 开库即 `PRAGMA journal_mode=WAL`、`PRAGMA busy_timeout=5000`、
  `PRAGMA synchronous=NORMAL`（WAL 下安全且更快）。

### D6：首启迁移（JSON → SQLite）

本期只提供存储层函数，不弹窗：`json_io::detect_and_import(data_dir)` 检测
`<data_dir>` 与 cwd 下的 `config.json` / `credentials.json`，存在则读入
SQLite，成功后把源文件改名 `.bak`。是否调用、何时提示，由 change 4/5 的
界面决定；本期 `main.rs` 在装配前调用一次（等价 CLI 的「有文件就读」）。

### D7：模型目录注入口（根仓最小改动）

桌面重启后要恢复 `/v1/models`，需要把 `model_catalog` 表读回内存目录。但
`MultiTokenManager` 当前唯一的本地写入口是 `test_seed_model_cache`，带
`#[cfg(test)]`，release 编译不可见。方案：

- 去掉 `#[cfg(test)]`，更名 `test_seed_model_cache` → `seed_model_cache`，
  语义不变（写入指定凭据的模型缓存并重建全局聚合）；
- 新增只读访问器 `credential_model_catalog(&self, id) -> Vec<UpstreamModelInfo>`，
  供桌面端把单凭据缓存快照落盘；
- 根仓两处测试调用点（`admin/service.rs:2163,2805`）随更名更新。

桌面装配链在 `bootstrap` 之后读 `model_catalog` 表、逐凭据调
`seed_model_cache` 回填（预热任务已在后台跑，两路写同一把锁，最终一致：
预热成功则上游新数据覆盖回填值，失败则保留回填值）。

落盘触发：`spawn_warmup_models` 在 `bootstrap` 内部，桌面侧拿不到刷新完成
信号，不为此加回调钩子。桌面端在装配完成后起一个后台快照任务（tokio 侧，
首拍延迟 20 秒等预热落地，此后每 120 秒一次）：逐凭据读内存目录，非空
则写 `model_catalog` 表。「退出前最后一拍」依赖两阶段退出，留给 change 5。
凭据 id 枚举走现有 `snapshot()`，不新增根仓 API。根仓改动锁定在两个
访问器上，`bootstrap` / trait / CLI 二进制零改动。

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| `rusqlite` bundled 编译时间显著增加（自带 SQLite 源码） | 一次性代价，进独立 `Cargo.lock`；`cargo check` 门禁照常 |
| keyring 的 Linux 后端依赖 zbus + Secret Service，headless 下探测失败 | 回退 `EncryptedFileBackend` 已设计且有测试；探测失败不阻塞启动 |
| 钥匙串写成功但 SQLite 写失败，产生悬空钥匙串条目 | 写入顺序「先钥匙串后 SQLite」+ SQLite 事务；悬空条目下次写同 key 覆盖，无泄漏风险 |
| 加密文件回退的口令存本地文件，安全边界弱于钥匙串 | 界面（change 5）明示「文件回退」；文档注明该回退只防偶发读取，不防拿到文件权限的攻击者 |
| JSON 导入导出格式漂移 | 导出格式对齐现有 `credentials.example.*.json`，往返一致性测试覆盖 |
| 模型目录落盘引入旧数据 | `refreshed_at` 时间戳 + 刷新即覆盖；目录过期由现有刷新逻辑处理 |
| 桌面端 `seed_model_cache` 回填与预热刷新竞态 | 回填在 `bootstrap` 返回后执行（预热任务已启动）；预热刷新成功即覆盖回填值（上游新数据优先），失败则保留回填值，两路最终一致 |
