# OpenSpec Verify Report: add-stream-error-propagation

验证时间：2026-08-20（归档前门禁）

复核记录：同日按 skill 六步重跑一轮，结论维持通过。本轮真实运行：

1. `openspec status --change add-stream-error-propagation --json`：schema
   spec-driven，proposal/specs/design/tasks 四工件均 done 且已落盘；
2. `openspec validate --all`：25 passed / 0 failed；
   `openspec validate add-stream-error-propagation`：valid；
3. tasks.md：16/16 勾选、0 未勾选，逐项支撑见 spec-compliance-report.md
   的 Scenario→证据映射表；
4. delta specs：`stream-error-propagation` 7 Requirement / 17 Scenario，
   `openai-responses`（MODIFIED）1 Requirement / 9 Scenario（含新增
   「流内错误事件构成上游失败」），每个 Requirement 均有 Scenario 且
   有实现/测试对应；
5. proposal/design 与实际改动一致（含合规复核的 hunk 级抽查：
   `close_open_blocks()` 提取为 design §2.2 所需、行为不变）；
6. evidence 四件齐备：bridge-plan / spec-compliance-report（PASS，含复核记录）/
   verification-before-completion（含告警基线对比）/ 本报告，均只记录
   本会话真实运行的命令与结果。

## Completeness（齐全性）

- `openspec status --change add-stream-error-propagation --json`：proposal / design /
  tasks / specs（`stream-error-propagation` 新增 + `openai-responses` MODIFIED）四类工件
 齐备且均已落盘。
- `openspec validate --all`：25 项全部通过，含 `change/add-stream-error-propagation`。
- tasks.md：16/16 勾选（1.x 分类层、2.x Anthropic、3.x OpenAI Chat、4.x Responses、
  5.x 遥测与安全、6.x 门槛），每项均有源码改动与测试支撑（映射见
  `spec-compliance-report.md` 的 Scenario→证据表）。
- evidence：`bridge-plan.md`（实现前桥接）、`spec-compliance-report.md`（实现后合规，
  PASS）、本报告；verification-before-completion 证据在最终回复前补齐。

## Correctness（正确性）

- 全部 Scenario 的实现语义经合成事件测试断言：三协议错误渲染形状、首个硬错误生效、
  ContentLength 语义保留、成功路径不变、渲染字段白名单、诊断只记 code。
- 门槛证据（本会话真实运行）：
  - `cargo test --release`：824 passed / 0 failed / 0 ignored；
  - `cargo check --release --all-targets`：0 warning；
  - `cargo check --release --all-targets --no-default-features`：0 warning；
  - `openspec validate --all`：25 passed / 0 failed。
- 成功路径回归由既有 parity/收尾测试在全量测试中把关（824 全绿）。

## Coherence（一致性）

- design 与实现一致；两处文档定位偏差（design §1.2 的 responses.rs、tasks 2.2 的
  类型名）已记录为 spec-compliance-report.md F1/F2，不影响行为。
- AGENTS.md 纪律：零新增告警（两组合）、合成测试无凭据、OpenSpec 先行均满足。
- README/AGENTS 无需同步（错误路径行为修复，未动启动/构建/部署/测试命令与
  API 入口清单），与 proposal Impact 声明一致。

## 失败项

无。

## 剩余风险

沿用 `spec-compliance-report.md`：真实客户端对协议错误事件的处理未验证（有回退预案）；
上游错误消息原样透传为既定设计决策，诊断侧已隔离。

## 结论

三维均通过，change 具备归档条件（待提交与最终验证证据补齐后执行 archive）。
