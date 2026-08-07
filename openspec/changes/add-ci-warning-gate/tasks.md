## 0. 前置对齐与事实确认

- [x] 0.1 dev 快进对齐 master：`git merge --ff-only origin/master` 实跑输出 `Updating d85cfb6..dcd9351 / Fast-forward`，退出码 0。验证：`git rev-parse HEAD` = `dcd9351`（与 origin/master 同点）、`git diff --quiet origin/master dev` 退出码 0、`git status --short` 仅本 change 的未跟踪文件，无文件被修改
- [x] 0.2 确认 `rust:1-alpine` tag 存在：用户于 2026-08-05 实跑 `docker pull rust:1-alpine`，Docker Hub 正常解析并开始拉取三层（Alpine 基础层复用 + 75MB + 273MB 工具链层）。**注**：AI 侧无法复核（本机 `auth.docker.io` / `registry-1.docker.io` 超时不可达，且未安装 docker CLI）
- [x] 0.2a 确认 `rust:1-alpine` 实际指向的编译器版本：用户实跑 `docker run --rm rust:1-alpine rustc --version` → `rustc 1.97.1 (8bab26f4f 2026-07-14)`，与本地、官方 channel 清单、7 个 runner 镜像**逐位一致**。「`1-alpine` 指向 1.x 最新」的语义假设成立
- [x] 0.3 GHCR 取证方式已落定：用户已执行 `gh auth refresh -h github.com -s read:packages`，scope 生效（实测 `gh auth status` 含 `read:packages`）。取证命令与基准值见 0.3a
- [x] 0.3a 记录 dry-run 前的 GHCR 基准（2026-08-05 实测）。包为 org **私有**包，`version_count=18`，`updated_at=2026-08-05T07:42:53Z`（即 `dcd9351` 合入 master 那次 run）。**`latest` 当前钉在 `sha256:dcd5c510f9ed`**（与 `v2026.8.4` 同 digest）。取证命令：

```powershell
gh api "orgs/wtfdelphia/packages/container/kiro-rs/versions" --jq '.[0:6] | .[] | "\(.name[0:19]) tags=\(.metadata.container.tags)"'
```

  当前输出（dry-run 后必须逐行一致）：

```text
sha256:f960b53f3ebe tags=["beta","beta-dcd935"]
sha256:b998aa1599e3 tags=["beta-dcd935-amd64"]
sha256:c435feec2a16 tags=["beta-dcd935-arm64"]
sha256:dcd5c510f9ed tags=["v2026.8.4","latest"]
sha256:dc89e43eb150 tags=["v2026.8.4-amd64"]
sha256:d2c98b828b92 tags=["v2026.8.4-arm64"]
```

  **注**：`gh api "orgs/wtfdelphia/packages?package_type=container"` 对该 org 私有包返回空数组 `[]`，取证必须用具名路径 `orgs/wtfdelphia/packages/container/kiro-rs`，不能用列表接口
- [x] 0.4 记录变更前基线：`cargo check --release --all-targets` 实跑（2026-08-05，dev @ `dcd9351`），退出码 0，`Finished release profile in 55.49s`，**输出无任何 warning 行 → 基线告警数 = 0**

## 1. Docker 依赖锁定与 smoke test

三处依赖锁定互为前提，**必须同一提交**：缺任一处则 docker build 必失败。

