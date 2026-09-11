# Tasks: desktop-sqlite-storage

## 1. 依赖与骨架

- [x] 1.1 `desktop/crates/kiro-desktop/Cargo.toml` 加依赖：`rusqlite`
      （bundled）、`keyring`（默认 v1）、`parking_lot`、`ring`（复用已在锁
      内的版本）；生成并核对独立 `Cargo.lock`
- [x] 1.2 `store/mod.rs` 骨架：`SqliteStore` 结构、连接持有、`SecretBackend`
      注入点

## 2. schema 与迁移

- [x] 2.1 `schema.rs`：七表 DDL（§7.3），`PRAGMA journal_mode=WAL` /
      `busy_timeout=5000` / `synchronous=NORMAL` / `foreign_keys=ON`
- [x] 2.2 `secrets` 表复合主键 `(credential_id, field)`（每凭据最多 4 个
      secret 字段：`refresh_token` / `client_secret` / `kiro_api_key` /
      `proxy_password`），外键 `ON DELETE CASCADE`
- [x] 2.3 迁移幂等：重复开库不报错不丢数据，`schema_version` 可查；迁移测试

## 3. trait 实现

- [x] 3.1 `CredentialStore::load`：`credentials` + `secrets`（经钥匙串取明文）
      拼回 `KiroCredentials`，`needs_migration` 恒 false
- [x] 3.2 `CredentialStore::persist`：写事务内「先钥匙串后 SQLite」，钥匙串
      失败回滚不落库；persist 收全量列表，删除语义由差集完成（不在新列表的行
      连同 `secrets` 行与钥匙串条目一并清理）
- [x] 3.3 `ConfigStore::load` / `save`：`config.doc` 整表 JSON 往返
- [x] 3.4 `cache_dir` 返回 `<data_dir>`，stats / 余额缓存派生路径验证

## 4. 钥匙串与回退

- [x] 4.1 `secrets.rs`：`KeyringBackend`（Entry::new + set/get/delete，
      NoEntry → None）
- [x] 4.2 `crypt.rs` + `EncryptedFileBackend`：AEAD 加密 `secrets.enc`，
      密钥文件 0600；写读删往返测试
- [x] 4.3 后端选择：启动探测钥匙串（写探针再删），可用则钥匙串否则文件回退
- [x] 4.4 钥匙串条目缺失只禁用该凭据、不影响启动的处置路径

## 5. JSON 导入导出

- [x] 5.1 `json_io.rs`：JSON → SQLite 导入（成功备份 `.bak`）
- [x] 5.2 SQLite → JSON 导出，格式兼容 `credentials.example.*.json`
- [x] 5.3 导入导出往返一致性测试

## 6. 接线与并发

- [x] 6.1 桌面 `main.rs` 装配分支：显式 `--config` / `--credentials` 保留
      JSON store（等价 CLI 调试），否则用 `SqliteStore`（数据目录）；
      `json_io::detect_and_import` 在装配前调用一次
- [x] 6.2 并发写测试：多线程写不损坏（WAL + busy_timeout）
- [x] 6.3 模型目录落盘：`model_catalog` 读写，重启后可读回
- [x] 6.4 根仓最小改动：`test_seed_model_cache` 去 `#[cfg(test)]` 更名
      `seed_model_cache`；新增 `credential_model_catalog` 只读访问器；
      两处测试调用点随更名更新（`admin/service.rs`）
- [x] 6.5 桌面装配后回填：读 `model_catalog` 表 → 逐凭据 `seed_model_cache`；
      后台快照任务（120 秒一次 + 退出前）把内存目录落盘

## 7. 验收

- [x] 7.1 `desktop/` 内 `RUSTFLAGS="-D warnings" cargo check --release
      --all-targets --locked` 零告警
- [x] 7.2 `desktop/` 内 `cargo test --release` 全绿
- [x] 7.3 Xvfb 启动冒烟：SqliteStore 注入后窗口创建 + `/v1/models` 与
      change 2 等价
- [x] 7.4 根仓影响最小化验证：根侧 `cargo check --release --all-targets`
      告警数不变、全量 `cargo test` 绿；`openspec validate
      desktop-sqlite-storage` 通过；`git status` 无敏感文件
