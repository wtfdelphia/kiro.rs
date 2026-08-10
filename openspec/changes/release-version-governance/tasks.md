## 1. 基线与实施桥接

- [x] 1.1 运行 `openspec-superpowers-bridge`，记录规格到文件、风险、工具和验证命令的映射
- [x] 1.2 记录 `Cargo.toml` / `Cargo.lock` 当前版本、`cargo check --release --all-targets` 告警数、三条产物 workflow 触发器及 `origin/main...origin/master` 拓扑基线
- [x] 1.3 确认工作树中的 `config.json`、`credentials.json`、`credentials.*` 与 `.codegraph/` 不进入变更

## 2. 稳定分支前置

- [x] 2.1 由维护者通过普通 PR 或 merge 将 `master` 所需独有提交纳入 `main`，不得强推、reset 或删除分支
- [x] 2.2 获取远端最新状态并验证 `master` 所需提交均可从 `origin/main` 到达；验证失败时停止后续触发器迁移

## 3. Cargo 版本、MSRV 与运行时可观测性

- [x] 3.1 将 `Cargo.toml [package].version` 更新为目标 CalVer 并同步 `Cargo.lock`
- [x] 3.2 在 `Cargo.toml` 声明 `rust-version = "1.97.1"`
- [x] 3.3 在 tracing 初始化后、凭据处理前输出首条 `kiro-rs v<version>` info 日志
- [x] 3.4 验证 `cargo run --quiet -- --version` 与 Cargo 版本一致，并验证启动版本日志早于凭据相关 info 日志

## 4. Version Gate 与正式发布接线

- [x] 4.1 新增只读权限的 reusable `.github/workflows/version-gate.yaml`，使用 Python `tomllib` 和日期标准库校验严格 `vYYYY.M.D`、有效日期、附注 tag、Cargo 一致性与 `origin/main` 可达性
- [x] 4.2 为 version gate 增加本地正反例覆盖：合法身份、Cargo 失配、非法日期、修订后缀、轻量 tag 与 main 不可达
- [x] 4.3 在 `build.yaml` 中将 version gate 与 warning gate 并列接入所有正式产物前置依赖，稳定分支触发器由 `master` 迁至 `main`，保留 dev artifact 路径
- [x] 4.4 在 `docker-build.yaml` 中并列接入 version gate，稳定分支触发器由 `master` 迁至 `main`
- [x] 4.5 收紧 Docker 人工发布：`publish=true` 时在非矩阵 job 从当前提交解析唯一附注 `v*` tag 并复用正式门禁，禁止自由 version 覆盖；`publish=false` 保持无副作用 dry run
- [x] 4.6 验证 version gate 失败时 build、Release、镜像和 manifest job 均不启动，且失败 annotation 包含修复指引

## 5. Docker 版本元数据

- [x] 5.1 在 Dockerfile 最终 stage 增加 `ARG VERSION=unknown` 与 `org.opencontainers.image.version` OCI label
- [x] 5.2 从 `docker-build.yaml` 向 Docker build 传入门禁确认的版本，并保留现有 source/description labels
- [x] 5.3 使用测试版本构建并 inspect 镜像，确认 OCI version label 与传入值一致；记录无法运行 Docker 时的 SKIPPED 原因与剩余风险

## 6. 文档与规格同步

- [x] 6.1 更新 README 的版本约定、MSRV、正式/非正式构建边界、附注 tag 命令、main 发版清单与人工镜像发布规则
- [x] 6.2 更新 build.yaml 与 docker-build.yaml 的 dispatch 描述和示例，不改 build-dev-release.yaml 的 dev-latest 语义
- [x] 6.3 核对 `admin-ui/package.json` 保持不变，并在 README 中声明内嵌前端不独立跟随 CalVer
- [x] 6.4 运行 `openspec validate --all`，确保 delta spec 可在归档时同步到 `openspec/specs/release-version-governance/`，且不新建顶层 `spec/`

## 7. 本地与 CI 验证

- [x] 7.1 运行 `cargo check --release --all-targets`，报告告警数并确认相对基线零新增
- [x] 7.2 在 Rust 1.97.1 上运行 `RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked`
- [x] 7.3 在 Rust 1.97.1 上运行 `RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features`
- [x] 7.4 验证 branch artifact、dev-latest Release 元数据与 Actions run 均可追溯到 commit，且 dev workflow 不出现 version gate
- [ ] 7.5 由维护者执行正式版本一致的 CI 绿路径，并提供 build 与 Docker workflow 的 Actions run URL
- [ ] 7.6 由维护者创建 Cargo 失配的临时附注 `v*` tag，提供两个 workflow 均被 version gate 拦截且产物 job skipped 的 run URL，随后确认远端临时 tag 已删除
- [x] 7.7 验证同一 run 中 version gate 与 warning gate 并行，version gate 不等待 warning gate 完成

## 8. 合规与完成门禁

- [x] 8.1 运行 `spec-compliance-check` 并修复范围、设计、场景、项目规则、验证和文档同步问题
- [x] 8.2 运行 `openspec-verify-change`，产出归档前验证报告
- [x] 8.3 运行 `verification-before-completion`，记录真实命令、告警数、文档同步、`git status --short` 与剩余风险
- [ ] 8.4 用户确认实现与证据后再运行 `openspec-archive-change`；本任务不自动推送、创建 PR、合并或删除远端分支