- [x] 1.1 `.dockerignore` 删除 `Cargo.lock` 排除行（原第 3 行）。附带：原文件缺尾换行，本次补齐（无功能影响）
- [x] 1.2 `Dockerfile:14` `COPY Cargo.toml Cargo.lock* ./` → `COPY Cargo.toml Cargo.lock ./`
- [x] 1.3 `Dockerfile:18` `cargo build --release --no-default-features` → 追加 `--locked`；确认**未**设置 `RUSTFLAGS`
- [x] 1.4 `Dockerfile:10` `FROM rust:1.92-alpine` → `FROM rust:1-alpine`
- [x] 1.5 `Dockerfile` 最终阶段 `COPY` 之后增加 `RUN ./kiro-rs --version`，附三行注释说明只断言退出码、无需凭据、版本字符串属 version-governance 范围
- [x] 1.6 `docker-build.yaml` `workflow_dispatch` 新增布尔输入 `publish`（`type: boolean`、`default: false`、`required: false`）
- [x] 1.7 `pre-check` job（非矩阵）新增 `should_publish` 输出。计算块置于 `should_build` 的 `if/else` 之外无条件执行，因此三条路径均有定义：`workflow_dispatch` 且 `inputs.publish == true` → true；`workflow_dispatch` 其余 → false 并发 `::notice::Dry run`；非 dispatch（master/tag push）→ true。附注释说明为何不放在矩阵 job
- [x] 1.8 `should_publish` 约束三处：GHCR 登录步骤加 `if:`（:103）、`build-push-action` 的 `push:` 改为表达式（:117）、manifest job 加 `needs: [pre-check, build]` 与 `if:`（:125-128）
- [x] 1.9 `cache-from: type=gha` 保留；`cache-to` 改为条件表达式（:116），dry-run 时为空串。验证：`python yamlcheck` 解析三条 workflow 全部 OK，job 依赖图为 `pre-check → build → manifest(needs pre-check+build)`
- [x] 1.10 验证（2026-08-07 实跑）：自 dev 建 `codex/warning-gate-verify`，以 `publish=false` dispatch `docker-build.yaml` → run [31148217637](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148217637) **success**。双架构 build 全绿，`manifest` job **skipped**，`Log in to GitHub Container Registry` 步骤 **skipped**（step 5，证明 dry-run 从未登录 registry）。构建日志逐项确认：`rust:1-alpine` 解析为 `sha256:3c38f3f8`、`[builder 4/7] COPY Cargo.toml Cargo.lock ./` 命中（三处锁定自洽，未因去通配失败）、`[builder 7/7] RUN cargo build --release --no-default-features --locked` 成功、`[stage-2 5/5] RUN ./kiro-rs --version` 输出 `kiro-rs 2026.3.1` 退出 0（只断言退出码，版本漂移属 version-governance 范围，符合 D7）
- [x] 1.10a 用 0.3a 的取证命令复跑（dry-run 后）：`version_count=18`、`updated_at=2026-08-05T07:42:53Z`（**未变**）、`latest` 仍为 `sha256:dcd5c510f9ed`，六行 digest/tag 输出与 0.3a 基准**逐行一致**，无新 digest 进入。dry-run 零发布副作用成立
- [x] 1.11 临时分支删除见 6.5a（与红路径分支同批清理，两条分支的取证均已固化到本文件）

## 2. CI reusable gate 与流水线挂载

