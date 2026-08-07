> 修订记录：
> - 2026-08-05 初版。本文档是 `docs/ci-warning-gate-design.md` 之后的**独立方案**，不是它的修订版。原方案（截至五审）采用「全线钉版 + 双层 `-D warnings`」结构；本轮拷问会话推翻该结构，改为「浮动生产线 + 钉版单一仪器 + 本地 pre-push 第一防线」。原方案文档保持原样不动，两份并存，实施以本文档为准。

# 告警门禁两道防线方案

分析日期：2026-08-05
分析基线：`dev` @ `d85cfb6`（= `origin/dev`，工作树仅有未跟踪的设计文档）。`origin/master` @ `dcd9351`（Merge pull request #2 from wtfdelphia/dev），与 dev **内容树完全相同**。
环境：Windows 11 / PowerShell / 本地 rustc 1.97.1（8bab26f4f 2026-07-14）

## 与原方案的关系

`docs/ci-warning-gate-design.md` 已经过五轮审核，其**事实查证部分基本可靠**（本轮逐条复核，见「继承的事实」）。被推翻的是它的**结构决策**：

| 维度 | 原方案（五审） | 本方案 |
| --- | --- | --- |
| `-D warnings` 位置 | 前置 gate + 7 腿产物 + Docker（双层） | 仅前置 gate（单层） |
| 工具链 | 新增 `rust-toolchain.toml` 钉 1.97.1，全仓生效 | 不新增钉版文件；gate 内钉 `dtolnay/rust-toolchain@1.97.1`，产物线与 Docker 浮动 |
| Docker builder | 钉 `rust:1.97.1-alpine` | 浮动 `rust:1-alpine` |
| 工具链一致性检查 | 必需（决策 #12，脚本比较 TOML 与 Docker tag） | 整块删除（无两处版本事实，无需比较） |
| 平台专属告警 | 由 7 腿 `-D warnings` 机器拦截 | 已知盲区 + 后续项 |
| 本地第一防线 | 未涉及（纯人/AI 纪律） | 入库 `pre-push` hook |

推翻的理由见「核心论证」。

## 核心论证

### 论证一：`-D warnings` 与浮动 stable 不能共存于发布路径

Rust 的稳定性承诺覆盖「今天能编译的合法代码，明天的 stable 仍能编译」，**不覆盖「不产生新告警」**。每个 stable 周期都会新增 lint、扩大既有 lint 的触发范围，future-incompat 一类机制本身就是「先警告、后报错」的设计。

因此一旦 `-D warnings` 进入发布路径，编译器版本就成为 pass/fail 契约的一部分：某天 Rust 发版，仓库零代码变更，7 腿全红，发布中断，且中断时机由外部决定（很可能撞上发版日）。

原方案察觉到这一点，选择的解法是「把全线都钉住」。代价是本仓库要为一个测量目的承担全局钉版的成本（见论证二），且每 6 周需要显式 bump 并重跑验证。

本方案改为**把两件事分开**：

- **生产线浮动**：7 个二进制发布矩阵腿与 Docker builder 都跟随 stable，享受编译器性能与安全修复，符合 Rust 向下兼容前提，不带 `-D warnings`，因此新 lint 不会阻断发布。
- **仪器钉版**：`-D warnings` 只存在于一个钉在 `1.97.1` 的 gate job 内。仪器版本固定，读数才有意义。

一句话：**用一把会自己变长的尺子量东西，量出的变化分不清是东西变了还是尺子变了。**

### 论证二：全局钉版在本仓库有持续成本

本轮实测的三条事实叠加起来，构成反对 `rust-toolchain.toml` 的具体理由：

1. **7 个 runner 镜像预装的 Rust 恰好都是 1.97.1**（Ubuntu 22.04 x64/arm64、Ubuntu 24.04、Windows 2022、macOS 15 x64/arm64 的 Readme 均为 `Rust 1.97.1` / `Rustup 1.29.0`）。当前 `dtolnay/rust-toolchain@stable` + 预装 stable 的组合，rustup 发现已安装即跳过，实际是**零下载**。
2. **`channel = "1.97.1"` 与预装的 `stable` 在 rustup 眼里是两个不同的 toolchain 目录**，即使指向同一 rustc 版本。引入钉版文件后，每腿每次 run 都会真实下载一份 1.97.1。
3. **`Swatinem/rust-cache` 不缓存 `~/.rustup`**（`src/config.ts:270-286` 的 `cachePaths` 只含 `CARGO_HOME/registry`、`CARGO_HOME/git` 与 target 目录）。因此该下载**每腿每 run 持续发生**，不是一次性成本。

附加成本：`rust-cache` 的 cache key 包含 `rust-toolchain.toml`（`src/config.ts:162`），新增该文件会让 7 腿现有缓存一次性全部失效。

