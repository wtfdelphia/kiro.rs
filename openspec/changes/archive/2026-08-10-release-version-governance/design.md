## Context

当前正式发布由两个独立值驱动：Rust 编译期读取 `Cargo.toml [package].version`，GitHub Actions 则用 `github.ref_name` 或人工输入命名 Release、资产和镜像。两者没有机器约束，已经连续五次发布漂移。镜像没有 OCI version label，启动日志也没有项目版本。

仓库同时存在三条产物路径：正式 `v*` tag、分支/人工 artifact、`dev-latest` 滚动预发布。只有第一类需要全表面版本一致；后两类需要 commit 可追溯，但不能被描述为正式版本。GitHub 默认分支为 `main`，正式 workflow 仍监听已经分叉的 `master`。现有 `warning-gate.yaml` 已建立 reusable workflow 模式和 Rust 1.97.1 的固定检查环境。

约束：

- CI、Docker 和发布脚本变更必须经 OpenSpec、红/绿 CI 证据和零新增告警门禁。
- 不改协议、认证、凭据、Admin 或 admin-ui 构建机制。
- 不强推、重置或删除 `main/master`，不追溯重写历史 tag。
- 远端临时 tag 的创建和删除由维护者执行。

## Goals / Non-Goals

**Goals:**

- 让正式 `v*` 发布的 Cargo 版本、tag、二进制、日志、资产名、镜像 tag 和 OCI 元数据一致。
- 在构建开始前用机器门禁阻止非法或漂移的正式发布身份。
- 统一 `main` 为稳定发版落点，并保证迁移不遗漏 `master` 独有提交。
- 保留人工 dry run 和开发滚动包，同时使非正式构建可追溯到 commit。
- 明确 Rust 1.97.1 为 MSRV，并建立可验证的升级纪律。

**Non-Goals:**

- 不自动创建 tag，不引入 cargo-release/release-plz。
- 不把 git SHA 编译进二进制，不引入 build.rs/vergen。
- 不修改历史 Release、tag 或二进制。
- 不启用分支保护，不决定停止监听后的 master 删除/归档策略。
- 不独立版本化 admin-ui，不生成 CHANGELOG。

## Decisions

### D1. Cargo 是声明源，附注 tag 是发布身份

`Cargo.toml [package].version` 是源码版本声明源。正式发布 tag 必须严格为 `vYYYY.M.D`，去掉 `v` 后与 Cargo 版本完全相等，且日期必须是有效日历日期。该格式没有修订后缀，因此同一自然日只能存在一个正式版本；紧急修复使用下一个自然日。

新正式 tag 必须是附注 tag。门禁用 `git cat-file -t refs/tags/<tag>` 验证对象类型为 `tag`；历史轻量 tag 不改写。相比继续使用轻量 tag，附注 tag 为发布身份保留 tagger、创建时间和说明，且不需要签名密钥治理。

门禁使用 Ubuntu runner 自带 Python 3 的 `tomllib` 结构化读取 `[package].version`，并用标准库日期解析校验 CalVer；不依赖 `sed` 的“第一个 version 行”假设。

### D2. Version gate 与 warning gate 并行

新增 `.github/workflows/version-gate.yaml`（`workflow_call`、`permissions: contents: read`）。`build.yaml` 和 `docker-build.yaml` 的产物 job 同时 `needs` pre-check、warning-gate 和 version-gate。两个 gate 正交并行，版本错误无需等待编译门禁结束。

正式 tag 事件向 version gate 传入 `github.ref_name`。gate 使用完整 tag 历史，依次校验：

1. tag 匹配严格 CalVer 且日期有效；
2. tag 为附注 tag；
3. Cargo 版本等于 tag 去掉 `v`；
4. `GITHUB_SHA` 可从 `origin/main` 到达。

任一失败均输出带修复动作的 annotation 并非零退出，使全部构建、Release、镜像和 manifest job 保持 skipped。

### D3. 人工镜像发布复用正式身份，dry run 保持自由

`docker-build.yaml` 的非矩阵 pre-check 继续计算 `should_publish`，并新增 `release_tag` 输出。

