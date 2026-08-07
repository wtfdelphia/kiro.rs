# Bridge Plan: add-ci-warning-gate

生成时间：2026-08-05
分支：`dev` @ `dcd9351`（已快进对齐 `origin/master`，tasks 0.1 完成）
工作区：干净（仅本 change 的未跟踪工件 + 三份 docs 设计文档）
`openspec status --change add-ci-warning-gate --json`：四工件全 `done`，**state 字段不存在**（该 schema 无 blocked 状态），`isComplete` 覆盖工件维度而非任务维度；任务进度 `5/63`

## 范围

两道防线，一个 CI 机器强制点 + 一个本地默认动作。

改动文件（10 个，`src/` 零改动）：

| 文件 | 动作 |
| --- | --- |
| `.github/workflows/warning-gate.yaml` | 新增（reusable，`workflow_call`） |
| `.github/workflows/build.yaml` | 加 gate 调用 job、`build.needs`、7 腿 `--locked` |
| `.github/workflows/build-dev-release.yaml` | 同上 |
| `.github/workflows/docker-build.yaml` | 加 gate 调用 job、`publish` 输入、`should_publish` 约束 |
| `Dockerfile` | builder tag、`COPY` 去通配、`--locked`、smoke test |
| `.dockerignore` | 删除 `Cargo.lock` 行 |
| `scripts/git-hooks/pre-push` | 新增 |
| `AGENTS.md` | 告警小节、高风险矩阵、准绳措辞 |
| `README.md` | hook 安装说明 |
| `openspec/.../specs/build-warning-hygiene/spec.md` | delta（3 个 ADDED Requirement） |

## 非目标

- 不修改任何 Rust 源码
- 不给 7 腿产物与 Docker 的实际 `cargo build` 设 `RUSTFLAGS="-D warnings"`
- 不引入 `rust-toolchain.toml`
- 不引入 clippy 与 `cargo test`
- 不加 `pull_request` 触发器与分支保护
- 不动 `main` 分支、不治理版本漂移
- 不断言 Docker smoke test 的版本字符串

## 关键设计决策

| # | 决策 | 一句话依据 |
| --- | --- | --- |
| D1 | `-D warnings` 只在钉版 gate 内，不在发布产物线 | Rust 兼容承诺不覆盖「不产生新告警」；浮动 + `-D warnings` 会让 Rust 发版成为发布中断的随机触发器 |
| D2 | 不引入 `rust-toolchain.toml`，钉版用 `dtolnay/rust-toolchain@1.97.1` | 钉版文件会导致 7 腿每 run 各下一份工具链（rust-cache 不缓存 `~/.rustup`）+ 缓存一次性失效 |
| D3 | gate 硬失败 | 第一防线无机器强制，软化第二防线等于零强制 |
| D4 | 第一防线用 `pre-push` | 告警是提交树属性而非单提交属性；推送时刻与 CI 判定面对齐 |
| D5 | RUSTFLAGS 而非 `[lints]` | `[lints]` 会让本地准绳首个告警即中止，破坏现行 spec 的计数口径 |
| D6 | Docker builder 改浮动 `rust:1-alpine` | Docker 是生产线，与 D1 一致；已实测该 tag 指向 `rustc 1.97.1` |
| D7 | smoke test 只断言退出码 | 当前 `--version` 输出 `2026.3.1` 属版本治理范围，断言字符串会导致该 change 落地后返工 |
| D8 | 人工 Docker 触发默认 dry-run | 现状 dispatch 固定 `push: true` 且走 `is_beta=false`，会覆盖 `latest`，实验分支无法安全验证 |

## 高风险项

按 `AGENTS.md` 高风险矩阵，本 change 命中 **Docker / 发布 / CI 部署脚本**。

| 风险 | 触发条件 | 停止/控制 |
| --- | --- | --- |
| **依赖锁定三处不同批** | 只改 COPY 去通配而不删 `.dockerignore:3` 的 `Cargo.lock` 行 → docker build 找不到文件必失败 | tasks 1.1-1.3 强制同一提交；任一处遗漏立即停止 |
| **红路径 dispatch 打错 workflow** | dispatch `build-dev-release.yaml` → 其 release job `git tag -f dev-latest` force-push + `gh release delete` 重建滚动 prerelease | tasks 3.2 明文禁止；只允许 dispatch `build.yaml` |
| **Docker dry-run 意外发布** | `publish` 默认值写成 `true`，或 `should_publish` 放在矩阵 job | tasks 1.6-1.8；`should_publish` 必须在非矩阵 `pre-check` job（GHA 对矩阵 job 输出取最后完成腿的值） |
| **gate 在全新检出下编译失败** | 漏 `mkdir -p admin-ui/dist` → rust-embed 派生宏无条件编译错误 | tasks 2.3；dev 实跑确认 |
| **平台专属告警盲区** | 未来引入 `cfg(windows)` 等平台分叉后，gate 覆盖不到 | 已写入 proposal Risks 与后续项 7.1；当前全仓无平台 cfg 分叉 |
| **实验分支残留** | 临时分支未删除 | tasks 1.11、3.4 |

