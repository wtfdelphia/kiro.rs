# Verification Before Completion: clean-release-build-warnings

- 日期：2026-08-04，基线 `dev` @ `e8c5eda`
- 结论：**实现完成，可提交**（以下命令全部为本会话真实运行）

## Verification 列表

| 命令 | 结果 | 结论 |
|---|---|---|
| `cargo check --release --all-targets`（基线） | 14 项唯一告警行 | 与 design 清单逐条一致（§1.3/§1.4） |
| `cargo check --release --all-targets`（C+D 后） | 12 项 | §2 门槛通过 |
| `cargo check --release --all-targets`（B 后） | 6 项 | §3 门槛通过，无新增告警类型 |
| `cargo check --release --all-targets`（A 后） | 1 项（仅 Beta） | §4 check 门槛通过 |
| `cargo test`（A 后） | **724 passed, 0 failed** | §4 test 门槛通过 |
| `cargo check --release --all-targets`（E 后） | **0 项** | 目标达成（准绳） |
| `cargo build --release` | exit 0，**0 warnings**（5m22s） | 成功标准满足 |
| `openspec validate --all` | 20 passed, 0 failed | 含新 capability build-warning-hygiene |
| `openspec status --change clean-release-build-warnings --json` | 4 artifact 均 done，isComplete=true | 未 blocked |
| `codegraph status` / `callers` ×4 | 索引最新；A/D 硬约束证实 | 见 evidence/bridge-plan.md |
| `git diff -U0`（A 类文件） | 仅新增 `#[cfg(test)]` 行 | 无签名/函数体变更 |
| `git diff --stat`（B 类） | 不含任何 mod.rs | 导出无需同步 |
| `git status --short` | 无 config.json / credentials.* / .codegraph/ | 安全检查通过 |

告警数改动前后对比：**14 → 0**（判定口径：`cargo check --release --all-targets` 全行 sort -u 唯一告警行；符合 AGENTS.md「零新增编译告警」准绳，全程无新增）。

## Documentation Sync

| 入口 | 是否需要同步 | 说明 |
|---|---|---|
| AGENTS.md | 已同步（本 change 交付物） | 「零新增编译告警」小节、验证纪律、高风险矩阵行 |
| spec/requirements.md、spec/design.md | 已同步（本 change 交付物） | 长期规则入长期事实 |
| openspec/project.md | 已同步（本 change 交付物） | 约束与常用验证 |
| .codex/skills/verification-before-completion/SKILL.md | 已同步（本 change 交付物） | 增加告警数检查点 |
| README.md | 无需改动 | 仅 README.md:100 提 `cargo build --release`，无告警门禁内容；CI 门禁为独立后续 change |
| CLAUDE.md | 无需改动 | 薄入口，明示指向 AGENTS.md 且不重复条款 |
| openspec/specs/ | 不直接修改 | 新 capability 由本 change delta 承载，归档时合入 |
| docs/tooling-sources.md | 无需改动 | 工具来源未受影响 |

## Residual Risk

- 未 archive / push / PR / merge（等待用户决定）
- CI 告警门禁未落地：独立后续 change；落地前回归防护依赖本 spec 与人工纪律
- `EndpointStatus::Beta` 的窄范围 `#[allow(dead_code)]` 是唯一受控例外（删除会违反 public-api-catalog 生效 spec）
- 协议语义无集成测试（项目级既有缺口，本 change 不改运行时行为，风险不增）
