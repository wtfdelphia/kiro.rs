# Verification Before Completion

> 本文件含三轮记录。第三轮（2026-08-10 补验）完成任务 5.3 的真实 Docker 取证，第二轮（2026-08-10）是实现后的最终验证，第一轮（2026-08-07）保留为实现前基线。

## 第三轮：任务 5.3 Docker 补验（2026-08-10）

范围：仅任务 5.3。本机取得 Docker 访问权限后，将第二轮的 SKIPPED 替换为真实构建与 inspect 证据。

### Verification

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `docker info --format '{{.ServerVersion}}'` | `29.1.3` | 本机 Docker 可用，第二轮「命令不存在」结论已失效 |
| `docker build --build-arg VERSION=v2026.8.10 -t kiro-rs-version-label-test:v2026.8.10 .` | 成功；镜像内 `./kiro-rs --version` 输出 `kiro-rs 2026.8.10` | Dockerfile 全链路可构建，二进制自报版本等于 Cargo 版本 |
| `docker image inspect ... --format '{{json .Config.Labels}}'` | `{"org.opencontainers.image.version":"v2026.8.10"}` | OCI version label 精确等于传入 build-arg |
| 不传 `VERSION` 并附加 source/description labels 构建后 inspect | `version=unknown` 且 source/description 共存 | `ARG VERSION=unknown` 默认值生效，未覆盖既有 labels |
| `docker rmi`（两个临时 tag） | 已删除 | 未推送任何仓库，无残留发布副作用 |
| `openspec validate --all` | 21 passed, 0 failed | 文档改动后 schema 仍通过 |
| `git status --short` | 见下方说明 | 仅本 change 相关文件，无凭据或 `.codegraph/` |

明细见 `evidence/docker-oci-version-label.md`。

### Residual Risk（第三轮后更新）

- 本轮只验证 linux/amd64 与经典 builder；arm64 矩阵腿和 `cache-*: type=gha` 仍只有 CI 侧证据。
- `docker-build.yaml` 传入的版本值本身仍靠静态断言，真实取值待任务 7.5 的 CI run。
- 本机仍无 actionlint；workflow 只经 `yaml.safe_load` 与作业图断言。
- 任务 7.5/7.6 的 Actions run URL 与临时 tag 清理仍待维护者；任务 8.4 归档待用户授权。

---

## 第二轮：实现后最终验证（2026-08-10）

范围：任务 2.1-7.7 实现完成；任务 5.3 环境 SKIPPED，7.5/7.6 待维护者，8.4 待授权。

### Verification

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `cargo check --release --all-targets` | exit 0，**告警 0** | 与实现前基线 0 一致，零新增告警 |
| `RUSTFLAGS='-D warnings' cargo check --release --all-targets --locked`（rustc 1.97.1） | exit 0 | MSRV 默认 feature 判定面通过；执行后已 `Remove-Item Env:RUSTFLAGS` 并确认为空 |
| `RUSTFLAGS='-D warnings' cargo check --release --all-targets --locked --no-default-features` | exit 0 | MSRV 无默认 feature 判定面通过 |
| `cargo test --release --locked` | 724 passed, 0 failed | 既有 Rust 测试无回归 |
| `cargo metadata --locked --format-version 1 --no-deps` | `version 2026.8.10`、`rust_version 1.97.1` | Cargo 接受 MSRV 声明，锁文件与 manifest 一致 |
| `cargo update --workspace --offline` | `kiro-rs v2026.3.1 -> v2026.8.10` | `Cargo.lock` 已同步，未改动任何依赖版本 |
| `cargo run --quiet -- --version` | `kiro-rs 2026.8.10` | 二进制自报版本等于 Cargo 版本 |
| `python -m unittest scripts.tests.test_check_release_version` | 11 passed | 门禁正反例：合法、Cargo 失配、非法日历日、修订后缀、补零、轻量 tag、tag 指向错误、main 不可达、0/1/多 tag |
| `python -m unittest scripts.tests.test_release_governance_files` | 7 passed | gate 只读权限与身份校验、caller 接线、Dockerfile OCI label、MSRV、启动日志顺序（首条 info 为 `kiro-rs v`，早于「加载配置失败」） |
| `python -m unittest scripts.tests.test_release_workflow_graph` | 5 passed | 两条正式 workflow 只监听 `main`；build/release/manifest 传递依赖 version-gate 与 warning-gate；version-gate 不 needs warning-gate；version-gate 无 `if`（避免 skipped 传播）；dev workflow 无 version gate |
| `python scripts/check_release_version.py resolve --commit HEAD` | exit 1，`::error::expected exactly one v* tag at HEAD, found 0: none` | 人工发布在无 tag 时前置失败，annotation 可读 |
| `python scripts/check_release_version.py validate --tag v2026.8.4 --commit HEAD --main-ref origin/main` | exit 1，`::error::release tag 'v2026.8.4' must be an annotated tag` | 轻量历史 tag 被拒，失败信息含具体原因 |
| `git rev-list --left-right --count --no-merges origin/main...origin/master` | `0 0` | master 无独有非合并提交 |
| `git diff --quiet origin/main origin/master` | exit 0 | 两分支树内容一致，触发器迁移前置成立 |
| `openspec validate --all` | 21 passed, 0 failed | schema 通过 |
| `git status --short --untracked-files=all` | 9 项 modified + 12 项 untracked，全部属于本 change | 无 `config.json`、`credentials.*`、`.codegraph/`；`__pycache__` 已由新增 `.gitignore` 规则排除 |
| `docker` / `actionlint` | 命令不存在 | **SKIPPED**：任务 5.3 镜像 inspect 与 workflow lint 无法本地执行（注：`docker` 部分已由第三轮补验推翻，见本文件顶部） |

