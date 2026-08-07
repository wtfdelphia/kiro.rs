# OpenSpec Verify Report: add-ci-warning-gate

验证时间：2026-08-07
验证基线：`dev` @ `0a81533`（已与 `origin/master` @ `693ea21` 同步）
合入基点：`dcd9351`（本 change 全部改动 = `git diff dcd9351 HEAD`）
结论：**通过，具备归档条件**。3 项一致性缺陷已在本次验证中修正，2 项剩余风险如实登记。

## 一、Completeness（工件、tasks、Requirement、evidence 齐全性）

### 工件

`openspec status --change add-ci-warning-gate --json` 实跑：`isComplete: true`，四工件全 `done`（proposal / design / specs / tasks），schema `spec-driven`。

`openspec validate --all` 实跑：**21 passed, 0 failed**。

### tasks

`openspec instructions apply --change add-ci-warning-gate --json` 实跑：`state: all_done`，进度 **90/90**，`rg -c "^- \[ \]"` 退出码 1（无未勾选项）。

### Requirement 与 Scenario

3 个 ADDED Requirement，13 个 Scenario，每个 Requirement 均有 Scenario：

| Requirement | Scenario 数 |
| --- | --- |
| 发布路径必须有固定编译器版本上的机器强制点 | 6 |
| 人工触发的发布流水线默认不得产生发布副作用 | 3 |
| 本地防线必须是默认执行的机器动作而非纯纪律 | 4 |

### evidence

| 证据类型 | 文件 | 状态 |
| --- | --- | --- |
| Bridge | `evidence/bridge-plan.md` | 存在（13,230 字节），含 CodeGraph/rg 取证、事实源一致性检查、停止条件、证据边界 |
| Compliance | 无独立文件 | tasks 5.4 承担逐 Requirement 比对；本报告第二节复核 |
| Completion / Verification | 本文件 | 本次产出 |

**登记**：Compliance 未产出独立文件，其职责由 `tasks.md` 5.4（三个 Requirement 逐条 rg 取证）与本报告第二节共同覆盖。归档时如需独立 Compliance 文件，可由本报告第二节拆出。

## 二、Correctness（完成内容是否符合 Scenario 意图与成功标准）

### Scope 三条硬线（proposal.md「Scope」）

| 硬线 | 验证命令 | 结果 |
| --- | --- | --- |
| 不修改任何 Rust 源码 | `git diff --stat dcd9351 HEAD -- src/ admin-ui/` | 无输出，**零改动** |
| 不给产物线设 `RUSTFLAGS="-D warnings"` | `rg -n "RUSTFLAGS" build.yaml build-dev-release.yaml docker-build.yaml Dockerfile` | 退出码 1，**零命中** |
| 不引入 `rust-toolchain.toml` | `Get-ChildItem -Filter "rust-toolchain*"` | 无输出，**不存在** |

`RUSTFLAGS` 仅出现在 `warning-gate.yaml:68` 与 `:75`，与 D1「仪器钉版、生产线浮动」一致。

### Requirement 1：发布路径机器强制点

实现取证（`rg` 实跑，行号为当前值）：

- 钉版：`warning-gate.yaml:45` `dtolnay/rust-toolchain@1.97.1`；版本断言 `:50` `EXPECTED="1.97.1"`
- 判定面：`:69` `cargo check --release --all-targets --locked`、`:76` 同命令加 `--no-default-features`，两遍均带 `:68/:75` 的 `RUSTFLAGS: -D warnings` —— 与本地准绳逐 flag 一致，仅显式附加告警升级与依赖锁定
- 全新检出可执行：`:42` `mkdir -p admin-ui/dist`
- 挂载点在产物之前：`build.yaml:40-48`、`build-dev-release.yaml:53-59`、`docker-build.yaml:66-74`，三条流水线的 `build.needs` 均含 `warning-gate`

Scenario 验证：