避开这些成本的唯一写法是 `channel = "stable"`，但那等于放弃钉版，自相矛盾。所以钉版必须付这些成本 —— 而本方案让 gate **单个 job** 付，不是 7 腿 + 每台开发机都付。

### 论证三：第一防线当前无任何机器强制

实测：`.git/hooks` 下无任何非 sample 钩子，无 `.pre-commit-config.yaml`，无 `lefthook.yml`，`core.hooksPath` 未设置。

所以本地那套「清理告警机制」由三样东西构成：`AGENTS.md:31/91/97` 的判定命令与报告义务、`build-warning-hygiene` spec 的四个 Requirement、AI 协作的验证纪律。**全部是人/AI 纪律。**

这套模式在本仓库已有一次实证失效：`docs/version-governance-optimization-design.md` 记录的版本漂移 —— 早期每次发版前手动 bump `Cargo.toml`，2026-03-30 之后被遗忘，此后 5 个正式版全部漂移。同一仓库、同一纪律模式，已证明会衰减。告警计数目前没漂移，是因为它 2026-08-04 刚被清零。

推论：**「gate 是第二防线」描述的是发现顺序，不是权威顺序。**在第一防线无机器强制的前提下，gate 是唯一的强制点，因此它的失败行为必须是硬失败。同时，值得把第一防线也做成机器动作（pre-push hook），把「遗忘型失效」堵住 —— hook 不入库、`--no-verify` 可绕、新克隆默认没有，所以它堵不住刻意绕过，这正是两道防线互补而非替代的原因。

## 目标与约束

目标：让「零新增编译告警」在发布路径上获得**一个确定性的机器强制点**，同时不让编译器升级成为发布中断的随机触发器。

硬约束：

- 不修改 `src/`。本方案改动范围：`.github/workflows/`（新增 1 个 + 修改 3 个）、`Dockerfile`、`.dockerignore`、`scripts/git-hooks/`、`openspec/`、`AGENTS.md`、`README.md`。
- gate 的判定命令与本地准绳 `cargo check --release --all-targets` **逐 flag 一致**，仅显式附加 `-D warnings` 与 `--locked`。
- gate 失败 MUST 阻断产物与镜像发布（硬失败）。
- gate job 必须在全新检出下可编译：`admin-ui/dist` 等本地恒满足的前置条件必须在 job 内显式供给。
- 产物线与 Docker 的实际 `cargo build` **不设置** `-D warnings`；新 lint 不得阻断发布。
- 临时分支与人工验证默认不得推送镜像、移动 tag 或覆盖 `latest`。
- 命中 `AGENTS.md`「OpenSpec 条件」的 CI / 发布脚本强制项，必须走 OpenSpec change。

## 决策记录

本轮拷问会话达成的 9 项决策：

| # | 决策点 | 结论 | 被否选项与理由 |
| --- | --- | --- | --- |
| 1 | dev → master 合入方式 | PR + Create a merge commit（已实际发生一次，`dcd9351`）；方案的 ff 判据改为内容判据 | Squash and merge：dev 提交不进 master 历史，merge-base 退回旧点，重复提交与重复变更冲突（这正是 #171/#184 之后长期分叉的成因）；Rebase and merge：官方语义会重写 committer 与 SHA，等于把提交复制一份，分叉更碎 |
| 2 | 门禁 change 的开发分支 | dev 先 `git merge --ff-only origin/master` 快进到 `dcd9351`（纯 ff、零文件变化），在 dev 开发验证，再 PR 合 master | 不同步 dev 直接开发：下次 PR 的 diff 基点是 `d85cfb6`，PR 混入已合入历史，评审噪声大；直接在 master 开发：红路径实验与 dispatch 实验落在发布分支，且 master push 会触发 Docker 真实推送 |
| 3 | CI 取证方式 | 由 AI 用已登录的 `gh` 自主触发与取证（实测 ADMIN 权限可用） | 人工看 Actions 页面回贴：每步实测一次往返，红路径与失配实验各需一轮 |
| 4 | `main` 分支 | 本 change 不动，登记为已知偏差 + 后续项 | 顺手改 default branch 或加触发器：属分支策略整理，与告警卫生是不同关注点，且会影响 PR 默认基点与克隆默认分支 |
| 5 | 与版本治理方案的顺序 | 门禁先行；Docker smoke test 只断言 `./kiro-rs --version` 退出码为 0，不断言版本字符串 | 版本治理先行：体量更大且自身无告警门禁保护；合并成一个 change：范围翻倍，验证矩阵交叉后失败定位困难 |
| 6 | 告警升级手段 | `RUSTFLAGS="-D warnings"` | `Cargo.toml [lints]`（实测 `unused = { level = "deny", priority = -1 }` 确实把告警变 error）：会让本地准绳在首个告警处中止编译，**现行 spec 的计数口径直接失效**，且任何 WIP 中间态编译不过，属超范围的行为变更；两者并用：无收益的重复 |
| 7 | 仪器与生产线分离 | 7 腿产物与 Docker 浮动 stable 且**不带** `-D warnings`；`-D warnings` 只在 gate 内，钉版用 `dtolnay/rust-toolchain@1.97.1`，**不新增 `rust-toolchain.toml`** | 原方案的全线钉版 + 全线 `-D warnings`：见论证一、二；全线浮动 + 全线 `-D warnings`：把 Rust 发版变成发布中断的随机触发器；浮动但滞后（该 action 支持 `stable minus 1 release` 与 `stable N months ago`，已在解析正则中确认）：只降低撞车概率，确定性问题未解 |
| 8 | gate 失败行为 | 定位是第二防线，失败行为为**硬失败**（红则不发产物） | `continue-on-error` 软失败：第一防线无机器强制，软化第二防线等于零强制，gate 退化为无人盯的仪表盘（版本漂移案例即先例）；分级（dev 软、master 硬）：日常永远软、发版日突然硬，问题攒到发版当天集中爆发 |
| 9 | 第一防线形态 | 入库的 `pre-push` hook：`scripts/git-hooks/pre-push` + `core.hooksPath` 安装；并入同一 change 作为独立任务组 | `pre-commit`：每次提交付十几秒到 2 分钟，且阻断 WIP 提交，被 `--no-verify` 磨掉概率高；只强化 `AGENTS.md` 措辞：即当前已失效过一次的模式；pre-push 顺带塞 `pnpm build` 与 `openspec validate`：范围膨胀，`pnpm build` 在 hook 里太重 |