- [x] 2.1 新增 `.github/workflows/warning-gate.yaml`：`on: workflow_call`（无输入），单 job `warning-gate`，`runs-on: ubuntu-latest`，`timeout-minutes: 20`；顶部注释记录「仪器钉版、生产线浮动」的设计理由并指向设计文档
- [x] 2.2 gate 步骤 1：`actions/checkout@v5`
- [x] 2.3 gate 步骤 2：`mkdir -p admin-ui/dist`，附注释说明 rust-embed 编译期依赖与「空占位即足够」的理由
- [x] 2.4 gate 步骤 3：`dtolnay/rust-toolchain@1.97.1`，未传 `toolchain` 参数（该分支无此输入）
- [x] 2.5 gate 步骤 4：`set -euo pipefail` + 字符串包含断言，不符则 `::error::` 并 `exit 1`
- [x] 2.6 gate 步骤 5：`Swatinem/rust-cache@v2`，`shared-key: warning-gate`、`cache-on-failure: true`
- [x] 2.7 gate 步骤 6-7：两遍判定拆为独立 step（便于定位哪一遍失败），`RUSTFLAGS: -D warnings` 用 step 级 `env:` 而非 shell 前缀
- [x] 2.8 `build.yaml` 新增 `warning-gate` 调用 job（`needs: pre-check` + `should_build` 条件）；`build.needs` 改为 `[pre-check, warning-gate]`
- [x] 2.9 `build.yaml:131` `Build app` 增加 `--locked`；确认未加 `RUSTFLAGS`、`dtolnay/rust-toolchain@stable` 未动
- [x] 2.10 `build-dev-release.yaml` 新增 `warning-gate` 调用 job（无条件，该 workflow 无 `should_build`）；`build.needs` 改为 `[prepare, warning-gate]`
- [x] 2.11 `build-dev-release.yaml:123` `Build app` 增加 `--locked`；同样未加 `RUSTFLAGS`
- [x] 2.12 `docker-build.yaml` 新增 `warning-gate` 调用 job（`needs: pre-check` + `should_build` 条件）；`build.needs` 改为 `[pre-check, warning-gate]`
- [x] 2.12a YAML 语法与接线验证：`python yamlcheck` 四条 workflow 全部 OK；job 图为 gate 插在 pre-check/prepare 与 build 之间，release/manifest 经 build 传递依赖；gate 的 `on` 键解析为 `workflow_call`、7 个 step、`timeout-minutes: 20`
- [x] 2.13 验证绿路径（2026-08-07 实跑）：run [31148152382](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148152382) **success** —— `pre-check` 绿 → `warning-gate / warning-gate` 绿 → **7 腿全绿**（Linux-x64 3m52s、Linux-arm64 3m37s、Linux-musl-x64 2m33s、Linux-musl-arm64 2m51s、Windows-x64 5m17s、macOS-x64 3m48s、macOS-arm64 4m11s），`release` job **skipped**。Docker 绿见 1.10。

  **偏离说明**：本任务原文写「推 dev」，实际改为 `codex/warning-gate-verify` 分支 + `workflow_dispatch` `build.yaml`。理由：推 dev 会触发 `build-dev-release.yaml`，其 release job 会 `git tag -f dev-latest` force-push 并 `gh release delete` 重建滚动 prerelease，属真实发布副作用；而 `build.yaml` 的 release job 条件为 `startsWith(github.ref, 'refs/tags/v')`，dispatch 到分支时不满足故 skipped，只产 Actions artifact。gate 与 7 腿的证据等价（同一 reusable workflow、相同 `needs` 接线、相同 7 腿矩阵），但零发布副作用。这与 design.md 验证策略中「禁止 dispatch build-dev-release.yaml」的同一顾虑一致，此处把该顾虑同样应用于绿路径
- [x] 2.14 gate 冷缓存首跑实测 **2m03s**（run 31148152382，独立 `shared-key: warning-gate` 故必为冷缓存，含 vendored OpenSSL 编译）。距 `timeout-minutes: 20` 有约 10 倍余量，**无需放宽**；按 2-3 倍余量收紧的判据对应 4-6 分钟，但冷缓存首跑样本仅 1 个，暂不收紧以留波动空间，登记为后续项 7.9

## 3. 红路径实验（门禁真的会拦的直接证据）

只验证绿路径不能证明门禁会拦。

- [x] 3.1 自 dev 建 `codex/warning-gate-red-test`，植入 commit `7c9533b`：`src/main.rs` 末尾加 `fn gate_red_test_probe() {}`（无下划线前缀，规避 4.5b 记录的 lint 豁免陷阱），附注释说明该提交只存在于临时分支
- [x] 3.2 用 `workflow_dispatch` 对该分支触发 **`build.yaml`**（未触碰 `build-dev-release.yaml`，理由见任务原文）
- [x] 3.3 **红路径成立**（2026-08-07 实跑）：run [31148197200](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148197200) **failure** —— `pre-check` success → `warning-gate / warning-gate` **failure** → `build` **skipped** → `release` **skipped**。矩阵 job 从未启动，无产物上传。失败日志的完整因果链：

```text
rustc: rustc 1.97.1 (8bab26f4f 2026-07-14)          <- 钉版断言通过
error: function `gate_red_test_probe` is never used  <- dead_code 经 -D warnings 升级为 error
268 | fn gate_red_test_probe() {}
error: could not compile `kiro-rs` (bin "kiro-rs") due to 1 previous error
error: could not compile `kiro-rs` (bin "kiro-rs" test) due to 1 previous error
##[error]Process completed with exit code 101
```

  该 run 顺带实证三项此前只是推断的假设：① `dtolnay/rust-toolchain@1.97.1` 分支的钉版行为如设计所述，版本断言步骤通过；② **空 `admin-ui/dist` 占位足以通过编译** —— 编译推进到了 `src/main.rs:268` 的探针，说明 rust-embed 派生宏的目录存在性检查已被满足（proposal.md Assumptions 第 2 条由「继承的实验结论」升级为 CI 实测事实，无需升级为完整 `pnpm build`）；③ `--all-targets` 覆盖 bin 与 test 两个目标
