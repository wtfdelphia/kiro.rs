> 修订记录：
> - 2026-08-04 初稿。基于 CodeGraph 索引分析 + git/CI/运行时实测证据。
> - 2026-08-04 审核修订：基线分支由误记的 master 更正为 dev（实测 git branch --show-current = dev；tag v2026.8.4 仅可从 dev 到达，master 落后 21 个提交）；同步修正 D6 发布清单的推送分支；新增「问题 6：发布分支漂移」。

# 版本治理优化方案

分析日期：2026-08-04
分析基线：dev @ d85cfb6（该提交同时是 tag v2026.8.4 的指向点，工作树干净；注意 master 已落后 dev 21 个提交，该 tag 仅可从 dev 到达，见问题 6）
环境：Windows 11 / PowerShell / Rust 1.94.1（见 docs/tooling-sources.md）
分析工具：CodeGraph（codegraph status/query，索引 135 文件 / 2777 节点，up to date）、rg、git

## 背景与问题

项目采用 CalVer 日期版本（tag 名即发布日期，如 v2026.8.4 = 2026-08-04），发布流水线以 git tag 名作为 Release 产物与 Docker 镜像的版本号（github.ref_name），与 Cargo 包版本（Cargo.toml [package].version）是两条互不校验的独立链路。CodeGraph 检索显示，除 clap derive 隐式引用 CARGO_PKG_VERSION 外，全仓源码无任何显式版本号引用点——版本号既不落启动日志，也不落镜像元数据。

由此产生六个已证实的问题，按严重度排列。

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

- src/model/arg.rs 的 clap derive #[command(version, ...)] 以 CARGO_PKG_VERSION 作为 --version 输出。据此机制推断：自 v2026.7.27 起的所有 release 二进制，kiro-rs --version 均输出 kiro-rs 2026.3.1，与产物文件名（kiro-rs-v2026.8.4-*.exe）矛盾。此结论由代码机制推导，未在本机实测（本地无 release 二进制），实现阶段须补实测。
- src/main.rs 启动日志（凭据加载、端点清单等十余条 tracing::info!）不打印任何项目版本，线上问题排查时无法从日志确认运行版本。
- Docker 镜像内二进制的自报版本同样错误，且镜像本身无版本 label（见问题 5）。

### 问题 2：tag 命名规范分裂，存在杂散 tag

全仓 30 个 tag 实测分四批：

| 批次 | 格式 | 示例 | 数量 |
|---|---|---|---|
| 早期 | 无 v 前缀 | 2025.12.1 ～ 2026.1.3 | 10 |
| 杂散 | 非法命名 | add（疑似误推的分支名） | 1 |
| 滚动包 | dev 前缀 | dev-latest、dev-60b58dd | 2 |
| 现行 | v 前缀 | v2026.1.4 ～ v2026.8.4 | 17 |

两个 workflow 均只以 tags: ['v*'] 触发发布，旧格式 tag 若重推不会触发流水线；add 这类杂 tag 永久污染 tag 列表与 git describe 类工具的输出。

### 问题 3：版本 scheme 无文档，CI 描述与现行约定脱节

- tag 日期与 tag 名一一对应（实测 6 个 tag 全部吻合），可确认实际约定为 CalVer YYYY.M.D + v 前缀，但 README、spec/ 均未记载；2026.3.1 这类写法同时可被解读为 semver，语义模糊。
- 两个 workflow 的 workflow_dispatch 输入仍写 description: '... (e.g., 2025.12.1)'、default: '2026.1.1'——示例是旧格式，默认值是 7 个月前的版本号。
- 同日二次发布无约定（当前历史上未发生过，6 个漂移 tag 分属 6 天）。

### 问题 4：admin-ui 版本与主项目脱轨

admin-ui/package.json 实测 version: 1.0.0，从未跟随主项目。该前端经 rust-embed 内嵌进二进制、不独立发布，实际危害小，但属于无策略声明的悬空版本源。

### 问题 5：容器与工具链版本元数据缺失

- Dockerfile 无 ARG VERSION、无 OCI org.opencontainers.image.version label；镜像版本只存在于外部 tag，docker inspect 无法获知，镜像内二进制还自报旧版本（问题 1 叠加）。
- 工具链三处漂移：Dockerfile 固定 rust:1.92-alpine，CI 用 dtolnay/rust-toolchain@stable，本机 docs/tooling-sources.md 记录 1.94.1；Cargo.toml 无 rust-version 字段声明最低支持版本，edition 2024（需 ≥1.85）之外的下限完全不受保护。