### 决策 7 让方案瘦掉的部分

原方案中以下内容在本方案里**整块消失**：

- 决策 #12 的工具链一致性检查（读 `rust-toolchain.toml` 的 channel、提取 Dockerfile builder tag、严格比较、大小写不敏感匹配）—— 不存在两处版本事实，无可比较
- `rust-toolchain.toml` 本身，及其引发的 7 腿缓存失效与每腿每 run 工具链下载
- 候选 A / A' / B / C 的 target 隔离分析整节 —— 不移除 dtolnay 步骤，`targets` 输入照常把矩阵 target 装在同一工具链上
- `rustup set profile minimal` 前置步骤、`CARGO_INCREMENTAL` 与 `CARGO_TERM_COLOR` 的丢失兜底 —— 均为「移除 dtolnay 步骤」的衍生问题
- 「工具链 bump 两处同步」的 spec Requirement —— 只剩 gate 一处版本事实，bump 是单点动作

### 决策 7 的代价

失去原方案四审加入的平台专属告警拦截。gate 退回「单宿主机 Ubuntu + 两种 feature 组合」，仅在 Windows / macOS / musl / arm64 某个 target 上触发的项目告警不再被机器拦截。

当前盲区未被触发的证据：全仓 `cfg(feature)` 只有 `src/http_client.rs:59` 一处（musl 的 native-tls 分支），且无 `cfg(windows)`、`cfg(unix)`、`target_os`、`target_arch` 等平台业务分叉。但这只说明「现在没有」，不能证明未来没有。**记为已知盲区 + 后续项**，若将来引入平台条件代码，需要重新评估是否给矩阵腿加回 `-D warnings`（那时应同步钉住矩阵工具链，否则回到论证一的问题）。

## 方案设计

### 1. 可复用 workflow：`.github/workflows/warning-gate.yaml`

- `on: workflow_call`，无输入；单 job（名 `warning-gate`），`runs-on: ubuntu-latest`，`timeout-minutes: 20`。
- 步骤顺序：
  1. `actions/checkout@v5`
  2. `mkdir -p admin-ui/dist` —— 空占位目录，满足 rust-embed 的目录存在性检查
  3. `dtolnay/rust-toolchain@1.97.1` —— 钉版仪器；该分支把版本硬编码在 action 内部（`action.yml:51` 为 `toolchain: 1.97.1`，且无 `toolchain` 输入），无需传参
  4. 断言 `rustc --version` 含 `1.97.1`
  5. `Swatinem/rust-cache@v2`，`shared-key: warning-gate`（独立于 7 腿的 key）
  6. 两遍检查

```bash
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
```

说明：