- [x] 3.4 分支删除见 6.5a。实验 commit 未落 dev：`7c9533b`/`900914f` 只存在于 `codex/warning-gate-red-test`

## 4. 第一防线：pre-push hook

- [x] 4.1 新增 `scripts/git-hooks/pre-push`（可执行位已设为 `100755`，Windows 下需显式设否则类 Unix 克隆不执行）：运行原始准绳命令，存在告警则非零退出并打印完整输出与去重后的计数
- [x] 4.2 异常处理三类：非 git 工作树（`rev-parse --show-toplevel` 为空）→ 拒绝；判定命令自身失败（编译错误等）→ 拒绝并回显；cargo 定位失败 → 拒绝并提示。**cargo 定位加 Windows 兜底**（见 4.1a）
- [x] 4.1a Windows 兜底（实施中发现的真实缺口，用户确认按方案 2 处理）：`command -v cargo` 之外依次尝试 `$CARGO_HOME/bin/cargo{,.exe}`、`$HOME/.cargo/bin/cargo{,.exe}`、`$USERPROFILE/.cargo/bin/cargo.exe`。根因：本机 `bash` 是 WSL2 Linux bash，看不到 Windows PATH 与 `CARGO_HOME`；而 `git push` 实际调用的是 Git for Windows 的 MSYS bash（`D:\Program Files\Git\bin\bash.exe`），后者能看到二者。若只测 WSL bash 会得出错误结论
- [x] 4.3 `README.md`「1. 编译」小节末尾增补「可选：启用本地告警检查」引用块，含 `git config core.hooksPath scripts/git-hooks` 与「幂等，执行一次即可」
- [x] 4.4 README 与 hook 脚本头注释均写明非强制性三条（需一次性配置才生效、新克隆默认未启用、`--no-verify` 可绕过），并指出强制判定点在 CI 门禁
- [x] 4.5 验证（拒绝路径）：在 `src/main.rs` 末尾植入 `fn hook_probe_dead_fn() {}`，用 MSYS bash 实跑 hook → 输出 `pre-push: 1 distinct compiler warning site(s) found. Push refused.`，退出码 1。**计数口径实测修正**：见 4.5a
- [x] 4.5a 计数过滤器修正（实施中发现的真实 bug）：初版写 `grep -vE '^warning: [0-9]+ warning'`（rustc 格式），但 cargo 的汇总行实际是 ``warning: `kiro-rs` (bin "kiro-rs") generated 1 warning``，导致 1 个告警被计成 3。改为 `grep -vE '^warning: .* generated [0-9]+ warning'` 并追加 `sort -u`（bin 与 test 目标重复告警只计一次，符合 spec「计数口径为唯一告警点」）。修正后实测计数为 1
- [x] 4.5b 探针命名陷阱（实施中发现）：首次探针命名 `__hook_probe_unused_fn` 与 `__x`，**不产生任何告警** —— Rust 的 `dead_code`/`unused_variables` lint 对以 `_` 开头的标识符豁免。改用无下划线前缀的 `hook_probe_dead_fn` 后正常报警。曾误判为缓存问题，经 `cargo metadata` 确认 bin target 指向 `src/main.rs`、注入语法错误确认 cargo 能看到改动后定位到真因
- [x] 4.6 验证（放行路径）：恢复 `src/main.rs`（`git diff` 确认为空）并 touch 强制重编后实跑 hook → `pre-push: 0 warnings. OK.`，退出码 0。探针备份已清理，`git status` 确认 `src/main.rs` 无改动
- [x] 4.7 验证（2026-08-07 真实推送实跑，非手动调用）：在 `codex/warning-gate-red-test` 追加第二个探针 commit `900914f`（`fn no_verify_probe() {}`），两次真实 `git push` 对比：
  - **拒绝路径**：`git push` → hook 触发，输出 `warning: function 'gate_red_test_probe' is never used` 与 `warning: function 'no_verify_probe' is never used`，判定 `pre-push: 2 distinct compiler warning site(s) found. Push refused.`，退出码 1，ref 未更新。计数口径再次验证：cargo 输出含 4 行 `warning:`（2 个真实告警点 + `generated 2 warnings` 与 `generated 2 warnings (2 duplicates)` 两条汇总行），过滤与 `sort -u` 后正确得 2
  - **绕过路径**：`git push --no-verify` → 3.7s 完成、**无任何 `pre-push:` 输出**、`7c9533b..900914f` 推送成功，退出码 0

  故 README.md 与 AGENTS.md 的「`--no-verify` 可绕过」表述属实，非强制性记录准确