### 问题 6：发布分支漂移，master 已脱离发布链路

实测 git rev-list --left-right --count master...dev = 0/21：master 是 dev 的严格祖先，落后 21 个提交且无任何独有提交。tag 可达性实测（git branch --contains）：

| 发布 tag | 可达分支 |
|---|---|
| 2026.1.3 / v2026.2.7 / v2026.3.1 | dev、main、master |
| v2026.7.27 | dev、main |
| v2026.7.31 / v2026.8.4 | 仅 dev |

即 7 月起发布改为只在 dev 上打 tag，master 停在 b9e757e（"增加对 Claude Opus 4.8 的支持"），而 origin/HEAD 仍指向 master（仓库默认分支）。影响：

- build.yaml 与 docker-build.yaml 的 master 分支触发器实际已成死代码，Docker 镜像现仅经 v* tag 路径发布；
- 通过默认分支克隆的用户拿到的是陈旧代码；
- 发布分支无文档约定——本设计初稿将基线误写为 master，正是该缺口的直接后果。

## 目标

- G1：任一发布产物的版本号在所有可观测面一致——tag 名、Cargo 包版本、--version 输出、启动日志、产物文件名、Docker 镜像 tag 与元数据。
- G2：漂移在发布前被机器拦截，不依赖人工记忆；门禁失败信息直接给出修复动作。
- G3：版本约定成文，新参与者无需考古即可正确发版。
- G4：零运行时行为变化（版本号与日志行除外），不触碰协议转换、认证、Admin 业务逻辑。

## 非目标（v1 不做）

- 不引入 cargo-release / release-plz 等自动发版框架（收益存在，但新增工具链依赖与流程复杂度，待 v1 门禁跑稳后再评估）。
- 不做 CHANGELOG 自动生成。
- 不把 git commit SHA 嵌进二进制（需引入 build.rs/vergen，列为 future work）。
- 不改 admin-ui 的构建方式与内嵌机制。
- 不追溯修复已发布 release 的版本号（历史产物保持原样，只向前生效）。

## 方案设计

### D1 版本约定（单一事实源）

成文规则，写入 README「下载 / Releases」小节与 spec/：

1. 版本号 = CalVer YYYY.M.D，git tag 加 v 前缀：v2026.8.4。
2. tag 名是发布版本的单一事实源；Cargo.toml 版本必须等于 tag 名去掉 v 前缀。
3. 同日二次发布：tag 用 semver 预发布后缀 v2026.8.4-1（Cargo 侧 2026.8.4-1 合法且排序正确，仍匹配 v* 触发器）。
4. 杂散 tag（如 add）不允许存在；dev 滚动 tag（dev-latest）属既定机制，保留。

### D2 CI 一致性门禁（核心机制）

在 build.yaml 与 docker-build.yaml 各加一个轻量 version-gate job，置于所有构建 job 之前（构建 job needs: [version-gate]），秒级完成：

    version-gate:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v5
        - name: Assert Cargo.toml version matches tag
          run: |
            if [[ "${GITHUB_REF_TYPE}" != "tag" ]]; then
              echo "非 tag 触发（branch/dispatch），跳过一致性门禁"; exit 0
            fi
            EXPECTED="${GITHUB_REF_NAME#v}"
            ACTUAL=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
            if [[ "$EXPECTED" != "$ACTUAL" ]]; then
              echo "::error::Cargo.toml 版本 ${ACTUAL} 与 tag ${GITHUB_REF_NAME} 不一致。"
              echo "修复：把 Cargo.toml 的 [package].version 改为 ${EXPECTED}，运行 cargo check --release 刷新 Cargo.lock，提交后重打 tag。"
              exit 1
            fi
            echo "版本一致：${ACTUAL}"

要点：

- 只对 tag 推送强校验；branch 构建与 workflow_dispatch 跳过（dispatch 的 version 输入是产物标签，允许 dev-* 等自由格式）。
- 失败信息包含具体修复步骤，与 AGENTS.md「验证纪律」一致：门禁不只是红灯，还要指路。
- docker-build.yaml 复制同一 step（约 15 行），不为消除重复引入 reusable workflow——两文件各自独立触发，耦合成本高于重复成本。

