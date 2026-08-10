> 修订记录：
> - 2026-08-04 初稿。基于 CodeGraph 索引分析 + git/CI/运行时实测证据。
> - 2026-08-04 审核修订：基线分支由误记的 master 更正为 dev（实测 git branch --show-current = dev；tag v2026.8.4 仅可从 dev 到达，master 落后 21 个提交）；同步修正 D6 发布清单的推送分支；新增「问题 6：发布分支漂移」。
> - 2026-08-07 复审重基线。初稿基线 d85cfb6 之后仓库落地了告警门禁（9366872 / add-ci-warning-gate 归档），六项事实过期，本轮据实重测全部修正：
>   - 新增第三条发布流水线 build-dev-release.yaml 的分析（初稿只认 build.yaml 与 docker-build.yaml，漏检 dev 滚动发布路径）；
>   - 问题 6 已自愈并降级为「已消解」：实测 origin/master 现领先 origin/dev 一个合并提交，v2026.8.4 在 dev/main/master 全部可达，据此重写 D6 发布清单与实施步骤 7b；
>   - 问题 5 的 Dockerfile 基础镜像由 rust:1.92-alpine 变为 rust:1-alpine，D4 的 rust-version 依据随之改锚；工具链漂移点由三处更正为四处；
>   - 问题 2 的杂散 tag add 远端已不存在（仅本地残留），清理动作降级；补记 tag 类型（附注/轻量）不统一；
>   - 问题 1 的 --version 输出由「机制推导」升级为本机实测证实；
>   - D2 补齐与既有 warning-gate 的接线关系、reusable 取舍改判，验证方案补 CI 红路径要求。
> - 2026-08-07 决策复核：MSRV 不再取 edition 2024 的理论下限 1.85，改为当前已实际验证且被 warning-gate 钉住的 Rust 1.97.1；后续升级须显式验证。
> - 2026-08-07 设计定稿：正式版本以 Cargo.toml 为声明源，`v*` 附注 tag 为发布身份；正式发布落点统一为 GitHub 默认分支 main；同一自然日只允许一个正式版；人工镜像发布不得绕过版本门禁；长期规格写入 openspec/specs；本地私有 tag 不列入项目验收。
> - 2026-08-10 **格式纠正（correct-calver-to-month-sequence）**：本文档第 84 行「实测 6 个 tag 全部吻合，可确认实际约定为 CalVer YYYY.M.D」是一次抽样误判，据此定稿的「第三段=日历日」与「同一自然日只允许一个正式版」两条结论**不成立**，现已推翻。
>   - 纠正依据：复核全部 29 个历史 tag 与其提交日期，原约定始终是 `YYYY.MM.MICRO`（第三段=当月发布序号）。`2025.12.1`–`2025.12.7` 集中在 2025-12-28 至 12-31 四天内发布；`2026.1.2`/`2026.1.3` 同为 2026-01-07；`2026.2.1`/`2026.2.2`/`2026.2.3` 同为 2026-02-06；`v2026.1.4`/`v2026.1.5` 同为 2026-01-13。第三段显然不是日期。
>   - 误判成因：只核对了 `v2026.7.27` 起最近 6 个 tag。那几次恰好一天一发，序号与当日日期数值重合，被当成了约定。
>   - 外部依据：calver.org 明确收录 `YYYY.MM.MICRO` 三段式（Twisted 自 2002 年沿用至今，并扩散到 Klein、Treq、PyOpenSSL），其弃用 SemVer 的理由（组件众多、各自独立弃用与破坏兼容）与本项目一致；同一规范指出「four-numeric-segment versions are discouraged」，故 `vYYYY.M.D.N` 不作为备选。SemVer 亦不适用：semver.org 的前提是识别 public API 并向依赖方传递兼容性信号，而本项目是兼容代理，用户以镜像/二进制运行，无下游代码依赖。
>   - 实际影响：误判引入了能力回退。2026-08-10 已发布 `v2026.8.10` 后，同日完成的 Claude Opus 5 支持无法再发正式版。纠正后同月可发多版，序号递增。
>   - 不追溯改写历史 tag。`v2026.7.27` 起若干 tag 第三段与日期重合的痕迹保留；代价是该区段序号存在语义空洞（7 月并未真的发布 31 次），换取已发布镜像与二进制引用稳定。

# 版本治理优化方案

分析日期：2026-08-04 初稿 / 2026-08-07 复审重基线
分析基线：dev @ fa97837（工作树干净；初稿基线为 d85cfb6 = tag v2026.8.4 指向点，其后仓库合入告警门禁，故本轮重测）
环境：Windows 11 / PowerShell / 本机 rustc 1.97.1（实测 rustc --version；docs/tooling-sources.md 记 1.94.1，已过期，见问题 5）
分析工具：CodeGraph（codegraph status/query，索引 137 文件 / 2777 节点 / 7643 边，up to date）、rg、git

