# Spec Compliance Report

Change：`release-version-governance`
日期：2026-08-10
范围：任务 2.1-7.7 实现后、归档前审查
总体状态：**PASS**（2026-08-10 第二轮更新：Docker inspect、CI 红路径与绿路径均已实测取证）

## 六维审查

| 维度 | 状态 | 依据 |
| --- | --- | --- |
| Scope | PASS | 改动限于 `Cargo.toml`、`Cargo.lock`、`src/main.rs`（1 行版本日志）、`Dockerfile`、`.gitignore`、`README.md`、`docs/version-governance-optimization-design.md`、三个 workflow、`scripts/check_release_version.py` 与 `scripts/tests/`。未触碰协议、认证、凭据、Admin 或 admin-ui 构建；`admin-ui/package.json` 保持 `1.0.0`（`git diff --stat -- admin-ui/` 无输出） |
| Design | PASS | D1 附注 CalVer 身份、D2 双 gate 并列、D3 人工发布身份解析在非矩阵 job、D4 稳定触发器迁 `main`、D5 运行时与 OCI 可观测、D6 MSRV 1.97.1 均已落地 |
| Scenarios | PASS（本地）| 7 个 Requirement 的场景由 `scripts/tests/` 23 个用例与本地命令覆盖；镜像 label 场景已由 2026-08-10 补验的真实 `docker build` + `docker image inspect` 证实（`evidence/docker-oci-version-label.md`） |
| Project Rules | PASS | OpenSpec 流程完整；`cargo check --release --all-targets` 告警 0，相对基线零新增；无真实凭据、token、`.codegraph/` 进入候选提交 |
| Verification | PASS | 本地命令全部真实运行；Docker inspect 已补验；CI 红路径（runs 31363555851 / 31363555870）与绿路径（runs 31367150384 / 31367150378）均已实测，红路径临时 tag 已清理 |
| Docs Sync | PASS | README 已同步 MSRV、CalVer 约定、附注 tag 发布清单、`main` 落点、正式/非正式边界、人工镜像发布规则、`beta` 来源改为 `main`；AGENTS.md 无需改动（既有 OpenSpec/零告警/CI 红路径纪律已覆盖） |

## 场景到证据映射

| Requirement | 证据 |
| --- | --- |
| 正式版本唯一一致身份 | `scripts/check_release_version.py` + `test_check_release_version.py` 11 例（合法、Cargo 失配、非法日期、修订后缀、补零、轻量 tag、tag 指向错误、main 不可达、0/1/多 tag 解析） |
| main 为唯一稳定落点 | `test_release_workflow_graph.py::test_stable_release_triggers_listen_to_main_only`；`git rev-list --left-right --count --no-merges origin/main...origin/master` = `0 0`，`git diff --quiet origin/main origin/master` exit 0 |
| 人工镜像发布不得绕过身份 | `docker-build.yaml` pre-check 在 `publish=true` 时调用 `resolve` 并输出 `enforce_version/release_tag`；矩阵 `Determine version` 优先消费 `release_tag`；`publish=false` 不登录、不推送 |
| 运行时与容器元数据可观测 | `cargo run --quiet -- --version` → `kiro-rs 2026.8.10`；`test_startup_reports_version_before_config_error` 断言首条 info 为 `kiro-rs v...`、早于配置失败日志；`Dockerfile` `ARG VERSION` + OCI label，实机 inspect 得 `org.opencontainers.image.version=v2026.8.10`、缺省为 `unknown`；workflow 传 `build-args` |
| 非正式构建可追溯 | `build-dev-release.yaml:218-219` 保留 Commit 与 Short SHA，且不含 version gate（`test_dev_rolling_workflow_has_no_version_gate`） |
| MSRV 1.97.1 | `Cargo.toml` `rust-version`；`rustc 1.97.1` 下两种 `-D warnings --locked` 检查 exit 0 |
| admin-ui 不独立版本化 | `admin-ui/package.json` 未改；README 已声明 |

## 发现项

1. 门禁失败传播已由真实红路径 run 证实：两条 workflow 的 `build`/`release`/`manifest` 均为 skipped（非 failure、非 success），与 `test_every_artifact_job_depends_on_both_gates` 的静态断言一致。
1b. 本轮红路径另暴露并修复一处真实缺陷：`actions/checkout` 执行 `git fetch --no-tags origin +<sha>:refs/tags/<tag>` 覆盖本地 tag ref，使 `git cat-file -t` 返回 `commit`，导致任何合法附注 tag 被误报为轻量 tag。已改为经 `git ls-remote` 的 peeled `^{}` 行从远端权威判定，并新增 6 个回归用例（23 → 40 passed）。
2. `version-gate` job 不带 `if`，避免 skipped 传播误伤分支与 dry-run 产物，已由 `test_version_gate_is_never_skipped_for_non_release_builds` 固定。
3. 目标 CalVer 取 `2026.8.10`（当日，远端最新正式 tag 为 `v2026.8.4`，`v2026.8.10` 不存在）。发版日期若不是当天，需维护者改 `Cargo.toml` 与 `Cargo.lock` 后重跑检查。

## 剩余风险

- 本机无 actionlint：workflow lint 仍为 SKIPPED，只经 YAML 解析、作业图断言与真实 run 行为验证。
- 任务 5.3 的镜像 inspect 已在 Docker 29.1.3 上完成（linux/amd64、经典 builder）；arm64 矩阵腿与 GHA 缓存行为已由绿路径 run `31367150378` 的 amd64/arm64 双腿与 manifest 成功间接覆盖。
- 已发布 GHCR 镜像的 OCI label 未经直接 inspect（package 私有，本机 token 无 `read:packages`）；一致性由 CI 日志的 `--build-arg VERSION=v2026.8.10` 与本地同一 Dockerfile 的 label 实测两段证据推得。
- 正式 tag 必须打在 `main`：`main` 与 `master` 因双 PR 各自产生 merge commit，`master` HEAD 不从 `main` 可达，在其上打 tag 会被门禁正确拒绝。
- 任务 7.5/7.6 的真实 CI 绿/红 run URL 需维护者提供；临时失配 tag 的创建与删除不由本次实现执行。
- 未 push、未创建 PR、未合并、未创建远端 tag、未归档。
