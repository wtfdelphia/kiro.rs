# Spec Compliance Report

Change：`release-version-governance`
日期：2026-08-10
范围：任务 2.1-7.7 实现后、归档前审查
总体状态：**WARN**（本地实现与验证完整；CI 红/绿 run 与 Docker inspect 需维护者补证）

## 六维审查

| 维度 | 状态 | 依据 |
| --- | --- | --- |
| Scope | PASS | 改动限于 `Cargo.toml`、`Cargo.lock`、`src/main.rs`（1 行版本日志）、`Dockerfile`、`.gitignore`、`README.md`、`docs/version-governance-optimization-design.md`、三个 workflow、`scripts/check_release_version.py` 与 `scripts/tests/`。未触碰协议、认证、凭据、Admin 或 admin-ui 构建；`admin-ui/package.json` 保持 `1.0.0`（`git diff --stat -- admin-ui/` 无输出） |
| Design | PASS | D1 附注 CalVer 身份、D2 双 gate 并列、D3 人工发布身份解析在非矩阵 job、D4 稳定触发器迁 `main`、D5 运行时与 OCI 可观测、D6 MSRV 1.97.1 均已落地 |
| Scenarios | PASS（本地）| 7 个 Requirement 的场景由 `scripts/tests/` 23 个用例与本地命令覆盖；镜像 label 场景已由 2026-08-10 补验的真实 `docker build` + `docker image inspect` 证实（`evidence/docker-oci-version-label.md`） |
| Project Rules | PASS | OpenSpec 流程完整；`cargo check --release --all-targets` 告警 0，相对基线零新增；无真实凭据、token、`.codegraph/` 进入候选提交 |
| Verification | WARN | 本地命令全部真实运行并记录；Docker inspect 已补验通过，真实 CI run（7.5/7.6）仍明确待维护者并写入剩余风险 |
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

1. 门禁失败传播只有静态作业图证据（`test_every_artifact_job_depends_on_both_gates` 覆盖 build/release/manifest 的传递依赖），GitHub 上的 skipped 传播行为仍需真实红路径 run 证实。非阻塞。
2. `version-gate` job 不带 `if`，避免 skipped 传播误伤分支与 dry-run 产物，已由 `test_version_gate_is_never_skipped_for_non_release_builds` 固定。
3. 目标 CalVer 取 `2026.8.10`（当日，远端最新正式 tag 为 `v2026.8.4`，`v2026.8.10` 不存在）。发版日期若不是当天，需维护者改 `Cargo.toml` 与 `Cargo.lock` 后重跑检查。

## 剩余风险

- 本机无 actionlint：workflow lint 仍为 SKIPPED，只经 YAML 解析与作业图断言。任务 5.3 的镜像 inspect 已在 Docker 29.1.3 上完成（linux/amd64、经典 builder）；arm64 矩阵腿与 GHA 缓存行为仍待 CI 证据。
- 任务 7.5/7.6 的真实 CI 绿/红 run URL 需维护者提供；临时失配 tag 的创建与删除不由本次实现执行。
- 未 push、未创建 PR、未合并、未创建远端 tag、未归档。