## 背景与问题

项目采用 CalVer 日期版本（tag 名即发布日期，如 v2026.8.4 = 2026-08-04），发布流水线以 git tag 名作为 Release 产物与 Docker 镜像的版本号（github.ref_name），与 Cargo 包版本（Cargo.toml [package].version）是两条互不校验的独立链路。CodeGraph 检索显示，除 clap derive 隐式引用 CARGO_PKG_VERSION 外，全仓源码无任何显式版本号引用点（codegraph query "version" 命中的全部是 kiro_version / node_version / credentials_version 等业务字段，与项目版本无关）——版本号既不落启动日志，也不落镜像元数据。

### 发布链路全景（复审补充）

.github/workflows/ 实测有 4 个 workflow，初稿只分析了前两个，漏检 build-dev-release.yaml：

| workflow | 触发 | 版本号来源 | 是否受版本门禁约束 |
|---|---|---|---|
| build.yaml | push master/dev、tag v*、dispatch | tag 名 / dev-SHA / beta-SHA / dispatch 输入 | 目标：tag 路径强校验；分支与 dry-run dispatch 仅要求 commit 可追溯 |
| docker-build.yaml | push master、tag v*、dispatch | 同上（镜像 tag） | 目标：tag 与人工发布路径强校验；dry-run dispatch 可使用自由标签 |
| build-dev-release.yaml | push dev、dispatch | 固定滚动 tag dev-latest（或 dispatch 覆盖） | 否（永不接 v* tag，见 D2 判定） |
| warning-gate.yaml | workflow_call（被上面三条复用） | 无 | 不适用 |

build-dev-release.yaml 产出与 build.yaml 相同的 7 腿二进制并强推 dev-latest 预发布，README 的「开发版下载」入口正指向它。它不属于正式版本一致性承诺：二进制 `--version` 显示 Cargo 基线版本，commit 身份由 Release 标题、正文中的完整 SHA 与 Actions run 记录提供。

由此产生六个问题。问题 6 的旧形态（master 落后 dev）曾短暂消失，但进一步远端核验发现默认分支 main 与发布分支 master 分裂，因此仍需治理。按严重度排列。

### 问题 1：Cargo 包版本冻结，与发布 tag 漂移 5 个版本（最严重）

Cargo.toml 当前 version = "2026.3.1"（Cargo.lock 同步冻结），最后一次 bump 是 2026-03-30 的提交 a8b20dc "bump: v2026.3.1"。此后发布的 5 个正式版均未同步：

| 发布 tag | 发布日期 | 当时 Cargo.toml 版本 | 漂移 |
|---|---|---|---|
| v2026.3.1 | 2026-03-30 | 2026.3.1 | ✅ 一致（最后一次手动 bump） |
| v2026.7.27 | 2026-07-27 | 2026.3.1 | ❌ |
| v2026.7.28 | 2026-07-28 | 2026.3.1 | ❌ |
| v2026.7.30 | 2026-07-30 | 2026.3.1 | ❌ |
| v2026.7.31 | 2026-07-31 | 2026.3.1 | ❌ |
| v2026.8.4 | 2026-08-04 | 2026.3.1 | ❌（HEAD 所在 tag） |

这不是流程从未建立，而是既有习惯中断：实测 git show v2026.1.4:Cargo.toml 与 git show v2026.2.7:Cargo.toml，版本分别为 2026.1.4 与 2026.2.7，说明早期每次发版前有手动 bump，2026-03-30 后被遗忘。纯靠人工纪律、无机器门禁兜底，是漂移的根因。

已证实的影响：

- src/model/arg.rs:5 的 clap derive #[command(version, ...)] 以 CARGO_PKG_VERSION 作为 --version 输出。2026-08-07 本机实测 cargo run --quiet -- --version，输出 kiro-rs 2026.3.1——初稿的机制推导已由实测证实。据此，自 v2026.7.27 起的所有 release 二进制自报版本均为 2026.3.1，与产物文件名（kiro-rs-v2026.8.4-*.exe）矛盾。仍未实测的是已发布的 release 二进制本体与镜像内二进制（本地无该产物），但二者与本机走同一编译期常量路径。
- dev 滚动包的 `--version` 同样显示 Cargo 基线版本，但它不冒充正式版本；其 commit 身份通过 dev-latest Release 元数据与 Actions run 追溯。本次不把 SHA 嵌入二进制。
- src/main.rs 启动日志（凭据加载、端点清单等十余条 tracing::info!，最早一条在第 58 行）不打印任何项目版本，线上问题排查时无法从日志确认运行版本。
- Docker 镜像内二进制的自报版本同样错误，且镜像本身无版本 label（见问题 5）。
- Dockerfile 第 28 行的 smoke test 已在跑 ./kiro-rs --version，但注释显式声明「只断言退出码，不断言版本字符串（版本漂移属 version-governance 范围）」——即镜像构建链路已预留该断言点，本方案落地后可顺势收紧。