- 不需要 `pnpm install` + `pnpm build`。告警只由 Rust 源码决定，嵌入内容不参与告警生成；空目录已足够（原方案二审、三审各做过一次强制重编实验确认，本方案继承该结论）。
- `--locked` 锁依赖树，依赖漂移不得静默引入新告警；lockfile 未随依赖变更提交时 CI 明确报错。
- 冷缓存首跑的已知成本：default 遍经 `reqwest/native-tls-vendored` 拉入 vendored OpenSSL（`Cargo.lock` 含 `openssl-src`），build script 会编译完整 OpenSSL。`--no-default-features` 遍与 default 遍共享 target 目录，feature 差异只涉及 native-tls 一线，未受影响依赖的产物可被复用。
- gate 只回答布尔问题「有没有告警」，**不承担告警计数与报告职责**（那是本地准绳的活）。因此 `-D warnings` 的提前中止语义是可接受的，spec 也不必为 CI 再写一套计数口径。

### 2. 三条流水线的挂法

- `build.yaml`：新增 `warning-gate` 调用 job（`needs: pre-check`，条件与 build 相同的 `should_build`，避免被跳过的 commit 白跑门禁）；`build` 的 `needs` 增加该 job。**7 腿 `Build app` 不加 `RUSTFLAGS`**，保持浮动 stable 与现有命令，仅增加 `--locked`。
- `build-dev-release.yaml`：同样新增调用 job（`needs: prepare`）；`build` 的 `needs` 增加该 job；7 腿同样只加 `--locked`。
- `docker-build.yaml`：新增调用 job（带 `should_build` 条件）；`build` 的 `needs` 增加该 job。
- 语义：gate 红 → 产物构建不启动 → 无 release、无镜像、无多架构 manifest。gate 绿 → 产物线按浮动 stable 正常构建，新 lint 只是警告不阻断。

`dtolnay/rust-toolchain@stable` 在两条产物 workflow 中**保持不动**。本轮查证其行为：`@stable` 是该 action 仓库的分支名（该仓库有 `stable`/`beta`/`nightly`/`master` 四个特殊分支），`stable` 分支把 `toolchain` 输入设为 `required: false, default: stable`；内部执行 `rustup toolchain install stable --profile minimal --no-self-update`。因 runner 预装版本当前即 stable，该步骤实际零下载。

### 3. Dockerfile 对齐

- builder 阶段 `FROM rust:1.92-alpine` → `FROM rust:1-alpine`。选 `1-alpine` 而非 `alpine`：语义上明确「Rust 1.x 系列的最新」，与向下兼容前提自洽，且不会在假想的 Rust 2.0 发布时跳版。
- 依赖锁定**三处**同步收紧（缺任一处则 docker build 必失败）：
  1. `.dockerignore` 删除 `Cargo.lock` 行（第 3 行）—— lockfile 当前从未进入构建上下文
  2. `COPY Cargo.toml Cargo.lock* ./` → `COPY Cargo.toml Cargo.lock ./`
  3. `cargo build --release --no-default-features` 加 `--locked`
- **不设置** `RUSTFLAGS="-D warnings"`（决策 7）。
- 最终阶段复制二进制后 `RUN ./kiro-rs --version`，**只断言退出码为 0**。Clap 在加载配置与凭据前处理 `--version`（`src/model/arg.rs:5` 的 `#[command(version)]`），无需真实凭据。
  - 已知：当前输出为 `2026.3.1`，与产物名不符。这是 `docs/version-governance-optimization-design.md` 记录的版本漂移，属该方案范围，**不是本 change 的缺陷**，因此 smoke test 不断言版本字符串（决策 5）。
- `docker-build.yaml` 的 `workflow_dispatch` 新增布尔输入 `publish`（默认 `false`），在**非矩阵**的 `pre-check` job 计算单一 `should_publish` 输出（GHA 对矩阵 job 输出取最后完成腿的值，放 pre-check 避免语义歧义）：master/tag 自动触发为 true；人工触发仅在显式 `publish=true` 时为 true。GHCR 登录、`build-push-action` 的 `push`、manifest job 均受该输出约束。dry-run 保留 `cache-from: type=gha` 但关闭 `cache-to`。

### 4. 第一防线：`pre-push` hook

- 落点 `scripts/git-hooks/pre-push`（`scripts/` 目录当前为空且未被 `.gitignore` 排除）。
- 内容：运行一次 `cargo check --release --all-targets`（**原始准绳，不加 `-D warnings`**），统计唯一告警点数，非零则拒绝推送并打印告警清单与绕过方式。
- 安装：`git config core.hooksPath scripts/git-hooks`（幂等，一条命令），README 增补一行说明。用 `core.hooksPath` 而非拷进 `.git/hooks`：脚本入库、可版本化、可审查。
- 选 pre-push 而非 pre-commit 的理由：告警是「整个提交树」的属性而非「单个提交」的属性，WIP 提交允许中间态；推送是内容离开本机的时刻，与 CI 判定面天然对齐 —— 本地拒推条件与 CI 拒发条件一致，不会出现「本地过了 CI 却红」。成本从每次提交降到每次推送。
- 本机成本参考：`CARGO_TARGET_DIR` 指向 `D:\ProgramData\rust\cargo_home\target`（共享 target 目录），热缓存下判定命令约十几秒；冷/半冷情形原方案记录为 1m54s。
- **必须在文档中写明其非强制性**：hook 不入库生效（需一次 `core.hooksPath` 配置）、`--no-verify` 可绕过、新克隆默认没有。它堵遗忘型失效，不堵刻意绕过。

