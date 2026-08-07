## Context

### 当前实现

三条 workflow，全部实测确认：

| workflow | 触发器 | 矩阵 | Rust 步骤 |
| --- | --- | --- | --- |
| `build.yaml` | push master/dev、tag `v*`、dispatch | 7 平台 | `dtolnay/rust-toolchain@stable` + `targets: ${{ matrix.target }}` |
| `build-dev-release.yaml` | push dev、dispatch（可指向任意分支） | 7 平台 | 同上 |
| `docker-build.yaml` | push master、tag `v*`、dispatch | 双架构 | 无宿主机 Rust 步骤，编译在 `rust:1.92-alpine` 内 |

三条流水线**都没有** `pull_request` 触发器、`cargo test`、clippy、`-D warnings`、`RUSTFLAGS`。

`build.yaml` 的 release job 只在 `refs/tags/v` 触发；master push 只产 `beta-<sha>` 产物不发 release。`docker-build.yaml` 的 master push 走 `is_beta=true` 只动 `beta` alias 不动 `latest`；但 `workflow_dispatch` 固定 `push: true` 且走 `is_beta=false` 路径，会更新 `latest`。

`build-dev-release.yaml` 的 release job 会 `git tag -f dev-latest` 并 force-push，然后 `gh release delete` 再重建滚动 prerelease。

### 编译期前置条件

`src/admin_ui/router.rs:14` 以 `#[derive(Embed)] #[folder = "admin-ui/dist"]` 在编译期嵌入前端产物。rust-embed 8.9.0 的派生宏在目录缺失且未设 `allow_missing` 时**无条件返回编译错误**。`admin-ui/dist` 是 gitignore 项、未入库；两条产物 workflow 在 cargo 步骤前用 `pnpm install` + `pnpm build` 供给，Dockerfile 由 frontend-builder 阶段供给。

因此任何新增的 cargo job **必须自行供给该目录**，不能依赖「本地能跑」。

### Docker 依赖锁定的实际状态

三处叠加导致「比未加锁更甚」：

1. `.dockerignore:3` 将 `Cargo.lock` 列为排除项 → lockfile 从未进入构建上下文
2. `Dockerfile` builder 阶段 `COPY Cargo.toml Cargo.lock* ./` 用通配写法 → lockfile 缺失也不报错，通配实际从未命中
3. `cargo build --release --no-default-features` 不带 `--locked` → 依赖每次完全从零解析

### 工具链的三套版本事实

同一个 commit 当前用三套编译器构建：

- 两条二进制流水线：浮动 stable（`dtolnay/rust-toolchain@stable` 是该 action 仓库的**分支名**，不是版本约束；该分支把 `toolchain` 输入设为 `required: false, default: stable`，内部执行 `rustup toolchain install stable --profile minimal --no-self-update`）
- Docker：钉死 `rust:1.92-alpine`，落后 5 个小版本
- 本地：1.97.1（手动升级）

没有任何一处声明过意图，也没有任何一处会在漂移时报错。

`Cargo.toml` 无 `rust-version`，即项目**从未声明 MSRV**。

## Goals / Non-Goals

**Goals:**

- 让「零新增编译告警」在发布路径上获得一个**确定性的机器强制点**
- 不让编译器升级成为发布中断的随机触发器
- 把第一防线从「记得跑」变成「默认会跑」
- 收紧 Docker 依赖锁定，脱离落后 5 个小版本的 builder
- 人工 Docker 触发默认不产生发布副作用

**Non-Goals:**

- 不覆盖平台专属告警（明确的已知盲区）
- 不引入 clippy 与 `cargo test`
- 不加 `pull_request` 触发器与分支保护
- 不修改 `src/`
- 不治理 `main` 分支与版本漂移

## Decisions

### D1：`-D warnings` 只放在钉版 gate 内，不放在发布产物线

Rust 的稳定性承诺覆盖「能否编译」，不覆盖「是否产生告警」。新 lint 每个周期都会出现。因此 `-D warnings` 一旦进入发布路径，编译器版本就成为 pass/fail 契约的一部分。

浮动 stable + `-D warnings` 的组合意味着：仓库零代码变更，某天 Rust 发版即导致 7 腿全红、发布中断，中断时机由外部决定。

