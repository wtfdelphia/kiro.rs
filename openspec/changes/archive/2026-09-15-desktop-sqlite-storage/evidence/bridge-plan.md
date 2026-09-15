# Bridge Plan: desktop-sqlite-storage

门禁：`openspec-superpowers-bridge`。实现前把 OpenSpec 工件、项目规则、
CodeGraph/rg 证据与验证计划连成执行检查点。

- change：`desktop-sqlite-storage`
- 分支：`dev-desktop`（HEAD `09a31f4`，工作区在工件修订后为待提交状态）
- 状态：`openspec validate` 通过，state 非 blocked

## 一、范围 / 非目标 / 关键决策

范围（与 proposal 一致）：`SqliteStore` 实现 `CredentialStore` +
`ConfigStore`，schema 与幂等迁移，`keyring` 钥匙串 + 加密文件回退，JSON
导入导出，模型目录落盘，桌面 `main.rs` 接线。

非目标（与 design Non-Goals 一致）：不新增视图/对话框；不动
`bootstrap`/`with_stores`/CLI JSON store；不改 JSON 格式；不做 profileArn
冷却持久化；不引入 `db-keystore`。

关键决策见 design D1-D7，本 bridge 重点固化三处审核中发现并修订的矛盾。

## 二、三处矛盾的定夺（审核发现，工件已同步修订）

### 矛盾 1：config 级 secret 是否进钥匙串

- proposal 原文把「配置里的 `apiKey` / `adminApiKey` / 代理密码」列入钥匙串；
  design D4 与 tasks 3.3 却明确 config 是整表 JSON 往返。两者冲突。
- 证据：`Config` 字段 20+（`src/model/config.rs:23`），secret 字段为
  `api_key`、`count_tokens_api_key`、`proxy_password`、`admin_api_key`
  （`config.rs:50,67,84,88`）。逐字段拆分需把一半字段搬进 secrets 管理，
  与「整表 JSON」直接冲突。
- 定夺：本期钥匙串只覆盖凭据级四个字段 `refresh_token` / `client_secret` /
  `kiro_api_key` / `proxy_password`；config 级 secret 留在 `config.doc`，
  拆分推迟到 change 5。已同步修订 proposal 与 design。

### 矛盾 2：模型目录恢复需要根仓注入口，与「根仓零改动」冲突

- tasks 6.3 要求「重启后 `/v1/models` 不退回空目录」，但
  `MultiTokenManager` 唯一本地写入口 `test_seed_model_cache` 带
  `#[cfg(test)]`（`src/kiro/token_manager.rs:2692`），release 不可见；
  `/v1/models` 读 `global_model_catalog()`（`anthropic/handlers.rs:88,112`）。
- 证据：`codegraph impact MultiTokenManager::with_stores` 52 符号；
  `test_seed_model_cache` 调用点仅 2 处（`admin/service.rs:2163,2805`）。
- 定夺：根仓做最小改动，`test_seed_model_cache` 去 `#[cfg(test)]` 更名
  `seed_model_cache`，新增只读访问器 `credential_model_catalog`，两处测试
  调用点随更名更新。桌面装配后回填 + 后台快照落盘。`bootstrap`/trait/CLI
  二进制零改动。已修订 proposal、design（新增 D7）、tasks（6.4/6.5）。

### 矛盾 3：`secrets` 表主键撑不住多 secret 字段

- 主设计文档 §7.3 原定义 `secrets (credential_id INTEGER PRIMARY KEY ...)`
  每凭据一行；但每凭据最多 4 个 secret 字段，单行存不下。
- 定夺：改复合主键 `(credential_id, field)`，外键 `ON DELETE CASCADE` +
  `PRAGMA foreign_keys=ON`，删除凭据级联清理 `secrets` 行与钥匙串条目。
  已修订主文档 §7.3 与本 change tasks 2.2。

## 三、CodeGraph 证据

| 命令 | 结论 |
| --- | --- |
| `codegraph status` | 189 文件 / 3587 节点，WAL，索引可用 |
| `codegraph sync` | Added 15, Modified 4（补上 change 1/2） |
| `codegraph callers MultiTokenManager::with_stores` | 仅 `bootstrap` 与 `MultiTokenManager::new` 两处 |
| `codegraph impact MultiTokenManager::with_stores` | 52 符号，全部测试与构造路径 |
| `codegraph callees bootstrap` | 装配经 `Config::load`、`migrate_to_native`、`into_sorted_credentials` |
| `codegraph query seed model cache` | 确认 `test_seed_model_cache` 为唯一本地写入口 |