### D3 版本运行时暴露

- src/main.rs 启动段（「启动 Anthropic API 端点」日志附近）新增一行：
  tracing::info!("kiro-rs v{}", env!("CARGO_PKG_VERSION"));
  编译期内嵌，零依赖、零运行时开销。
- kiro-rs --version 由 clap 提供，无需改动——D2 保证 CARGO_PKG_VERSION 正确后，此路径自动修复。

### D4 容器元数据与工具链锚定

- Dockerfile builder 与最终镜像增加：ARG VERSION=unknown + LABEL org.opencontainers.image.version=$VERSION；两个 workflow 的 docker build 传 --build-arg VERSION=$...。
- Cargo.toml 增加 rust-version = "1.92"（与 Docker 固定镜像一致，为当前最老受支持工具链）。CI 继续用 stable 不受影响；未来 Docker 基础镜像升级时同步抬升该字段。
- docs/tooling-sources.md 维持现状（它记录本机工具，不是发布事实源），但在版本约定文档中交叉引用。

### D5 admin-ui 版本策略

声明而非同步：admin-ui/package.json 保持 1.0.0，在版本约定文档中明确「前端内嵌、不独立发布、版本号不跟随主项目」。零成本消除「悬空版本源」的困惑；若未来 admin-ui 独立分发再改为同步。

### D6 发布清单（人工步骤成文）

写入 README 的版本约定小节，作为 D2 门禁的配套操作：

1. 将 Cargo.toml 的 version 改为目标日期版本（如 2026.8.5）。
2. cargo check --release（自动刷新 Cargo.lock）。
3. 提交：bump: v2026.8.5（沿用 a8b20dc 的既有格式）。
4. git tag v2026.8.5 && git push origin dev v2026.8.5（实际发布分支为 dev；master 角色按实施步骤第 7 条决策落定后，同步更新本条与两个 workflow 的分支触发器）。
5. 门禁失败则按报错回修，绝不先打 tag 后补版本。

## 候选方案取舍

| 方案 | 结论 | 理由 |
|---|---|---|
| 手动 bump + CI 门禁（D1+D2+D6） | ✅ 采纳 | 最小改动即闭环；门禁兜住「忘记 bump」这一实际故障模式 |
| cargo-release / release-plz 自动 bump | ⏸ 缓议 | 需引入新工具与发布权限配置，复杂度与团队当前规模不匹配；门禁跑稳后可再评估 |
| 版本事实源反转（Cargo.toml 为准，CI 自动打 tag） | ❌ 否决 | 与现有「push tag 触发发布」的流水线方向相反，需重写两个 workflow 的触发与产物命名，改动面大 |
| 启动日志嵌 git SHA（vergen/build.rs） | ⏸ 缓议 | 有诊断价值但非本次故障模式；避免为可缓议项引入 build script |

## 变更影响面

| 文件 | 变更 | 类别 |
|---|---|---|
| Cargo.toml | bump 版本；新增 rust-version | 配置 |
| Cargo.lock | 随 bump 刷新 | 配置 |
| src/main.rs | +1 行启动版本日志 | 代码（零行为变化） |
| Dockerfile | ARG/LABEL | 部署 |
| .github/workflows/build.yaml | version-gate job + 构建 job needs + dispatch 描述更新 | CI |
| .github/workflows/docker-build.yaml | 同上 + build-arg | CI |
| README.md | 版本约定与发布清单小节；修正下载示例的格式描述 | 文档 |
| spec/（requirements/design） | 记录版本约定长期事实 | 文档 |

代码改动只有 main.rs 一行，其余为配置、CI 与文档。不触碰 src/anthropic/、src/kiro/、src/openai/、src/admin/ 任何业务模块。

## 实施步骤

按 AGENTS.md「OpenSpec 条件」，CI/部署脚本与配置 schema 变化必须先建 OpenSpec change（建议名：release-version-governance），本设计文档作为其设计输入。实施顺序：

