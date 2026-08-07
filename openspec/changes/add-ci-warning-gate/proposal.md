## Why

`clean-release-build-warnings`（已归档于 `openspec/changes/archive/2026-08-04-clean-release-build-warnings`）把 `cargo check --release --all-targets` 的告警基线从 14 降到 0，提交于 `d85cfb6`。该 change 的完成验证中明确记录剩余风险：**CI 门禁作为后续独立 change 未落地**。

清零只是把计数器归零，没有任何机制阻止它重新涨上去。

### 当前的强制手段全部是人/AI 纪律

实测确认：`.git/hooks` 下无任何非 sample 钩子，无 `.pre-commit-config.yaml`，无 `lefthook.yml`，`core.hooksPath` 未设置（`git config --get` 退出码 1）。CI 三条 workflow 中没有 `-D warnings`、没有 `RUSTFLAGS`、没有 clippy、没有 `cargo test`。

所以「零新增告警」当前只由三样东西承载：`AGENTS.md:31/91/97` 的判定命令与报告义务、`build-warning-hygiene` spec 的四个 Requirement、AI 协作的验证纪律。**没有一处机器强制。**

### 这套纪律模式在本仓库已实证失效过一次

`docs/version-governance-optimization-design.md` 记录：早期每次发版前手动 bump `Cargo.toml` 版本，2026-03-30 之后被遗忘，此后 5 个正式版（v2026.7.27 至 v2026.8.4）全部漂移，`--version` 仍自报 `2026.3.1`。同一仓库、同一「靠纪律记得做」的模式，已经证明会衰减。告警计数目前没漂移，是因为它 2026-08-04 刚被清零。

### 为什么不能简单地给发布路径加 `-D warnings`

Rust 的稳定性承诺覆盖「今天能编译的合法代码，明天的 stable 仍能编译」，**不覆盖「不产生新告警」**。每个 stable 周期都会新增 lint、扩大既有 lint 触发范围，future-incompat 一类机制本身就是「先警告、后报错」的设计。

而两条产物 workflow 用的是 `dtolnay/rust-toolchain@stable`（浮动）。若给 7 个发布矩阵腿加上 `-D warnings`，某天 Rust 发版时仓库零代码变更即可导致 7 腿全红、发布中断，且中断时机由外部决定。

因此本 change 采用**仪器与生产线分离**：`-D warnings` 只存在于一个钉版的检查 job 内，发布产物构建保持浮动且不升级告警。完整论证见 `docs/warning-gate-two-line-defense-design.md`。

## What Changes

两道防线，一个机器强制点加一个本地默认动作。

### 第一防线：本地 `pre-push` hook（新增）

- 新增 `scripts/git-hooks/pre-push`，运行 `cargo check --release --all-targets`（原始准绳，不加 `-D warnings`），存在告警则拒绝推送并打印告警清单。
- 通过 `git config core.hooksPath scripts/git-hooks` 一次性安装，脚本入库、可版本化、可审查。
- 选 pre-push 而非 pre-commit：告警是「整个提交树」的属性而非「单个提交」的属性，WIP 提交允许中间态；推送是内容离开本机的时刻，与 CI 判定面天然对齐。
- **明确非强制**：hook 需一次配置才生效、`--no-verify` 可绕过、新克隆默认没有。它堵遗忘型失效，不堵刻意绕过。

### 第二防线：CI reusable gate（新增，硬失败）

- 新增 `.github/workflows/warning-gate.yaml`，`on: workflow_call`，单 job，`runs-on: ubuntu-latest`。
- 工具链钉在 `dtolnay/rust-toolchain@1.97.1`（该分支把版本硬编码在 `action.yml:51`，无 `toolchain` 输入），并断言 `rustc --version`。
- checkout 后 `mkdir -p admin-ui/dist` 供给 rust-embed 的目录存在性检查（`src/admin_ui/router.rs:14` 的 `#[folder = "admin-ui/dist"]`；该目录为 gitignore 项，全新检出下缺失会导致编译错误）。
- 两遍判定：

