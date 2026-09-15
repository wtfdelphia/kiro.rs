# Verification Before Completion: desktop-sqlite-storage

门禁：`verification-before-completion`。只记录本会话真实运行的命令与结果。

分支 `dev-desktop`，日期 2026-09-11。所有命令均在无 root 环境用本地
sysroot（`desktop/README.md` 固化方法）跑。

## 判定命令与结果

| 验证项 | 命令 | 结果 |
| --- | --- | --- |
| 桌面编译门禁 | `RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked`（desktop/） | 零告警零错误，`GATE_EXIT=0` |
| 桌面测试 | `cargo test --release`（desktop/） | 32 过 0 败（`store::` 全绿） |
| 根仓告警 | `cargo check --release --all-targets`（根） | 0 告警（基线 0，无新增） |
| 根仓全量测试 | `cargo test --release`（根） | 全绿，`EXIT=0`（874 + 其余套件） |
| OpenSpec 校验 | `openspec validate desktop-sqlite-storage` | 通过 |

## Xvfb 启动冒烟（`KIRO_RS_DATA_DIR=/tmp/kiro-smoke-ch3-data`）

首启（数据目录含 `credentials.json` + `config.json`，timeout 15s）：

- 存活满 15 秒，`EXIT=124`（timeout 正常退出）
- 日志链：单实例锁 → 钥匙串探测失败（无 Secret Service）→ 回退加密文件 →
  SQLite 就绪 → JSON 导入 1 条凭据 + 配置 + 两份 `.bak` 备份 → 窗口创建 →
  核心装配完成，凭据数: 1
- 预热刷新对上游乐观失败（403 占位 key），不影响装配

重启（同数据目录，timeout 10s）：

- 从 SQLite 读回 1 条凭据，未重复导入（`empty` 判断生效）
- 窗口创建 + 装配完成，凭据数: 1

## 冒烟后数据核验（python3 + sqlite3 直读 `kiro.db`）

- `schema_version = 1`
- `credentials` 表：`fields` 列无明文 secret（仅 id/authMethod/machineId/disabled）
- `secrets` 表：`(1, 'kiro_api_key', 'cred-1:kiro_api_key')`，只存钥匙串引用
- `config` 表：单行 `doc` 完整配置
- `secrets.enc` / `secrets.key` 权限均 `-rw-------`（0600）
- `secrets.enc` 不含 `ksk_smoke_test_placeholder` 明文（已加密）

## 测试失败与修复留痕

实现期 3 个测试失败，均为测试断言本身问题，非实现缺陷，已修复：

- `model_catalog_roundtrip`：外键约束要求先建凭据（补 `persist`）；读回按
  `model_id` 排序与插入顺序无关（改集合比较）
- `concurrent_writes_do_not_corrupt`：`persist` 是全量替换语义，8 线程并发
  覆盖后剩 2 条为正确行为，断言改为校验 `integrity_check=ok` + 最终自洽

## 剩余风险 / 未验证

- 有显示环境的窗口画面：本机无头，验收按 change 2 既定口径（日志链等价）
- 真实系统钥匙串路径（macOS Keychain / Windows Credential Manager / 有
  Secret Service 的 Linux 桌面）：本环境无钥匙串，只验证了回退分支；
  `KeyringBackend` 的写/读/删与探针逻辑经单测覆盖，系统后端联调留给有
  钥匙串的环境
- 导出按钮入口在 change 5，`export_credentials` / `export_config` 本期只有
  测试引用（已加最小 `#[allow(dead_code)]` 并注明）
- 「退出前最后一拍」模型目录快照依赖两阶段退出，留给 change 5