**决定**：生产线浮动且不升级告警；`-D warnings` 只在一个钉版 job 内。

比喻上，gate 是一台测量仪器。仪器必须固定，被测的生产线可以自由升级。仪器和生产线用同一个浮动版本，等于用一把会自己变长的尺子量东西。

被否方案：

- 全线钉版 + 全线 `-D warnings`（原方案 `docs/ci-warning-gate-design.md` 五审形态）：见 D2 的成本分析
- 全线浮动 + 全线 `-D warnings`：把 Rust 发版变成发布中断的随机触发器
- 浮动但滞后（该 action 支持 `stable minus 1 release` 与 `stable N months ago`，已在其 `action.yml` 解析正则中确认）：只降低撞车概率，确定性问题未解

### D2：不引入 `rust-toolchain.toml`，钉版用 `dtolnay/rust-toolchain@1.97.1`

三条实测事实叠加：

1. 7 个 runner 镜像预装的 Rust **恰好都是 1.97.1**（与官方 channel 清单一致）。当前 `@stable` + 预装 stable 的组合，rustup 发现已安装即跳过，实际零下载。
2. `channel = "1.97.1"` 与预装的 `stable` 在 rustup 眼里是**两个不同的 toolchain 目录**，即使指向同一 rustc 版本。
3. `Swatinem/rust-cache` 的 `cachePaths` 只含 `CARGO_HOME/registry`、`CARGO_HOME/git` 与 target 目录（`src/config.ts:270-286`），**不缓存 `~/.rustup`**。

结论：引入钉版文件后，每腿每次 run 都会真实下载一份工具链，且这是**持续成本**不是一次性成本。附加地，cache key 含 `rust-toolchain.toml`（`src/config.ts:162`），新增该文件会让 7 腿现有缓存一次性全部失效。

避开这些成本的唯一写法是 `channel = "stable"`，但那等于放弃钉版，自相矛盾。

**决定**：钉版只在 gate 一处，用 action 的版本分支实现。实测 `dtolnay/rust-toolchain` 的 `1.97.1` 分支存在，且该分支**无 `toolchain` 输入**，版本硬编码在 `action.yml:51`（`toolchain: 1.97.1`）；`1.99.0` 分支同构。所以 gate 里不需要传参。

连带收益：原方案决策 #12 的工具链一致性检查整块消失（不存在两处版本事实，无可比较）；候选 A/A'/B/C 的 target 隔离分析作废（不移除 dtolnay 步骤，`targets` 输入照常把矩阵 target 装在同一工具链上）；`rustup set profile minimal` 前置步骤与 `CARGO_INCREMENTAL`/`CARGO_TERM_COLOR` 的丢失兜底均不再需要。

### D3：gate 失败为硬失败

第一防线（`AGENTS.md` + spec + AI 纪律）当前无任何机器强制。同一纪律模式在本仓库已实证失效一次（版本漂移，5 个正式版）。

因此「gate 是第二防线」描述的是**发现顺序**，不是**权威顺序**。在第一防线无机器强制的前提下，gate 是唯一的强制点。

**决定**：gate 红则阻断产物与镜像发布。

触发概率极低（本地纪律正常工作时 CI 永远绿，gate 只在纪律失效时才响），成本几乎为零而兜底价值完整。

被否方案：

- `continue-on-error: true` 软失败：gate 退化为无人盯的仪表盘，版本漂移案例即先例
- 分级（dev 软、master 硬）：dev 是主开发分支、master 只接 PR，等于日常永远软、发版日突然硬，问题攒到发版当天集中爆发

### D4：第一防线用 `pre-push` 而非 `pre-commit`

告警是「整个提交树」的属性，不是「单个提交」的属性。WIP 提交（临时注释掉调用、留个待用函数）应允许中间态。

推送是内容离开本机的时刻，与 CI 判定面天然对齐：本地拒推条件与 CI 拒发条件一致，不会出现「本地过了 CI 却红」。

成本从「每次提交」降到「每次推送」。本机 `CARGO_TARGET_DIR` 指向 `D:\ProgramData\rust\cargo_home\target`（共享 target），热缓存下判定命令约十几秒。

