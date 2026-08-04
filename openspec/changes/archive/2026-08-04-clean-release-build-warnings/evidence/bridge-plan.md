# Bridge Plan: clean-release-build-warnings

> 原版 2026-07-31；2026-08-04 按 `openspec-superpowers-bridge` 复验更新：CodeGraph 索引与 callers 证据刷新、工作区快照更新、端点计数与调用点计数更正记录（见「2026-08-04 复验」节）。

## 范围确认

### 目标

将 `cargo check --release --all-targets` 的唯一告警数从 14 降至 0,为后续 CI 门禁建立清洁基线。

### 非目标

- **不在本次实施 CI 门禁。** CI 流程变更命中 `AGENTS.md` 的 OpenSpec 强制项,且超出「不改运行时行为」范围,作为独立 change 处理
- **不改变任何运行时行为、API 签名、配置 schema、数据流。** 全部修复均为编译期可见性调整
- **不重构未直接告警的代码。** 保留项(`new_with_runtime`、`convert_request_with_policy`、`with_auth_runtime` 等)逐字不变

### 关键设计决策(来自 design.md)

1. **A 类传递依赖原子性。** `default_resolution_policy` 与 `convert_request` 必须同一次提交加 `#[cfg(test)]`,拆步会让其中一个报 E0425
2. **D 类双层 `super`。** 测试模块内导入 `map_model` 必须写 `use super::super::converter::map_model;`,单层 `super` 会报 E0432
3. **B 类连带风险已排除。** 删除 `KiroProvider::default_endpoint`(getter)不会产生「只写不读字段」新告警;删除 `AdaptedDocument::credentials` 不会让整个 `impl` 变死代码
4. **E 类规格约束。** `EndpointStatus::Beta` 受 `openspec/specs/public-api-catalog/spec.md:39` 无条件 MUST 保护,删除会违反生效 spec

## 高风险项

| 风险类别 | 具体内容 | 应对 |
|---|---|---|
| 编译期传递依赖 | `default_resolution_policy` 与 `convert_request` 必须同批加 `#[cfg(test)]` | tasks.md § 4.2 强制原子提交 |
| 模块路径误判 | D 类单层 `super` 会报 E0432 | tasks.md § 2.3 明确必须两层;已实测通过 |
| A 类测试链接失败 | `#[cfg(test)]` 收敛后测试无法链接到依赖符号 | tasks.md § 4.7 要求跑 `cargo test`(check 无法覆盖) |
| B 类误删有调用点符号 | 预期无调用点实为 grep 未覆盖到 | tasks.md § 3.8 停止条件:失败停下重新 grep,禁强删 |
| 假零告警 | 只跑 `cargo build --release` 会漏测试目标告警 | 验证策略全程以 `--all-targets` 为准绳 |

## CodeGraph 证据

### 状态

```
$$ codegraph status

CodeGraph Status

Project: D:\MyProgram\wtf\wkspace\github\kiro.rs

Index Statistics:
  Files:     135
  Nodes:     2,782
  Edges:     7,414
  DB Size:   7.65 MB
  Backend:   node:sqlite - built-in (full WAL)
  Journal:   wal

Nodes by Kind:
  function        1,208
  import          531
  method          368
  struct          198
  enum_member     124
  file            113
  variable        56
  interface       61
  route           43
  constant        33
  enum            30
  type_alias      15
  trait           2

Files by Language:
  rust            76
  tsx             26
  yaml            22
  typescript      9
  javascript      2

[OK] Index is up to date
```

**关键发现:** CodeGraph 覆盖 76 个 Rust 文件,索引为最新（2026-08-04 复验）。能够追踪函数/方法/结构/枚举,支持用 `codegraph query` / `callers` / `callees` 验证调用点。

### CodeGraph callers 复核（2026-08-04 补跑）

针对 design 标注的硬约束符号运行 `codegraph callers`,与 rg 互为佐证:

| 符号 | callers 结果 | 结论 |
|---|---|---|
| `default_resolution_policy` | 8 处:7 测试 + `convert_request`（converter.rs:466） | A 类传递依赖证实:唯一非测试调用点在另一 A 类项体内,必须同批加 `#[cfg(test)]` |
| `convert_request` | 6 处,全部为测试 | 无生产调用点,A 类收敛成立 |
| `map_model` | 生产调用点 `get_context_window_size`（converter.rs:351）+ 4 处 converter 测试 | D 类必须移动不能删除（handlers.rs:1183/1210/1211 三处测试使用点经 rg 证实） |
| `add_credential` | 6 处,全部为 token_manager 测试 | `MultiTokenManager::add_credential` 与生产在用的 `AdminService::add_credential`（service.rs:317,经 admin/router.rs:31 挂载,handlers.rs:98 转调）消歧成立 |