## CodeGraph 证据

```text
codegraph status
  → Files 136 / Nodes 2,777 / Edges 7,637 / DB 12.61 MB
  → backend node:sqlite (full WAL)

codegraph query "admin_ui router Embed folder"
  → src/admin_ui/mod.rs:7  pub use router::create_admin_ui_router;
  （未直接命中 Embed derive，符号级索引不覆盖属性宏参数 → 转 rg 补盲）

codegraph impact "create_admin_ui_router"
  → 1 affected symbol：src/admin_ui/router.rs:18 自身
  → 结论：admin-ui 嵌入点无扩散影响面；本 change 不触碰该符号，仅在 CI 侧供给其编译期前置目录
```

CodeGraph 对本 change 的价值有限且**符合预期**：改动集中在 CI 配置、Dockerfile、hook 脚本，全部在符号图之外。唯一的代码关联是 rust-embed 的编译期目录依赖，而它是**属性宏参数**，不是符号引用，CodeGraph 结构上不覆盖。这正是必须走 rg 补盲的地方。

## rg / 源码补盲

CodeGraph 不覆盖的六处，全部用 rg 或直读确认：

```text
rg -n "Embed|folder|admin-ui/dist|allow_missing" src/admin_ui/router.rs
  → :10 use rust_embed::Embed;  :13 #[derive(Embed)]  :14 #[folder = "admin-ui/dist"]
  → 无 allow_missing → 目录缺失即编译错误（gate 必须供给）

rg -n -A2 'name = "rust-embed"' Cargo.lock
  → version 8.9.0（与设计文档引用的版本一致）
Cargo.toml:38  rust-embed = "8"

rg -n "Build app|cargo build|--locked|RUSTFLAGS|dtolnay|rust-cache|shared-key" .github/workflows
  → build.yaml:114 dtolnay/rust-toolchain@stable
  → build.yaml:119-121 Swatinem/rust-cache@v2, shared-key "rust-cache-${{ matrix.target }}"
  → build.yaml:130-131 Build app: cargo build --release --target ... （无 --locked、无 RUSTFLAGS）
  → build-dev-release.yaml:106/111/113/122-123 同构，shared-key "rust-cache-dev-${{ matrix.target }}"
  → docker-build.yaml 零命中 → 确认无宿主机 Rust 步骤，版本定义在 Dockerfile

rg -n "rust:|FROM|alpine" Dockerfile
  → :1 node:22-alpine (frontend-builder)  :10 rust:1.92-alpine (builder)  :21 alpine:3.21 (runtime)

rg -n "credentials.json|config.json" .gitignore
  → :2 /config.json  :3 /credentials.json（均已忽略）

工作树敏感文件检查
  → config.json 不存在、credentials.json 不存在
  → 仅 5 个 credentials.example.*.json（示例，允许入库）
```

**Dockerfile 版本定义位置的澄清**：`docker-build.yaml` 中 rg 对 `rust`/`alpine`/`FROM` **零命中**，唯一相关是 `:91 context: .`。Docker 的 Rust 版本完全由 Dockerfile 决定，本 change 的 builder tag 改动落在 `Dockerfile:10`，不在 workflow。

## 事实源一致性检查

按 `openspec/project.md` 的事实源优先级逐层比对「告警门禁」表述：

| 层 | 位置 | 现表述 | 本 change 后 |
| --- | --- | --- | --- |
| 2 | `AGENTS.md:31` | 判定命令 `cargo check --release --all-targets`（唯一准绳） | 保持准绳语义，补说明 gate 附加 `-D warnings` 与 `--locked` |
| 2 | `AGENTS.md:35` | 提交前告警数高于基线视为未完成 | 不变 |
| 2 | `AGENTS.md:91` | 高风险矩阵「任意代码改动」行 | 补 CI 行 |
| 2 | `AGENTS.md:97` | 必须报告告警数 | 不变（gate 不承担计数职责，spec delta 已明文） |
| 3 | `spec/design.md:40` | 告警门禁须无新增告警 | 不变 |
| 3 | `spec/requirements.md:39` | 同上，指向 AGENTS.md 细则 | 不变 |
| — | `openspec/project.md` | 约束含「不得引入新告警」；常用验证含准绳命令 | 不变 |

**无冲突**。三个 ADDED Requirement 是在现有四个之上叠加「机器强制点」，不修改也不收窄任何现行表述。

## 任务到执行步骤映射