### 5. 规格与文档同步

- `build-warning-hygiene` delta 新增 **3 个** Requirement（原方案的「工具链一致性检查」那条随决策 7 消失，但拆出了 dry-run 与本地防线两条）：
  1. 「发布路径必须有固定编译器版本上的机器强制点」：CI MUST 在钉版编译器上执行与本地准绳同判定面的 default / 无默认 feature 检查，并把项目告警升级为错误、锁定依赖；失败时 MUST NOT 创建 release、镜像或 manifest。同时发布产物构建 MUST NOT 升级告警，也 MUST NOT 为门禁目的钉版。附 Scenario 含「全新检出下必须自行供给前置条件」与「bump 后必须重认基线」。
  2. 「人工触发的发布流水线默认不得产生发布副作用」：人工路径默认 dry-run，发布开关须显式开启且 MUST NOT 由矩阵 job 输出承载。
  3. 「本地防线必须是默认执行的机器动作而非纯纪律」：hook 脚本入库、可安装、有告警则拒绝操作；MUST 记录非强制性，且 MUST NOT 被用作软化 CI 门禁的理由。
- `AGENTS.md`：同步「零新增编译告警」小节（补 CI 强制点与 pre-push 第一防线）、高风险矩阵补 CI 行、准绳命令措辞与 gate 判定命令对齐（含 `--locked`）。
- `README.md`：编译小节增补 hook 安装一行。
- 归档后更新 `docs/release-build-warnings-cleanup-design.md` 的「后续项」小节，标注已落地及归档位置。

## 实施顺序

```text
0. dev 快进对齐：git merge --ff-only origin/master（dev d85cfb6 → dcd9351，零文件变化）
   verify: git diff --quiet origin/master dev 退出码 0；git status --short 干净

1. Dockerfile：builder 改 rust:1-alpine + 依赖锁定三处收紧 + 最终镜像 --version smoke test；
   docker-build.yaml 增加默认 false 的 publish 输入，统一约束登录/push/manifest
   verify: 从 codex/ 临时分支以 publish=false 跑双架构 dry-run，确认 build 与 smoke test 绿、
           manifest job skipped、GHCR 无新 tag、latest digest 不变

2. warning-gate.yaml（dist 占位 + @1.97.1 钉版 + 版本断言 + 两遍检查）+ 三条流水线挂载；
   7 腿 cargo build 只增加 --locked，不加 RUSTFLAGS
   verify: 推 dev 实测 gate 绿、7 腿全绿；对照基线为 master run 30985624336（Build Artifacts 5m59s）
           与 30985624269（Docker 7m22s）

3. 红路径实验：自 dev 建 codex/warning-gate-red-test，植入一条告警的 commit，
   用 workflow_dispatch 对该分支手动触发 build.yaml
   verify: gate 红、矩阵不启动、无产物；验证后删除临时分支。
           禁止 dispatch build-dev-release.yaml（其 release job 会 force-move dev-latest 滚动 prerelease）

4. scripts/git-hooks/pre-push + core.hooksPath 安装说明 + README 增补
   verify: 本地植入一条告警后 git push 被拒；移除后推送通过；--no-verify 可绕过（确认非强制性表述属实）

5. spec delta + AGENTS.md + 文档同步
   verify: openspec validate --all 通过

6. 合入 master：PR + Create a merge commit
   前置 verify: git diff --quiet $(git merge-base origin/master dev) origin/master 退出码 0
                （当前实测为 0；该判据取代原方案的 --is-ancestor，后者在 merge commit 形态下恒为 false）
```

第 3 步是告警红路径的直接证据。只验证绿路径不能证明门禁真的会拦。所有临时分支 dispatch 必须保持 `publish=false` 并在验证后立即删除分支。

## 成功标准

```bash
cargo check --release --all-targets
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
openspec validate --all
```

PowerShell 下执行后两条前先 `$env:RUSTFLAGS = "-D warnings"`，完成后 `Remove-Item Env:RUSTFLAGS`。原始准绳用于报告告警数，严格命令用于证明 CI 的告警升级与依赖锁定语义。三条命令均要求 `admin-ui/dist` 存在（本地恒满足；CI 由 gate 的占位步骤保证）。