- [x] 4.8 附带实证（本轮真实推送顺带取得）：hook 的**放行**路径同样在真实 `git push` 中验证 —— 推送 `codex/warning-gate-verify`（内容为门禁实现提交 `9366872`，零告警）时输出 `pre-push: /d/ProgramData/rust/cargo_home/bin/cargo check --release --all-targets` 与 `pre-push: 0 warnings. OK.` 后正常推送。此前 4.5/4.6 仅以手动 `bash` 调用验证，本项补齐了「git 真实调用 hook」这一层

## 4b. 代码审查修复（2026-08-07 审查结论）

- [x] 4b.1 [P1] `${HOME}` 在 `set -u` 下未保护 → `pre-push:39` 改为 `${HOME:-}`，与相邻 `CARGO_HOME`/`USERPROFILE` 写法统一。根因：脚本开头 `set -uo pipefail`，裸 `${HOME}` 在 HOME 未设置时（Windows 部分 Git 调用路径、CI runner、`sudo -E`）直接 `unbound variable` 退出 127，用户看到晦涩 bash 错误而非预期提示。验证：修复前 `bash -c 'set -uo pipefail; unset HOME; c+=("${HOME}/...")'` → `HOME: unbound variable` exit=127；修复后同场景 → 正常走到候选检查并干净退出 0
- [x] 4b.2 [P1] 未消费 stdin 可能阻塞 git → 在定位 cargo 与执行判定命令**之前**加入 `while read` 排空循环。根因：`pre-push` 协议由 git 经 stdin 传入 ref 列表（`<local ref> <local sha1> <remote ref> <remote sha1>`），本脚本原先完全不读，却在其后跑数十秒 `cargo check`；推送大量 ref（`git push --all`、首次推送多分支）时 git 仍在写管道，缓冲区填满即阻塞。git 官方 `pre-push.sample` 亦带读取循环
- [x] 4b.2a 顺带优化：排空 stdin 时判定是否**全为删除**（`local sha1` 全为 40 个 0），纯删除推送不引入新代码，直接放行并输出提示，避免无谓等待。加 `saw_ref` 标志确保无 stdin（手动调用）时不被误判为删除
- [x] 4b.3 [P2] `should_publish` 计算块补工作树依赖警示注释 → `docker-build.yaml:46-50`。理由：`pre-check` 的 checkout 带 `if: event_name == 'push' && ...`，dispatch 路径无工作树，而该路径正是唯一可能误发布的路径；当前实现只用 event/input 上下文是正确的，但需注释固定该约束，防止未来加入基于文件或 git 历史的判断后在 dispatch 路径静默取错值
- [x] 4b.4 流程修复：`git add scripts/git-hooks/pre-push`。审查指出 staged 版本是含上述缺陷、且计数过滤器仍为错误版本的旧快照（状态 `AM`）。现为 `A `，staged 与工作区一致，mode 保持 `100755`
- [x] 4b.5 stdin 四路径实测（MSYS bash）：① 正常推送（含新 sha）→ 跑门禁、0 告警、exit 0；② 纯删除 → `deletion-only push, skipping the warning gate.`、exit 0、不跑 cargo；③ 无 stdin（手动调用）→ 跑门禁、exit 0（未被误判为删除）；④ 混合（一删一增）→ 跑门禁、exit 0
- [x] 4b.6 修复后回归：`bash -n` 语法通过；`cargo check --release --all-targets` 告警数 0；两条严格命令退出码 0（11.27s / 0.34s）；`openspec validate --all` 21 passed；四条 workflow YAML 解析通过且 job 接线不变
- [x] 4b.7 附带：按 `AGENTS.md` ASCII 默认约定，移除 `pre-push:21` 与 `warning-gate.yaml:8,29` 的 em dash（U+2014），改用 ASCII 标点。实测三文件（含 `docker-build.yaml`）现为纯 ASCII；`Dockerfile` 的中文注释保留，与仓库既有惯例（`Cargo.toml` 中文注释）一致

