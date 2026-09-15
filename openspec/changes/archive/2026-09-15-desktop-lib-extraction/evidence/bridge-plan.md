# Bridge Plan: desktop-lib-extraction

日期：2026-09-10
分支：`dev-desktop`（`a742bdc` 之后，工作区含本变更工件）
前置文档：`docs/desktop-gpui-embedded-design.md` v2.1 §3/§4.3/§7.2

## 范围与非目标

范围：`kiro-rs` crate 加 `lib.rs` 变双目标；`main.rs` 装配链抽成
`bootstrap()` / `build_routes()`；三条持久化线收口到 `CredentialStore` /
`ConfigStore` trait，CLI 实现体 `JsonFileStore` 保持字节级行为。

非目标：不引入 SQLite / keyring / gpui 依赖；不动孤儿文件 `debug.rs` /
`test.rs`；不改配置文件格式与默认路径；不新建 `desktop/` crate（change 2）。

关键设计决策（见 design.md）：lib/bin 双目标不拆仓库（D1）；serve 与信号处理
留 bin（D2）；`load` 返回 `Option` 复刻 `load_detailed` 错误语义（D3）；
`MultiTokenManager` 双构造入口（D4）；`pub(crate)` 一律不动（D5）。

## 高风险项

1. 行为回归：装配链含五处 `process::exit(1)` 与两处迁移/回写逻辑，抽取时任何
   分支错位都会改变启动语义。缓解：基线冒烟前后比对 + 五条错误路径逐一触发。
2. `persist_load_balancing_mode` 的旁路（codegraph 补查发现，design 已修正）：
   它直接 `Config::load` + `Config::save`，若漏改道，SQLite 阶段会出现配置双写。
3. 零告警门禁：lib 化后 `pub(crate)` 与 `pub` 的可见性视角变化可能翻出新告警，
   尤其 `anthropic` 模块那批 `pub(crate) use` 转发项在同 crate 双目标下的行为。

## CodeGraph 证据

| 命令 | 结论 |
| --- | --- |
| `codegraph status` | 索引 up to date，189 文件 / 3587 节点 |
| `codegraph impact persist_credentials` | 74 个受影响符号；生产调用方 14 处（`set_profile_arn`、`add_credential`、`delete_credential`、`force_refresh_token_for` 等），全部走同一方法，改道只动一处 |
| `codegraph callers save_config` | 6 个，全在 `src/admin/service.rs` 的设置更新方法 |
| `codegraph callers Config::save` | 4 个：`save_config`、`persist_load_balancing_mode`、`config.rs:482` 测试、1 处未解析 |

## rg / 源码补盲

- `rg "block_in_place" src`：三处，`token.rs:118`（count_tokens，不在本变更）、
  `token_manager.rs:1607`（随 `persist` 平移进 `JsonFileStore`）、
  `online_auth.rs:473`（注释，不动）。
- `rg "std::fs::write" src`：`config.rs:424`、`token_manager.rs:1686`、
  `service.rs:1073`，与 design §7.1 审计一致。
- `rg -l "axum" src`：核心层零命中结论不变，lib 导出面含 axum 类型
  （`build_routes` 返回 `axum::Router`），依赖已在。
- workflow：`warning-gate.yaml` 的 `--all-targets` 会自动覆盖新 lib 目标，
  无需改 CI；`admin-ui/dist` 占位步骤已有。
- 凭据路径：`.gitignore` 覆盖 `config.json` / `credentials.json`，本变更不新增
  入库文件类型。

## 任务到执行步骤映射

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1 冒烟基线 | 临时目录配 config+凭据，`cargo run`，`curl /v1/models` | 响应体与日志存档 | 基线拿不到则停，不开始改码 |
| 1.2 告警基线 | `cargo check --release --all-targets` 计数 | 数字存档 | 当前基线已有告警则先报告再动手 |
| 2.1-2.5 存储接缝 | 先写 `src/storage/`，再逐条改道，每条改道后跑 `cargo test kiro::token_manager` 局部 | 存储单测全绿 | 任一改道破坏既有测试且半小时内无解则回退该条 |
| 3.1-3.4 lib 抽取 | `lib.rs` → `bootstrap.rs` → `main.rs` 瘦身，逐步编译 | 每步 `cargo check` | 编译错误滚雪球超过两步则回退 |
| 4.1-4.4 回归 | 告警比对、全量测试、冒烟复录、五条错误路径触发 | 全部等于基线 | 任一不等则定位修复，不得掩盖 |

## 必跑验证

1. `cargo check --release --all-targets`（告警数 = 1.2 基线）
2. `cargo test`（全量）
3. 冒烟：起服务 + `curl /v1/models` 与基线一致；五条 `exit(1)` 路径触发（坏配置
   路径、坏凭据文件、未知默认端点、凭据指向未知端点、空凭据列表）
4. `openspec validate desktop-lib-extraction`
5. `git status --short` 无凭据与 `.codegraph/` 误入

## 文档同步判断

- README：启动方式不变，无需更新；`docs/desktop-gpui-embedded-design.md` 是本
  变更的依据，实现中若偏离需回填该文档
- AGENTS.md / spec/：无影响
- `spec/structure.md`：`src/` 树需补 `lib.rs` 与 `storage/` 两行，随实现同提交
  更新（README/AGENTS/spec 同步纪律）

## 停止条件

- 冒烟基线无法建立（服务起不来）
- 改道导致既有测试失败且非测试本身需要更新
- 出现需要改配置/凭据文件格式才能推进的情况（越界，应拆新 change）
- 告警无法在不违反零告警纪律的前提下清零