**决定**：`scripts/git-hooks/pre-push` + `git config core.hooksPath scripts/git-hooks`。

用 `core.hooksPath` 而非拷进 `.git/hooks`：脚本入库、可版本化、可审查，安装是一条幂等命令。

hook 内用**原始准绳**（不加 `-D warnings`），因为它需要输出可读的告警清单帮助开发者定位，而非仅回答布尔问题。

### D5：告警升级用 RUSTFLAGS 而非 `Cargo.toml [lints]`

`[lints]` 的诱惑是「一个文件生效于所有腿、Docker 与本地，不存在漏加」。实测确认它确实有效：临时 crate 加 `[lints.rust] unused = { level = "deny", priority = -1 }` 后，`cargo check` 把 `unused_variables` 与 `dead_code` 报为 error 并终止编译。

但它与现行 spec 冲突：

- 现行 Requirement「告警判定以 `--all-targets` 为准绳」与「告警数必须被报告」都建立在「告警是 warning、可计数」之上。`[lints]` 会让本地准绳在首个告警处中止编译，**计数口径直接失效**（看不到后面还有多少）。
- 它改变本地开发体感：任何 WIP 中间态编译不过，属超出「CI 门禁」范围的行为变更。

**决定**：RUSTFLAGS。本地准绳保持 warning 语义与可计数口径，CI 单独升级。「漏加」风险由本 change 的 spec Requirement 覆盖。

### D6：Docker builder 改浮动 `rust:1-alpine`

与 D1 一致：Docker 是生产线，应当浮动。选 `1-alpine` 而非 `alpine`：语义上明确「Rust 1.x 系列的最新」，与向下兼容前提自洽，且不会在假想的 Rust 2.0 发布时跳版。

补充说明 patch 级 tag 的强度边界（避免未来误解）：patch tag 只锁编译器版本、**不锁镜像内容**，官方镜像会因基础 Alpine 安全更新重建同名 tag，digest 变而 rustc 不变。完全可复现需钉 digest，维护成本高于收益，不做。

### D7：Docker smoke test 只断言退出码

`RUN ./kiro-rs --version`。Clap 在加载配置与凭据前处理 `--version`（`src/model/arg.rs:5` 的 `#[command(version)]`），无需真实凭据。

当前输出为 `2026.3.1`，与产物名不符 —— 这是 `docs/version-governance-optimization-design.md` 记录的版本漂移，属该方案范围。若本 change 断言版本字符串，版本治理 change 落地后必须返工并重测 Docker 双架构构建（每次约 7 分钟）。

**决定**：只断言退出码为 0。它已能拦住 Alpine 静态链接失败、缺 musl 运行时、二进制损坏这几类真实风险；版本内容正确性由版本治理负责。

### D8：Docker 人工触发默认 dry-run

现状 `workflow_dispatch` 固定 `push: true` 且走 `is_beta=false` 路径，人工运行会推送带版本号的双架构镜像并更新 `latest`。因此实验分支不能用现有 dispatch 验证 Docker。

**决定**：新增布尔输入 `publish`（默认 `false`）；`should_publish` 在**非矩阵**的 `pre-check` job 计算（GHA 对矩阵 job 输出取最后完成腿的值，放 pre-check 避免语义歧义）；master/tag 自动触发为 true，人工触发仅在显式 `publish=true` 时为 true；GHCR 登录、`build-push-action` 的 `push`、manifest job 均受约束。

dry-run 保留 `cache-from: type=gha` 加速但关闭 `cache-to`，实验分支层不进缓存（层缓存按内容寻址，无正确性风险，但占配额且可能落入读取路径）。

## 数据流与影响面

### gate 判定链

```text
push / tag / dispatch
  → pre-check（should_build）
  → warning-gate（workflow_call）
      checkout
      → mkdir -p admin-ui/dist        （rust-embed 目录存在性）
      → dtolnay/rust-toolchain@1.97.1 （钉版仪器）
      → assert rustc --version 含 1.97.1
      → Swatinem/rust-cache@v2（shared-key: warning-gate）
      → RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
      → RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
  → build（7 腿 / 双架构，浮动 stable，无 RUSTFLAGS，加 --locked）
  → release / manifest
```

