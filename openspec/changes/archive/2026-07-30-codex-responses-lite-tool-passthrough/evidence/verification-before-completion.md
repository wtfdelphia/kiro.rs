# Verification Before Completion: codex-responses-lite-tool-passthrough

日期：2026-07-29
门禁：最终回复 / 归档前验证（`verification-before-completion`）
结论：**通过（可归档）** — 全部关键验证在本会话真实运行，无隐藏失败

> 本次工作区同时存在三个 active change，三者文件归属互不重叠。
> 本报告的验证覆盖整个工作区（测试与构建无法按 change 切分），
> 但范围判定与文档同步只针对本 change 的 `src/openai/**` 与 `docs/codex-responses-lite-wire-analysis.md`。

## Verification 列表

全部命令在本会话真实执行，输出为实际粘贴：

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `cargo test --bin kiro-rs` | `570 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` | ✅ 通过 |
| `cargo build` | `10 warnings` / `Finished dev profile` | ✅ 通过，警告零增量 |
| `openspec validate --all --strict` | `Totals: 20 passed, 0 failed (20 items)` | ✅ 通过 |
| `openspec status --change <name> --json` | `isComplete: true`，proposal/design/specs/tasks 全 `done` | ✅ 工件完整 |
| `openspec list` | 本 change `✓ Complete`（60/60 tasks） | ✅ 无未勾选任务 |
| `git status --short` | 16 条，全部为预期文件 | ✅ 无敏感文件 |
| `git check-ignore -v` | `config.json`(.gitignore:2) / `credentials.*`(:9) / `.codegraph/`(:14) / `admin-ui/dist/`(:7) 均被忽略 | ✅ |

分模块测试（本会话逐项实跑，用于核对 tasks 各组声称）：

| 模块 | 结果 | tasks 声称 | 一致性 |
| --- | --- | --- | --- |
| `openai::responses_tools` | 27 passed; 0 failed | 5.5 写 29 | ⚠ 计数笔误，已校正记录 |
| `openai::responses::` | 46 passed; 0 failed | 6.6 写 46 | ✅ |
| `openai::handlers` | 31 passed; 0 failed | 7.8 写 31 | ✅ |
| `openai::responses_stream` | 27 passed; 0 failed | 8.6 写 27 | ✅ |
| `openai::converter` | 20 passed; 0 failed | 10.2 写 20 | ✅ 零回归 |
| `anthropic::` | 103 passed; 0 failed | 10.2 写 103 | ✅ 零回归 |

警告零增量的验证方法（非推断）：用 `git stash push --include-untracked` 取 `HEAD` 基线后
`cargo build`，得 10 条；恢复工作区后再 build，同为 10 条。10 条均为既有 dead_code /
unused_import。基线检查后工作区已完整还原（`git stash list` 为空）。

范围与残留核实：

```
$ git diff --stat -- src/anthropic/
（空 —— tasks 1.1/1.2 的可见性提升已完整撤回）

$ git diff --stat -- Cargo.toml Cargo.lock
（空 —— 未引入依赖）

$ grep -rn "shorten_tool_name|TOOL_NAME_MAX_LEN" src/openai/
（空 —— 归一层不预测缩短，design D3.1 落实）

$ grep -rn "strip_encrypted" src/ | grep -v responses_tools.rs
（空 —— 作用域收敛在归一层，未触碰 /v1/messages 共享路径）

$ git diff -U0 -- src/ | grep -E "^\+.*(dbg!|eprintln!|println!|wire capture|TODO|FIXME)"
（空 —— 无调试残留）

$ ls kiro_release/ wire-capture/
（均不存在 —— tasks 10.5 的 debug 探针遗留已消解）
```

## SKIPPED（未运行的验证）

| 项 | SKIPPED 原因 | 剩余风险 |
| --- | --- | --- |
| `collaboration__*` 展平工具的真实调用与回程 namespace 还原 | 端到端会话中模型只调用了 `exec`，该路径未被真实流量触发 | 中。有两条锁定测试覆盖（`test_flattened_tool_restored_to_namespace_and_original_name`、`test_stream_namespace_restored_in_items`）。若还原有误，症状是客户端静默拒绝自己的工具调用 |
| 超长工具名的两级映射真实往返 | 实测工具名最长 30 字符（`collaboration__interrupt_agent`），未触发 `TOOL_NAME_MAX_LEN=63` 缩短 | 低。锁定测试 `test_long_freeform_tool_name_roundtrip_still_custom_tool_call` 与 `test_two_level_mapping_long_flattened_name` 覆盖 |
| lark 文法降级后模型输出的长期合规性 | 端到端样本量为 1（本次生成的 `exec` 输入被客户端接受） | 中。proposal Risks 已声明；Codex 侧对格式错误有容错与重试 |
| 本 change 单独的回归基线 | 三个 change 的改动同时在工作区，无法按 change 切分测试 | 低。分模块测试已定位到本 change 影响面（`openai::*`），`anthropic::` 与 `kiro::` 零回归可佐证无跨模块污染 |
| 真实上游对降级后 schema 的长期接受度 | 需持续观察，非单次验证可覆盖 | 低。端到端已证明 9 个工具送达且工具链未断 |

