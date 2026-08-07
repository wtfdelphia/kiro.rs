# Capability: build-warning-hygiene

## Purpose

Hold compiler warnings at zero for the project's own sources. Any code change, whether or not it goes through the OpenSpec process, MUST NOT increase the warning count; the authoritative gate is `cargo check --release --all-targets` counting distinct warning sites. Warnings MUST be eliminated by fixing the real problem the compiler reports, and narrow-scope suppression is permitted only where removal itself would violate an active spec or narrow a published contract.

Enforcement is by machine, not by discipline alone. Two lines of defense back the rule: a version-controlled local pre-push check that runs the authoritative command, and a hard-failing CI gate on a pinned compiler that blocks release artifacts, images, and manifests. The instrument is pinned while the release production line floats, so a compiler upgrade cannot break releases on its own. The local check is explicitly non-binding and never a substitute for the CI gate.

## Requirements

### Requirement: 任何代码实现不得引入新的编译告警

The project MUST hold compiler warnings at zero for its own sources. Any code change MUST NOT increase the warning count relative to its baseline, **regardless of whether that change goes through the OpenSpec process**. This explicitly includes changes that `AGENTS.md` exempts from OpenSpec (pure spelling, comment, or single-line fixes with no behavior change): the OpenSpec exemption covers the specification workflow, not the warning gate.

Warning counts grow monotonically when unenforced, and each accumulated warning reduces the signal value of the next one. Once the count is nonzero, a genuinely dead symbol is indistinguishable from accepted noise, so the cleanup cost is paid repeatedly rather than once.

A change that leaves new warnings MUST be treated as incomplete, not as complete-with-cleanup-pending.

#### Scenario: 走 OpenSpec 流程的变更

- GIVEN 一个走 OpenSpec change 的代码实现
- WHEN 实现完成并准备声称完成
- THEN 告警数 MUST NOT 高于该 change 开始前的基线
- AND 存在新增告警时 MUST 视为未完成

#### Scenario: 豁免 OpenSpec 的变更同样受约束

- GIVEN 一个按 `AGENTS.md` 可豁免 OpenSpec 的改动（纯拼写、注释、单行且无行为变化）
- WHEN 该改动引入了新的编译告警
- THEN 该改动 MUST NOT 被视为完成
- AND 豁免 OpenSpec MUST NOT 被解释为豁免告警门禁

#### Scenario: 告警数必须被报告

- GIVEN 一个包含代码改动的会话
- WHEN 产出最终完成报告
- THEN 报告 MUST 包含告警判定命令的实际运行结果与告警数
- AND 未运行该命令时 MUST 按验证纪律写明 SKIPPED、原因与剩余风险

### Requirement: 告警判定以 `--all-targets` 为准绳

The authoritative warning check MUST be `cargo check --release --all-targets`. A check that omits test targets MUST NOT be used as the gate.

This project has no `[lib]` target, so a plain `cargo build --release` compiles only the binary target and cannot observe warnings whose only trigger is inside a `#[cfg(test)]` module — an unused test helper is invisible to it. Using the looser command as the gate would let a whole class of warnings accumulate undetected.

The count MUST be taken as the number of **distinct** warning sites. Cargo's per-target summary lines are not warning sites and MUST NOT be counted, and a warning reported for both the bin and test targets MUST be counted once.

#### Scenario: 测试目标专属告警必须可见

- GIVEN 某告警仅由 `#[cfg(test)]` 模块内的代码触发
- WHEN 执行告警判定
- THEN 判定命令 MUST 覆盖测试目标从而报出该告警
- AND MUST NOT 以仅覆盖 binary target 的命令代替

#### Scenario: 计数口径为唯一告警点

- GIVEN 判定命令输出包含每个 target 的汇总行，且部分告警同时出现在 bin 与 test target
- WHEN 统计告警数
- THEN 汇总行 MUST NOT 被计入
- AND 重复出现的同一告警点 MUST 只计一次

