# Release Version Governance Bridge Plan

日期：2026-08-07
Change：`release-version-governance`
状态：实现前桥接完成；分支触发器迁移仍受维护者前置约束。

## 1. 范围与非目标

### 范围

- `Cargo.toml` / `Cargo.lock`：目标 CalVer 与 `rust-version = "1.97.1"`。
- `src/main.rs`：tracing 初始化后的第一条应用 info 日志输出项目版本。
- `.github/workflows/version-gate.yaml`：正式版本身份的 reusable gate。
- `.github/workflows/build.yaml`、`.github/workflows/docker-build.yaml`：gate 接线、`main` 稳定触发器、人工镜像发布身份。
- `Dockerfile`：OCI `org.opencontainers.image.version`。
- `README.md`、本 change delta spec 与设计证据：版本、MSRV、发布和非正式构建规则。

### 非目标

- 不修改协议、认证、凭据、Admin API 或 admin-ui 构建/内嵌机制。
- 不把 SHA 编译进二进制，不引入 build script、vergen 或自动发版框架。
- 不改写历史 tag、Release 或二进制，不强推/reset/删除 `main`、`master`。
- 不启用分支保护，不决定停止监听后的 `master` 处置。
- 不修改 `build-dev-release.yaml` 与 `warning-gate.yaml` 的既有行为。

## 2. 关键设计决策

1. `Cargo.toml [package].version` 是源码版本声明；唯一附注 `vYYYY.M.D` tag 是正式发布身份。日期必须有效，Cargo/tag 必须一致，不支持同日修订后缀。
2. `main` 是唯一稳定发版落点。正式 tag 指向提交必须可从 `origin/main` 到达。
3. Version gate 与 warning gate 并行；任一失败都必须使下游产物 job 保持 skipped。
4. 分支、`dev-latest` 和 `publish=false` 是非正式构建，只要求 commit 可追溯，不要求其标签等于 Cargo 版本。
5. Docker `workflow_dispatch publish=true` 必须从当前提交的唯一合法附注 tag 推导版本；自由输入不得覆盖发布身份。
6. Rust 1.97.1 是 MSRV；产物线继续允许使用高于 MSRV 的浮动 stable。

## 3. 当前基线

| 项 | 命令 / 证据 | 结果 |
| --- | --- | --- |
| Change 状态 | `openspec status --change release-version-governance --json` | `isComplete: true`，4/4 工件完成，无 blocked 状态 |
| 当前分支 | `git status --short --branch` | `dev`；仅设计文档与本 change 工件有改动 |
| Cargo 版本 | `Cargo.toml:3` / `Cargo.lock:888` | 均为 `2026.3.1` |
| Rust / Cargo | `rustc --version` / `cargo --version` | 均为 1.97.1 |
| 告警基线 | `cargo check --release --all-targets` | exit 0，无 warning 行，基线 0 |
| 默认分支 | `git ls-remote --symref origin HEAD` | `main` |
| main/master 图 | `git rev-list --left-right --count origin/main...origin/master` | `1 1`，各有一个独有合并提交 |
| main/master 内容 | `git diff --quiet origin/main origin/master` | 树内容相同，但 ancestry 未收敛 |
| CodeGraph | `codegraph status` | 138 文件 / 2777 节点 / 7643 边，索引 up to date；索引由旧引擎生成 |
| Docker/actionlint | `Get-Command docker/actionlint` | 本机均不可用 |
| 敏感路径 | `git check-ignore` | `config.json`、`credentials.*`、`.codegraph/` 均被忽略，工作树未列出这些路径 |

目标 CalVer 尚未在工件中固定。执行任务 3.1 前必须由维护者明确本次目标版本；不得依据执行日静默猜测。

## 4. CodeGraph 证据

- `codegraph query main` 定位 `src/main.rs:24`。
- `codegraph impact main` 只报告 `src/main.rs` 的 `main`，说明运行时代码影响局限在入口函数。
- `codegraph explore main` 显示 tracing 在 `src/main.rs:29-34` 初始化，配置加载从第 36 行开始，凭据迁移的首条既有 info 在第 58 行；版本日志应插在第 34 行之后、第 36 行之前。这样即使配置加载失败，第一条应用 info 仍是版本。
- `codegraph query CARGO_PKG_VERSION` 与 `query tracing` 无结果，表明宏/日志调用不在该索引的符号搜索覆盖面内，已由 `rg` 和源码精读补足。
- 本机 CodeGraph 1.5.0 不支持项目文档所列 `context` 子命令，改用实际支持的 `explore/query/impact` 取得等价证据。

## 5. rg 与源码补盲