### 问题 2：tag 命名规范分裂，杂散 tag 残留本地

本地 30 个 tag 实测分四批：

| 批次 | 格式 | 示例 | 数量 |
|---|---|---|---|
| 早期 | 无 v 前缀 | 2025.12.1 ～ 2026.1.3 | 10 |
| 杂散 | 非法命名 | add（疑似误推的分支名，指向 7baa884 / 2026-07-27） | 1 |
| 滚动包 | dev 前缀 | dev-latest、dev-60b58dd（后者仅本地） | 2 |
| 现行 | v 前缀 | v2026.1.4 ～ v2026.8.4 | 17 |

复审补测远端（git ls-remote --tags origin，29 行）：

- 杂散 tag add **远端已不存在**，仅当前克隆残留。它不是可由项目仓库治理保证的状态，不列入变更任务或验收；维护者可自行执行 `git tag -d add`。
- dev-60b58dd 远端亦无，只有 dev-latest 在远端（由 build-dev-release.yaml 每次强制移动）。
- 补记一条初稿遗漏：**tag 类型不统一**。远端输出中仅 v2026.1.6 带 `^{}` 解引用行，说明它是附注 tag（annotated），其余为轻量 tag（lightweight）。影响 git describe、git for-each-ref 的 creatordate/taggerdate 取值一致性，属规范分裂的一部分，建议在 D1 中一并约定。

build.yaml 与 docker-build.yaml 均只以 tags: ['v*'] 触发发布，旧格式 tag 若重推不会触发流水线。

### 问题 3：版本 scheme 无文档，CI 描述与现行约定脱节

- tag 日期与 tag 名一一对应（实测 6 个 tag 全部吻合），可确认实际约定为 CalVer YYYY.M.D + v 前缀，但 README 与 `openspec/specs/` 均未记载；2026.3.1 这类写法同时可被解读为 semver，语义模糊。
  > **已于 2026-08-10 推翻**：本条只抽样了最近 6 个 tag。复核全部 29 个后，原约定实为 `YYYY.MM.MICRO`（第三段=当月序号），见顶部修订记录。
- workflow_dispatch 的 version 输入实测分两种语义，修描述时不可一刀切：
  build.yaml:12 与 docker-build.yaml:11 写 description: '... (e.g., 2025.12.1)'、required: true、default: '2026.1.1'——示例是旧的无前缀格式，默认值是 7 个月前的版本号，二者都该更新为现行 v 前缀格式。
  build-dev-release.yaml:19 的语义不同：description 为 'Optional tag override. Leave empty to use rolling tag dev-latest.'、required: false、default: ''，留空即走 dev-latest。它本身表述正确，**不需要改**，也不应被套上 CalVer 示例。
- 同日二次发布无约定（当前历史上未发生过，6 个漂移 tag 分属 6 天）。本方案明确同一自然日只允许一个正式版，紧急修复使用下一个自然日版本。
  > **已于 2026-08-10 推翻**：「历史上未发生过」不成立——`2026.2.1`–`2026.2.3` 同日发布，`2026.1.2`/`2026.1.3` 同日发布。同月多版本为原有能力，现已恢复。
- 仓库已有 `openspec/specs/` 长期规格体系；版本治理应新增对应 capability，不另建顶层 `spec/`。

### 问题 4：admin-ui 版本与主项目脱轨

admin-ui/package.json 实测 version: 1.0.0，从未跟随主项目。该前端经 rust-embed 内嵌进二进制、不独立发布，实际危害小，但属于无策略声明的悬空版本源。

### 问题 5：容器与工具链版本元数据缺失

- Dockerfile 无 ARG VERSION、无 OCI org.opencontainers.image.version label；docker-build.yaml 的 labels 只写了 image.source 与 image.description 两项。镜像版本只存在于外部 tag，docker inspect 无法获知，镜像内二进制还自报旧版本（问题 1 叠加）。
- 工具链漂移实测为**四处**（初稿记三处，且 Dockerfile 事实已变）：

| 位置 | 实测值 | 性质 |
|---|---|---|
| Dockerfile:10 | rust:1-alpine | 浮动到 1.x 最新（提交 9366872 由 rust:1.92-alpine 改为此值） |
| build.yaml:121 / build-dev-release.yaml:111 | dtolnay/rust-toolchain@stable | 浮动 stable |
| warning-gate.yaml:45 | dtolnay/rust-toolchain@1.97.1 | 钉版（并有 Assert pinned toolchain 步骤强校验） |
| docs/tooling-sources.md:12 | 1.94.1 | 记录值已过期，本机实测为 1.97.1 |

  即当前唯一被钉住的是告警门禁的度量工具链，产物构建与镜像构建都在浮动。Cargo.toml 无 rust-version 字段声明最低支持版本，edition 2024（需 ≥1.85）之外的下限完全不受保护。

