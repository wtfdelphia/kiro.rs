> **本方案的结构决策已被 `docs/warning-gate-two-line-defense-design.md` 取代，实施以后者为准。**
> 本文档的事实查证部分（工具链版本、rust-embed 编译期依赖、`.dockerignore` 排除 `Cargo.lock`、缓存行为等）仍然可靠并被后者继承；被推翻的是「全线钉版 + 7 腿与 Docker 统一追加 `-D warnings`」这一结构。下文保持原样不动，仅作为决策过程留档。

> 修订记录：
> - 2026-08-04 初版。依据拷问会话的 9 项决策共识，把 `clean-release-build-warnings` 的后续项「CI 缺少告警门禁」落成可执行方案。本方案未创建 OpenSpec change，创建前以本文档为准。
> - 2026-08-04 审核修正：基线分支由误记的 `master` 更正为 `dev`（`d85cfb6` 仅存在于 dev，master 停在 `b9e757e`）；「dtolnay 步骤」范围由三条 workflow 更正为两条（docker-build 无宿主机 Rust 步骤）；软化「@stable 覆盖钉版」断言为待实测；修正 PR 流程表述与 docker-build 触发器描述。
> - 2026-08-04 审核结论优化：新增 master 基线分叉风险与落 master 前置验证步骤（门禁必须与清零提交同批或之后到达 master）；决策记录同步「本地与 CI 工具链版本已实际分裂」的查证事实。
> - 2026-08-05 工具链升级同步：本地升级至 rustc 1.97.1（当前 stable）；钉版目标、Dockerfile 镜像 tag 全部由 1.94.1 同步为 1.97.1；两条门禁命令在 1.97.1 实跑零告警（含此前未跑的 release + `--no-default-features` 组合）。
> - 2026-08-05 审核修订：修复 P0 设计缺陷——gate job 在全新检出下因缺少 gitignored 的 `admin-ui/dist` 而编译不过（rust-embed 编译期强制目录存在，已实验复现退出码 101），warning-gate 定义补入 dist 供给步骤（空占位目录，决策记录新增第 10 项）；master 指针由误记的本地 `b9e757e` 更正为 origin/master `606c6bc`（2026-07-18，#184，且为 dev 严格祖先、无分叉）；修正「无活跃 PR 流程」论据（#184 即分析窗口内 squash 合入 master 的 PR）；删除不成立的「dtolnay/rust-toolchain@master 不带参数」候选（该 action `toolchain` 输入 required，空值直接失败）并补版本断言；Docker 镜像 tag 收紧为 `rust:1.97.1-alpine`。
> - 2026-08-05 二审修订：修复 P1——候选 A 的 target 隔离缺陷（dtolnay action 把矩阵 targets 装在 stable 工具链上，而 `rust-toolchain.toml` 使仓库目录内 cargo 解析到 1.97.1，3 个交叉 target 腿会直接报 target may not be installed，且版本断言拦不住该失败），候选改写为 A（否决）/ A' / B（推荐）/ C（决策 #3 取向冲突仅备案）；决策 #8 补 `--locked` 与 `Cargo.lock` COPY 收紧——此前门禁的依赖锁定结论对 Docker 产物不成立；「工具链 bump 两处同步」由 change 任务义务升格为 spec delta Requirement（决策 #6 同步）；新增决策 #11：红路径实验由 dev 直推+还原改为 `codex/` 临时分支 + `workflow_dispatch`，避免污染 dev 历史；实施顺序第 6 步的前置检查由「origin/master 树实跑 cargo check」改为「先 `git merge-base --is-ancestor` 确认 fast-forward，非 ff 才实跑」（ff 场景下前者无新增信息量）；二审本地实跑确认空 dist 占位足够（含 `cargo clean -p` 强制重编排除缓存），决策 #10 与风险评估同步；「两遍增量」表述经冷缓存分析细化（第二遍在冷缓存下同样复用大部分产物，补首跑耗时估算口径）。
> - 2026-08-05 三审修订：修复 P1——决策 #8 的依赖锁定修复集不完整（`.dockerignore:3` 排除 `Cargo.lock`，lockfile 从未进入 Docker 构建上下文，现状实为完全无锁定；COPY 去通配后若不同步删除该行，docker build 会因找不到文件必失败）；决策 #8 与第 4 节由「两行」扩为「三处」，实施顺序第 4 步同步，`.dockerignore` 纳入硬约束变更范围；查证 `Swatinem/rust-cache@v2` 源码内置导出 `CARGO_INCREMENTAL=0`，候选 B 的 env 补记降级为兜底；补记门禁冷缓存已知成本（default 遍经 `openssl-src` 编译 vendored OpenSSL）；门禁命令表述由「逐字一致」改为「逐 flag 一致 + 显式附加 `-D warnings`/`--locked`」，AGENTS.md 措辞同批对齐；gate job 补 `timeout-minutes`，候选 B 补 rustup 默认 profile 注记。
> - 2026-08-05 四审修订：把单宿主机 warning gate 明确降为前置快检，7 个二进制发布矩阵腿与 Docker 实际编译统一追加 `-D warnings`，形成「快检 + 产物构建权威兜底」双层门禁；新增 `rust-toolchain.toml` 与 Docker builder tag 的机器一致性检查；Docker 手动验证改为默认不推送的 dry-run，避免实验分支覆盖 `latest`；Docker 最终镜像构建期执行 `kiro-rs --version` smoke test；删除「绝不改变运行时行为」的不成立承诺；成功标准补齐 `--locked` 与 `-D warnings`。
> - 2026-08-05 五审修订：修复 P1——实施顺序自相矛盾（原第 2 步引入含工具链版本一致性检查的 gate 并要求「门禁绿」，但 Dockerfile bump 排在第 4 步；第 2 步落地时 `rust-toolchain.toml`=1.97.1 对 Dockerfile=1.92 必然失配、gate 必红，第 2 步验证与红路径实验不可达，窗口期 master/tag 的 Docker 构建全部被拦），Docker 变更（bump + 三处锁定 + smoke test + publish 输入）整体提前为第 2 步、gate 挂载顺延为第 3 步，下游引用同步更新；决策 #13 的 `should_publish` 钉在非矩阵 pre-check job，dry-run 关闭 `cache-to`（保留 `cache-from`）；一致性检查脚本明确大小写不敏感匹配与空白归一化；候选 B 的 `rustup set profile minimal` 由「在意下载量可补」升格为实施前置必做；修正「build-dev-release.yaml 只认 dev push」表述（其另有 `workflow_dispatch` 且可指向任意分支，会移动 dev-latest 滚动 prerelease），红路径实验明确禁止 dispatch 该 workflow；补 timeout 冷缓存首跑监控注记；验证边界追加五审独立复核证据。