**CodeGraph 盲区（rg 补齐）:** `codegraph callers get_context_window_size` 只报 3 处,rg 全仓核对实为 **5 处生产调用点**（`anthropic/handlers.rs:740`、`anthropic/stream.rs:640`、`openai/handlers.rs:378`、`openai/stream.rs:150`、`openai/responses_stream.rs:168`）。调用点核实以 rg 为准,印证 `AGENTS.md`「CodeGraph 不替代 rg、源码精读与测试」。

## rg / 源码补盲

### CI workflow 门禁现状(补盲 1)

```bash
$$ rg "cargo (build|check|test)" .github/workflows/*.yaml
```

产出(已在 design.md 提及):

- `build.yaml:131` 与 `build-dev-release.yaml:123` 均只跑 `cargo build --release`
- **无 `-D warnings` 标志**
- **无 clippy 步骤**
- **无 `--all-targets` 覆盖测试目标**

**结论:** CI 当前不强制告警门禁。清零之后没有机器防护,回归风险依赖人工。这是已知剩余风险,作为后续独立 change 处理(CI/发布脚本变更命中 `AGENTS.md` 强制项)。

### credentials / config 防护(补盲 2)

```bash
$$ rg -l "credentials\.json|config\.json" --type-not rust | Select-Object -First 5
AGENTS.md
README.md
Dockerfile
spec/design.md
spec/structure.md
```

**结论:** 只在文档中提及,`.gitignore` 已覆盖,`git status --short` 最终验证时会再确认。

### 当前工作区状态(补盲 3)

```bash
$$ git status --short
 M .codex/skills/verification-before-completion/SKILL.md
 M AGENTS.md
 M openspec/project.md
 M spec/design.md
 M spec/requirements.md
?? docs/release-build-warnings-cleanup-design.md
?? openspec/changes/clean-release-build-warnings/
```

**结论:**

- 五个 `M` 文件为「零新增告警」条款同步到规范入口,已在上一轮完成,本 change 不再触碰
- 未跟踪文件包含本 change 的全部工件
- **无 `config.json`、`credentials.json`、`.codegraph/`**,满足安全前置条件
- 原快照中的 `.codex-tmp-fix-docs.py` 为一次性修文档脚本,已于 2026-08-04 删除

## 2026-08-04 复验（openspec-superpowers-bridge 重跑）

bridge 重跑的实测记录,全部命令本会话真实运行:

| 验证 | 结果 |
|---|---|
| `openspec status --change clean-release-build-warnings --json` | 4 个 artifact 均 `done`,`isComplete: true`,未 blocked |
| `openspec validate --all` | 20 项全部通过（19 specs + 本 change） |
| `cargo check --release --all-targets` | 唯一告警行数恰为 **14**（按口径全行 `sort -u` 去重;`middleware.rs:60` bin/test 各报一条,唯一位置 13 个）,与 design「当前实现」清单逐条一致 |
| `codegraph status` | 索引最新（135 files / 2,782 nodes / 7,414 edges） |
| `codegraph callers` × 4 | 见「CodeGraph 证据」节,A/D 类硬约束全部证实 |

### 复验中发现并已修复的文档错误

1. 端点目录计数:误记 8 `Live` + 1 `Planned`,实测 7 `Live` + 1 `Planned`（共 8 条）——已更正 docs、design、tasks 三处
2. `get_context_window_size` 生产调用点:误记 8 处,rg 实测 5 处——已更正 docs、proposal、design 三处
3. 行号 off-by-one:`load_detailed` `:287`→`:286`、`convert_request_with_policy` `:470`→`:471`——已更正 tasks
4. `.codex-tmp-fix-docs.py` 一次性脚本——已删除

### 同步判断复核

- README:仅 `README.md:100` 提及 `cargo build --release`,无告警门禁内容 → 支持 tasks 6.3「无需改动」结论
- 凭据示例:`credentials.example.*.json` 5 个均存在,且被 `.gitignore` 的 `/credentials.*` 规则例外放行

## 任务到执行步骤映射

(tasks.md 已提供详细检查点,本节只摘要映射关系。完整内容见 tasks.md)

| 任务 § | 类别 | 核心动作 | 验证 | 停止条件 |
|---|---|---|---|---|
| 2 | C + D | 删 1 处 `mut`;移动 `map_model` 导入到测试模块 | 告警 14 → 12;双层 `super` 必须实测 | D 类仍报错停下汇报 |
| 3 | B | 删 6 个无调用点符号 | 告警 12 → 6;无新增「只写不读字段」 | 出现未预期调用点停下重 grep |
| 4 | A | 6 项加 `#[cfg(test)]` | 告警 6 → 1;`cargo test` 全绿 | 测试失败回退该项 |
| 5 | E | 给 `Beta` variant 加窄范围 `#[allow]` 并注明规格依据 | 告警 1 → 0 | — |
| 6 | 规格 | 确认规范入口已同步;`openspec validate --all` | 新 capability `build-warning-hygiene` 被校验 | validate 失败停下修规格 |
| 7 | 完成前 | `spec-compliance-check` + `verification-before-completion` | 产出两个 evidence、全量验证命令通过 | 任一验证失败不得声称完成 |

## 必跑验证

