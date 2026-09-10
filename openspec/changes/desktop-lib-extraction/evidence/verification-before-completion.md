# 验证记录: desktop-lib-extraction

日期：2026-09-10
分支：`dev-desktop`

## 基线（任务 1，实现前记录）

- 告警基线：`cargo check --release --all-targets` = **0**
- 冒烟基线：`/tmp/kiro-smoke/`（config 端口 8990，凭据 `[]`），
  `GET /v1/models` 返回 200 / 3014 字节，
  sha256 前 16 位 `5cae3c3da4cc711d`（`baseline-models.json` /
  `baseline-startup.log`）

## 4.1 告警

```
$ cargo check --release --all-targets 2>&1 | grep -c "^warning"
0
```

等于基线，零新增。

## 4.2 全量测试

```
$ cargo test --release
test result: ok. 874 passed; 0 failed; 0 ignored   # lib 单测
test result: ok. 1 passed; 0 failed                # bin（drain_backstop）
test result: ok. 3 passed; 3 ignored               # doctest（2 个需网络的
                                                   # 集成示例标 ignore）
```

共 878 通过。与任务 2 阶段的 875 相比多出的 3 个来自 lib 化后 doctest
开始参与编译：`src/kiro/model/requests/kiro.rs:15` 的示例引用了不存在的
API（`requests::KiroRequest` 再导出、`KiroRequest::new`、`to_json`，bin
时代 doctest 从不运行所以从未暴露），已按真实 API 修正并通过；其余两个
是原本就标 `ignore` 的。

## 4.3 冒烟复录与错误路径

冒烟复录（同目录、同端口、同凭据）：

```
$ curl -s -H "x-api-key: ..." http://127.0.0.1:8990/v1/models
200, 3014 字节
$ sha256sum baseline-models.json post-models.json
5cae3c3da4cc711d...  baseline-models.json
5cae3c3da4cc711d...  post-models.json   # 逐字节一致
```

启动日志与基线的唯一差异是发出日志的模块路径：装配日志从 `kiro_rs` 变为
`kiro_rs::bootstrap`（代码搬家的必然结果，非行为差异）。

错误路径（实现后逐条触发，退出码均为 1，错误信息经 `bootstrap` 的
`Err` 链传出）：

| # | 场景 | 结果 |
| --- | --- | --- |
| 1 | 非法 JSON 配置 | `启动失败: 加载配置失败`，exit=1 |
| 2 | 非法 JSON 凭据 | `启动失败: 加载凭据失败`，exit=1 |
| 3 | `defaultEndpoint` 未注册 | `启动失败: 默认端点 "nope" 未注册`，exit=1 |
| 4 | 凭据指向未注册端点 | `启动失败: 凭据 id=None 指定了未知端点 "nope"（已注册: ["ide"]）`，exit=1 |
| 5 | `MultiTokenManager` 构造失败（重复凭据 ID） | 无法经文件触发：`load_detailed` 的 `normalize_record` 不读数字 `id`（既有语义），文件加载后全为 `id=None`，由 `with_stores` 重排为 1/2，不会重复；由单测 `test_multi_token_manager_duplicate_ids` 覆盖该分支 |

第 5 条满足 design.md 的验收口径（「各配一条手动验证或单测」）。

## 4.4 OpenSpec 与工作区

```
$ openspec validate desktop-lib-extraction
Change 'desktop-lib-extraction' is valid
$ openspec validate --all
30 passed, 0 failed (30 items)
$ git status --short
 M docs/desktop-gpui-embedded-design.md
 M openspec/changes/desktop-lib-extraction/...（design/proposal/tasks + evidence）
 M spec/structure.md
 M src/anthropic/mod.rs
 M src/kiro/model/requests/kiro.rs
 M src/kiro/token_manager.rs
 M src/main.rs
?? src/bootstrap.rs
?? src/lib.rs
?? src/storage/
```

无 `.codegraph/` 或凭据文件误入。

## 与 design 草案的实现偏差（已回填 design/proposal/主设计文档）

1. `JsonFileStore` 拆成 `JsonCredentialStore` / `JsonConfigStore` 两个 store，
   各自持路径；`BootOptions` 因此不再携带 `config_path` / `credentials_path`
   字段（SQLite store 没有「路径」概念）。
2. `CredentialStore::load` 返回 `LoadedCredentials`（携带迁移标记）而非
   `Option<CredentialsConfig>`；`bootstrap` 内经 `migrate_to_native` 完成迁移。
3. `Bootstrapped` 不含 `app_state` 字段：`AppState` 是路由构建的产物，由
   `build_routes` 连同 `Router` 一起返还。
4. `MultiTokenManager` 的注入入口命名为 `with_stores`，同时接受
   `credential_store` 与 `config_store`。

## Documentation Sync

| 入口 | 是否需要同步 | 处理 |
| --- | --- | --- |
| README | 否 | 启动方式不变（仍是 `kiro-rs --config ...`），对外行为零变化 |
| AGENTS.md / CLAUDE.md | 否 | AI 纪律与验证命令不变 |
| `spec/structure.md` | 是 | `src/` 树补 `lib.rs` / `bootstrap.rs` / `storage/` 三行，随本提交 |
| `docs/desktop-gpui-embedded-design.md` | 是 | §3.2 / §3.3 / §4.3 / §7.2 / §12 回填实现终稿（`JsonFileStore` 拆双 store、`bootstrap` 签名、`with_stores`），随本提交 |
| `openspec/changes/desktop-lib-extraction/` | 是 | design / proposal 的草案签名回填为终稿，tasks 全勾，随本提交 |
| `docs/tooling-sources.md` | 否 | 无新增外部工具 |

## Residual Risk

- 变更未归档、未推送：按提交粒度纪律，代码与工件在本提交一起落库后，
  归档走 `openspec-archive-change` 流程，不在本提交内完成。
- 第 5 条错误路径（`MultiTokenManager` 构造失败）无法经文件触发，依赖
  单测 `test_multi_token_manager_duplicate_ids` 覆盖；若未来 `normalize_record`
  开始读取数字 `id`，该路径变为可触发，需补手动验证。
- lib 化后 doctest 开始参与编译，暴露并修正了一处失效文档示例
  （`src/kiro/model/requests/kiro.rs:15`）；后续新增 doc 示例若引用不存在的
  API 会在 `cargo test` 阶段直接失败，属预期收益。
- `admin-ui/dist` 的占位目录约定未变（`warning-gate.yaml` 已有 `mkdir -p`
  步骤），desktop CI 沿用；若 change 2 修改该步骤需复查。