CI 侧证据：dev 绿提交的 gate 通过且 7 腿全绿；告警红路径被 gate 拦截且矩阵不启动；Docker 双架构 dry-run 通过且无 GHCR / tag / `latest` 副作用；最终镜像 `--version` 退出码 0；gate 在全新检出下可编译（dist 供给生效）。

本地证据：pre-push hook 在有告警时拒绝推送、无告警时放行。

## 明确不采用的做法

- **不引入 `rust-toolchain.toml`**。见论证二：在本仓库它带来每腿每 run 的工具链下载与 7 腿缓存失效，而钉版的收益只有 gate 需要。
- **不给产物矩阵与 Docker 加 `-D warnings`**。见论证一：会把 Rust 发版变成发布中断的随机触发器。
- **不用 `Cargo.toml [lints]` 替代 RUSTFLAGS**。会让本地准绳在首个告警处中止编译，破坏现行 spec 的计数口径，并阻断 WIP 中间态。
- **不引入 clippy 门禁**。项目从未跑过 clippy（全仓仅 `src/openai/responses_stream.rs:95` 一处 `#[allow(clippy::too_many_arguments)]`），存量未知，等于夹带新一轮清零。后续项。
- **不加 `cargo test`**。CI 缺测试是真实缺口，属另一个 change 的范围与验证标准。后续项。
- **不加 `pull_request` 触发器与分支保护**。实测 master 与 main 均无分支保护，仓库同时允许 merge/squash/rebase 三种方式。本门禁定位是发布路径强制；PR 触发属合并时强制，需与分支保护配套。后续项。
- **不把 gate 做成软失败**。见论证三。
- **不内联复制 gate job**。三处副本必然漂移。
- **不只挂部分流水线**。Docker 镜像同样是发布产物。
- **不动 `main` 分支**（决策 4）。
- **不在本 change 新增 health endpoint**。项目当前无独立 health endpoint；服务级启动与 HTTP 探针需单独定义契约。
- **不钉 Docker 镜像 digest**。patch 级 tag 只锁编译器版本、不锁镜像内容（官方镜像会因基础 Alpine 安全更新重建同名 tag，digest 变而 rustc 不变）。完全可复现需钉 digest，维护成本高于收益。本方案既然改为浮动 tag，该问题不再适用，此处记录以免未来误解 patch tag 的强度。

## 风险评估

- **平台专属告警盲区**（决策 7 的主要代价）：gate 只覆盖 Ubuntu 宿主 target。当前全仓无平台 cfg 分叉，盲区未被触发；引入平台条件代码时需重新评估。
- **gate 与产物线编译器版本分离**：gate 用 1.97.1，产物线用浮动 stable。当 stable 前进后，产物线可能产生 gate 看不见的新 lint 告警 —— 这是**有意接受**的：那些告警不阻断发布，等到 gate 版本 bump 时统一面对。反过来，gate 版本长期不 bump 会让它逐渐脱离实际编译环境，需要定期（建议每 2-3 个 Rust 周期）显式 bump 并重跑准绳命令。这是单点动作，不涉及第二处同步。
- **Docker 浮动带来的产物变化**：从 1.92 跳到 1.x 最新，且依赖从现场解析改为 `Cargo.lock` 锁定，最终二进制的依赖版本与代码生成可能变化。不承诺运行时逐字节或逐行为不变。由 `--version` smoke test 与既有发布矩阵控制风险；smoke test 只证明二进制可执行，不证明服务完整功能。
- **Docker 发布副作用**：人工 dispatch 默认 dry-run。验证必须记录 run URL、`publish=false`、manifest skipped，以及运行前后 `latest` digest 未变。
- **master 基线**：原方案的「master 落后、不含清零提交」风险**已消解** —— `dcd9351` 与 dev 内容树相同，清零提交 `d85cfb6` 已在 master 上。剩余风险只有合入窗口期有新 PR 先落 master，由第 6 步的内容判据覆盖。
- **gate 的 dist 供给**：空目录占位有源码查证与两轮强制重编实验支撑（继承原方案）。剩余缺口是 CI 侧确认，由第 2 步的 dev 实跑覆盖。若空目录不足则升级为完整前端构建，不改门禁语义。
- **hook 非强制**：`--no-verify` 可绕、新克隆默认没有、需一次 `core.hooksPath` 配置。这是第一防线的固有属性，正是保留硬失败第二防线的理由。
- **`--locked` 严格化**：依赖变更忘提交 lockfile 时 CI 直接失败并明确报错，属期望行为。
- **可复用 workflow 的条件语义**：job 级 `if`/`needs` 写在调用方，与现有 pre-check 一致；以 dev 实跑确认。

## 后续项（本次不做）