# CI 告警门禁方案

分析日期：2026-08-04（2026-08-05 五审修订）
分析基线：`dev` @ `d85cfb6`（告警清零已提交并归档，工作树干净）。origin/master 在 `606c6bc`（2026-07-18，#184），尚未包含清零提交；本地 master 还落后远端一个提交，仍停在 `b9e757e`（2026-05-29），后续凡提「master」一律以 origin/master 为准。门禁 change 同样先走 dev。
环境：Windows 11 / Rust 1.97.1（2026-08-05 由 1.94.1 升级）

## 背景

`clean-release-build-warnings` 已把 `cargo check --release --all-targets` 基线清零（14 → 0），提交于 `d85cfb6`，归档于 `openspec/changes/archive/2026-08-04-clean-release-build-warnings`。该 change 的完成验证中明确记录剩余风险：**CI 门禁作为后续独立 change 未落地**。

清零只是把计数器归零，没有任何机制阻止它重新涨上去。本方案就是那个机制：让引入新告警的 push 无法产出发布产物与 Docker 镜像。对应 `docs/release-build-warnings-cleanup-design.md` 的「后续项：CI 缺少告警门禁」小节。

## 现状实测（初版与审核会话查证）

- 三条 workflow：`build.yaml`（master/dev/tag/dispatch，7 平台矩阵）、`build-dev-release.yaml`（dev push 自动触发；另有 `workflow_dispatch` 且可指向任意分支，7 平台矩阵 + 滚动 prerelease）、`docker-build.yaml`（master/tag/dispatch，双架构镜像，Rust 编译在 Docker 镜像内完成，无宿主机工具链步骤）。
- **三条流水线都没有 `pull_request` 触发器**；git 历史无 merge commit，但 master 持续有 squash 合入的 PR——不止 `#171`，`#184`（Claude Sonnet 5 支持）于 2026-07-18 合入 origin/master，就在本方案分析窗口内。即：dev 为直推模式，master 侧存在低频但真实的 PR 合入流。origin/master 落后 dev 20 个提交（含清零提交），但为 dev 的严格祖先（merge-base 即 `606c6bc`），**无分叉**，dev 合入 master 可 fast-forward。
- CI 里**没有 `cargo test`、没有 clippy、没有 `-D warnings`、没有任何 RUSTFLAGS**。测试回归目前只靠 AI 协作纪律在本地兜底。
- 工具链为浮动 stable（`dtolnay/rust-toolchain@stable`）；仓库无 `rust-toolchain.toml`；Dockerfile builder 阶段钉在 `rust:1.92-alpine`，比本地准绳版本 1.97.1 旧五个小版本。
  - 注：本地 2026-08-05 已升级至 `rustc 1.97.1 (8bab26f4f 2026-07-14)`（当前 stable），与 CI 浮动 stable 的分裂暂时收敛；但浮动 stable 仍会随时间漂移，钉版把「升级」从静默漂移变成显式动作。
- **Docker 镜像构建不锁依赖，且 lockfile 从未进入构建上下文**：Dockerfile builder 阶段 `COPY Cargo.toml Cargo.lock* ./` 用通配写法（lockfile 缺失也不报错），`cargo build --release --no-default-features` 不带 `--locked`；且 `.dockerignore:3` 将 `Cargo.lock` 列为排除项——lockfile 根本进不了 Docker 构建上下文，通配实际从未命中，镜像依赖每次都完全无锁定地从零解析，漂移风险比「未加锁」更甚。修复因此需三处同步：`.dockerignore` 删除排除行、COPY 去通配、`cargo build` 加 `--locked`（见决策 #8 与第 4 节）。
- 门禁两条命令已于 2026-08-05 在 1.97.1 上实跑，均零告警：`cargo check --release --all-targets --locked`（1m54s）与 `cargo check --release --all-targets --locked --no-default-features`（13.5s）。**清零基线原在 1.94.1 建立，升级后已重新验证，无新增告警。**
  - 注：上述实跑都在本地工作树完成，`admin-ui/dist` 恰好存在（见下条）。该结果证明「告警基线为零」，**不能**证明门禁命令在全新检出的 CI runner 上可编译——本地与 CI 的漂移不只有工具链一个维度。
- **门禁命令编译依赖 `admin-ui/dist` 目录存在**：`src/admin_ui/router.rs:12` 以 `#[derive(Embed)] #[folder = "admin-ui/dist"]` 在编译期嵌入前端产物；rust-embed 8.9.0 的派生宏在目录缺失且未设 `allow_missing` 时**无条件返回编译错误**（本地 registry 中 rust-embed-impl 源码查证）。`admin-ui/dist` 为 gitignore 项、未入库；两条产物 workflow 在 cargo 步骤前显式 `pnpm install` + `pnpm build` 供给，Dockerfile 由 frontend-builder 阶段供给。复现实验：临时隐藏 `admin-ui/dist` 后跑 `cargo check --release --all-targets --locked`，退出码 101、3 处编译错误；恢复后复跑退出码 0。**gate job 必须包含 dist 供给步骤**（方案见「方案设计」第 1 节）。
- `Cargo.lock` 已入库。
- 全仓当前显式业务 feature 分叉只有一处：`src/http_client.rs:59` 的 native-tls 分支（musl target 以 `--no-default-features` 出货）；当前源码未发现 `cfg(windows)`、`cfg(unix)`、`target_os`、`target_arch` 等平台业务分叉。该事实只能说明**当前**单宿主机快检的盲区尚未被触发，不能证明未来的 Ubuntu 检查代表全部 7 个发布 target；Cargo 的 `--all-targets` 只覆盖当前编译 target 下的 lib/bin/test/bench/example 等 Cargo target 类型，不会切换 Windows、macOS、musl 或 CPU 架构。平台与架构告警必须由实际发布矩阵的 `cargo build` 兜底。
- `docker-build.yaml` 的 `workflow_dispatch` 当前与正式发布共用 `push: true`，手动运行会推送带版本号的双架构镜像，并在 `is_beta=false` 路径更新 `latest`。因此实验分支不能用现有 dispatch 直接验证 Docker；落地时必须提供默认不推送的 dry-run，并让 manifest job 在 dry-run 下跳过。
- `cargo check --no-default-features --locked`（RUSTFLAGS=`-D warnings`）早期实验为省时间用了 dev profile 实跑：退出码 0，零告警。release 组合（`--release --all-targets --no-default-features`）已于 2026-08-05 在 1.97.1 补跑确认零告警（见上），「实施前先实跑」的前置条件已满足。
- cargo 对 registry 依赖自动 `--cap-lints allow`，`-D warnings` 只会对项目自身代码报警，不会误伤第三方 crate。