### 问题 6：默认分支与发布分支分裂

初稿（基线 d85cfb6）曾实测 master 落后 dev；随后 dev 回流 master，该局部问题已经消失。但 2026-08-07 通过 GitHub API 与远端 HEAD 复核得到新的当前事实：仓库默认分支是 `main`，而正式产物 workflow 仍监听 `master`（build.yaml 另监听 dev，docker-build.yaml 只监听 master）。

复审实测（基线 fa97837）：

| 项 | 实测命令 | 结果 |
|---|---|---|
| 远端默认分支 | `git ls-remote --symref origin HEAD` / `gh repo view` | `main` |
| main 与 master | `git rev-list --left-right --count origin/main...origin/master` | 1 / 1，已各有独有合并提交 |
| main 与 dev | `git rev-list --left-right --count origin/main...origin/dev` | 1 / 0，main 领先一个合并提交 |
| 分支保护 | GitHub rulesets / branch protection API | 无 ruleset；main/master/dev 均未保护 |

因此默认分支与稳定发布分支必须统一。本方案选择 `main` 作为唯一稳定发版落点，并同步正式产物 workflow 的分支触发器。main/master 的独有提交如何收敛属于实施前置：必须先把 master 独有内容合入 main，确认 main 包含所需历史后再停止监听 master；不得用强制覆盖解决分叉。分支保护可作为后续增强，不在本 change 自动启用。

## 目标

- G1：正式 `v*` 发布的版本号在所有可观测面一致——Cargo 包版本、tag、`--version`、启动日志、产物文件名、Docker 镜像 tag 与元数据。
- G1a：非正式分支、`dev-latest` 与 dry-run dispatch 构建不冒充正式版本，并能通过产物标签、Release 元数据或 Actions run 追溯到 commit。
- G2：漂移在发布前被机器拦截，不依赖人工记忆；门禁失败信息直接给出修复动作。
- G3：版本约定成文，新参与者无需考古即可正确发版。
- G4：零运行时行为变化（版本号与日志行除外），不触碰协议转换、认证、Admin 业务逻辑。

## 非目标（v1 不做）

- 不引入 cargo-release / release-plz 等自动发版框架（收益存在，但新增工具链依赖与流程复杂度，待 v1 门禁跑稳后再评估）。
- 不做 CHANGELOG 自动生成。
- 不把 git commit SHA 嵌进二进制；非正式构建沿用 CI/Release 元数据实现 commit 追溯。
- 不改 admin-ui 的构建方式与内嵌机制。
- 不追溯修复已发布 release 的版本号（历史产物保持原样，只向前生效）。

## 方案设计

### D1 版本约定与发布身份

成文规则，写入 README「下载 / Releases」小节与 `openspec/specs/release-version-governance/spec.md`：

1. 版本号 = CalVer YYYY.MM.MICRO（第三段为当月发布序号，非日历日），git tag 加 v 前缀：v2026.8.11。
   > 2026-08-10 纠正：原文为 `YYYY.M.D`（日历日），见顶部修订记录。
2. `Cargo.toml [package].version` 是源码版本声明源；正式 `v*` tag 是不可变发布身份，必须等于 Cargo 版本加 `v` 前缀。
3. 同一年月内可发布多个正式版本，序号严格递增且不复用；同一自然日 MAY 发布多次。不用 `-1` 等 SemVer 预发布后缀冒充第二个正式版。
   > 2026-08-10 纠正：原文限制「同一自然日只允许一个正式版，紧急修复使用下一个自然日」，见顶部修订记录。
4. 杂散 tag（如 add）不允许存在；dev 滚动 tag（dev-latest）属既定机制，由 build-dev-release.yaml 维护，保留。
5. 新的正式发布 tag 统一用**附注 tag**，保留 tagger、创建时间和说明；历史 tag 不追溯改写。
6. `main` 是唯一稳定发版落点。正式 tag 所指提交必须可从 `origin/main` 到达；dev 为日常开发分支并通过 PR/合并回流 main。正式产物 workflow 的分支触发器从 master 迁移到 main。

### D2 CI 一致性门禁（核心机制）

在 build.yaml 与 docker-build.yaml 加一个轻量 reusable version-gate job，秒级完成。build-dev-release.yaml **不加**：它永不接正式 `v*` tag（只接 push dev 与 dispatch），版本一致性门禁对该滚动预发布无判定价值。

