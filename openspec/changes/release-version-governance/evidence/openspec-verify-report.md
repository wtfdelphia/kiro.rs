# OpenSpec Verify Report

Change：`release-version-governance`
日期：2026-08-10
结论：**归档条件未完全满足**——本地实现与验证已完成，任务 5.3 为环境 SKIPPED，任务 7.5/7.6/8.4 需维护者提供 CI 证据与授权。

## Completeness

| 项 | 结果 |
| --- | --- |
| `openspec status --change release-version-governance --json` | `isComplete: true`，proposal/design/specs/tasks 4/4 done |
| `openspec validate --all` | 21 passed, 0 failed |
| tasks.md | 33 项中 27 项完成；未完成为 5.3（环境 SKIPPED）、7.5、7.6、8.2、8.3、8.4 |
| evidence | `bridge-plan.md`、`spec-compliance-report.md`、`openspec-verify-report.md`、`verification-before-completion.md` 均存在 |

## Correctness

每个 Requirement 都有可核对的实现落点与本地证据：

- 身份唯一一致：`scripts/check_release_version.py`（tomllib 读 Cargo、`datetime.date` 校验日历日、`git cat-file -t` 判附注、tag 指向、`merge-base --is-ancestor` 判 main 可达），11 个正反例用例全绿。
- main 唯一落点：两条正式 workflow 的 `push.branches` 已为 `main`；远端拓扑核验 `--no-merges` 独有提交 `0 0`、树内容一致，迁移前置成立。
- 人工发布不绕过：`publish=true` 在非矩阵 pre-check 解析唯一 `v*` tag 并置 `enforce_version=true`；矩阵版本优先取 `release_tag`；`publish=false` 无登录/推送/manifest。
- 运行时与镜像：`cargo run --quiet -- --version` = `kiro-rs 2026.8.10`；启动首条 info 为 `kiro-rs v...` 且早于配置失败日志；Dockerfile 有 `ARG VERSION=unknown` 与 OCI version label，workflow 传入门禁确认版本。
- 非正式构建：`build-dev-release.yaml` 保留 Commit/Short SHA，未接 version gate。
- MSRV：`rust-version = "1.97.1"`；`rustc 1.97.1` 下默认与 `--no-default-features` 两种 `-D warnings --locked` 检查均 exit 0。
- admin-ui：`package.json` 未改，README 已声明不跟随 CalVer。

## Coherence

- design.md 的 D1-D6 与实现一一对应，无相互冲突的事实源。
- README 的版本约定、MSRV、发布清单、Docker tag 表、人工发布规则与 workflow 实际行为一致（`beta` 来源已随触发器迁移改为 `main`）。
- AGENTS.md 无需修改；本 change 不改变 AI 协作或告警门禁口径。
- `warning-gate.yaml` 与 `build-dev-release.yaml` 的既有行为未被本 change 修改。

## 未完成项与阻塞点

| 任务 | 状态 | 说明 |
| --- | --- | --- |
| 5.3 镜像 inspect | SKIPPED | 本机无 Docker（`docker` 命令不存在），OCI label 仅有静态断言 |
| 7.5 CI 绿路径 run URL | 待维护者 | 需真实 tag 发布 run |
| 7.6 CI 红路径 run URL + 临时 tag 清理 | 待维护者 | 需创建/删除远端临时失配 tag |
| 8.4 归档 | 待授权 | 需用户确认实现与证据后执行 |

归档应在 7.5/7.6 证据补齐后进行；在此之前不建议执行 `openspec-archive-change`。