## 4c. 代码审查修复（2026-08-07 第二轮审查结论）

- [x] 4c.1 [P2] 门禁 job 缺 `permissions` → `warning-gate.yaml` 在 `on:` 之后新增顶层 `permissions: contents: read`，附注释说明理由。根因：可复用 workflow 未自行声明时**继承调用方**的权限块，于是这个只跑 `cargo check` 的 job 会拿到 `build.yaml`/`build-dev-release.yaml` 的 `contents: write` 与 `docker-build.yaml` 的 `packages: write`；而它恰恰要执行全部依赖的 build script（default feature 下含 vendored OpenSSL），是唯一「跑第三方代码却不需要写权限」的 job。可复用 workflow 只能等于或收紧调用方权限，故对三个调用方无影响。验证：`yaml.safe_load` 解析 `permissions == {'contents': 'read'}`，四条 workflow 的 job 接线不变
- [x] 4c.2 [P2] 旧方案文档缺作废横幅 → `docs/ci-warning-gate-design.md` 修订记录之前新增两行横幅，声明结构决策已被 `docs/warning-gate-two-line-defense-design.md` 取代、事实查证部分仍被继承、下文仅作决策过程留档。根因：作废关系此前只写在**新**文档第 2 行，旧文档第 2 行仍称「创建前以本文档为准」、第 9-10 行仍主张被推翻的「7 腿与 Docker 统一追加 `-D warnings`」，读者先打开旧文档时无任何失效线索
- [x] 4c.3 [P3] hook 判定面比 CI 门禁窄未记录 → `pre-push` 头注释新增段落，写明门禁额外带 `--locked` 与 `--no-default-features` 第二遍，因此本地绿**不蕴含**门禁绿（仅在无默认 feature 下出现的告警、或过期 lockfile 都能溜过本地这道）。原注释只解释了「不用 `-D warnings` 是为显示告警清单」，未提判定面本身的差异
- [x] 4c.4 流程修复（与 4b.4 同一个坑复发）：改注释后 `pre-push` 退回 `AM`，重新 `git add --chmod=+x` → 现为 `A `，staged 与工作区一致，mode 保持 `100755`
- [x] 4c.5 hook 三路径实测（Git for Windows MSYS bash，`D:\Program Files\Git\bin\bash.exe`，隔离 `CARGO_TARGET_DIR` 避开被占用的全局 target 锁）：① `bash -n` 语法通过 exit 0；② 纯删除 ref → `deletion-only push, skipping the warning gate.`、exit 0、不跑 cargo；③ 正常推送含新 sha → 跑门禁、`0 warnings. OK.`、exit 0
- [x] 4c.6 hook 拒绝路径实测：`src/main.rs` 末尾植入 `fn review_probe_dead_fn() {}`（无下划线前缀，规避 4.5b 记录的 lint 豁免陷阱）→ 输出 `pre-push: 1 distinct compiler warning site(s) found. Push refused.`，退出非零；cargo 汇总行 `generated 1 warning (1 duplicate)` 未被计入，去重口径正确。探针已还原，`git diff -- src/main.rs` 为空，`.review-bak` 备份已删除
- [x] 4c.7 修复后回归全量实跑（隔离 target 目录）：`cargo check --release --all-targets` 无 warning/error 行 → **告警数 0**，与 0.4 基线一致无新增；`RUSTFLAGS=-D warnings` + `--locked` 退出码 0（1m19s）；再加 `--no-default-features` 退出码 0（25.59s），随后 `Remove-Item Env:RUSTFLAGS` 确认 unset；`openspec validate --all` 21 passed, 0 failed；临时 `target-review` 目录已清理
- [x] 4c.8 审查观察备案（本 change 不改）：`docker-build.yaml:46-49` 新增注释立下「发布开关不得由矩阵 job 输出承载」，而同文件 `manifest` 仍从矩阵 `build` 取 `version`/`is_beta`（:152-153）。当前无害（两腿算出的 version 恒等）且属既有代码，不在本 change 范围，登记为后续项 7.8