接线关系（复审补充，初稿未交代）：三条流水线当前均已挂 reusable 的 warning-gate（needs: [pre-check, warning-gate]，见 warning-gate.yaml 的 workflow_call）。version-gate 与 warning-gate **并列而非串行**：

    build:
      needs:
        - pre-check
        - warning-gate
        - version-gate

理由：两个门禁互不依赖，version-gate 是秒级 sed 比对，warning-gate 需编译两种 feature 组合（timeout-minutes: 20）。串起来会让一个必然快速失败的检查白等编译，且两者失败原因正交，并列能一次暴露两类问题。version-gate 沿用 warning-gate 的 if: needs.pre-check.outputs.should_build == 'true' 条件，保持与既有跳过逻辑一致。

正式 tag 路径的核心比对逻辑：

    version-gate:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v5
        - name: Assert Cargo.toml version matches tag
          run: |
            EXPECTED="${GITHUB_REF_NAME#v}"
            ACTUAL=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
            if [[ "$EXPECTED" != "$ACTUAL" ]]; then
              echo "::error::Cargo.toml 版本 ${ACTUAL} 与 tag ${GITHUB_REF_NAME} 不一致。"
              echo "修复：把 Cargo.toml 的 [package].version 改为 ${EXPECTED}，运行 cargo check --release 刷新 Cargo.lock，提交后重打 tag。"
              exit 1
            fi
            echo "版本一致：${ACTUAL}"

要点：

- 正式 tag 推送必须强校验。branch 构建和 `publish=false` 的 workflow_dispatch 是非正式构建，可使用 dev/beta/自定义标签，但必须保留 commit 可追溯元数据。
- `docker-build.yaml` 的 `publish=true` 人工路径不得直接采用自由输入的 `version`：必须完整 checkout tag 历史，从当前提交解析出**唯一一个**附注 `v*` tag，以该 tag 作为镜像版本，并复用同一 Cargo 一致性检查。没有 tag、存在多个候选、tag 不是附注类型或版本不一致时均在登录 GHCR 前失败。
- build.yaml 的手工 dispatch 只上传 Actions artifact，不创建正式 Release，因此仍可使用自由标签，但文档不得称其为正式版本。
- 失败信息包含具体修复步骤，与 AGENTS.md「验证纪律」一致：门禁不只是红灯，还要指路。
- 该核心片段已于 2026-08-07 本机等价实测（bash + 真实 Cargo.toml）：tag=v2026.3.1 退出 0 输出「版本一致：2026.3.1」；tag=v2026.8.4 退出 1 并打印修复指引；sed 命中的是 Cargo.toml 第 3 行。人工发布的 tag 解析与附注类型检查须在实施阶段补正反例测试。
- 实现形态取舍**改判为 reusable workflow**（初稿主张两处复制）。理由：仓库已经用 warning-gate.yaml 证明了 workflow_call 复用在本项目可行且被三条流水线消费，同一仓库内再对第二个门禁采用复制路线会造成两种并存风格；且 D1 第 3 条的预发布后缀、未来 workspace 化都会改动比对逻辑，单点维护优于双点同步。形态上与 warning-gate 对齐：新增 .github/workflows/version-gate.yaml（on: workflow_call、permissions: contents: read），两个 caller 各写 3 行 uses 引用。

### D3 版本运行时暴露

- src/main.rs 新增一行：
  tracing::info!("kiro-rs v{}", env!("CARGO_PKG_VERSION"));
  编译期内嵌，零依赖、零运行时开销。
- **位置修正**（初稿写「启动 Anthropic API 端点」日志附近，即第 230 行前后）：实测 main.rs 第 58 行已有 info 日志（凭据备份），第 82/93 行为凭据加载日志。若按初稿放在第 230 行，该行不会是首屏第一条，与验收标准 3「启动日志首屏包含」对不上。正确位置是 tracing 初始化完成之后、凭据加载之前，即现有第 58 行日志之前，使版本行成为服务打印的第一条日志。
- kiro-rs --version 由 clap 提供，无需改动——D2 保证 CARGO_PKG_VERSION 正确后，此路径自动修复。

### D4 容器元数据与工具链锚定

- Dockerfile 最终镜像增加：ARG VERSION=unknown + LABEL org.opencontainers.image.version=$VERSION；docker-build.yaml 的 build-push-action 传 --build-arg VERSION=${{ steps.version.outputs.version }}，并把该 label 并入既有 labels 块（现有两项 image.source / image.description 保留）。
- Cargo.toml 增加 **rust-version = "1.97.1"**。该值不再取 edition 2024 的理论语言下限 1.85，而是明确声明项目当前支持的最低 Rust 版本为已实际验证的 1.97.1：本机工具链为 1.97.1，warning-gate 也钉在 1.97.1 并执行 default 与 `--no-default-features` 两种 `cargo check --release --all-targets --locked`。这是一项显式 MSRV 契约；低于 1.97.1 的编译器不在支持范围内。Dockerfile rust:1 与两条产物流水线 @stable 继续浮动，它们是实际产物工具链，不改变 MSRV 声明。后续调整 `rust-version` 时必须在目标版本上重跑完整编译门禁并同步相关文档。
- Dockerfile 第 28 行现有的 `RUN ./kiro-rs --version` smoke test 可在本方案落地后收紧为断言版本字符串等于 ARG VERSION（其注释已预告此事属本方案范围）。列为 D4 可选项，不作为验收硬标准。
- docs/tooling-sources.md 维持现状（它记录本机工具，不是发布事实源），但在版本约定文档中交叉引用。
- docs/tooling-sources.md 的 Rust 记录值 1.94.1 与本机实测 1.97.1 不符，属该文档自身的维护滞后，可顺带更新，但不属本方案必需项。