1. **平台专属告警覆盖**：若引入平台条件代码，重新评估是否给矩阵腿加回 `-D warnings`（需同步钉住矩阵工具链）。
2. **clippy 门禁**：先做一轮 clippy 存量清零，再以 `cargo clippy --all-targets -- -D warnings` 挂入。
3. **测试门禁**：CI 目前完全不跑测试；独立 change 决定 profile、失败策略与矩阵联动。
4. **PR 触发 / 分支保护**：协作模式变化时补 `pull_request` 触发器。master 与 main 当前均无保护。
5. **`main` 分支治理**：当前是 default branch 但停在 `d85cfb6` 且无任何 workflow 触发（三条 workflow 的 push 触发器只有 `master` 与 `dev`）。仓库另有 `backup/main-pre-reset` 分支，暗示 main 历史曾被重置。
6. **`docs/tooling-sources.md` 更新**：第 12 行仍记 Rust 1.94.1，实际本地为 1.97.1。
7. **gate 工具链定期 bump**：建议每 2-3 个 Rust 周期显式 bump 并重跑准绳命令。
8. **版本治理方案落地**：`docs/version-governance-optimization-design.md`，含 `Cargo.toml` 版本漂移（5 个正式版）、镜像 label、tag 规范等六个问题。

## 流程门禁

按 `AGENTS.md`「OpenSpec 条件」，CI / 发布脚本为强制项：

- change 名：`add-ci-warning-gate`（`openspec/changes/` 下无同名目录，已实测）
- capability：`build-warning-hygiene`（delta 新增 1 个 Requirement）
- 流程：`openspec-new-change` / `openspec-propose` → `openspec-superpowers-bridge` → `openspec-apply-change` → `spec-compliance-check` → `openspec-verify-change` → `verification-before-completion` → 归档

## 验证边界

### 本轮会话实跑 / 查证（2026-08-05）

仓库与 git：

- `origin/master` = `dcd9351`（Merge pull request #2 from wtfdelphia/dev），父节点 `606c6bc` + `d85cfb6`
- `git diff --stat origin/master dev` 输出为空 → master 与 dev **内容树相同**
- `merge-base(origin/master, dev)` = `d85cfb6` = dev 尖端；`--is-ancestor origin/master dev` 退出码 **1**（不再是祖先），`--is-ancestor dev origin/master` 退出码 **0**（方向已反转）→ 原方案的 ff 判据失效
- `git diff --quiet $(git merge-base origin/master dev) origin/master` 退出码 **0** → 新判据当前成立
- 本地 master 停在 `b9e757e`，落后远端 3 个提交；本地 dev = `origin/dev` = `d85cfb6`
- `origin/main` = `d85cfb6`，是仓库 default branch，与 master 内容相同但少那个 merge commit；另有 `backup/main-pre-reset` 分支
- 工作树只有未跟踪的两份设计文档，无脏改动
- 仓库允许 merge / squash / rebase 三种合并方式；master 与 main 均**无分支保护**（HTTP 404 Branch not protected）
- `gh` 登录为 `WTFGEDelphia`，对 `wtfdelphia/kiro.rs` 为 ADMIN；scopes = `admin:public_key, gist, read:org, repo`
- master 合入后两条 run 均 success：`30985624336`（Build Artifacts, 5m59s）、`30985624269`（Docker, 7m22s）

工具链与 action：

- 本地 `rustc 1.97.1 (8bab26f4f 2026-07-14)`、`cargo 1.97.1`、active toolchain = `stable-x86_64-pc-windows-msvc (default)`，`rustup toolchain list` 仅一项
- 本地**无** `rust-toolchain.toml` / `rust-toolchain`
- 官方 channel 清单 `channel-rust-stable.toml`：`pkg.rust` = `1.97.1 (8bab26f4f 2026-07-14)`
- `dtolnay/rust-toolchain` 有 `stable` / `beta` / `nightly` / `master` 四个特殊分支；`stable` 尖端 `4360b525`（2026-08-05T04:31Z，msg `toolchain: stable`），`master` 尖端 `6c977a6c`（2026-08-05T04:30Z）
- `stable` 分支 `action.yml`：`toolchain` 为 `required: false, default: stable`；`master` 分支为 `required: true` 且空值 `exit 1`
- 版本分支存在性：`1.95` / `1.96` / `1.96.1` / `1.97` / `1.97.1` / `1.98` / `1.99` 等均存在
- `1.97.1` 分支 `action.yml`：**无** `toolchain` 输入，版本硬编码在 `action.yml:51`（`toolchain: 1.97.1`）；`1.99.0` 分支同构（`toolchain: 1.99.0`）
- 安装命令：`rustup toolchain install <ver> [--target ...] --profile minimal --no-self-update`；`CARGO_INCREMENTAL=0` 与 `CARGO_TERM_COLOR=always` 均为「未设置才设置」
- runner 镜像预装 Rust（Readme 查证）：Ubuntu 22.04 x64 / 22.04 arm64 / 24.04、Windows 2022、macOS 15 x64 / 15 arm64 **全部为 Rust 1.97.1 + Rustup 1.29.0**
- `Swatinem/rust-cache` `src/config.ts`：`cachePaths` 只含 `CARGO_HOME/registry`、`CARGO_HOME/git`、target 目录（270-286 行）→ **不缓存 `~/.rustup`**；cache key 含 `rust-toolchain.toml`（162 行）