```bash
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
```

- 挂进全部三条流水线（`build.yaml`、`build-dev-release.yaml`、`docker-build.yaml`），gate 红则产物 job 不启动，无 release、无镜像、无 manifest。

### 发布产物线：保持浮动，只加依赖锁定

- 7 个矩阵腿的 `cargo build` **只增加 `--locked`**，不加 `RUSTFLAGS`，`dtolnay/rust-toolchain@stable` 保持不动。
- Dockerfile builder `rust:1.92-alpine` → `rust:1-alpine`（脱离落后 5 个小版本的旧编译器，跟随 Rust 1.x 最新）。

### Docker 依赖锁定与 smoke test

- 依赖锁定三处同步收紧，缺任一处则 docker build 必失败：`.dockerignore` 删除 `Cargo.lock` 排除行、`COPY Cargo.toml Cargo.lock* ./` 去通配、`cargo build` 加 `--locked`。当前 lockfile 被 `.dockerignore:3` 排除，从未进入构建上下文，镜像依赖每次完全无锁定地从零解析。
- 最终阶段 `RUN ./kiro-rs --version`，**只断言退出码为 0**。
- `docker-build.yaml` 的 `workflow_dispatch` 新增布尔输入 `publish`（默认 `false`），在非矩阵的 `pre-check` job 计算单一 `should_publish`，约束 GHCR 登录、`push` 与 manifest job。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `build-warning-hygiene`: 新增三个 Requirement，把「零新增告警」从纯纪律约束扩展为「发布路径必须有机器强制点」。现有四个 Requirement 不变。
  1. **发布路径必须有固定编译器版本上的机器强制点**：门禁在钉版编译器上执行，判定面与本地准绳一致，失败则不产出发布物；同时发布产物构建 MUST NOT 升级告警，也 MUST NOT 为门禁目的而钉版。
  2. **人工触发的发布流水线默认不得产生发布副作用**：人工路径默认 dry-run，发布开关必须显式开启，且不得由矩阵 job 输出承载。
  3. **本地防线必须是默认执行的机器动作而非纯纪律**：hook 脚本入库、可安装、有告警则拒绝操作；同时必须记录其非强制性，且不得被用作软化 CI 门禁的理由。

## Impact

CI 与构建配置：

- 新增 `.github/workflows/warning-gate.yaml`
- 修改 `.github/workflows/build.yaml`：新增 gate 调用 job、`build` 的 `needs` 增加该 job、7 腿 `cargo build` 加 `--locked`
- 修改 `.github/workflows/build-dev-release.yaml`：同上
- 修改 `.github/workflows/docker-build.yaml`：新增 gate 调用 job、`publish` 输入、`should_publish` 约束
- 修改 `Dockerfile`：builder tag、`COPY` 去通配、`--locked`、`--version` smoke test
- 修改 `.dockerignore`：删除 `Cargo.lock` 行

本地工具链：

- 新增 `scripts/git-hooks/pre-push`（`scripts/` 目录当前存在且为空，未被 `.gitignore` 排除）

规格与文档：

- `openspec/changes/add-ci-warning-gate/specs/build-warning-hygiene/spec.md`：delta（三个 ADDED Requirement）
- `AGENTS.md`：「零新增编译告警」小节补 CI 强制点与 pre-push 第一防线；高风险矩阵补 CI 行；准绳命令措辞与 gate 判定命令对齐
- `README.md`：编译小节增补 hook 安装说明
- 归档后更新 `docs/release-build-warnings-cleanup-design.md` 的「后续项」小节

**不改 `src/`。** 不改 admin-ui。

## Scope

见 Impact。范围边界的三条硬线：

1. 不修改任何 Rust 源码
2. 不给发布产物矩阵与 Docker 的实际 `cargo build` 设置 `RUSTFLAGS="-D warnings"`
3. 不引入 `rust-toolchain.toml`

## Non-Goals