- 自动 tag 事件：`release_tag = github.ref_name`。
- `workflow_dispatch` 且 `publish=false`：不要求正式身份，允许自由 `version` 作为本次构建标签，且不得登录、推送或创建 manifest。
- `workflow_dispatch` 且 `publish=true`：完整获取 tag，从当前 `github.sha` 解析唯一一个 `v*` tag；零个或多个均失败。随后 version gate 验证它是附注 tag、与 Cargo 一致且从 main 可达。镜像版本只能使用门禁确认的 `release_tag`，不得使用自由输入覆盖。

把发布身份解析放在非矩阵 job，避免矩阵输出“最后完成腿获胜”的不确定性。

### D4. main 是唯一稳定发版落点

迁移前先获取远端最新状态，把 `master` 独有提交通过普通 PR 或 merge 纳入 `main`，并验证所需提交均可从 `origin/main` 到达。确认后，`build.yaml` 与 `docker-build.yaml` 的稳定分支 push 触发器从 `master` 改为 `main`；build.yaml 保留 dev artifact 路径，build-dev-release.yaml 保留 dev 滚动路径。

不允许强推、reset 或删除分支。若无法证明 main 已包含 master 所需内容，迁移停止，workflow 触发器不得切换。

### D5. 正式版本可观测，非正式构建可追溯

正式版本：

- Clap 继续从 `CARGO_PKG_VERSION` 提供 `--version`。
- tracing 初始化后、任何凭据处理日志前输出 `kiro-rs v<version>`。
- Docker 最终 stage 声明 `ARG VERSION=unknown` 与 `LABEL org.opencontainers.image.version=$VERSION`；workflow 传入已确认的正式 tag。
- Release 资产名和镜像 tag 继续使用正式 tag。

非正式构建不承诺外部标签等于 Cargo 版本。branch artifact 沿用 dev/beta + short SHA；`dev-latest` Release 标题/正文保留短 SHA、完整 commit 和 Actions run。build.yaml 的手工 dispatch 只上传 artifact，不创建正式 Release。

### D6. MSRV 为 Rust 1.97.1

在 Cargo.toml 声明 `rust-version = "1.97.1"`。该值是最低支持版本，不是浮动产物工具链记录。现有 warning gate 已钉在 1.97.1，并覆盖 default 与 `--no-default-features` 的 `cargo check --release --all-targets --locked`；实现后必须重跑这两种判定面。Rust 1.96 及以下明确不受支持。

后续调整 MSRV 必须在目标版本上重跑两种锁定检查、报告告警数并同步 README 与长期规格。

## Risks / Trade-offs

- [main/master 分叉迁移遗漏提交] → 迁移前普通合并并验证祖先关系；无法证明时停止，不切触发器。
- [人工 publish 解析到错误 tag] → 只接受当前提交唯一附注 `v*` tag；零个、多个、轻量 tag、Cargo 失配全部前置失败。
- [reusable workflow 权限继承过宽] → version gate 显式声明只读 contents 权限。
- [MSRV 1.97.1 排除旧环境] → 这是已确认的兼容边界；README 明示，并在该版本上持续验证。
- [浮动产物工具链高于 MSRV] → 接受“最低支持版本”和“实际产物编译器”分离；MSRV 调整必须显式验证。
- [dev-latest 二进制自身不含 SHA] → 接受该限制，用 Release/Actions 元数据追溯；嵌 SHA 留作后续独立变更。
- [同日无法发布第二个正式版] → 使用下一个自然日，换取严格 CalVer 和无歧义排序。

## Migration Plan

1. 记录实现前版本、告警与远端分支基线。
2. 由维护者以普通合并收敛 master 独有提交到 main；核验后才允许修改触发器。
3. 修改 Cargo 版本与 MSRV、启动日志和 lockfile，完成本地编译验证。
4. 新增 version gate，接入两个正式 workflow，收紧人工发布路径并迁移稳定分支触发器。
5. 增加 Docker OCI version label，更新 README 与 OpenSpec 文档。
6. 完成本地正反例、Docker 检查和 OpenSpec 校验。
7. 维护者执行 CI 绿/红 tag 实验并提供两个 workflow 的 run URL；确认临时 tag 已删除。
8. 完成合规审查、归档前验证和最终验证后归档。

回滚：version gate、caller needs、main 触发器和 Docker label 可分别回退；若 main 迁移后发现问题，可恢复 workflow 对 master 的监听，但不回退或重写已合入 main 的历史。

## Open Questions

无。远端分支合并和临时 tag 操作仍需实施时由维护者执行或明确授权，但其职责边界已确定。