## 目标与约束

目标：让「零新增编译告警」在发布路径上机器化执行——**任何在对应发布配置或发布 target 上引入项目告警的变更，都必须在产物发布前失败**。

硬约束：

- 不修改应用源码，不有意改变业务语义。本方案只改 CI 配置、工具链钉版文件、`.dockerignore`、Dockerfile 与规格文档，不触碰 `src/`。但编译器从 1.92/stable 统一到 1.97.1、Docker 依赖从现场解析切换为 `Cargo.lock` 后，最终二进制的依赖版本和代码生成可能变化，不能承诺运行时逐字节或逐行为不变；必须用 Docker smoke test 和既有发布矩阵验证控制风险。
- 前置 reusable gate 的判定命令与本地准绳 `cargo check --release --all-targets` 逐 flag 一致，仅显式附加 `-D warnings` 与 `--locked`。本地纪律继续以原始准绳统计告警；CI 在零基线上将任何项目告警升级为错误。
- 单个 Ubuntu reusable gate 只负责快速失败和 default/`--no-default-features` 覆盖，**不宣称覆盖全部平台**。两条二进制产物 workflow 的 7 个发布矩阵腿必须在实际 `cargo build` 上设置 `RUSTFLAGS=-D warnings`；Dockerfile 的实际 release build 同样设置 `-D warnings`。产物构建是平台、架构与 Docker 配置的权威兜底。
- 门禁 job 必须在全新检出（无本地构建残留）的 runner 上可编译——本地环境恒满足的前置条件（如 `admin-ui/dist`）必须在 job 内显式供给，不得依赖「本地能跑」。
- `rust-toolchain.toml` 是工具链版本的规范来源，Dockerfile builder tag 是受机器检查约束的镜像值；二者不一致时 reusable gate 必须在编译前失败。
- 临时分支和人工验证默认不得推送镜像、移动 tag 或覆盖 `latest`；只有 master/tag 自动触发或显式确认发布的人工运行可以产生外部发布副作用。
- CI / 发布脚本命中 `AGENTS.md`「OpenSpec 条件」强制项，必须走 OpenSpec change。

## 决策记录（拷问共识）

| # | 决策点 | 结论 | 被否选项与理由 |
|---|---|---|---|
| 1 | 门禁位置与语义 | 前置 reusable gate 挂进**全部三条**流水线以快速失败；两条二进制 workflow 的 7 腿 `cargo build` 与 Dockerfile 实际 build 再以 `-D warnings` 权威兜底；任一层红则不发产物/镜像；不加 PR 触发器 | 只用 Ubuntu 前置 gate（未来平台条件代码可绕过）；只在矩阵内判定（失败反馈晚且重复消耗构建资源） |
| 2 | 门禁命令 | 前置 gate 跑两条 `RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked [--no-default-features]`；产物层给原有 `cargo build` 追加同一 `RUSTFLAGS` 与 `--locked` | check+clippy、仅 clippy：clippy 存量未清且与本地准绳不一致，范围膨胀 |
| 3 | 工具链 | 新增 `rust-toolchain.toml` 钉 1.97.1，全仓生效 | 浮动 stable（新 rustc 带来与代码无关的 gate 红；初版时已查实本地版本落后 stable 四个多月，版本分裂是既成事实）；仅门禁 job 钉版（两套版本事实） |
| 4 | feature 与平台覆盖 | Ubuntu 快检跑 default + `--no-default-features`；7 个实际发布矩阵腿覆盖 Windows/macOS/Linux、GNU/musl 与 x64/arm64 的目标专属告警 | 把 `--all-targets` 误当成跨平台检查：它不会切换 Rust target triple |
| 5 | 定义形态 | 可复用 workflow（`workflow_call`），三条流水线引用同一份 | 三处内联副本：判定命令会漂移，违背单一准绳 |
| 6 | 规格归属 | 给 `build-warning-hygiene` delta 加两个 Requirement：「CI 双层强制执行」与「工具链版本规范来源及机器一致性检查」，同步 `AGENTS.md` | 只改 workflow 不写规格（义务归档后无载体）；新开 capability（割裂同一关注点） |
| 7 | 测试缺口 | 本 change 不做，记为后续项 | 顺手加 `cargo test`：范围从「告警卫生」漂移成「CI 质量门禁」 |
| 8 | Docker 工具链与依赖 | Dockerfile `rust:1.92-alpine` → `rust:1.97.1-alpine`；`.dockerignore` 删除 `Cargo.lock`、COPY 去通配、build 加 `--locked`；实际 `cargo build` 设置 `RUSTFLAGS="-D warnings"`；最终镜像构建期执行 `./kiro-rs --version` smoke test | 不动：门禁结论对 Docker 产物不成立；只在 Ubuntu check 中禁告警：Docker 的 musl build 仍有盲区；只改 COPY 不动 `.dockerignore`：COPY 必失败 |
| 9 | 流程形态 | 独立 OpenSpec change `add-ci-warning-gate` | 并入已归档 change（已归档，不可追补） |
| 10 | gate 的 dist 供给（审核新增） | gate job 在 checkout 后 `mkdir -p admin-ui/dist` 建空占位目录，满足 rust-embed 的目录存在性检查（二审本地实跑确认足够，见验证边界） | 完整 `pnpm install` + `pnpm build`（告警只由 Rust 源码决定，嵌入内容不参与告警生成，前端构建徒增耗时与 Node 依赖面）；给 derive 加 `allow_missing`（触碰 `src/`，违反硬约束且改变运行时行为） |
| 11 | 红路径证据产生方式（二审新增） | 自 dev 建 `codex/` 临时分支植入告警，用 `workflow_dispatch` 手动触发 `build.yaml` 观察门禁拦截，验证后删除分支；dispatch 仅限 `build.yaml`，不得 dispatch `build-dev-release.yaml`（其 dispatch 可指向任意分支，且 release job 会移动 dev-latest 滚动 prerelease） | 在 dev 直推实验 commit 再还原（实验 commit 会永久留在 dev 分支历史中，且会被 build-dev-release 滚动发布流水线拾取） |
| 12 | 工具链一致性（四审新增） | reusable gate 在编译前结构化读取 `rust-toolchain.toml` 的 channel，提取 Dockerfile builder 的 patch tag 并严格比较，不一致立即失败 | 只写 spec/任务提醒：无法阻止未来漏改；让 Dockerfile 复制 toolchain 文件：官方 rust 镜像自身已选定工具链，复制文件可能触发无意义的二次安装 |
| 13 | Docker 验证发布副作用（四审新增） | `docker-build.yaml` 的 `workflow_dispatch` 新增布尔输入 `publish`，默认 `false`；dry-run 只 build、不登录、不 push、不建 manifest，并关闭 `cache-to`（保留 `cache-from` 加速），实验分支层不进缓存；只有 master/tag 自动触发或人工显式 `publish=true` 才发布；`should_publish` 在非矩阵 pre-check job 计算（矩阵 job 输出取最后完成腿的值，不宜承载发布开关） | 继续用现有 dispatch 验证：会推实验镜像并覆盖 `latest`；用事后删除恢复：存在污染窗口且审计复杂 |