- **不引入 `rust-toolchain.toml`。** 实测：`channel = "1.97.1"` 与 runner 预装的 `stable` 在 rustup 眼里是两个不同 toolchain 目录（即使当前都指向 1.97.1），而 `Swatinem/rust-cache` 的 `cachePaths` 不含 `~/.rustup`（`src/config.ts:270-286`），故引入后每腿每 run 都会真实下载一份工具链；且 cache key 含 `rust-toolchain.toml`（`src/config.ts:162`），会让 7 腿现有缓存一次性失效。钉版的收益只有 gate 需要，成本不应由 7 腿与每台开发机承担。
- **不给发布矩阵与 Docker 加 `-D warnings`。** 见 Why：会把 Rust 发版变成发布中断的随机触发器。
- **不用 `Cargo.toml [lints]` 替代 RUSTFLAGS。** 实测 `[lints.rust] unused = { level = "deny", priority = -1 }` 会让 `cargo check` 在首个告警处报 error 并终止编译，直接破坏现行 spec「告警数必须被报告」与「计数口径为唯一告警点」的可计数前提，且阻断本地 WIP 中间态。
- **不引入 clippy 门禁。** 项目从未跑过 clippy（全仓仅 `src/openai/responses_stream.rs:95` 一处 `#[allow(clippy::too_many_arguments)]`），存量未知，等于夹带新一轮清零。后续项。
- **不加 `cargo test` 到 CI。** 真实缺口，但属另一个 change 的范围与验证标准。后续项。
- **不加 `pull_request` 触发器与分支保护。** 实测 master 与 main 均无分支保护（HTTP 404 Branch not protected），仓库同时允许 merge/squash/rebase。本门禁定位是发布路径强制；PR 触发属合并时强制，需与分支保护配套。后续项。
- **不把 gate 做成软失败。** 第一防线无机器强制，软化第二防线等于零强制。
- **不动 `main` 分支。** 它是 default branch 但停在 `d85cfb6` 且无任何 workflow 触发（三条 workflow 的 push 触发器只有 `master` 与 `dev`）。属分支策略整理，与告警卫生是不同关注点。后续项。
- **不断言 Docker smoke test 的版本字符串。** 当前 `--version` 输出 `2026.3.1`，与产物名不符，属 `docs/version-governance-optimization-design.md` 的范围。
- **不新增 health endpoint。** 项目当前无独立 health endpoint；服务级启动与 HTTP 探针需单独定义契约。
- **不钉 Docker 镜像 digest。** patch 级 tag 只锁编译器版本、不锁镜像内容；完全可复现需钉 digest，维护成本高于收益。

## Assumptions

- ~~`rust:1-alpine` tag 存在且指向 1.x 最新~~ —— **已不是假设**。用户于 2026-08-05 实跑确认：`docker pull rust:1-alpine` 正常解析并拉取；`docker run --rm rust:1-alpine rustc --version` 输出 `rustc 1.97.1 (8bab26f4f 2026-07-14)`，与本地、官方 channel 清单、7 个 runner 镜像逐位一致。AI 侧无法复核（本机 `auth.docker.io` 与 `registry-1.docker.io` 超时不可达，且未安装 docker CLI）。
- **空 `admin-ui/dist` 占位足以通过编译。** 继承 `docs/ci-warning-gate-design.md` 的两轮强制重编实验结论（rust-embed 8.9.0 派生宏只检查目录存在性，嵌入内容不参与告警生成）。若 CI 实跑发现不足，升级为完整 `pnpm install` + `pnpm build`，不改门禁语义。
- **gate 只需回答布尔问题。** 告警计数与报告职责属本地准绳，因此 `-D warnings` 的提前中止语义可接受，spec 不必为 CI 再写一套计数口径。
- **runner 预装 Rust 当前即 stable。** 实测 Ubuntu 22.04 x64/arm64、Ubuntu 24.04、Windows 2022、macOS 15 x64/arm64 全部为 Rust 1.97.1 + Rustup 1.29.0，与官方 channel 清单一致。故 `@stable` 步骤当前实际零下载。这是巧合而非机制保证，不构成本 change 的依赖。
- **gate 与产物线的编译器版本会随时间分离。** stable 前进后产物线可能产生 gate 看不见的新 lint 告警。这是有意接受的：那些告警不阻断发布，等 gate 版本 bump 时统一面对。