## 四、rg / 源码补盲（CodeGraph 不覆盖）

| 事项 | 证据 |
| --- | --- |
| 依赖版本 | `cargo search`：`rusqlite` 0.40.2、`keyring` 4.2.0 在架；`ring` 0.17.14 已在 `desktop/Cargo.lock` |
| 测试基础设施 | `tempfile`/`uuid`/`fastrand` 已在 `desktop/Cargo.lock`，可直接用于测试 |
| secret 字段清单 | 凭据级：`credentials.rs:27,52,121` + `proxy_password`；config 级见矛盾 1 |
| 导入导出格式基线 | `credentials.example.social/multiple/apikey/idc/external.json`、`config.example.json` |
| 编译前提 | 无 root，需 `PKG_CONFIG_PATH` + `LIBRARY_PATH` 指到本地 sysroot（`desktop/README.md` 固化）；`PKG_CONFIG_PATH` 已含 `alsa/libzstd/vulkan/x11-xcb/xcb-xkb/xkbcommon-x11` |
| stats / 余额缓存锚点 | 均经 `CredentialStore::cache_dir()` 派生（`token_manager.rs:1648`、`admin/service.rs:72`），SQLite 后端返回 `<data_dir>` 即兼容 |

## 五、任务到执行步骤映射

| 任务 | 执行步骤 | 验证 / 停止条件 |
| --- | --- | --- |
| 1.1 依赖 | 改 `desktop/.../Cargo.toml`，`cargo check` 生成锁 | 锁文件含四依赖，`--locked` 可解析 |
| 1.2-2.3 骨架与 schema | `store/mod.rs`+`schema.rs`，七表 + 复合主键 + PRAGMA | 迁移幂等测试绿 |
| 3.1-3.4 trait | `SqliteStore` 实现 load/persist/cache_dir + config 往返 | 单元测试绿，`needs_migration` 恒 false |
| 4.1-4.4 钥匙串回退 | `secrets.rs`+`crypt.rs`，探测写探针再删 | 写读删往返测试绿，回退路径有测试 |
| 5.1-5.3 导入导出 | `json_io.rs` 导入 `.bak` / 导出 / 往返 | 往返一致性测试绿 |
| 6.1 接线 | `main.rs` 装配分支 + `detect_and_import` | 编译零告警 |
| 6.2 并发 | 多线程写测试 | 不损坏 |
| 6.3/6.5 模型目录 | `model_catalog` 读写 + 回填 + 快照任务 | 重启可读回 |
| 6.4 根仓访问器 | 更名 + 新增访问器 + 两处调用点 | 根侧测试绿、告警不变 |
| 7.1-7.4 验收 | 双门禁 + 全量测试 + Xvfb 冒烟 | 见第六节 |

## 六、必跑验证

1. `desktop/`：`RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked`（零告警）
2. `desktop/`：`cargo test --release`（全绿）
3. 根仓：`cargo check --release --all-targets`（告警数不变，基线 0）
4. 根仓：`cargo test`（全量绿，验证 CLI 零影响）
5. Xvfb 冒烟：`xvfb-run -a target/release/kiro-desktop`（SqliteStore 注入后窗口创建 + 装配日志）
6. `openspec validate desktop-sqlite-storage`
7. `git status --short`（无敏感文件、无 `.codegraph/`、无临时缓存误入）

## 七、README / AGENTS / spec / openspec 同步判断

- `docs/tooling-sources.md`：新增 `rusqlite` / `keyring`（仅 `desktop/`）依赖登记，标注引入变更 `desktop-sqlite-storage`。
- `desktop/README.md`：补一句 change 3 状态（存储换 SQLite）与数据目录含 `kiro.db`。
- 主文档 `docs/desktop-gpui-embedded-design.md`：§7.3 `secrets` 表 schema 已修订（复合主键）。
- `spec/`：本变更为存储后端，无需求级行为变化，`skip_specs: true`，无需同步主 specs。
- `AGENTS.md`：不新增启动/构建/部署命令（desktop 门禁口径不变），无需改。

## 八、停止条件

- OpenSpec 工件缺失、互相矛盾或状态 blocked（当前已修订并通过校验）。
- 发现未写入规格的高风险影响。
- 工作区存在会被提交的真实 config / credentials / token / Cookie / 本地缓存。
- 无法确定验证命令或剩余风险。
- 钥匙串在无 Secret Service 的 CI/本机环境探测行为与设计不符且回退路径无法覆盖。