## 方案设计

### 1. 可复用 workflow：`.github/workflows/warning-gate.yaml`

- `on: workflow_call`，无输入参数；单一 job（建议名 `warning-gate`），`runs-on: ubuntu-latest`；`timeout-minutes: 20`（冷缓存首跑含 vendored OpenSSL 编译的上界，dev 实测出时长后按 2–3 倍余量收紧；dev 首跑需监控冷缓存耗时，若逼近 20 分钟先放宽，待缓存稳定后再收紧）。
- 步骤：checkout → **校验 `rust-toolchain.toml` channel 与 Dockerfile builder patch tag 完全一致** → **`mkdir -p admin-ui/dist`（空占位目录，供给 rust-embed 的目录存在性检查）** → 安装钉版工具链（读 `rust-toolchain.toml`）→ 断言 `rustc --version` 为钉版 → `Swatinem/rust-cache@v2`（独立 `shared-key`，如 `warning-gate`）→ 两遍检查：

```bash
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
```

- 工具链一致性检查使用 runner 自带 Python 3 的 `tomllib` 读取 TOML，不用正则解析结构化文件；Dockerfile 只允许一个形如 `FROM rust:<patch>-alpine AS builder` 的 builder 声明。脚本必须检查「恰好命中一次」和「版本完全相等」，任何缺失、多重命中或失配都退出非零；Dockerfile 关键字匹配必须大小写不敏感并归一化空白（Dockerfile 关键字本身大小写不敏感），避免未来 `from ... as builder` 小写化编辑造成误报或漏检。`rust-toolchain.toml` 是规范来源，Docker tag 是机器校验的镜像值，而不是依赖人工记忆的第二事实来源。

- **为什么必须有占位目录**：`src/admin_ui/router.rs` 的 `#[derive(Embed)]` 声明 `#[folder = "admin-ui/dist"]`，rust-embed 派生宏在编译期强制该目录存在，而它未入库（见「现状实测」与复现实验）。本地实跑零告警恰因本地有现成 dist；全新检出的 runner 不带它，若 gate 只有「checkout → 工具链 → check」必然编译失败（退出码 101 已复现）。空目录即可通过存在性检查：告警生成只依赖 Rust 源码，嵌入内容不参与。**二审已本地实跑确认空目录足够**（空目录 + `cargo clean -p kiro-rs` 强制重编，两条门禁命令均退出码 0，见验证边界）；dev 实跑（实施顺序第 3 步）保留为 CI 侧确认。
  - 已知取舍：占位目录使 gate 的嵌入内容与真实构建不同。若未来出现对 dist 内具体文件的 `include_bytes!` / `include_str!` 类硬依赖，gate 会比真实构建先红（误报方向：安全但扰人）；届时把占位步骤升级为与 build job 一致的前端构建（Node 22 + pnpm 11 + `pnpm install --frozen-lockfile` + `pnpm build`）。
- `--locked` 锁住依赖树：依赖漂移不得静默引入新告警；`Cargo.lock` 未随依赖变更提交时 CI 明确报错，属预期行为。
- 两遍共享同一 target 目录与缓存，第二遍主要是 feature 差异部分的增量。feature 差异当前只涉及 native-tls 一线（`Cargo.toml` 的 `default = ["native-tls"]`），未受影响依赖的单元哈希不变，其产物在冷缓存下同样可被第二遍复用；gate job 冷缓存首跑按「一遍全量 + 一遍小增量」估算耗时，实际拆分以 dev 实跑时长为准。已知成本：default 遍经 `reqwest/native-tls-vendored` 拉入 vendored OpenSSL（Cargo.lock 含 `openssl-src`），冷缓存下 build script 会编译完整 OpenSSL。该 job 是快速反馈层；其通过不替代 7 腿发布矩阵与 Docker 实际 build 的告警判定。

### 2. 工具链钉版