| 任务组 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 0（已完成 5/5） | dev ff 对齐、镜像 tag 与版本、GHCR 基准 | 已记录实跑输出 | — |
| 0.4（待跑） | `cargo check --release --all-targets` | 记录告警数，期望 0 | 非 0 则先定位是否为既有基线，不得直接开工 |
| 1（Docker） | `.dockerignore` 删行 + `COPY` 去通配 + `--locked` + builder tag + smoke test（**同一提交**）；`publish` 输入 + `should_publish` | `codex/` 临时分支 `publish=false` dry-run：build 绿、smoke test 绿、manifest skipped；GHCR 复跑比对 `latest` = `sha256:dcd5c510f9ed`、`version_count` = 18 | 出现任何 push/tag 副作用立即停止并回滚 |
| 2（gate） | 新建 `warning-gate.yaml`（6 步）+ 三条流水线挂载 + 7 腿加 `--locked` | 推 dev：gate 绿、7 腿全绿、Docker 绿；记录 run URL 与冷缓存耗时 | gate 因 dist 缺失失败 → 升级为完整 `pnpm build`，不改门禁语义 |
| 3（红路径） | `codex/warning-gate-red-test` 植入告警 → dispatch **`build.yaml`** | gate 红、矩阵未启动、无产物 | 误 dispatch `build-dev-release.yaml` → 立即停止，检查 `dev-latest` tag 与 prerelease 状态 |
| 4（hook） | `scripts/git-hooks/pre-push` + `core.hooksPath` + README | 有告警拒推、无告警放行、`--no-verify` 可绕 | 脚本在无 cargo/非仓库根时静默放行 → 修正后重测 |
| 5（规格文档） | AGENTS.md 三处 + spec delta 复核 | `openspec validate --all` | 与现行表述冲突 → 停止并重新评估 delta |
| 6（完成验证） | 三条 cargo 命令 + `git status --short` + CI 证据汇总 + 合入判据 | 告警数 0；后两条退出码 0；`git diff --quiet $(git merge-base origin/master dev) origin/master` 退出码 0 | 任一命令未实跑 → 按验证纪律写 SKIPPED + 原因 + 剩余风险 |
| 7（后续项登记） | 归档记录中登记 7 项 | — | — |

## 必跑验证

```bash
# 本地（PowerShell 下后两条前先 $env:RUSTFLAGS = "-D warnings"，完成后 Remove-Item Env:RUSTFLAGS）
cargo check --release --all-targets                                                          # 报告告警数，须 0
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked                          # 退出码 0
RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked --no-default-features    # 退出码 0
openspec validate --all                                                                      # 通过
git status --short                                                                            # 无密钥、无 .codegraph/、无临时文件
```

```powershell
# GHCR 前后比对（基准见 tasks 0.3a）
gh api "orgs/wtfdelphia/packages/container/kiro-rs/versions" --jq '.[0:6] | .[] | "\(.name[0:19]) tags=\(.metadata.container.tags)"'
```

CI 证据必须逐条附 run URL：gate 绿路径、红路径拦截、7 腿全绿、Docker dry-run 无副作用。

**未实跑的必须写明原因与剩余风险，不得以推断代替。**

## README / AGENTS / spec 同步判断

| 入口 | 是否需要同步 | 理由 |
| --- | --- | --- |
| `AGENTS.md` | **需要**（tasks 5.1-5.3） | 影响验证命令与 AI 纪律：新增 CI 强制点与 pre-push 第一防线，高风险矩阵需补 CI 行 |
| `README.md` | **需要**（tasks 4.3） | 影响开发者启动流程：hook 安装是一次性本地配置 |
| `openspec/specs/build-warning-hygiene/spec.md` | **需要**（归档时由 delta 合入） | 规格层承载三个新 Requirement |
| `spec/design.md`、`spec/requirements.md` | **不需要** | 二者只声明「告警门禁须无新增告警」，本 change 不改判定语义，只增加执行层。现表述在变更后仍然成立 |
| `openspec/project.md` | **不需要** | 约束与常用验证已含准绳命令，无需改动 |
| `docs/release-build-warnings-cleanup-design.md` | **需要**（tasks 6.8，归档后） | 其「后续项：CI 缺少告警门禁」小节需标注已落地与归档位置 |
| `docs/tooling-sources.md` | **不在本 change**（后续项 7.6） | 第 12 行仍记 Rust 1.94.1，属独立的文档准确性问题 |

## 停止条件

1. 依赖锁定三处未同批改动
2. 误 dispatch `build-dev-release.yaml`
3. Docker dry-run 出现任何 push / tag / `latest` 变动
4. `openspec validate --all` 失败
5. 工作区出现真实 `config.json`、`credentials.json`、token 或 `.codegraph/` 待提交
6. 告警数高于 0.4 记录的基线
7. 工件之间出现冲突且无法判断有效事实源
8. 无法确定某项验证命令或其剩余风险

## 证据边界

本 Bridge Plan 阶段实跑：`openspec status --change ... --json`、`openspec validate --all`（21 passed）、`codegraph status`、`codegraph query`、`codegraph impact`、上列全部 rg 命令、工作树敏感文件检查、`git rev-parse` / `git diff` / `git merge --ff-only`。

**未实跑**：三条 cargo 门禁命令（tasks 0.4 与第 6 组执行）、任何 CI 运行、docker build（本机无 docker CLI；镜像 tag 与版本由用户实跑确认）。

**本阶段未修改任何实现文件**：`Dockerfile`、`.dockerignore`、三条 workflow、`AGENTS.md`、`README.md` 经 `git diff` 确认全部 unchanged；`warning-gate.yaml` 与 `scripts/git-hooks/pre-push` 尚不存在。