1. OpenSpec change：openspec-propose 建档，引用本文档；openspec-superpowers-bridge 产出实现桥接。
2. 代码与配置（一个提交）：Cargo.toml bump 到下一个发布版本 + rust-version；main.rs 启动日志；cargo check --release --all-targets 确认零新增告警且 lock 刷新。
3. CI 门禁（一个提交）：两个 workflow 加 version-gate 并挂 needs；更新 dispatch 描述与默认值。
4. 容器元数据（一个提交）：Dockerfile ARG/LABEL + workflow build-arg。
5. 文档同步（一个提交）：README 版本约定/发布清单、spec 长期事实。
6. 验证与归档：按「验证方案」逐项实测，spec-compliance-check → openspec-verify-change → verification-before-completion → 归档。
7. 一次性清理（人工确认）：a) 删除远端杂散 tag add（git push origin :refs/tags/add，删除前需用户明确同意；本地同名 tag 一并清理）。历史无前缀 tag 不动——它们对应已发布产物，改名会破坏既有下载链接。b) 决策 master 角色：当前 master 落后 dev 21 个提交且不再接收发布 tag，二选一——将 master 快进到 dev，恢复「master = 最新稳定」语义；或明确降级 master 为归档只读，发布文档与 workflow 分支触发器全部改以 dev 为准。属分支级操作，执行前需用户确认。

## 验证方案

| 项 | 命令 / 方法 | 通过标准 |
|---|---|---|
| 编译卫生 | cargo check --release --all-targets | 退出码 0，告警数 ≤ 变更基线（零新增） |
| 版本号修复 | cargo run --quiet -- --version（或 release 二进制实测） | 输出与当前 tag 去 v 后一致 |
| 启动日志 | cargo run 后观察首屏日志 | 出现 kiro-rs v<版本> 行 |
| 门禁正例 | 本地等价执行 D2 shell 片段（tag=当前、Cargo.toml=已 bump） | 退出码 0 |
| 门禁反例 | 临时把 Cargo.toml 版本改错后执行同一片段 | 退出码 1，输出含修复指引；验证后必须还原 |
| Docker 元数据 | docker build --build-arg VERSION=x . && docker inspect | label org.opencontainers.image.version=x（可选，CI 实测亦可） |
| OpenSpec | openspec validate --all | 全部通过 |
| 发布演练 | 下一次真实发版走 D6 清单 | tag、Cargo、--version、产物名、镜像 tag 五者一致 |

## 风险与回退

| 风险 | 评估 | 缓解 |
|---|---|---|
| 门禁误伤 dispatch / branch 构建 | 低：仅 GITHUB_REF_TYPE == tag 时校验 | 反例/正例双向验证；dispatch 显式跳过 |
| sed 解析 Cargo.toml 被 workspace/多包结构破坏 | 低：本项目单包、version 为第 3 行；取首个匹配 | 门禁脚本 head -1 取首个；未来引入 workspace 时同步升级脚本 |
| 删除 add tag 影响未知下游 | 低：非 v*，不触发任何流水线 | 要求用户明确确认后手工执行，不进自动化 |
| rust-version = "1.92" 误伤更老环境 | 低：声明的是下限，且与 Docker 现状一致 | 如存在更老需求，落地前下调该值而非删除 |
| 同日二次发版规则首次使用 | 低：尚无先例 | 2026.8.4-1 为 semver 合法预发布号，Cargo 与 v* 触发器均兼容 |

所有提交均可独立回退；CI 门禁若出现误拦，可临时在 workflow 中注释 version-gate 的 needs 引用（秒级恢复），不影响其他 job 逻辑。

## 验收标准

1. 给定下一次 tag 推送 vX，当 Cargo.toml 版本 ≠ X（去 v）时，流水线在任何构建开始前失败并给出修复指引；版本一致时构建正常完成。
2. kiro-rs --version 输出等于当前 tag 去 v 后的版本。
3. 服务启动日志首屏包含 kiro-rs v<版本>。
4. docker inspect ghcr.io/<owner>/kiro-rs:<tag> 可读到与 tag 一致的 org.opencontainers.image.version。
5. README 含版本约定与发布清单（推送分支与实际发布分支 dev 一致）；两个 workflow 的 dispatch 描述示例为现行 v 前缀格式。
6. cargo check --release --all-targets 零新增告警；openspec validate --all 通过。
7. 远端不存在 add tag（经用户确认后清理）。