### Documentation Sync

| 文档 | 状态 |
| --- | --- |
| `README.md` | 已更新：MSRV 1.97.1、CalVer 与附注 tag 约定、`main` 发版清单、正式/非正式边界、人工镜像发布规则、Docker `beta` 来源改为 `main`、admin-ui 不跟随 CalVer |
| `AGENTS.md` | 无需修改；OpenSpec、零告警、CI 红路径与安全纪律已覆盖本 change |
| `docs/version-governance-optimization-design.md` | 已定稿并含复审修订记录 |
| `openspec/specs/` | 归档时同步 delta 到 `openspec/specs/release-version-governance/`；未新建顶层 `spec/` |
| `docs/tooling-sources.md` | 本 change 不更新；它是本机工具快照，不是 MSRV 事实源 |
| `admin-ui/package.json` | 未修改（保持 `1.0.0`） |

### Residual Risk

- 目标 CalVer 取当日 `2026.8.10`（远端最新正式 tag 为 `v2026.8.4`，`v2026.8.10` 不存在）。若实际发版不在当天，需改 `Cargo.toml` 与 `Cargo.lock` 并重跑 Cargo 检查。
- 本机无 Docker：OCI version label 只有静态断言，label 值与传入版本的一致性需 CI 或有 Docker 环境 inspect 补证。（第三轮已补证，本条仅保留为当时状态记录）
- 本机无 actionlint：workflow 仅经 `yaml.safe_load` 结构解析与作业图断言，未做 Actions 专用 lint。
- GitHub 上 gate 失败的 skipped 传播行为只有静态作业图证据，需真实红路径 run 证实（任务 7.6）。
- 任务 7.5/7.6 的 Actions run URL 与临时 tag 清理需维护者执行。
- 未 push、未创建 PR、未合并、未创建远端 tag、未归档。

---

## 第一轮：实现前基线（2026-08-07）

日期：2026-08-07
范围：设计文档定稿、创建 OpenSpec change 并完成实现前 bridge；尚未开始实现。

## Verification

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `openspec list --json` | `changes: []`（创建前） | 无活跃 change 冲突 |
| `openspec status --change release-version-governance` | 4/4 artifacts complete | proposal、design、specs、tasks 完整 |
| `openspec validate --all` | 21 passed, 0 failed | 全部主规格与本 change 校验通过 |
| `codegraph status` / `query main` / `impact main` / `explore main` | 索引 up to date；运行时代码影响集中于 `src/main.rs` | 版本日志应位于 tracing 初始化之后、配置/凭据处理之前 |
| `rustc --version` / `cargo --version` | 均为 1.97.1 | 当前环境与目标 MSRV 一致 |
| `cargo check --release --all-targets` | exit 0，无 warning 行 | 当前告警基线为 0；本轮无实现代码改动，无新增告警 |
| `git rev-list --left-right --count origin/main...origin/master` / `git diff --quiet origin/main origin/master` | `1 1`；树内容相同 | 分支内容相同但 ancestry 尚未收敛，触发器迁移仍被阻塞 |
| `git diff --check -- docs/version-governance-optimization-design.md openspec/changes/release-version-governance` | exit 0，无输出 | 无空白错误 |
| `git status --short --untracked-files=all` | 1 个已修改设计文档；change 目录为未跟踪文件 | 未发现敏感配置或 `.codegraph/` 进入候选变更 |

## Documentation Sync

| 文档 | 本轮状态 | 后续要求 |
| --- | --- | --- |
| `docs/version-governance-optimization-design.md` | 已按 Q1-Q10 确认结果定稿 | 实现发现事实变化时同步修订 |
| `README.md` | 未修改 | 实现任务 6.1 同步版本约定、MSRV 与发布清单 |
| `AGENTS.md` | 无需修改 | 现有 OpenSpec、零告警和 CI 红路径纪律已覆盖 |
| `openspec/specs/` | 未直接修改 | change 归档时把 delta 同步为长期规格 |
| `docs/tooling-sources.md` | 未修改 | 本 change 不把本机工具快照作为发布版本事实源 |

## Residual Risk

- 尚未运行实现后的 Cargo、CI、Docker 和运行时验证；当前仅建立了实现前基线。
- 目标 CalVer 尚未明确，任务 3.1 开始前必须由维护者确认。
- `main` 与 `master` 的分叉收敛必须由维护者以非破坏方式完成并提供证据，当前未执行。
- CI 版本门禁的绿路径与红路径尚未实施；临时远端 tag 的创建和删除由维护者执行。
- 本机无 Docker 与 actionlint；相关验证实施时须由 CI/有 Docker 的环境补证，或明确记录 SKIPPED。
- 未 push、未创建 PR、未合并、未归档。