- 新增 `rust-toolchain.toml`，`channel = "1.97.1"`——与 2026-08-05 重新验证零基线所用版本一致（清零最初在 1.94.1 验证，升级后两条门禁命令已实测零告警）。
- 两条产物 workflow（`build.yaml`、`build-dev-release.yaml`）现有的 `dtolnay/rust-toolchain@stable` 显式安装 stable 为默认工具链；rustup 的解析优先级为目录覆盖 > `rust-toolchain.toml` > 默认工具链，因此仓库目录内的 cargo 调用仍会按钉版文件自动安装并使用 1.97.1，`@stable` 步骤的实际后果是冗余安装一份 stable 与版本事实含糊。
- **target 隔离是改写的中心约束（P1，二审发现）**：rustup 的 target 按工具链隔离安装。该 dtolnay action 的源码（master 分支 `action.yml`）把 `targets` 输入拼进 `rustup toolchain install ${toolchain} --target X`，即矩阵 target 装在 **stable** 工具链上；而引入 `rust-toolchain.toml` 后，仓库目录内的 cargo 解析到 rustup 自动安装的 1.97.1 工具链，该工具链只带宿主 target。后果：矩阵中 3 个交叉 target 腿（macOS-x64、Linux-musl-x64、Linux-musl-arm64）直接报 "target may not be installed"，其余 4 个宿主 target 腿（macOS-arm64、Windows-x64、Linux-x64、Linux-arm64）幸免——失败的恰含两个 musl 腿，即 `--no-default-features` 出货配置，损失命中要害。且 job 内 `rustc --version` 断言在检出目录里报的就是 1.97.1，会照常通过，该失败拦不住。改写候选：
  - A：保留 `dtolnay/rust-toolchain@stable` 不动（最小 diff），依赖钉版文件在目录内生效，配合版本断言——**否决**（二审）：target 隔离（见上）直接打挂 3 个矩阵腿，断言无法拦截，候选不成立。
  - A'：A + 在 `rust-toolchain.toml` 声明 `targets`（全部 7 个矩阵 target）：rustup 安装 1.97.1 时连带装 target，构建可行；但全仓所有 rustup 使用者（含只跑宿主编译的 gate job 与每台本地开发机）都会下载全部 7 个交叉 target，纯浪费，不推荐。
  - B（**推荐**）：移除 dtolnay 步骤，利用 runner 预装的 rustup shim 按 `rust-toolchain.toml` 自动安装 1.97.1，矩阵 target 用独立 `rustup target add ${{ matrix.target }}` 步骤补齐（仓库目录内 rustup 命令同样解析到钉版工具链，target 落在 1.97.1 上）。版本事实单一，target 按需精确安装。三个副作用注记：dtolnay action 会隐式设置 `CARGO_INCREMENTAL=0`，但移除后该值实际不丢——`Swatinem/rust-cache@v2` 源码内置 `core.exportVariable("CARGO_INCREMENTAL", 0)`（三审查证），两条产物流水线在 cargo 步骤前都经过 rust-cache，故 build job 的 env 补 `CARGO_INCREMENTAL: 0` 是兜底而非必需；GitHub ubuntu/macos/windows runner 均预装 rustup shim，自动安装为网络行为，与现状一致；shim 自动安装按 rustup 默认 profile 拉取（比 dtolnay 的 `--profile minimal` 重），每腿首跑的下载成本必然发生，实施时必须在任何 rustup shim 触发前先执行 `rustup set profile minimal`，把自动安装降为最小 profile（必做而非可选）。
  - C：`dtolnay/rust-toolchain@1.97.1` + `targets: ${{ matrix.target }}`：可行且 target 安装正确；但版本事实出现在 `rust-toolchain.toml`、workflow 引用、Dockerfile 三处，与决策 #3 的单一钉版事实取向冲突，仅备案。
  - 已排除：`dtolnay/rust-toolchain@master` 不带 toolchain 参数——已拉取该 action master 分支 `action.yml` 查证，`toolchain` 输入 `required: true`，解析步骤对空值直接 `exit 1`，此候选不成立。
  **B 的具体写法与生效性（shim 自动安装、target 归属、CARGO_INCREMENTAL 补记）在实施时于 dev 分支实测确认，不凭文档推断。**`docker-build.yaml` 无宿主机 Rust 步骤，不受影响。
- bump 工具链从此是一次显式动作：先改规范来源 `rust-toolchain.toml`，同步 Dockerfile 镜像 tag，重跑本地准绳与两条严格门禁命令；遗漏同步时，reusable gate 的版本一致性检查必须在编译前失败。

### 3. 三条流水线的挂法

- `build.yaml`：新增调用 job（`needs: pre-check`，与 build 相同的 `should_build` 条件，避免被跳过的 commit 白跑门禁）；`build` 的 needs 增加该 job。7 腿 `Build app` 统一设置 `RUSTFLAGS: -D warnings`，命令增加 `--locked`，保留 musl 的 `--no-default-features`。release job 经 build 传递依赖，无需改动。
- `build-dev-release.yaml`：新增调用 job；`build` 的 needs 增加该 job；7 腿 `Build app` 同样设置 `RUSTFLAGS: -D warnings` 并增加 `--locked`。
- `docker-build.yaml`：新增调用 job（同样带 `should_build` 条件）；`build` 的 needs 增加该 job。Dockerfile 内的实际 musl build 设置 `RUSTFLAGS="-D warnings"`，所以 Docker 产物不依赖宿主 gate 代表其配置。
- 语义：前置 gate 红 → 产物构建不启动；前置 gate 绿但某发布 target 出现专属告警 → 对应矩阵腿红；任一产物腿红 → 无 release、无多架构 manifest。

### 4. Dockerfile 对齐

- builder 阶段 `FROM rust:1.92-alpine` → `FROM rust:1.97.1-alpine`，使门禁判定版本与镜像编译版本逐位一致。用 patch 级 tag 而非 `1.97-alpine`：minor 级 tag 会随 1.97.x patch 发布被重建漂移，与钉在 `1.97.1` 的 `rust-toolchain.toml` 失同步。两个 tag 均已查证存在于 Docker Hub（二审复核：`1.97.1-alpine`、`1.97-alpine` 均 2026-07-16 更新）。
- 依赖锁定三处同步收紧：① `.dockerignore` 删除 `Cargo.lock` 行；② `COPY Cargo.toml Cargo.lock* ./` → `COPY Cargo.toml Cargo.lock ./`；③ `cargo build` 加 `--locked`。实际命令同时设置 `RUSTFLAGS="-D warnings"`，使 Docker 的 `--no-default-features` musl 产物自行执行零告警判定。
- 最终阶段复制二进制后执行 `RUN ./kiro-rs --version`。Clap 在加载配置/凭据前处理 `--version`，该 smoke test 无需真实凭据，可验证最终 Alpine 镜像中的二进制可执行。项目当前没有独立 health endpoint；本 change 不虚构服务级健康检查，服务启动/HTTP smoke test 另行设计。
- Dockerfile 不复制 `rust-toolchain.toml`，镜像内编译版本由镜像 tag 决定；reusable gate 负责机器比较两者。编译器与依赖树变化可能改变二进制，故本节只承诺「无有意业务语义变更」，不承诺运行时完全不变。
- `docker-build.yaml` 的人工触发新增 `publish` 布尔输入（默认 `false`），并在**非矩阵**的 `pre-check` job 计算单一 `should_publish` 输出（GHA 对矩阵 job 输出取最后完成腿的值，放 pre-check 避免语义歧义）：master/tag 自动触发为 true；人工触发仅在显式 `publish=true` 时为 true。登录 GHCR、build-push-action 的 `push`、manifest job 都受该输出约束。临时分支 dry-run 只完成双架构 build（含 Dockerfile smoke test），不登录、不推送、不更新 `latest`；dry-run 保留 `cache-from: type=gha` 加速构建但关闭 `cache-to`，实验分支层不进缓存（层缓存按内容寻址，无正确性风险，但占配额且可能落入读取路径）。

