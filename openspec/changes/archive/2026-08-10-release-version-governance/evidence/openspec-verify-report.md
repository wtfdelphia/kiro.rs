# OpenSpec Verify Report

Change：`release-version-governance`
日期：2026-08-10（第二轮复核，含 CI 红/绿路径与分支收敛）
结论：**归档条件已满足**——32/33 完成，仅余 8.4 归档动作本身。任务 5.3 已由真实 Docker 构建取证，
7.5/7.6 已由真实 CI run 取证，临时 tag 已清理。

## 第二轮复核（归档就绪）

| 任务 | 状态 | 证据 |
| --- | --- | --- |
| 5.3 镜像 OCI label | **PASS** | Docker 29.1.3 实测：传入 `VERSION=v2026.8.10` 后 label 一致，缺省为 `unknown`，见 `evidence/docker-oci-version-label.md` |
| 7.5 CI 绿路径 | **PASS** | runs `31367150384`（7 腿产物 + release）与 `31367150378`（两架构 + manifest）全 success；Release 7 资产名均含 `v2026.8.10`；GHCR 出现 `v2026.8.10` 与 `latest` |
| 7.6 CI 红路径 | **PASS** | runs `31363555851` 与 `31363555870` 均因 Cargo 失配被拦，产物 job 全 skipped，无 Release 无镜像；两轮临时 tag 已从远端删除 |
| 7.7 gate 并行 | **PASS** | 两 gate 同秒启动，version gate 6s 失败而 warning gate 29s，实测不等待 |
| 7.4 非正式构建 | **PASS** | dev 滚动 workflow 无 version gate；`dev-latest` prerelease 含完整 commit |
| 8.4 归档 | 进行中 | 本轮执行 |

本轮另修复一处门禁真实缺陷：`actions/checkout` 改写本地 tag ref 导致附注 tag 误判，已改为从
远端 `ls-remote` 权威判定并新增 6 个回归用例（门禁测试 23 → 40 passed）。详见
`evidence/ci-red-green-path.md`。

分支按维护者要求收敛：`dev` 推送，`main` 与 `master` 均经 PR 合入（#6 #7 #8 #9 #10），
全程普通 merge，无强推、无 reset、无删分支；三分支树内容一致。

---

## 第一轮记录（保留）

## Completeness

| 项 | 结果 |
| --- | --- |
| `openspec status --change release-version-governance --json` | `isComplete: true`，proposal/design/specs/tasks 4/4 done |
| `openspec validate --all` | 21 passed, 0 failed |
| tasks.md | 33 项中 27 项完成；未完成为 5.3（环境 SKIPPED）、7.5、7.6、8.2、8.3、8.4（第一轮时状态） |
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
（第二轮：7.5/7.6 证据已补齐，归档条件成立。）