### D5 admin-ui 版本策略

声明而非同步：admin-ui/package.json 保持 1.0.0，在版本约定文档中明确「前端内嵌、不独立发布、版本号不跟随主项目」。零成本消除「悬空版本源」的困惑；若未来 admin-ui 独立分发再改为同步。

### D6 发布清单（人工步骤成文）

写入 README 的版本约定小节，作为 D2 门禁的配套操作：

1. 将 Cargo.toml 的 version 改为目标日期版本（如 2026.8.5）。
2. cargo check --release（自动刷新 Cargo.lock）。
3. 提交：bump: v2026.8.5（沿用 a8b20dc 的既有格式）。
4. 确认发版提交已在 main：dev 上开发完成后通过 PR/合并回流 main，并确认 `git merge-base --is-ancestor <commit> origin/main` 成功。
5. 在该提交上创建附注 tag 并推送：`git tag -a v2026.8.5 -m "Release v2026.8.5"`，再执行 `git push origin main v2026.8.5`。tag 推送与分支无关地触发两个正式发布 workflow，但门禁还会验证该提交可从 main 到达。
6. 门禁失败则按报错回修，绝不先打 tag 后补版本。

迁移前必须把 master 的独有提交以普通 PR/合并方式纳入 main，复核 main 已包含所需历史后再把 workflow 分支触发器从 master 改为 main。不得强推、重置或删除 master；停止监听后的 master 处置不属于本 change。

## 候选方案取舍

| 方案 | 结论 | 理由 |
|---|---|---|
| 手动 bump + CI 门禁（D1+D2+D6） | ✅ 采纳 | 最小改动即闭环；门禁兜住「忘记 bump」这一实际故障模式 |
| version-gate 做成 reusable workflow | ✅ 采纳（复审改判） | 与既有 warning-gate.yaml 的 workflow_call 形态一致，避免同仓两种门禁风格并存；比对逻辑未来会随预发布后缀/workspace 演进，单点维护更稳 |
| version-gate 在两个 workflow 各复制一份 | ❌ 改判否决 | 初稿理由「耦合成本高于重复成本」已被 warning-gate 的实际落地反驳：同一仓库三条流水线复用同一 reusable 门禁运行良好 |
| build-dev-release.yaml 也挂 version-gate | ❌ 否决 | 该 workflow 永不接 v* tag，门禁恒走跳过分支，无判定价值 |
| cargo-release / release-plz 自动 bump | ⏸ 缓议 | 需引入新工具与发布权限配置，复杂度与团队当前规模不匹配；门禁跑稳后可再评估 |
| Cargo.toml 为源码声明源、tag 为发布身份 | ✅ 采纳 | 符合实际发布顺序与 Rust 编译期版本机制；CI 只验证 tag 身份，不在构建时改写源码 |
| 启动日志嵌 git SHA（vergen/build.rs） | ⏸ 缓议 | 非正式构建先由 CI/Release 元数据追溯，避免为本次治理引入 build script |

## 变更影响面

| 文件 | 变更 | 类别 |
|---|---|---|
| Cargo.toml | bump 版本；新增 rust-version = "1.97.1" | 配置 |
| Cargo.lock | 随 bump 刷新 | 配置 |
| src/main.rs | +1 行启动版本日志（置于首条 info 之前，见 D3） | 代码（零行为变化） |
| Dockerfile | ARG/LABEL | 部署 |
| .github/workflows/version-gate.yaml | 新增 reusable 门禁（对齐 warning-gate.yaml 形态） | CI |
| .github/workflows/build.yaml | uses 引用 version-gate + build/release job needs；稳定分支触发器迁至 main；dispatch 描述与默认值更新 | CI |
| .github/workflows/docker-build.yaml | 同上；publish=true 从当前提交的唯一附注 v* tag 推导版本；Docker build-arg VERSION + image.version label | CI |
| .github/workflows/build-dev-release.yaml | **不改动**（不接 v* tag；其 dispatch 描述语义正确）。列出以证明已复审，避免再次漏检 | CI |
| README.md | 版本约定与发布清单小节；修正下载示例的格式描述 | 文档 |
| openspec/specs/release-version-governance/spec.md | 归档时同步版本治理长期事实 | 文档 |