任一环红则下游不启动。

### 两遍检查的增量关系

feature 差异当前只涉及 native-tls 一线（`Cargo.toml` 的 `default = ["native-tls"]`，全仓 `cfg(feature)` 只有 `src/http_client.rs:59` 一处）。未受影响依赖的单元哈希不变，产物可被第二遍复用。

已知冷缓存成本：default 遍经 `reqwest/native-tls-vendored` 拉入 vendored OpenSSL（`Cargo.lock` 含 `openssl-src`），build script 会编译完整 OpenSSL。

`timeout-minutes: 20` 是冷缓存首跑的上界。dev 首跑需监控实际耗时；若逼近 20 分钟先放宽，待缓存稳定后按 2-3 倍余量收紧。

### 影响面清单

| 文件 | 动作 |
| --- | --- |
| `.github/workflows/warning-gate.yaml` | 新增 |
| `.github/workflows/build.yaml` | 加 gate 调用 job、`build.needs`、7 腿 `--locked` |
| `.github/workflows/build-dev-release.yaml` | 同上 |
| `.github/workflows/docker-build.yaml` | 加 gate 调用 job、`publish` 输入、`should_publish` 约束 |
| `Dockerfile` | builder tag、`COPY` 去通配、`--locked`、smoke test |
| `.dockerignore` | 删除 `Cargo.lock` 行 |
| `scripts/git-hooks/pre-push` | 新增 |
| `AGENTS.md` | 告警小节、高风险矩阵、准绳措辞 |
| `README.md` | hook 安装说明 |
| `openspec/specs/build-warning-hygiene/spec.md` | 归档时由 delta 合入 |

**`src/` 零改动。**

## 异常路径

| 异常 | 表现 | 处理 |
| --- | --- | --- |
| `admin-ui/dist` 缺失 | gate job 编译错误（rust-embed 派生宏，退出码 101） | 由 `mkdir -p` 步骤预防；若空目录不足则升级为完整 `pnpm build` |
| `rust:1-alpine` 行为异常 | docker build 在 FROM 阶段失败 | 已消解：tag 存在性与指向版本（`rustc 1.97.1`）均由用户实跑确认（2026-08-05） |
| `Cargo.lock` 未随依赖变更提交 | gate 与产物线因 `--locked` 失败，报错明确 | 期望行为；提交 lockfile 即可 |
| `.dockerignore` 未同步删除 `Cargo.lock` 行 | 去通配后 COPY 找不到文件，docker build 必失败 | 三处必须同批改动，tasks 强制同一任务组 |
| gate 版本断言失败 | job 在 cargo 步骤前失败 | 说明 action 分支行为变化，需人工确认后调整 |
| 冷缓存首跑超 `timeout-minutes` | gate job 超时红 | 先放宽 timeout，缓存稳定后收紧；不降低判定严格性 |
| 产物线出现 gate 看不见的新 lint | 产物照常构建成功，告警只在日志中 | 有意接受（D1）；gate bump 时统一面对 |
| hook 被 `--no-verify` 绕过 | 第一防线失效 | 第二防线硬失败兜底（D3） |
| 矩阵 job 输出承载发布开关 | GHA 取最后完成腿的值，语义歧义 | `should_publish` 钉在非矩阵 pre-check job（D8） |

## 回滚

每步独立可回滚，全部是配置层变更，无数据迁移、无运行时状态：

- gate 挂载：从三条 workflow 的 `needs` 中移除该 job，并删除调用 job。gate workflow 本身留着不触发也无副作用。
- Docker 变更：`git revert` 对应提交即可。builder tag 回 `rust:1.92-alpine`、`.dockerignore` 恢复 `Cargo.lock` 行、`COPY` 恢复通配、去掉 `--locked` 与 smoke test 必须同批回退（三处依赖锁定互为前提）。
- `publish` 输入：移除输入与 `should_publish` 判断，恢复固定 `push: true`。
- hook：`git config --unset core.hooksPath` 即刻失效，无需改动仓库文件。
- spec delta：change 未归档前删除目录即可；已归档则需反向 delta。