## Documentation Sync 表

| 文档 | 是否需要同步 | 理由 |
| --- | --- | --- |
| `README.md` | **否** | 不改启动、构建、部署、测试入口。`/v1/responses` 已在 `:35`、`:539` 列明；本 change 是该端点内部的工具转换修复，不新增端点或配置项 |
| `AGENTS.md` | **否** | 未改变 AI 协作纪律、OpenSpec 条件、高风险矩阵或验证命令 |
| `CLAUDE.md` | **否** | 规则入口未变 |
| `spec/requirements.md` | **否** | `:12`「OpenAI Responses 兼容（`/v1/responses`，无状态）」为概括性描述，仍成立 |
| `spec/design.md` / `spec/structure.md` | **否** | 未改变模块划分或架构层次；新增的 `responses_tools.rs` 落在既有 `src/openai/` 内 |
| `openspec/specs/openai-responses/spec.md` | **待归档时同步** | 本 change 的 spec 含 1 个 MODIFIED + 7 个 ADDED Requirement，归档时由 `openspec-archive-change` / `openspec-sync-specs` 合并进长期 spec |
| `docs/tooling-sources.md` | **否** | 未引入新工具或依赖 |
| `config.example.json` | **否** | 无新增配置项 |
| `docs/codex-responses-lite-wire-analysis.md` | **已新增（长期参考）** | wire-level 协议逆向分析，记录 Codex 客户端的长期协议事实，非本次变更的过程记录。**归档时不随 change 目录移动**，它对后续所有 Responses 相关变更可复用 |

## 安全核查

- `git status --short` 16 条全部为预期文件；`--untracked-files=all` 全量展开后无意外条目。
- `config.json` / `credentials.*` / `.codegraph/` / `admin-ui/dist/` 经 `git check-ignore -v` 确认被忽略，
  不会进入候选提交。
- 敏感模式扫描（`eyJ*` / `Bearer *` / `arn:aws:*:<account>` / `Cookie` / `sk-*` /
  `refreshToken":"...`）在全部 diff、`docs/codex-responses-lite-wire-analysis.md`（254 行）
  与三份 evidence 上**零命中**。
- 新增的两条长期 info 诊断日志只打工具名与 item 类型（`handlers.rs:130-136`、`:1400-1405`、
  `responses_stream.rs:332-342`），**不含参数值、schema 内容、token 或凭据**。
- 端到端证据的日志摘录只含模型名、工具名、item 类型与时间戳。
- 无新增 `#[allow(...)]`；`types.rs` 反而移除了一处已不适用的 `#[allow(dead_code)]`。

## Residual Risk

| 风险 | 说明 |
| --- | --- |
| **未 archive** | 本 change 尚未执行 `openspec-archive-change`；spec 的 8 个 Requirement 未合并进长期 `openspec/specs/openai-responses/spec.md` |
| **未 commit / push / PR** | 改动仍在工作区未提交状态（当前分支 `dev`，主分支 `master`）。本会话未执行任何 git 提交、推送或 PR 操作 |
| **三个 change 共存于同一工作区** | 提交时需按 change 分离 staging（本 change 对应 `src/openai/**` + `docs/codex-responses-lite-wire-analysis.md`），否则三个变更会混入同一 commit |
| **lark 文法降级** | 模型可能生成不合文法输入；端到端样本量 1，长期有效性待观察 |
| **namespace 还原路径** | 未被真实流量触发，仅有单测覆盖 |
| **两级名字映射耦合** | 依赖 anthropic 层既有 `tool_name_map` 行为；若后者改动，两条锁定测试会先失败（有保护） |
| **`text.format` 不支持** | 诚实边界而非缺陷：上游 `userInputMessageContext` 只有 `toolResults` / `tools`，无处透传；已明确拒绝 prompt 层模拟 |
| **工具限制** | 无前端参与；Rust 侧无 coverage 工具，测试充分性靠 Scenario 对照人工判断 |

## 结论

**通过，可归档。** 本会话真实运行的 7 类验证全部通过：`cargo test` 570 passed / 0 failed、
`cargo build` 警告零增量（基线对比法验证）、`openspec validate` 20/20、工件 `isComplete: true`、
tasks 60/60、`git status` 无敏感文件。

5 项 SKIPPED 已逐条写明原因与剩余风险，其中 2 项（namespace 真实往返、超长名往返）
由锁定测试兜底，1 项（lark 文法长期合规性）在 proposal Risks 中已声明。

不存在被隐藏的失败。唯一与 tasks 记录不符的是 5.5 的测试计数（声称 29、实为 27），
已在 tasks 10.7 与两份上游报告中校正——**声称的每条断言都真实存在，无测试缺失**。

下一步：`openspec-archive-change`。提交时注意按 change 分离 staging，
并保留 `docs/codex-responses-lite-wire-analysis.md` 在 `docs/` 下不随归档移动。
