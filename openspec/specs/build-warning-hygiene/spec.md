# Capability: build-warning-hygiene

## Purpose

Hold compiler warnings at zero for the project's own sources. Any code change, whether or not it goes through the OpenSpec process, MUST NOT increase the warning count; the authoritative gate is `cargo check --release --all-targets` counting distinct warning sites. Warnings MUST be eliminated by fixing the real problem the compiler reports, and narrow-scope suppression is permitted only where removal itself would violate an active spec or narrow a published contract.

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