### 5. 规格与文档同步

- `build-warning-hygiene` delta：新增两个 Requirement——
  1. 「CI 双层强制执行」：Ubuntu reusable gate 必须执行与本地准绳同判定面的 default/无默认 feature 快检；每个二进制发布矩阵腿与 Docker 实际 build 必须把项目告警升级为错误并锁定依赖。任一层或任一 target 失败时不得创建 release 或 manifest。附平台专属告警被矩阵腿拦截、Docker dry-run 不发布等 Scenario。
  2. 「工具链版本规范来源及机器一致性检查」：`rust-toolchain.toml` 是规范来源；Dockerfile builder patch tag 必须与其完全一致，CI 必须在编译前机器校验；bump 后必须重跑本地准绳及严格门禁命令。失配或未验证时该变更 MUST 视为未完成。
- `AGENTS.md`：同步相关小节（零新增告警的 CI 执行、高风险矩阵中 CI 行、准绳命令措辞与门禁判定命令对齐——含 `--locked`）。
- 归档后更新 `docs/release-build-warnings-cleanup-design.md` 的「后续项」小节，标注已由本 change 落地及归档位置；同时登记本方案「后续项」中的未做事项。

## 实施顺序

按风险从低到高，每步独立可验证：

```text
1. rust-toolchain.toml + 两条产物 workflow（build.yaml、build-dev-release.yaml）工具链步骤按候选 B 改写；7 腿 cargo build 增加 RUSTFLAGS=-D warnings 与 --locked
   verify: 本地原始准绳与两条严格门禁命令零告警；推 dev 确认 7 个矩阵腿全部可构建，重点确认 3 个交叉 target 腿以及告警升级环境确实生效
2. Dockerfile bump 至 rust:1.97.1-alpine + 依赖锁定三处收紧 + Docker build 的 -D warnings + 最终镜像 --version smoke test；docker-build workflow_dispatch 增加默认 false 的 publish 输入并统一约束登录/push/manifest
   verify: 从 codex/ 临时分支以 publish=false 跑双架构 dry-run，确认 build 与 smoke test 绿，GHCR 无新 tag、latest digest 不变、manifest job skipped；正式发布能力只在 master/tag 自动路径或显式 publish=true 路径验证
   注（五审重排）：本步必须先于第 3 步——gate 的工具链一致性检查一经引入就会比较 rust-toolchain.toml（1.97.1）与 Dockerfile builder tag，Dockerfile 若仍停在 1.92，gate 必红，第 3 步「门禁绿」验证不可达，且窗口期 master/tag 的 Docker 构建全部被拦。publish 输入与 Docker 变更互为验证前提，故同批提前
3. warning-gate.yaml（含工具链版本一致性检查、admin-ui/dist 占位步骤与 rustc 版本断言）+ 三条流水线挂载
   verify: 推 dev 实测门禁绿（此时 TOML 与 Dockerfile 已同步为 1.97.1，一致性检查应通过）；另在临时分支只改 Docker builder tag 制造失配，确认 gate 在 cargo check 前失败且所有产物 job 不启动
4. 红路径实验（决策 #11）：自 dev 建 codex/warning-gate-red-test 临时分支，植入一条告警的 commit，用 workflow_dispatch 对该分支手动触发 build.yaml
   verify: 门禁红、矩阵不启动、无产物；验证后删除临时分支。实验 commit 不落 dev；build-dev-release.yaml 的红路径等价性由同一可复用 workflow + 相同 needs 接线推得。注意：该 workflow 另有 workflow_dispatch 触发器且可指向任意分支，其 release job 会移动 dev-latest 滚动 prerelease，实验中禁止对其 dispatch
5. spec delta + AGENTS.md + 文档同步
   verify: openspec validate --all 通过
6. 合入 master 前：先 git merge-base --is-ancestor origin/master dev 确认 fast-forward 仍成立
   verify: ff 成立时合入后的树即门禁已验证的 dev 树，无需额外实跑（二审更正：ff 场景下在 origin/master 树实跑 cargo check 无新增信息量）；仅当非 ff（窗口内有新 PR 先行落 master）才在 origin/master 树实跑门禁命令重认零基线后再合；门禁与清零提交同批到达 master
```

第 4 步是告警红路径的直接证据；第 3 步的版本失配实验是工具链同步门禁的直接证据。只验证绿路径不能证明门禁真的会拦。所有临时分支 dispatch 必须可明确识别、保持 `publish=false`，并在验证后立即删除分支。

## 成功标准

```bash
cargo check --release --all-targets
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features
openspec validate --all
```

Windows PowerShell 本地执行后两条命令时，先设置 `$env:RUSTFLAGS = "-D warnings"`，两条命令完成后执行 `Remove-Item Env:RUSTFLAGS`。原始准绳用于报告告警数，严格命令用于证明 CI 的告警升级和依赖锁定语义。

加上 CI 侧证据：dev 绿提交的前置 gate 和 7 个矩阵腿全部通过；告警红路径被 gate 拦截且矩阵不启动；工具链 tag 失配在编译前失败；Docker 双架构 dry-run 通过且无 GHCR/tag/`latest` 副作用；Dockerfile 的最终镜像 `--version` smoke test 通过；gate 在全新检出下可编译（dist 供给生效）。

注：上面三条 Cargo 命令都要求 `admin-ui/dist` 存在（本地开发树恒满足）；CI 侧由 gate job 的占位步骤保证同一前提。