## 5. 规格与文档同步

- [x] 5.1 `AGENTS.md`「零新增编译告警」小节新增「两道防线」子节：第一防线（本地 pre-push，含装法与三条非强制性说明）、第二防线（CI gate 硬失败、判定命令、覆盖两种 feature 组合）、钉版与浮动的理由、计数口径归属；文末补指向新设计文档
- [x] 5.2 `AGENTS.md` 高风险矩阵新增「CI / 告警门禁」行，明确要求红路径证据
- [x] 5.3 「任意代码改动」行措辞对齐：注明 CI 门禁在准绳基础上附加 `-D warnings` 与 `--locked`，准绳本身不变
- [x] 5.4 三个 ADDED Requirement 逐条比对实现（rg 取证）：
  - R1「发布路径机器强制点」→ `warning-gate.yaml:35` 钉版、`:59/:66` 判定命令两遍、`:58/:65` `RUSTFLAGS: -D warnings`、`:32` dist 供给；产物线未设 `RUSTFLAGS`（`build.yaml`/`build-dev-release.yaml`/`Dockerfile` rg 零命中，符合「产物构建不得升级告警」）
  - R2「人工触发默认 dry-run」→ `docker-build.yaml:15` `publish` 输入、`:30` 非矩阵 `pre-check` 输出、`:51-58` 三路径计算、`:110/:124/:135` 约束登录/push/manifest、`:123` dry-run 关 `cache-to`
  - R3「本地防线为机器动作」→ `pre-push:10` 装法、`:13/:57/:94` 非强制性与绕过方式、`:92` 去重计数拒绝；`AGENTS.md:39` 与 `README.md:108-111` 同步记录非强制性
- [x] 5.5 `openspec validate --all` 实跑：21 passed, 0 failed

## 6. 完成验证与合入

- [x] 6.1 `cargo check --release --all-targets` 实跑：无 warning 行、无 error 行，**告警数 0**（与 0.4 基线一致，无新增）
- [x] 6.2 `$env:RUSTFLAGS = "-D warnings"` + `cargo check --release --all-targets --locked`：退出码 0，20.02s
- [x] 6.3 同上加 `--no-default-features`：退出码 0，6.52s；随后 `Remove-Item Env:RUSTFLAGS`，确认 unset
- [x] 6.4 `git status --short`：7 个修改 + 1 个新增（已 `--chmod=+x` 入 index）+ 5 个未跟踪（gate workflow、3 份 docs 设计文档、change 目录）。无 `config.json`/`credentials.json`；`.codegraph` 经 `git check-ignore` 确认已忽略；探针备份已清理，`src/main.rs` 零改动
- [x] 6.5 CI 证据汇总（全部 2026-08-07 实跑，基线提交 `9366872`）：

| 证据 | run | 结果 |
| --- | --- | --- |
| gate 绿路径 + 7 腿全绿 | [31148152382](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148152382) | success；gate 2m03s，7 腿 2m33s-5m17s，`release` skipped |
| 告警红路径拦截 | [31148197200](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148197200) | failure；gate 红 → `build`/`release` **skipped**，exit 101，无产物 |
| Docker 双架构 dry-run | [31148217637](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148217637) | success；双架构绿、smoke test 输出 `kiro-rs 2026.3.1`、GHCR 登录 skipped、`manifest` skipped |
| GHCR 无副作用 | 取证命令（0.3a） | `version_count=18`、`updated_at` 未变、`latest` 仍 `sha256:dcd5c510f9ed`，六行逐行一致 |
| gate 在全新检出下可编译 | 31148197200 日志 | 编译推进至 `src/main.rs:268`，空 dist 占位生效 |
| hook 真实推送拒绝 / 绕过 | 本地 `git push` | 拒绝 exit 1（2 告警点）；`--no-verify` exit 0 且无 hook 输出 |

  **成功标准对照**：proposal.md「CI 侧」五行判定全部取得证据；对照基线 master run `30985624336`（5m59s）与 `30985624269`（7m22s），本次 gate 引入的额外前置耗时为 2m03s，7 腿本身耗时未见劣化