回滚后回到当前状态：无机器门禁、Docker 无锁定、人工 dispatch 直接推送。

## 验证策略

### 本地

```bash
cargo check --release --all-targets
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
openspec validate --all
```

第一条报告告警数（须为 0），后两条证明 CI 的告警升级与依赖锁定语义。三条均要求 `admin-ui/dist` 存在（本地恒满足；CI 由占位步骤保证）。

PowerShell 下先 `$env:RUSTFLAGS = "-D warnings"`，完成后 `Remove-Item Env:RUSTFLAGS`。

### CI

绿路径与红路径都必须验证。**只验证绿路径不能证明门禁真的会拦。**

- 绿路径：推 dev，确认 gate 通过、7 腿全绿。对照基线为 master run `30985624336`（Build Artifacts 5m59s）与 `30985624269`（Docker 7m22s），二者均为浮动 stable + 热缓存下的成功 run。
- 红路径：自 dev 建 `codex/warning-gate-red-test` 临时分支，植入一条告警的 commit，用 `workflow_dispatch` 对该分支手动触发 **`build.yaml`**，确认 gate 红、矩阵不启动、无产物，验证后删除分支。
  - **禁止 dispatch `build-dev-release.yaml`**：其 dispatch 可指向任意分支，且 release job 会 `git tag -f dev-latest` force-push 并删除重建滚动 prerelease。其红路径等价性由「同一可复用 workflow + 相同 needs 接线」推得。
  - 实验 commit 不落 dev。
- Docker：从 `codex/` 临时分支以 `publish=false` 跑双架构 dry-run，确认 build 与 smoke test 绿、manifest job skipped、GHCR 无新 tag、`latest` digest 不变。

### hook

植入一条告警后 `git push` 被拒；移除后放行；`--no-verify` 可绕过（确认文档的非强制性表述属实）。

### 合入

PR + Create a merge commit。前置判据：

```bash
git diff --quiet $(git merge-base origin/master dev) origin/master
```

退出码 0 表示 master 相对 merge-base 无内容变化，即 master 全部内容来自已验证的 dev 树。当前实测为 0。

该判据取代 `git merge-base --is-ancestor origin/master dev`：后者在 merge commit 形态下**恒为 false**（实测 `dcd9351` 之后已反转，`--is-ancestor dev origin/master` 才是 0），照字面执行会导致此后每次合入都被误判为非 ff。

## Risks / Trade-offs

| 风险 | 权衡 |
| --- | --- |
| **平台专属告警盲区** | 本方案最主要的代价。换来的是发布路径不受编译器升级影响。当前全仓无平台 cfg 分叉，盲区未被触发；引入平台条件代码时需重新评估（届时应同步钉住矩阵工具链，否则回到 D1 的问题） |
| gate 与产物线版本分离 | 有意接受。代价是 gate 版本长期不 bump 会逐渐脱离实际编译环境，需每 2-3 个 Rust 周期显式 bump。这是单点动作 |
| Docker 产物可能变化 | 编译器从 1.92 跳到 1.x 最新、依赖从现场解析改为锁定，最终二进制可能变化。不承诺运行时逐字节或逐行为不变。由 smoke test 与既有发布矩阵控制 |
| ~~`rust:1-alpine` 未实测~~（已消解） | tag 存在性与指向版本（`rustc 1.97.1 (8bab26f4f 2026-07-14)`）均由用户实跑确认。AI 侧无法复核（本机 registry 超时、无 docker CLI）。**当前 Docker、本地、7 个 runner 镜像、官方 channel 清单四方版本一致** |
| ~~GHCR 取证受限~~（已消解） | 已补 `read:packages` scope，dry-run 前后可机器比对。基准：`latest` = `sha256:dcd5c510f9ed`、`version_count` = 18（2026-08-05 实测）。注意包为 org 私有包，列表接口 `orgs/.../packages?package_type=container` 返回空数组，取证必须用具名路径 |
| hook 非强制 | 固有属性，正是保留硬失败第二防线的理由 |
| 冷缓存首跑耗时 | gate 新增独立 `shared-key`，首跑必冷且含 vendored OpenSSL 编译。一次性成本 |