- `src/model/arg.rs:5` 的 `#[command(version)]` 由 Clap 使用 `CARGO_PKG_VERSION`；无需改 CLI 解析。
- `Cargo.toml:3` 和 `Cargo.lock:888` 同为 `2026.3.1`；Cargo manifest 尚无 `rust-version`。
- `build.yaml` 当前监听 `master/dev/v*`，`docker-build.yaml` 监听 `master/v*`，两者都只依赖 warning gate。
- `build-dev-release.yaml` 已在 Release 标题/正文记录短 SHA、完整 commit 和 trigger，满足非正式构建追溯，不应接 version gate。
- `docker-build.yaml` 当前在矩阵 job 直接使用自由 dispatch version；`publish=true` 会登录并推送，必须先在非矩阵 `pre-check` 解析正式身份。
- `Dockerfile` 已用锁文件构建并运行 `./kiro-rs --version` smoke test，但最终 stage 无 VERSION ARG/OCI version label。
- README 的 Docker 表仍称 `master` 为 beta 来源，且尚无 MSRV、附注 tag 和正式/非正式边界说明。
- `rg --files` 仅发现 `credentials.example.*.json` 示例文件；未发现真实配置或凭据候选。

## 6. GitHub Actions 接线硬约束

1. **非正式路径必须绿通过 gate，而不是跳过整个 gate job。** GitHub Actions 中，下游 job 默认会因任一 `needs` job 为 skipped 而连带 skipped。version gate 应在 branch 与 dry-run dispatch 上实际执行一个成功的 no-op 分支；不得用 caller job 级 `if` 把它跳过。
2. **reusable 输入必须显式。** 建议至少定义 `enforce`（boolean）与 `release_tag`（string）输入。called workflow 不得依赖 caller `needs` 上下文；caller 负责把 `pre-check` 输出传入。
3. **tag 和 main 历史必须完整。** gate checkout 使用完整历史/tags，并显式确保 `origin/main` 可解析；否则附注 tag 类型或 ancestry 判断会产生假阴性。
4. **人工发布身份只在非矩阵 job 解析。** `pre-check` 在 `publish=true` 时必须 checkout/fetch tags，解析当前 SHA 上唯一 `v*` tag并输出 `release_tag`；零个或多个候选都在仓库登录前失败。
5. **正式身份不能回退到自由输入。** 自动 tag 和 `publish=true` 的镜像名、build arg、manifest 版本都取已验证 `release_tag`；只有 `publish=false` 可使用 inputs.version。
6. **warning gate 保持原状。** version gate 不改变告警计数口径、钉版工具链或 dev 发布接线。

## 7. 高风险项与缓解

| 风险 | 等级 | 缓解 / 停止条件 |
| --- | --- | --- |
| 切换触发器时 main/master ancestry 未收敛 | 高 | 两分支当前树相同但历史分叉。维护者必须普通 merge/PR 收敛并验证；未完成时不得执行任务 4.3/4.4 的触发器迁移 |
| gate skipped 传播导致 branch/dry-run 构建消失 | 高 | 非正式路径让 gate 成功 no-op；本地/CI 验证 branch 与 dry-run 仍进入 build |
| 人工 publish 继续使用自由 version | 高 | 非矩阵解析唯一附注 tag；build、manifest、label 只消费已验证输出 |
| checkout 浅历史造成 tag 类型/main 可达性误判 | 高 | gate 与人工 publish 解析均完整 fetch tags/main；增加轻量 tag、main 不可达反例 |
| gate 红但 release/manifest 因 needs 接线仍运行 | 高 | 两个 caller 的所有产物链必须经 version gate；CI 红路径直接检查 job graph 和无产物 |
| MSRV 声明无法被 Cargo 接受或依赖不支持 | 中 | 修改后先跑 `cargo metadata --locked`，再在 1.97.1 上跑两种锁定检查；任一失败即停止 |
| Docker label 只在单架构镜像存在、manifest 检查口径不清 | 中 | 本地 inspect 单架构镜像；CI 后检查最终多架构 tag 的 config/label 可见性，失败不得声称验收 4 完成 |
| 同日目标版本未明确 | 中 | 任务 3.1 前由维护者确认唯一 `YYYY.M.D`；已有同名 tag 或日期策略冲突即停止 |
| 本机缺少 Docker/actionlint | 中 | 本地对应项标 SKIPPED；YAML 结构检查加真实 Actions 绿/红 run，Docker 由 CI 或有 Docker 环境补证 |

## 8. 任务到执行步骤映射