代码改动只有 main.rs 一行，其余为配置、CI 与文档。不触碰 src/anthropic/、src/kiro/、src/openai/、src/admin/ 任何业务模块。warning-gate.yaml 亦不改动，version-gate 与它并列挂载。

## 实施步骤

按 AGENTS.md「OpenSpec 条件」，CI/部署脚本与配置 schema 变化必须先建 OpenSpec change（建议名：release-version-governance），本设计文档作为其设计输入。实施顺序：

1. OpenSpec change：openspec-propose 建档，引用本文档；openspec-superpowers-bridge 产出实现桥接。
2. 代码与配置（一个提交）：Cargo.toml bump 到下一个发布版本 + rust-version；main.rs 启动日志；cargo check --release --all-targets 确认零新增告警且 lock 刷新。
3. 分支前置：把 master 独有提交以普通合并纳入 main，确认 main 包含所需历史；不强推、不删除分支。
4. CI 门禁（一个提交）：新增 version-gate.yaml（reusable），build.yaml 与 docker-build.yaml 各加 uses 引用并把 version-gate 并入构建 job 的 needs 数组；稳定分支触发器由 master 迁至 main；更新 dispatch 描述与默认值；docker 人工发布从当前提交唯一附注 v* tag 推导版本（build-dev-release.yaml 不动）。
5. 容器元数据（一个提交）：Dockerfile ARG/LABEL + workflow build-arg。
6. 文档同步（一个提交）：README 版本约定/发布清单、OpenSpec delta spec；归档后进入 `openspec/specs/` 长期事实。
7. 验证与归档：维护者提供 CI 红/绿 run URL；按 `spec-compliance-check → openspec-verify-change → verification-before-completion` 完成门禁后归档。

本地 `add` / `dev-60b58dd` tag 不属于实施或验收范围；历史远端 tag 也不追溯改写。

## 验证方案

| 项 | 命令 / 方法 | 通过标准 |
|---|---|---|
| 编译卫生 | cargo check --release --all-targets | 退出码 0，告警数 ≤ 变更基线（零新增） |
| 版本号修复 | cargo run --quiet -- --version（或 release 二进制实测） | 输出与当前 tag 去 v 后一致 |
| 启动日志 | cargo run 后观察日志 | kiro-rs v<版本> 为**第一条** info 日志（早于凭据加载相关行） |
| MSRV | 使用 Rust 1.97.1 执行 default 与 `--no-default-features` 两种锁定检查 | 两条均退出码 0；Rust 1.96 及以下明确不在支持范围 |
| 门禁正例（本地） | 本地等价执行 D2 shell 片段（tag=当前、Cargo.toml=已 bump） | 退出码 0 |
| 门禁反例（本地） | 临时把 Cargo.toml 版本改错后执行同一片段 | 退出码 1，输出含修复指引；验证后必须还原 |
| **门禁绿路径（CI）** | 一致版本下推 tag（或临时 tag）触发流水线 | version-gate job 成功，构建 job 正常进入 |
| **门禁红路径（CI）** | 维护者推送故意失配的临时 tag 并提供两个 Actions run URL | 两个 workflow 的 version-gate 均失败，**且所有构建 job 未启动**；annotation 含修复指引；维护者确认已删除临时 tag |
| 门禁与 warning-gate 并列 | 观察同一次 run 的 job 图 | 两个 gate 并行执行，version-gate 不等待 warning-gate 完成 |
| dev 路径不受误伤 | push dev 触发 build-dev-release.yaml | 无 version-gate job；dev-latest 产物正常发布 |
| 非正式构建追溯 | 检查 branch artifact 与 dev-latest Release 元数据 | 可从短 SHA、完整 commit 和 Actions run 定位源码；不要求其 `--version` 等于滚动标签 |
| 人工镜像发布反例 | dispatch publish=true，分别模拟无 tag、多 tag、轻量 tag、Cargo 失配 | 均在登录/推送前失败；publish=false dry-run 仍允许自由标签 |
| 稳定分支迁移 | 检查远端拓扑与 workflow 触发器 | main 包含 master 所需独有提交；正式分支触发器监听 main，不再监听 master |
| Docker 元数据 | docker build --build-arg VERSION=x . && docker inspect | label org.opencontainers.image.version=x（可选，CI 实测亦可） |
| OpenSpec | openspec validate --all | 全部通过 |
| 发布演练 | 下一次真实发版走 D6 清单 | tag、Cargo、--version、产物名、镜像 tag 五者一致 |