项目与本地环境：

- `Cargo.toml`：`name = kiro-rs`、`version = 2026.3.1`、`edition = 2024`、**无 `rust-version`**（项目从未声明 MSRV）
- `cfg(feature)` 全仓仅 `src/http_client.rs:59` 一处；`src/admin_ui/router.rs:14` 为 `#[folder = "admin-ui/dist"]`
- clippy 相关全仓仅 `src/openai/responses_stream.rs:95` 一处 `#[allow(clippy::too_many_arguments)]`
- **无任何 git hook**：`.git/hooks` 下无非 sample 文件，无 `.pre-commit-config.yaml`、无 `lefthook.yml`，`core.hooksPath` 未设置（`git config --get` 退出码 1）
- 无 `rustfmt.toml` / `.rustfmt.toml`；无 `.cargo/config.toml`
- `CARGO_TARGET_DIR` = `D:\ProgramData\rust\cargo_home\target`（共享 target；仓库内无 `target`，另有 `target-codex`）
- `scripts/` 目录存在且为空，未被 `.gitignore` 排除（`.gitignore:1` 为 `/target*`）
- `admin-ui/dist` 当前存在（4 个文件）
- `admin-ui/package.json` scripts：`dev` / `build`（`tsc -b && vite build`）/ `preview` / `test`（`vitest run`）
- `[lints]` 可行性实测：临时 crate 加 `[lints.rust] unused = { level = "deny", priority = -1 }` 后，`cargo check` 把 `unused_variables` 与 `dead_code` 报为 error 并终止编译（决策 6 的否决依据）
- `Dockerfile` / `.dockerignore` 现状复核：builder `rust:1.92-alpine`、`COPY Cargo.toml Cargo.lock* ./`、`cargo build` 无 `--locked`、`.dockerignore` 第 3 行排除 `Cargo.lock`

### 未运行 / 未查证

- **Docker tag：AI 侧未能复核，已由用户实跑补齐（该项不再是未验证事项）**。`hub.docker.com`、`registry-1.docker.io` 与 `auth.docker.io` 从本机均**超时不可达**（非 404），且本机未安装 docker CLI。用户于 2026-08-05 实跑两条命令：`docker pull rust:1-alpine` 正常解析并拉取三层（Alpine 基础层复用 + 75MB + 273MB 工具链层）；`docker run --rm rust:1-alpine rustc --version` 输出 `rustc 1.97.1 (8bab26f4f 2026-07-14)`。**结论：tag 存在，且指向 1.x 最新；Docker、本地、7 个 runner 镜像、官方 channel 清单四方版本逐位一致。**决策 D6（Docker builder 改浮动 `rust:1-alpine`）的事实前提完整。
- **GHCR 取证：已解决（该项不再受限）**。GHCR 匿名 token 请求返回 401（`UNAUTHORIZED`），原因是 `kiro-rs` 为 org **私有**包；`gh api` 列包返回 403 明确要求 `read:packages`。用户已执行 `gh auth refresh -h github.com -s read:packages`，scope 生效。dry-run 前基准（2026-08-05 实测）：`version_count=18`、`updated_at=2026-08-05T07:42:53Z`、**`latest` = `sha256:dcd5c510f9ed`**（与 `v2026.8.4` 同 digest）。取证命令用具名路径 `gh api "orgs/wtfdelphia/packages/container/kiro-rs/versions"`；**列表接口 `orgs/.../packages?package_type=container` 对该私有包返回空数组 `[]`，不可用于取证**。
- **本轮未跑 cargo 门禁命令**：三条成功标准命令本轮未执行（原方案五审记录热缓存下退出码 0、零告警）。实施时必须重跑并报告告警数。
- **CI 实测未做**：gate 绿路径、红路径、7 腿、Docker dry-run 均需推 dev 或 dispatch。
- **本地无 Docker 环境**：docker build 无法本地验证。
- 本文档为设计产物，**未修改任何源码、CI 配置、Dockerfile 或 spec**。

### 遗留待办

- 临时探针目录 `%TEMP%\lintprobe-764145522\`（决策 6 的 `[lints]` 实测用）删除时被沙箱策略拦截，需手动清理。