- [x] 6.5a 临时分支清理：`git push --delete` 远端两条分支（输出 `- [deleted] codex/warning-gate-red-test`、`- [deleted] codex/warning-gate-verify`），`git branch -D` 删本地（`900914f`/`9366872`）。验证：`git ls-remote --heads "refs/heads/codex/*"` 返回空；本地 `git branch --list "codex/*"` 仅剩无关的 `codex/ai-engineering-baseline`（先前存在，按 surgical changes 纪律不动）。dev 的 `src/main.rs` 无探针残留
- [x] 6.6 合入前判据实测（2026-08-07）：`merge-base(origin/master, dev)` = `dcd9351`，`git diff --quiet dcd9351 origin/master` **退出码 0** —— master 相对 merge-base 无内容变化，全部内容来自已验证的 dev 树。`git rev-list --left-right --count origin/master...dev` = `0 1`（master 无独有提交，dev 领先 1 个即门禁实现提交 `9366872`）。合入窗口期无新 PR 先落 master 的风险
- [ ] 6.7 PR + Create a merge commit 合入 master —— **待用户决定，未自行执行**。实现提交 `9366872` 当前只在本地 dev，`origin/dev` 仍停在 `d85cfb6`。理由：推 dev 会触发 `build-dev-release.yaml` 的滚动 prerelease 重建（release job `git tag -f dev-latest` force-push + `gh release delete` 后重建），合并 master 更会触发正式发布路径，二者均属需用户确认时机的发布副作用。CI 证据已由 `codex/` 临时分支取得，不依赖推 dev
- [ ] 6.8 归档后更新 `docs/release-build-warnings-cleanup-design.md` 的「后续项」小节，标注已落地与归档位置 —— 依赖归档动作，归档前无法完成

## 7. 后续项登记（本 change 不做）

本组任务的动作是**登记**而非实现。以下条目随本 change 的 tasks.md 入库，归档时连同 change 目录进入 `openspec/changes/archive/`，即完成登记。

- [x] 7.1 登记：平台专属告警覆盖（本方案已知盲区）。引入平台条件代码时重新评估，届时需同步钉住矩阵工具链，否则回到 D1 的问题。当前全仓无 `cfg(windows)`/`cfg(unix)`/`target_os`/`target_arch` 平台业务分叉，盲区未被触发
- [x] 7.2 登记：clippy 门禁（需先做一轮存量清零；项目从未跑过 clippy，全仓仅 `src/openai/responses_stream.rs:95` 一处 `#[allow]`）
- [x] 7.3 登记：CI 测试门禁（`cargo test`）
- [x] 7.4 登记：`pull_request` 触发器与分支保护。2026-08-07 复核：`main` 为 default branch，master 与 main 均无分支保护，仓库为 public
- [x] 7.5 登记：`main` 分支治理（default branch 但无 workflow 触发；2026-08-07 复核 `origin/main` 与 `origin/dev` 同为 `d85cfb6`，三条 workflow 的 push 触发器只有 `master` 与 `dev`）
- [x] 7.6 登记：`docs/tooling-sources.md:12` 仍记 Rust 1.94.1，实际为 1.97.1（本 change 不改该文件：它记录本机工具核验快照而非发布事实源，见 design.md D4 的同类判断）
- [x] 7.7 登记：gate 工具链定期 bump（建议每 2-3 个 Rust 周期）。bump 时须按 spec 的「门禁工具链版本 bump 必须重新确认基线」场景重跑本地准绳命令并报告告警数
- [x] 7.8 登记：`docker-build.yaml` 的 `manifest` job 从矩阵 `build` 取 `version`/`is_beta` 输出（:152-153）。当前两腿算出的 version 恒等故无害，但与 4b.3 注释立下的「发布开关不得由矩阵 job 输出承载」原则相邻，宜迁至非矩阵 `pre-check` 计算（见 4c.8）
- [x] 7.9 登记：gate `timeout-minutes: 20` 的收紧判据。冷缓存首跑实测 2m03s（见 2.14），按 2-3 倍余量对应 4-6 分钟，但当前冷缓存样本仅 1 个；待积累若干 run 后按实际分布收紧