CI 红路径为**硬性要求**，不可用本地等价执行替代：AGENTS.md 高风险检查矩阵明确要求绿路径与红路径都有 run 证据。本地片段不能证明 needs 接线真的阻断构建。远端临时 tag 的创建与删除由维护者执行并提供两个 run URL；实施代理只核验证据，不擅自操作远端 tag。

复审已完成的实测（2026-08-07，本机）：

- cargo run --quiet -- --version → kiro-rs 2026.3.1（证实问题 1）
- D2 片段等价脚本：tag=v2026.3.1 → 退出 0；tag=v2026.8.4 → 退出 1 + 修复指引；ref_type=branch → 跳过
- 尚未实测：Docker label/manifest（本机未跑 docker build）、CI 红/绿 run（需实施阶段在流水线上执行）

## 风险与回退

| 风险 | 评估 | 缓解 |
|---|---|---|
| 门禁误伤 dispatch / branch 构建 | 低：普通 branch 与 dry-run dispatch 不作正式版本校验；仅正式 tag 和 publish=true 强校验 | 正反例双向验证；发布与 dry-run 条件分离 |
| 门禁误伤 dev 滚动发布 | 极低：build-dev-release.yaml 不挂 version-gate，且不接 v* tag | 验证方案含「dev 路径不受误伤」一项 |
| sed 解析 Cargo.toml 被 workspace/多包结构破坏 | 低：本项目单包、version 为第 3 行；取首个匹配 | 门禁脚本 head -1 取首个；未来引入 workspace 时同步升级脚本 |
| main/master 分叉迁移遗漏提交 | 高：直接切触发器可能使稳定分支丢历史或使构建来源改变 | 先普通合并 master 独有提交到 main并验证祖先关系；禁止强推/重置 |
| rust-version = "1.97.1" 排除旧工具链用户 | 中：这是有意收紧的兼容范围，Rust 1.96 及以下会在构建前被 Cargo 拒绝 | README / 长期规格明确 MSRV；发布前在 1.97.1 上验证 default 与 `--no-default-features` 两种构建面 |
| 声明的 MSRV 与浮动产物工具链分离 | 中：Dockerfile rust:1 与 CI @stable 均浮动，实际产物可能使用高于 1.97.1 的版本 | 将 1.97.1 定义为最低支持版本而非产物工具链记录；每次调整 MSRV 时显式重跑门禁并同步文档 |
| reusable version-gate 的 permissions 继承 | 低：reusable workflow 未声明时继承 caller 权限（build.yaml 的 contents: write） | 照 warning-gate.yaml 的做法显式声明 permissions: contents: read，收窄到只读 |
| 同日紧急修复需要等待日期变化 | 低：换取版本排序与语义无歧义 | 明确同日只允许一个正式版；紧急修复使用下一个自然日版本 |
| 人工 publish 绕过版本身份 | 高：自由输入可发布与 Cargo 无关的公共镜像 tag | publish=true 只能从当前提交的唯一附注 v* tag 推导版本，任何歧义均前置失败 |

所有提交均可独立回退；CI 门禁若出现误拦，可临时在 workflow 中注释 version-gate 的 needs 引用（秒级恢复），不影响其他 job 与 warning-gate 逻辑。

## 验收标准

1. 给定下一次 tag 推送 vX，当 Cargo.toml 版本 ≠ X（去 v）时，流水线在任何构建开始前失败并给出修复指引；版本一致时构建正常完成。
2. kiro-rs --version 输出等于当前 tag 去 v 后的版本。
3. 服务启动日志的第一条 info 即 kiro-rs v<版本>（早于凭据加载日志）。
4. docker inspect ghcr.io/<owner>/kiro-rs:<tag> 可读到与 tag 一致的 org.opencontainers.image.version。
5. README 含版本约定与发布清单，发版落点为 GitHub 默认分支 main；正式 workflow 的稳定分支触发器监听 main，不再监听 master。
6. cargo check --release --all-targets 零新增告警；openspec validate --all 通过。
7. build-dev-release.yaml 的 dev-latest 发布路径未受影响：push dev 后产物与预发布正常产出，且该 workflow 无 version-gate job；Release 元数据可追溯 commit。
8. version-gate 与 warning-gate 在同一次 run 中并行，二者互不阻塞。
9. docker workflow 的 publish=true 不能使用自由 version 输入绕过正式 tag/Cargo 一致性；publish=false dry-run 仍可使用自由标签且无发布副作用。
10. 新正式 tag 为附注 tag；同一年月内可发布多个正式版本，序号递增即可。
    > 2026-08-10 纠正：原文限制「同一自然日不创建第二个正式版本」，见顶部修订记录。
11. `rust-version = "1.97.1"`，并在该工具链上通过 default 与 `--no-default-features` 两种锁定检查。
12. 长期规格归入 `openspec/specs/release-version-governance/`，不新建顶层 `spec/`。