## Success Criteria

本地：

```bash
cargo check --release --all-targets
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
openspec validate --all
```

第一条报告告警数，须为 0；后两条退出码须为 0。PowerShell 下执行后两条前先 `$env:RUSTFLAGS = "-D warnings"`，完成后 `Remove-Item Env:RUSTFLAGS`。

CI 侧：

| 证据 | 判定 |
| --- | --- |
| dev 绿提交的 gate | 通过 |
| 7 个产物矩阵腿 | 全绿（对照基线：master run `30985624336` Build Artifacts 5m59s、`30985624269` Docker 7m22s） |
| 告警红路径 | gate 红且矩阵 job 不启动、无产物 |
| Docker 双架构 dry-run | build 与 smoke test 绿、manifest job skipped、GHCR 无新 tag、`latest` digest 不变 |
| gate 在全新检出下 | 可编译（dist 供给生效） |

本地 hook：植入告警后 `git push` 被拒；移除后放行；`--no-verify` 可绕过（确认非强制性表述属实）。

## Risks

| 风险 | 后果 | 控制措施 |
| --- | --- | --- |
| **平台专属告警盲区**（本方案主要代价） | 仅在 Windows / macOS / musl / arm64 某 target 触发的项目告警不被机器拦截 | 当前全仓 `cfg(feature)` 只有 `src/http_client.rs:59` 一处，无 `cfg(windows)`/`cfg(unix)`/`target_os`/`target_arch` 平台业务分叉，盲区未被触发。记为后续项；引入平台条件代码时重新评估（届时需同步钉住矩阵工具链） |
| ~~`rust:1-alpine` 行为异常~~（已消解） | — | tag 存在性与指向版本（1.97.1）均已实跑确认，见 Assumptions。Docker 变更仍独立成步并经 dry-run 验证 |
| Docker 从 1.92 跳到 1.x 最新 + 依赖改为锁定 | 最终二进制的依赖版本与代码生成可能变化，不承诺运行时逐字节不变 | `--version` smoke test 证明二进制可执行；既有 7 腿发布矩阵兜底；不承诺服务完整功能，由后续服务级 smoke test 承担 |
| Docker dry-run 产生发布副作用 | 实验镜像推送、`latest` 被覆盖 | `publish` 默认 `false`；`should_publish` 钉在非矩阵 pre-check job（GHA 对矩阵 job 输出取最后完成腿的值）；dry-run 关闭 `cache-to` 保留 `cache-from` |
| 空 dist 占位在 CI 上不足 | gate job 编译失败 | 第二步 dev 实跑确认；不足则升级为完整前端构建 |
| gate 工具链长期不 bump | 逐渐脱离实际编译环境 | 建议每 2-3 个 Rust 周期显式 bump 并重跑准绳命令。这是单点动作（只有 gate 一处版本事实），不涉及第二处同步 |
| hook 非强制 | `--no-verify` 可绕、新克隆默认没有 | 这是第一防线的固有属性，正是保留硬失败第二防线的理由；文档必须写明 |
| `--locked` 严格化 | 依赖变更忘提交 lockfile 时 CI 失败 | 属期望行为，报错明确 |
| 可复用 workflow 的条件语义 | gate 被跳过或白跑 | job 级 `if`/`needs` 写在调用方，与现有 pre-check 一致；以 dev 实跑确认 |
| 合入窗口期有新 PR 先落 master | 带入未经门禁的代码 | 合入前判据：`git diff --quiet $(git merge-base origin/master dev) origin/master` 退出码 0（当前实测为 0） |

风险类型（`AGENTS.md` 高风险矩阵）：**Docker / 发布 / CI 部署脚本**。

对应验证：三条 cargo 命令、`openspec validate --all`、CI run 取证、`git status --short`。
