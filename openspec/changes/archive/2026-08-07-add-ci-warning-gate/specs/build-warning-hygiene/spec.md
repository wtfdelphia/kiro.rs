## ADDED Requirements

### Requirement: 发布路径必须有固定编译器版本上的机器强制点

The zero-warning obligation MUST be enforced by machine on the release path, not by discipline alone. CI MUST run the warning check on a **pinned** compiler version, on a judgment surface identical to the local authoritative gate, with project warnings escalated to errors and dependencies locked. When that check fails, the pipeline MUST NOT produce release artifacts, container images, or multi-architecture manifests.

Conversely, the release artifact builds themselves MUST NOT escalate compiler warnings to errors, and MUST NOT be pinned for the sake of the warning gate.

Rust's stability promise covers "code that compiles today still compiles tomorrow"; it does **not** promise "produces no new warnings". Every stable cycle may add lints or widen existing ones. Escalating warnings to errors on a floating toolchain therefore makes an external release schedule into a release-blocking trigger: a repository with zero code changes can go red because the compiler moved. Pinning the measuring instrument while letting the production line float keeps the reading meaningful without making the compiler upgrade a release risk.

The gate answers a single boolean question — are there warnings — and MUST NOT be treated as the source of warning counts. Counting and reporting remain the local authoritative gate's job, so the gate MAY use an early-abort escalation mechanism.

A pinned gate diverges from the floating production line over time. That divergence is accepted: warnings the gate cannot see MUST NOT block releases. The pinned version MUST be treated as a maintained value that is bumped deliberately, and a bump MUST be accompanied by a re-run of the local authoritative command.

#### Scenario: 门禁失败时不得产出发布物

- GIVEN CI 的告警门禁检查在固定编译器版本上报出项目告警
- WHEN 该检查失败
- THEN 发布产物构建 MUST NOT 启动
- AND MUST NOT 创建 release、容器镜像或多架构 manifest

#### Scenario: 门禁判定面与本地准绳一致

- GIVEN CI 的告警门禁检查
- WHEN 定义其判定命令
- THEN 该命令 MUST 与本地准绳 `cargo check --release --all-targets` 逐 flag 一致
- AND MUST 仅显式附加告警升级与依赖锁定选项
- AND MUST 覆盖默认 feature 与无默认 feature 两种组合

#### Scenario: 发布产物构建不得升级告警

- GIVEN 某个发布产物构建腿在浮动编译器上产生了新的编译器 lint 告警
- AND 该告警由编译器版本前进引入，而非由代码变更引入
- WHEN 该腿执行构建
- THEN 构建 MUST NOT 因该告警而失败
- AND 发布 MUST NOT 被阻断

#### Scenario: 门禁必须在全新检出下可执行

- GIVEN 门禁 job 运行在无本地构建残留的全新检出上
- AND 项目在编译期依赖某个未入库的目录存在（前端嵌入产物）
- WHEN 该 job 执行判定命令
- THEN 该 job MUST 在编译前自行供给该前置条件
- AND MUST NOT 依赖本地开发环境恒满足该条件

#### Scenario: 门禁工具链版本 bump 必须重新确认基线

- GIVEN 门禁的固定编译器版本被 bump
- WHEN 声称该 bump 完成
- THEN MUST 重新运行本地准绳命令并报告告警数
- AND 存在新增告警时该 bump MUST 视为未完成

#### Scenario: 门禁不承担告警计数职责

- GIVEN 门禁使用「首个告警即失败」的升级机制
- WHEN 需要报告告警数
- THEN 计数 MUST 取自本地准绳命令的输出
- AND 门禁的提前中止 MUST NOT 被视为违反计数口径要求

### Requirement: 人工触发的发布流水线默认不得产生发布副作用

A manually triggered release pipeline MUST default to a dry run. Publishing side effects — registry login, image push, manifest creation, moving or overwriting alias tags such as `latest` — MUST require either an automatic trigger from a release branch or tag, or an explicit opt-in input on the manual trigger.

Verifying a release pipeline requires running it, and a pipeline that publishes unconditionally cannot be verified from an experimental branch without polluting the published artifact set. Making the manual path a dry run by default is what makes the pipeline verifiable at all.

The publish decision MUST be computed in a non-matrix job. A matrix job's outputs resolve to the value from whichever leg finishes last, which is not a well-defined basis for a publishing switch.

#### Scenario: 人工触发默认不发布

- GIVEN 一个人工触发的镜像构建流水线运行，且未显式开启发布
- WHEN 该运行完成构建
- THEN MUST NOT 登录镜像仓库、MUST NOT 推送镜像、MUST NOT 创建 manifest
- AND 别名 tag（如 `latest`）MUST NOT 被移动或覆盖

#### Scenario: 显式开启后允许发布

- GIVEN 一个人工触发的运行显式开启了发布开关
- WHEN 该运行完成构建
- THEN 发布步骤 MAY 执行

#### Scenario: 发布开关不得由矩阵 job 输出承载

- WHEN 计算发布开关
- THEN 该计算 MUST 位于非矩阵 job
- AND MUST NOT 依赖矩阵 job 的输出

### Requirement: 本地防线必须是默认执行的机器动作而非纯纪律

The repository MUST provide an installable local check that runs the authoritative warning command automatically and refuses the operation when warnings exist. The hook script MUST be version-controlled in the repository rather than existing only in an individual clone's uninstrumented hook directory.

Discipline-only enforcement decays. This repository has a documented instance: version bumps were performed manually before each release until 2026-03-30, after which the habit lapsed and five consecutive releases shipped with a stale self-reported version. A warning count that is currently zero is subject to the same decay.

The local check MUST use the authoritative counting command rather than an early-abort escalation, because its purpose includes showing the developer which warnings exist.

The check MUST be documented as **non-binding**: it requires a one-time local configuration, it can be bypassed, and a fresh clone does not have it active. Therefore it MUST NOT be presented as a substitute for the CI enforcement point, and the CI enforcement point MUST NOT be softened on the grounds that a local check exists.

#### Scenario: 存在告警时拒绝操作

- GIVEN 本地检查已安装
- AND 工作树在判定命令下存在告警
- WHEN 开发者执行被检查的操作
- THEN 该操作 MUST 被拒绝
- AND MUST 输出告警清单以便定位

#### Scenario: 无告警时放行

- GIVEN 本地检查已安装且判定命令报告零告警
- WHEN 开发者执行被检查的操作
- THEN 该操作 MUST 被放行

#### Scenario: 本地防线不得作为软化 CI 门禁的理由

- GIVEN 本地检查已存在并正常工作
- WHEN 评估 CI 告警门禁的失败行为
- THEN CI 门禁 MUST 保持为硬失败
- AND 本地检查的存在 MUST NOT 被用作把 CI 门禁降级为非阻断提示的理由

#### Scenario: 非强制性必须被记录

- WHEN 文档描述该本地检查
- THEN MUST 写明它需要一次性本地配置才生效、可被绕过、新克隆默认未启用
- AND MUST NOT 把它描述为发布路径的强制保证