## 明确不采用的做法

- **不引入 clippy 门禁**。项目从未跑过 clippy，存量未知，等于在门禁 change 里夹带新一轮清零。记为后续项。
- **不顺手加 `cargo test`**。CI 缺测试是真实缺口，但属另一个 change 的范围与验证标准，记为后续项。
- **不加 `pull_request` 触发器与分支保护**。dev 侧为直推模式；master 侧虽有低频 squash PR 合入（如 `#171`、分析窗口内的 `#184`），但本门禁定位是发布路径强制（红则不发产物/镜像），PR 触发属合并时强制，与分支保护配套才有意义。留给后续项（见「后续项」第 3 条），不在本 change 混做。
- **不内联复制门禁 job**。三处副本必然漂移，用可复用 workflow 保单一准绳。
- **不只挂部分流水线**。Docker 镜像同样是发布产物，漏挂等于门禁有后门。
- **不把 Ubuntu `--all-targets` 当成跨平台证明**。它是前置快检；发布矩阵的实际 build 才覆盖 target triple。
- **不在本 change 新增 health endpoint**。Docker 先用最终镜像 `--version` 做无凭据 smoke test；服务级启动与 HTTP 探针需要单独定义契约。

## 风险评估

- master 基线落后（初版误判为「分叉」，已更正）：origin/master 在 `606c6bc`（2026-07-18，#184），不含清零提交，但为 dev 的严格祖先（merge-base 即其自身，dev 领先 20 个提交，可 fast-forward 合入）——初版基于本地落后的 master（`b9e757e`）得出「树已分叉、基线未知」的结论不成立。剩余风险有二：门禁若先于清零提交落到 master，master 侧第一次 push 即红；合入窗口内若有新 PR 先行落 master，会带入未经清零的代码。缓解：本 change 与清零提交同批走 dev 合入 master；合入前按实施顺序第 6 步确认 fast-forward 仍成立（仅非 ff 时才需在 origin/master 树实测）；期间有新 PR 落 master 则重测后再合。
- 工具链钉版：矩阵构建从浮动 stable 切到 1.97.1，该版本即本地验证通过的版本，风险低；未来 bump 需两处同步并重跑准绳命令（已写入 change 任务并升格为 delta Requirement）。本次本地升级（1.94.1 → 1.97.1）即是一次现成例证：升级后必须重跑门禁命令确认基线仍为零，本次已执行并通过。改写本身的主要风险不在版本而在 target 供给归属（见第 2 节 target 隔离分析）：实施顺序第 1 步的 verify 以 7 腿全绿、尤其 3 个交叉 target 腿为判据。
- 平台覆盖：Ubuntu reusable gate 无法发现仅在其他 target triple 编译的项目告警；7 腿二进制矩阵与 Docker musl build 上的 `-D warnings` 负责权威兜底。代价是少数平台专属告警要到矩阵阶段才反馈，但不会进入发布产物。
- 工具链同步：版本仍以 TOML + Docker tag 两种语法表达，但不再只靠人工纪律；机器一致性检查将缺失、多重声明或失配全部转为前置失败。
- Docker 行为：改用锁文件并升级 rustc 可能改变最终二进制，不能归类为绝对无运行时变化。`--version` smoke test 只证明最终镜像二进制可执行，不证明服务完整功能；剩余风险由既有发布矩阵与后续服务级 smoke test 承担。
- Docker 发布副作用：人工 dispatch 默认 dry-run，只有明确发布路径才登录和推送。实施验证必须记录运行 URL、`publish=false`、manifest skipped，以及运行前后 `latest` digest 未变。
- gate 的 dist 供给（审核新增）：空目录占位有源码查证、复现实验与二审本地实跑支撑——空目录 + `cargo clean -p` 强制重编下两条门禁命令均退出码 0（见验证边界）；剩余缺口只有 CI 侧确认，由实施顺序第 3 步的 dev 实跑覆盖。若实跑发现空目录不满足，升级为完整前端构建步骤，不改门禁语义。
- 可复用 workflow：job 级 `if`/`needs` 条件写在调用方，语义与现有 pre-check 一致；实施时以 dev 实跑确认。
- `--locked`：依赖变更忘提交 lockfile 时 CI 直接失败并给出明确报错，属期望的严格化，非风险。
- 钉版工具链在 runner 上需要网络安装；GitHub runner 场景为常态，不额外处理。

## 后续项（本次不做，单独登记）

1. **clippy 门禁**：先做一轮 clippy 存量清零，再以 `cargo clippy --all-targets -- -D warnings` 挂入门禁。
2. **测试门禁**：CI 目前完全不跑测试；独立 change 决定 profile、失败策略与矩阵联动。
3. **PR 触发 / 分支保护**：协作模式变化（引入 PR 流程）时，为门禁补 `pull_request` 触发器。

## 流程门禁

按 `AGENTS.md`「OpenSpec 条件」，CI / 发布脚本为强制项，本方案**必须走 OpenSpec change**：

- 建议 change 名：`add-ci-warning-gate`（`openspec/changes/` 下无同名目录）。
- 触及 capability：`build-warning-hygiene`（delta 新增「CI 双层强制执行」与「工具链版本规范来源及机器一致性检查」两个 Requirement）。
- 实施流程：`openspec-new-change` / `openspec-propose` → `openspec-superpowers-bridge` → `openspec-apply-change` → `spec-compliance-check` → `openspec-verify-change` → `verification-before-completion` → 归档。

## 验证边界

初版会话已实跑/查证：

- 三条 workflow、Dockerfile、`Cargo.toml`、`Cargo.lock` 入库状态、git 历史（只读查证）
- `rg` 全仓 feature/平台 cfg 分叉统计（各 1 处与 0 处）
- `cargo check --no-default-features --locked`（RUSTFLAGS=`-D warnings`，dev profile，1.94.1，退出码 0 零告警）
- 2026-08-05（1.97.1）：`cargo check --release --all-targets --locked` 与 `cargo check --release --all-targets --locked --no-default-features` 均退出码 0 零告警

审核会话（2026-08-05）补充实跑/查证：