### 分步门槛(分别对应 tasks.md § 2–5)

1. C + D 类后:`cargo check --release --all-targets` → 14 → **12** 告警
2. B 类后:→ 12 → **6** 告警,且无新增告警类型
3. A 类后:→ 6 → **1** 告警(仅剩 `Beta`),**且 `cargo test` 全绿**
4. E 类后:→ 1 → **0** 告警

### 最终全量验证(tasks.md § 7.1)

```bash
cargo check --release --all-targets   # 0 告警(准绳)
cargo build --release                 # 0 warnings
cargo test                            # 全绿
openspec validate --all               # 通过
git status --short                    # 无敏感文件
```

**告警计数口径(准绳):** 以**唯一告警行数**计,不以 cargo 汇总行数字计。推荐:

```bash
cargo check --release --all-targets --message-format short 2>&1 \
  | grep "warning:" | grep -v "generated .* warning" | sort -u | wc -l
```

## README / AGENTS / spec 同步判断

### 已完成(前一轮)

以下五处规范入口已在前一轮会话同步「零新增告警」条款:

1. `AGENTS.md` — 新增「零新增编译告警」小节、验证纪律、高风险矩阵
2. `spec/requirements.md` — 「任何代码实现不得引入新的编译告警」
3. `spec/design.md` — 「告警门禁」小节
4. `openspec/project.md` — 「约束」列表
5. `.codex/skills/verification-before-completion/SKILL.md` — 检查点列表

`git status --short` 显示这五个文件为 `M` 状态。

### 本 change 无需再次修改

- **不触碰 README。** 若 README 未提及构建告警门禁则本次无需新增;若已提及则无需改动
- **不触碰已完成的五个规范入口**
- **OpenSpec 工件:** 本 change 的 `specs/build-warning-hygiene/spec.md` 已在 `openspec/changes/clean-release-build-warnings/specs/` 下,tasks.md § 6.4 会确认 `openspec validate --all` 能校验它

最终验证(tasks.md § 6)会运行 `openspec validate --all` 以确认新 capability 被系统识别。

## 停止条件(优先级从高到低)

1. **告警数不降反升。** 出现「只写不读字段」或 impl 级告警 → 回退该项,重新核对连带引用(design.md § 异常路径)
2. **测试失败。** A 类收敛后 `cargo test` 不全绿 → 回退该项;禁用 `allow` 绕过
3. **未预期的调用点。** B 类删除后编译失败 → 停下重新 grep,禁强删或加 `allow` 掩盖
4. **D 类路径错误。** 单层 `super` 报 E0432 → 改两层 `super::super::converter::map_model`
5. **A 类拆步提交。** 单独给 `default_resolution_policy` 或 `convert_request` 加属性 → 报 E0425,必须同批
6. **OpenSpec 校验失败。** `openspec validate --all` 不通过 → 停下修规格
7. **工作区出现敏感文件。** `git status --short` 显示 `config.json`、`credentials.json`、`.codegraph/` → 停下清理

## 剩余风险

1. **CI 门禁未落地。** 清零后无机器强制力防回归,依赖人工。已纳入后续 change,不在本次处理
2. **A 类测试链接不确定性。** design.md 分析阶段未跑 `cargo test`,「收敛后测试仍通过」在实施阶段才确认
3. **协议语义无自动化验证。** 本 change 不改运行时,但无集成测试证明 Anthropic/Kiro 流式转换不退化。这是项目级缺陷,不属本 change 范围

## 与现有纪律一致性

- `AGENTS.md` 的「Think Before Coding / Simplicity First / Surgical Changes / Goal-Driven Execution」— 本 change 全程无需猜测;只改 14 项告警点;无关代码零触碰;验证命令预先定义
- `AGENTS.md` 的「OpenSpec 条件」— 本 change 属「大范围重构」(76 个文件潜在影响面)且涉及「跨模块变更」(Anthropic/Kiro/Admin/OpenAI),已走 OpenSpec 流程
- `AGENTS.md` 的「验证纪律」— 本计划在 tasks.md 逐步明确了分步门槛与最终全量验证,`cargo check --release --all-targets` 作为准绳贯穿全程
- `AGENTS.md` 的「安全」— `git status --short` 已确认无真实凭据候选提交;本 change 无新增密钥处理逻辑

## 批准继续实施

- [x] 已读 AGENTS.md、spec/design.md、openspec/project.md、本 change 的 proposal/design/tasks/specs
- [x] 范围、非目标、任务、Requirement 一致性已核对
- [x] 高风险项与两处硬约束(A 类传递依赖、D 类双层 `super`)已明确
- [x] CodeGraph status 产出且索引可用
- [x] rg 补盲 CI 门槛、credentials、工作区状态完成
- [x] 任务到执行步骤映射与停止条件已明确
- [x] README/AGENTS/spec 同步判断已明确
- [x] 剩余风险已书面记录
- [x] 2026-08-04 复验完成:openspec status/validate、cargo check 基线、codegraph callers、rg 补盲全部重跑,文档错误已修复