| Scenario | 证据 | 判定 |
| --- | --- | --- |
| 门禁失败时不得产出发布物 | run [31148197200](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148197200) `failure`：gate 红 → `build` **skipped** → `release` **skipped**，exit 101，无产物上传 | 满足 |
| 门禁判定面与本地准绳一致 | 上列 `:69/:76` 与 `AGENTS.md:31` 准绳逐 flag 比对；覆盖 default 与 `--no-default-features` | 满足 |
| 发布产物构建不得升级告警 | 产物线 `RUSTFLAGS` 零命中；7 腿保持 `dtolnay/rust-toolchain@stable` 浮动 | 满足 |
| 门禁必须在全新检出下可执行 | 同一红路径 run 的编译推进至 `src/main.rs:268`，证明 rust-embed 目录存在性检查已被空占位满足 | 满足（CI 实测） |
| 门禁工具链 bump 必须重新确认基线 | 规则性要求，本 change 未 bump；已登记为后续项 7.7 并写明 bump 时的义务 | 满足（规则已固化） |
| 门禁不承担告警计数职责 | `AGENTS.md:42` 明文「计数始终取自原始准绳命令」；hook 用不带 `-D warnings` 的准绳以输出可读清单 | 满足 |

### Requirement 2：人工触发默认不得产生发布副作用

实现取证：`docker-build.yaml:15-19` `publish` 布尔输入 `default: false`；`:30` `should_publish` 声明为**非矩阵** `pre-check` job 的输出；`:55-64` 三路径计算；约束三处 `:115`（GHCR 登录）、`:129`（`push`）、`:140`（manifest job），另 `:128` dry-run 时关闭 `cache-to`。