- 两条门禁命令（RUSTFLAGS=`-D warnings`，1.97.1）复跑：均退出码 0，零告警
- 复现实验：临时移出 `admin-ui/dist` 后 `cargo check --release --all-targets --locked` 退出码 101、3 处编译错误；恢复 dist 后复跑退出码 0
- rust-embed-impl 8.9.0 源码查证：目录缺失且无 `allow_missing` 时派生宏无条件返回编译错误
- git 查证：origin/master = `606c6bc`（2026-07-18，#184），为 dev 严格祖先（dev 领先 20 提交、可 fast-forward）；本地 master 落后远端一个提交
- `dtolnay/rust-toolchain` master 分支 `action.yml` 查证：`toolchain` 输入 `required: true`，空值 `exit 1`
- Docker Hub 查证：`rust:1.97-alpine`、`rust:1.97.1-alpine`、`rust:1.92-alpine` tag 均存在

二审会话（2026-08-05）补充实跑/查证：

- 空 `admin-ui/dist` 占位足够性：真实 dist 改名后建空目录，两条门禁命令（RUSTFLAGS=`-D warnings`，1.97.1）均退出码 0，且均为 `kiro-rs` crate 真实重编；随后完全移除 dist 并 `cargo clean -p kiro-rs --release` 强制重编，退出码 101、3 处 E0599 错误（复现目录缺失失败）；恢复真实 dist 复跑退出码 0，文件完整性确认
- `dtolnay/rust-toolchain` master 分支 `action.yml` 复核：`targets` 输入拼进 `rustup toolchain install` 命令，确认 target 按工具链隔离安装（候选 A 否决、B 推荐的依据）
- Docker Hub 复核：`rust:1.97.1-alpine`、`rust:1.97-alpine` 存在（均 2026-07-16 更新）
- git 复核：origin/master（`606c6bc`）为 dev 严格祖先，merge-base 即其自身，dev 领先 20 提交，fast-forward 可行
- Dockerfile 查证：builder 阶段 `Cargo.lock` 通配 COPY、`cargo build` 无 `--locked`（决策 #8 收紧依据）
- feature 增量分析：`--no-default-features` 相对 default 只移除 native-tls 一线，未受影响依赖的产物预期可被第二遍复用（冷缓存亦然），实际拆分以 dev 实跑时长为准

三审会话（2026-08-05）补充实跑/查证（独立复审）：

- `.dockerignore` 查证：`Cargo.lock` 为排除项（第 3 行），lockfile 从未进入 Docker 构建上下文——决策 #8 必须同步删除该行，否则去通配 COPY 后 docker build 必失败
- `Swatinem/rust-cache` master 分支 `src/restore.ts` 源码查证：内置 `core.exportVariable("CARGO_INCREMENTAL", 0)`，移除 dtolnay 步骤后该变量不丢
- `Cargo.toml`/`Cargo.lock` 复核：`native-tls = ["reqwest/native-tls-vendored"]`，lockfile 含 `openssl-src`；gate default 遍冷缓存需经 build script 编译 vendored OpenSSL（首跑成本已知，见第 1 节）
- 两条门禁命令（RUSTFLAGS=`-D warnings`，1.97.1）独立强制重编（`cargo clean -p kiro-rs --release` 后）复跑：均退出码 0，零告警
- 空 `admin-ui/dist` 占位足够性独立复现：移出真实 dist 建空目录并强制重编，两条门禁命令均退出码 0；实验后恢复真实 dist（文件数与大小确认）
- git/workflow/源码断言复核：dev@`d85cfb6`、origin/master@`606c6bc`（2026-07-18，#184）、本地 master@`b9e757e`（落后远端 1 提交）、merge-base 即 `606c6bc`、fast-forward 可行、dev 领先 20 提交、历史无 merge commit、三条 workflow 触发器与矩阵、`cfg(feature)` 全仓仅 `src/http_client.rs:59` 一处、平台 cfg 为零、Docker Hub `1.97.1-alpine`/`1.97-alpine` tag 存在（均 2026-07-16 更新）

四审会话（2026-08-05）补充查证：

- Cargo `--all-targets` 语义复核：覆盖当前 target triple 下的 Cargo target 类型，不等于跨 Windows/macOS/Linux 或 x64/arm64/musl 编译；方案已改为 Ubuntu 快检 + 7 腿/Docker 产物层权威兜底
- `docker-build.yaml` 副作用复核：现有 `workflow_dispatch` 固定 `push: true`，人工路径 `is_beta=false` 并创建 `latest` manifest；方案已改为默认 `publish=false` 的 dry-run
- Docker smoke test 可行性复核：二进制使用 Clap `#[command(version)]`，`--version` 在应用加载配置和凭据前退出；项目当前无独立 health endpoint
- 工具链同步复核：当前设计中的 TOML 与 Docker tag 是两处表达，只有新增机器比较才能阻止未来漏同步

五审会话（2026-08-05）补充实跑/查证（设计文档评审）：

- 逐条复核全部命中：git 四点（dev@d85cfb6、origin/master@606c6bc 为 dev 严格祖先且 merge-base 即其自身、dev 领先 20 提交、本地 master@b9e757e、历史 merge commit 为 0）、三条 workflow 触发器与 job 接线、Dockerfile/.dockerignore 现状、rust-embed 8.9.0、`cfg(feature)` 仅 `src/http_client.rs:59` 一处且平台 cfg 为零、无 `build.rs`、clap `#[command(version)]`（`src/model/arg.rs:5`，`--version` 在加载配置前退出）、`build-warning-hygiene` capability 存在且 `openspec/changes` 无同名目录
- 独立实跑：两条门禁命令（RUSTFLAGS=`-D warnings`，1.97.1，热缓存）退出码 0；Docker Hub `rust:1.97.1-alpine` 独立确认存在（2026-07-16 更新）
- 发现并修复 P1：实施顺序自相矛盾——原第 2 步引入的 gate 含工具链一致性检查，而 Dockerfile bump 在原第 4 步，第 2 步落地时 gate 对未 bump 的 Dockerfile 必红；Docker 变更提前为第 2 步、gate 挂载顺延为第 3 步，下游引用同步更新

未运行：冷缓存强制重编（五审实跑为热缓存）、CI 实测（需推 dev 或 dispatch）、docker build（本地无 Docker 环境）。本文档为设计产物，未修改任何源码与 CI 配置；二审/三审仅对 `admin-ui/dist` 做临时改名/建空目录实验，实验后均已恢复真实产物并确认。