| Tasks | 执行动作 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1-1.3 | 产出本桥接计划，记录 Cargo/告警/workflow/分支基线，检查敏感路径 | 本文件第 3-5 节；`git status --short` | 出现真实凭据、config、token、Cookie 或未忽略缓存 |
| 2.1-2.2 | 维护者普通合并 main/master 历史；实现者只读核验远端 | `git fetch --prune`；ancestor/rev-list/diff 检查 | main 未包含所需提交，或需要强推/reset/delete |
| 3.1-3.4 | 确认目标 CalVer；修改 Cargo/lock/MSRV；在 tracing init 后加版本日志 | `cargo metadata --locked`；`cargo run --quiet -- --version`；缺失配置 smoke 检查首条 info | 目标版本未确认；Cargo 拒绝 rust-version；日志发生在 tracing 前或凭据日志后 |
| 4.1-4.2 | 新增 reusable gate 与严格身份校验 | 合法、Cargo 失配、非法日期、后缀、轻量 tag、main 不可达本地案例 | 任何非法案例通过或合法案例失败 |
| 4.3-4.4 | caller 并行接线，稳定触发器改 main | YAML 解析/审查；job graph；branch/tag 行为 | 任务 2 未完成；产物 job 未依赖 gate；dev 路径被改 |
| 4.5-4.6 | 非矩阵解析人工发布 tag，隔离 dry-run 与 publish | 无 tag/多 tag/轻量 tag/失配均前置失败；dry-run 无登录/push/manifest | 任一失败路径发生发布副作用 |
| 5.1-5.3 | Docker ARG/LABEL 和 workflow build arg | build/inspect 或 CI 镜像 inspect | label 与已验证 tag 不一致；无 Docker 证据且未记 SKIPPED |
| 6.1-6.4 | 同步 README、dispatch 文案、admin-ui 策略和 delta spec | `rg` 对照；`openspec validate --all` | README 与实现/规格冲突；新建顶层 spec |
| 7.1-7.4 | 本地编译、MSRV、运行时与非正式追溯验证 | 第 9 节必跑命令 | 告警数 > 0；任一严格检查失败；dev-latest 不可追溯 |
| 7.5-7.7 | 维护者执行真实 CI 绿/红 tag 实验并提供 run URL | 两 workflow job graph、annotation、产物与临时 tag 清理证据 | 缺少任一 run；红路径启动产物；临时 tag 未清理 |
| 8.1-8.4 | 合规、归档前和完成验证；用户确认后归档 | 三个项目 skill 证据 + `git status --short` | 合规/验证失败、敏感文件、未授权 push/merge/archive |

## 9. 必跑验证

### 本地

```text
cargo metadata --locked --format-version 1
cargo run --quiet -- --version
cargo check --release --all-targets
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
openspec validate --all
git diff --check
git status --short
```

Windows PowerShell 执行严格检查时使用 `$env:RUSTFLAGS = '-D warnings'`，完成后必须 `Remove-Item Env:RUSTFLAGS` 并确认已清除。告警数只取不带 `-D warnings` 的原始准绳命令。

另需执行 version gate 六类本地正反例、启动日志顺序 smoke、workflow YAML 结构审查。`actionlint` 与 Docker 当前不可用，相关本地项必须写 SKIPPED，并由真实 CI / Docker 环境补证。

### CI / 外部

- 正式身份绿路径：build 与 Docker workflow 的 version gate 绿，产物 job 进入。
- Cargo 失配红路径：两个 gate 红，下游 build/release/manifest skipped，annotation 含修复指引。
- gate 并行：version gate 不等待 warning gate。
- branch/dev/dry-run：version gate 成功 no-op 或不接入 dev workflow，既有非正式产物正常。
- `publish=true` 反例：无 tag、多 tag、轻量 tag、Cargo 失配均不得登录或发布。
- 镜像：最终正式 tag 的 OCI version label 与 tag 一致。

## 10. 文档同步判断

| 入口 | 判断 | 理由 |
| --- | --- | --- |
| README | 必须更新 | 影响构建最低 Rust、下载/发布、Docker tag、main/beta 和人工发布入口 |
| AGENTS.md | 当前无需更新 | OpenSpec、零告警、CI 红路径与安全纪律已覆盖；本 change 不改变 AI 协作规则 |
| `spec/design.md` | 当前无需更新 | 架构模块与运行时数据流不变；版本治理属于发布 capability |
| change delta spec | 已建立，实施时持续对照 | 承载本 change Requirement/Scenario |
| `openspec/specs/` | 归档时同步 | 新 capability 的长期规格落点 |
| `docs/tooling-sources.md` | 本 change 不更新 | 它是本机工具快照，不是 MSRV/发布事实源；其旧值作为独立维护滞后记录 |
| 设计文档 | 已定稿；事实变化时更新 | main/master、工具可用性或实现取舍变化必须回写 |

## 11. 停止条件

- 目标 CalVer 未明确或同名正式 tag 已存在。
- 维护者尚未非破坏地收敛 main/master ancestry，却准备迁移 workflow 触发器。
- OpenSpec 工件与实现出现无法在既定范围内解决的矛盾。
- 正式或人工发布存在绕过 version gate 的产物路径。
- branch/dry-run 因 skipped gate 传播而不再构建。
- `cargo check --release --all-targets` 告警数高于基线 0。
- 工作区出现真实 config、credentials、token、Cookie 或 `.codegraph/` 候选提交。
- 缺少 CI 绿/红 run、Docker label 证据或必要 SKIPPED/剩余风险说明。
- 需要 push、PR、merge、远端 tag 创建/删除或 archive，但尚未获得对应授权或维护者证据。