| Scenario | 证据 | 判定 |
| --- | --- | --- |
| 人工触发默认不发布 | run [31148217637](https://github.com/wtfdelphia/kiro.rs/actions/runs/31148217637)（`workflow_dispatch`，`publish=false`）：双架构 build `success`，`Log in to GitHub Container Registry` **skipped**（step 5），`manifest` **skipped**。GHCR 复核 `version_count` 仍 18、`updated_at` 未变、`latest` 仍 `sha256:dcd5c510f9ed`，六行 digest/tag 与基准逐行一致 | 满足 |
| 显式开启后允许发布 | **部分**：`should_publish=true` 分支经 master push run [31150605792](https://github.com/wtfdelphia/kiro.rs/actions/runs/31150605792) 验证（`manifest` **真实执行**，与 dry-run 的 skipped 构成对照）。但 `workflow_dispatch` + `publish=true` 这一**具体组合未实跑** | 见剩余风险 R-1 |
| 发布开关不得由矩阵 job 输出承载 | `:30` 声明在 `pre-check`（非矩阵）；`:46-54` 注释固化「不得依赖工作树」约束 | 满足 |

### Requirement 3：本地防线必须是机器动作

实现取证：`scripts/git-hooks/pre-push` 入库且 mode `100755`（`git ls-files -s` 实测）；`:16` 装法；`:18-19` 非强制性三条；`:86/:123` 绕过提示；`:112` `sort -u` 去重；`:121` 计数拒绝。

| Scenario | 证据 | 判定 |
| --- | --- | --- |
| 存在告警时拒绝操作 | 真实 `git push` 实跑：植入 2 个探针后输出两条 `warning: function ... is never used` 清单，判定 `pre-push: 2 distinct compiler warning site(s) found. Push refused.`，退出码 1，ref 未更新。计数口径正确：cargo 输出 4 行 `warning:`（2 真实告警点 + 2 条 `generated N warnings` 汇总行），过滤加去重后得 2 | 满足 |
| 无告警时放行 | 真实 `git push` 实跑三次（推 `codex/warning-gate-verify`、两次推 dev）均输出 `pre-push: 0 warnings. OK.` 后正常推送 | 满足 |
| 本地防线不得作为软化 CI 门禁的理由 | CI 门禁为硬失败，红路径 run 实证下游 job 被 skip；`AGENTS.md:39-40` 与 `README.md:111` 均写明强制判定点在 CI | 满足 |
| 非强制性必须被记录 | `AGENTS.md:39` 与 `README.md:111` 各含三条（需一次性配置、新克隆默认未启用、`--no-verify` 可绕过），且 `--no-verify` 绕过已实跑验证（3.7s 完成、无 hook 输出、退出码 0） | 满足 |

### 成功标准（proposal.md「Success Criteria」）

本地四条，全部本次实跑（隔离 `CARGO_TARGET_DIR` 避开被占用的全局 target 锁，跑完清理）：

| 命令 | 结果 |
| --- | --- |
| `cargo check --release --all-targets` | 无 warning/error 行，**告警数 0**，与 tasks 0.4 基线一致无新增 |
| `RUSTFLAGS=-D warnings` + `--locked` | 退出码 0（1m05s） |
| 同上加 `--no-default-features` | 退出码 0（20.82s），随后确认 `RUSTFLAGS` unset |
| `openspec validate --all` | 21 passed, 0 failed |

CI 侧五行判定全部有证据。**7 个被引用的 run ID 逐个经 `gh api` 核实**，conclusion / head_sha / event / branch 与 tasks 记录逐项吻合：

```text
31148197200 红路径拦截        -> failure  sha=7c9533b3 event=workflow_dispatch branch=codex/warning-gate-red-test
31148152382 build.yaml 绿路径 -> success  sha=93668721 event=workflow_dispatch branch=codex/warning-gate-verify
31148217637 Docker dry-run    -> success  sha=93668721 event=workflow_dispatch branch=codex/warning-gate-verify
31149901171 Build Dev Release -> success  sha=f77582a9 event=push              branch=dev
31149901178 Build Artifacts   -> success  sha=f77582a9 event=push              branch=dev
31150605792 Docker (master)   -> success  sha=693ea214 event=push              branch=master
31150605819 Build Artifacts   -> success  sha=693ea214 event=push              branch=master
```

## 三、Coherence（design / AGENTS / spec / README / tasks 一致性）

### 事实源比对

`bridge-plan.md`「README / AGENTS / spec 同步判断」声明的两类入口与实际改动**完全吻合**：

- 声明「需要同步」：`AGENTS.md`(+12/-2)、`README.md`(+10)、`docs/release-build-warnings-cleanup-design.md`(+9/-1) —— 均已改动
- 声明「不需要同步」：`spec/`、`openspec/project.md`、`docs/tooling-sources.md` —— `git diff --stat dcd9351 HEAD` 无输出，确认未动

`AGENTS.md:37-44`「两道防线」小节与 spec delta 的三个 Requirement 表述一致：钉版理由、判定命令、计数口径归属、非强制性三条逐项对应。`AGENTS.md:96` 高风险矩阵新增「CI / 告警门禁」行明确要求红路径证据，与 design.md 验证策略「只验证绿路径不能证明门禁真的会拦」一致。

### 本次验证修正的三处一致性缺陷

1. **tasks 5.4 的 R1 行号过期**：原记 `warning-gate.yaml:35` 钉版、`:59/:66` 判定命令、`:58/:65` RUSTFLAGS、`:32` dist —— 这些是 4c.1 加入 `permissions` 块**之前**的位置，加块后整体下移 10 行。已校正为 `:45/:69/:76/:68/:75/:42` 并注明校正原因
2. **tasks 5.4 的 R3 行号过期**：原记 `pre-push:10/:13/:57/:94/:92` —— 4c.3 加入判定面差异注释后位置变化。已校正为 `:16/:18-19/:86/:123/:112/:121`
3. **`docs/ci-warning-gate-design.md` 缺作废横幅**（上一轮审查发现并已修复）：该文档与现行方案 `docs/warning-gate-two-line-defense-design.md` 同批入库，但其主张的「7 腿与 Docker 统一追加 `-D warnings`」正是被推翻的结构。已在顶部加横幅声明取代关系

`tasks.md` 4b.1、4b.7 中的行号是**当时修改位置的历史记述**而非当前指针，保留原文。

### 口径调整登记（tasks 已注明）

| 任务 | 原文 | 实际 | 理由 |
| --- | --- | --- | --- |
| 2.13 | 推 dev 验证绿路径 | `codex/` 临时分支 dispatch `build.yaml` | 推 dev 会触发 `build-dev-release.yaml` 的 `git tag -f dev-latest` force-push 与 prerelease 重建；`build.yaml` 的 release job 条件为 tag，dispatch 到分支时 skipped。证据等价而副作用为零。事后合入时推 dev 已补齐 push 事件证据（run 31149901171） |
| 6.8 | 归档后更新设计文档 | 归档前完成 | 待记录事实此刻均已确定；归档目录日期前缀取决于归档时间，故按「归档动作执行时确定日期前缀」表述，不预填未发生的日期 |

### 安全纪律

- `git diff --name-only dcd9351 HEAD` 无 `config.json` / `credentials.json` / 非 example 凭据文件
- `git diff dcd9351 HEAD` 对 `gho_` / `ghp_` / `github_pat_` / `AKIA` / `aws_secret` 扫描零命中
- `git status --short` 空，工作区干净，无 `.codegraph/`、无临时文件残留
- 探针检查：`rg "gate_red_test_probe|no_verify_probe"` 在 `src/` 零命中
- 两条实验分支 `codex/warning-gate-verify` 与 `codex/warning-gate-red-test` 远端已删除（`git ls-remote --heads "refs/heads/codex/*"` 返回空）

## 四、剩余风险

**R-1（低）：`workflow_dispatch` + `publish=true` 组合未实跑。** Scenario「显式开启后允许发布」为 MAY 语义。`should_publish=true` 分支已由 master push 路径实证（`manifest` 真实执行、镜像与 alias 正确推送），`workflow_dispatch` 分支的 `publish=false` 路径也已实证；未覆盖的只是「dispatch 且显式开启」这一交叉组合。该组合的表达式 `:129` `push: ${{ ... should_publish == 'true' }}` 与已验证路径共用同一求值链，风险主要在于 `inputs.publish` 的字符串比较（`:56` `"${{ inputs.publish }}" == "true"`）。未实跑的理由：该组合会向 GHCR 推送带 `latest` alias 的镜像（dispatch 走 `is_beta=false` 路径），属真实发布副作用，不宜为取证而执行。

**R-2（已接受，非缺陷）：平台专属告警盲区。** 门禁只在 `ubuntu-latest` 单宿主机运行，仅在 Windows / macOS / musl / arm64 某 target 触发的项目告警不被机器拦截。这是 D1「仪器钉版、生产线浮动」的明示代价，已写入 proposal Risks、design Risks/Trade-offs 与后续项 7.1。当前全仓无 `cfg(windows)` / `cfg(unix)` / `target_os` / `target_arch` 平台业务分叉（`cfg(feature)` 仅 `src/http_client.rs:59` 一处），盲区未被触发；引入平台条件代码时需重新评估并同步钉住矩阵工具链。

**R-3（信息）：门禁与产物线编译器版本将随时间分离。** 有意接受：门禁看不见的告警不阻断发布。缓解为后续项 7.7 的定期 bump，且 bump 时须按 spec 场景重跑本地准绳命令。

## 五、归档就绪判定

| 停止条件（SKILL.md） | 状态 |
| --- | --- |
| 未提供 change name 且有多个活跃 change | 不适用，name 已提供 |
| tasks 未完成或缺少证据 | **不触发** —— 90/90 完成，CI 证据经 `gh api` 逐个核实 |
| validate 失败 | **不触发** —— 21 passed, 0 failed |
| 工件之间存在冲突且无法判断有效事实源 | **不触发** —— 3 处行号漂移已修正，无语义冲突 |

**结论：具备归档条件。** 归档时需补一处：`docs/release-build-warnings-cleanup-design.md` 状态横幅中的归档目录路径，将「归档动作执行时确定日期前缀」替换为实际日期前缀。

> **归档后补记（2026-08-07）**：上述占位已填为 `openspec/changes/archive/2026-08-07-add-ci-warning-gate/`。delta spec 已由 `openspec-sync-specs` 同步至 `openspec/specs/build-warning-hygiene/spec.md`（3 个 Requirement 纯追加，逐字比对 IDENTICAL），归档前复核待应用变更数为 0。

## 六、本次验证实跑命令

```text
openspec status --change add-ci-warning-gate --json          # isComplete true, 四工件 done
openspec validate --all                                      # 21 passed, 0 failed
openspec instructions apply --change ... --json              # state all_done, 90/90
git diff --stat dcd9351 HEAD                                 # 19 文件, +2058/-16
git diff --stat dcd9351 HEAD -- src/ admin-ui/               # 无输出（零改动）
git diff --stat dcd9351 HEAD -- spec/ openspec/project.md docs/tooling-sources.md   # 无输出
rg -n "RUSTFLAGS" <三条 workflow> Dockerfile                  # 退出码 1（零命中）
rg -n <各 Requirement 关键字> warning-gate.yaml docker-build.yaml pre-push AGENTS.md README.md
git ls-files -s scripts/git-hooks/pre-push                    # 100755
gh api repos/.../actions/runs/<7 个 run id>                   # conclusion/sha/event/branch 逐项吻合
gh api orgs/wtfdelphia/packages/container/kiro-rs[/versions]  # version_count 21, latest 未变
cargo check --release --all-targets                           # 告警数 0
RUSTFLAGS=-D warnings cargo check --release --all-targets --locked                        # exit 0
RUSTFLAGS=-D warnings cargo check --release --all-targets --locked --no-default-features  # exit 0
git status --short                                            # 空
```

**未实跑**：`workflow_dispatch` + `publish=true`（见 R-1）；`bash -n` 之外的 hook 静态检查（本轮未重跑，上一轮已验证且脚本自那以后仅新增注释）；docker build 本机（无 docker CLI，由 CI run 覆盖）。