### Requirement: 消除告警必须修正真实问题

Warnings MUST be eliminated by fixing what the compiler is reporting, not by silencing the report.

Permitted actions are: relocating an import to the scope that actually uses it, deleting code that has no call site under any compilation configuration, gating test-only symbols behind `#[cfg(test)]`, and removing redundant mutability or similar no-op annotations.

Suppression at crate or module scope is forbidden: a single `#![allow(dead_code)]` converts a one-time cleanup into permanent blindness for every symbol in that scope, including ones that become dead later.

Fabricating data, fixtures, or call sites so that a symbol appears used is forbidden. It defeats the warning's purpose while adding untrue content to the codebase, and where the fabricated data is itself specified elsewhere, it risks violating that specification too.

#### Scenario: 死代码删除

- GIVEN 某 `pub` 项在任何编译配置下都无调用点
- WHEN 消除该告警
- THEN MUST 删除该项
- AND `pub` 可见性 MUST NOT 被当作豁免理由（本项目无 `[lib]` target，`pub` 不构成对外 API）

#### Scenario: 测试专用符号收敛

- GIVEN 某符号的全部调用点都位于 `#[cfg(test)]` 内，且生产路径已有等价入口
- WHEN 消除该告警
- THEN MUST 为该符号加 `#[cfg(test)]`
- AND MUST NOT 删除它（删除会破坏测试编译）

#### Scenario: 导入未使用时须先判定符号性质

- GIVEN 某未使用导入告警指向的符号在其他模块有生产调用点
- WHEN 消除该告警
- THEN MUST 只移动导入到实际使用它的作用域，MUST NOT 删除符号本体
- AND 自动修复工具建议的「删除导入」MUST 先经编译验证，因为它可能破坏 `#[cfg(test)]` 内的使用点

#### Scenario: 禁止 crate 与 module 级抑制

- WHEN 试图以 `#![allow(dead_code)]` 或同类 crate/module 级属性消除告警
- THEN 该做法 MUST 被拒绝
- AND MUST 改为逐项修正真实问题

#### Scenario: 禁止构造假数据

- GIVEN 某枚举 variant 或结构因无构造点而告警
- WHEN 消除该告警
- THEN MUST NOT 为使其「被使用」而新增虚假业务数据
- AND 若该数据的取值域由其他 spec 约束，构造假数据 MUST 被视为同时违反该 spec

### Requirement: 窄范围抑制仅在删除会违反规格或契约时允许

When a symbol cannot be removed because removing it would violate an active spec or narrow an already-published external contract, the warning MAY be suppressed with the narrowest possible `#[allow(...)]`, and the reason MUST be recorded adjacent to it.

This is the only sanctioned escape from the previous requirement, and it is deliberately narrow: without the "removal is itself forbidden" precondition, `#[allow]` becomes a universal bypass and the gate stops meaning anything.

The attribute MUST be attached to the smallest item that resolves the warning — a single variant, field, or function — never a module or the crate.

#### Scenario: 受生效 spec 保护的枚举 variant

- GIVEN 某 status 枚举的一个 variant 当前无构造点
- AND 生效 spec 无条件要求该取值可被声明
- AND 其序列化分支参与对外 DTO 契约
- WHEN 消除该告警
- THEN MUST 保留该 variant 并加最小范围 `#[allow(dead_code)]`
- AND MUST 在紧邻位置注明保留理由与被依赖的 spec

#### Scenario: 抑制范围必须最小

- WHEN 使用 `#[allow(...)]`
- THEN 该属性 MUST 附加在恰好解决该告警的最小项上
- AND MUST NOT 提升到 module 或 crate 级

#### Scenario: 无正当理由不得抑制

- GIVEN 某告警对应的符号删除后不违反任何生效 spec 且不改变对外契约
- WHEN 试图用 `#[allow(...)]` 消除它
- THEN 该做法 MUST 被拒绝
- AND MUST 改为删除该符号

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
