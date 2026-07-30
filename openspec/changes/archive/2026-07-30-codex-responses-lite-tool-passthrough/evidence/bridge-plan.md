# Bridge Plan

- change：`codex-responses-lite-tool-passthrough`
- 分支：`dev`
- 工作区：仅本 change 新增文件与 `docs/codex-responses-lite-wire-analysis.md`，无其他改动
- `openspec status --change ... --json`：4 个工件 `status: done`，**无 blocked**
- `openspec validate --all`：18 passed, 0 failed

## 1. 范围

请求侧：`additional_tools` 提取、`custom` → function 降级、`namespace` 展平 + 逆映射 +
冲突 400、`encrypted` 剔除、历史 item 与 `tool_choice` 改写、超长名复用现有截断。

响应侧：`function_call` → `custom_tool_call` 还原（流式 + 非流式）、namespace 还原、
流式参数缓冲到完成才发。

可观测性：丢弃工具 warn、`text.format` 不支持 warn。

## 2. 非目标

| 项 | 理由 |
| --- | --- |
| `tool_search` proxy | 4 份 wire 抓包零出现，无真实驱动 |
| `web_search` 放宽 `should_emulate` | lite 模式下 hosted 工具根本不发；放宽会影响既有 Anthropic/Chat 行为，属独立变更 |
| `normalize_json_schema` 对齐官方 `W19` | 在 `/v1/messages` 共享路径上，回归面覆盖全部现有流量，且缺本地故障证据 |
| Chat Completions 端点 | `custom` / `namespace` / `additional_tools` 均为 Responses 协议概念 |
| `text.format` 的 prompt 层模拟 | 会把 `strict: true` 降格为「尽力」，属假装支持 |
| SSE `event:` 行强约束放宽 | 比实际客户端需求严格但无害 |

## 3. 关键设计决策

| 编号 | 决策 |
| --- | --- |
| D1 | 一切降级为 function tool（上游只有 `toolSpecification`） |
| D2 | 归一层放 `src/openai/responses_tools.rs`，不改 Chat 共享路径 |
| D3 | 状态由归一层直接返回，**不穿 `prepare`**；归一层自行预测缩短名 |
| D5 | `custom` 降级用 `{input: string}` + description 前插调用约定覆盖说明 |
| D6 | 展平名 `<namespace>__<name>`；超长交下游，不新写哈希规则；冲突 400 不改名 |
| D8 | 历史改写归并到 `convert_items` 现有分支，复用配对逻辑 |
| D9.2 | 流式参数必须缓冲到完成，一个上游事件对应 0 或 2 个下游事件 |
| D10 | `text.format` 只补 warn |

## 4. 高风险项

| 风险 | 性质 | 对策 |
| --- | --- | --- |
| **freeform 集合 key 用错名字** | 静默：编译器不报，超长名时还原失效，表现为模型反复收到 "exec expects raw JavaScript source text" | tasks 7.6 锁定测试 |
| **两级映射漏一级** | 静默：工具调用链断 | tasks 7.7 锁定测试 |
| **流式参数透传** | 客户端收到载荷语义错误的 delta | tasks 8.5 断言上游增量不得出现在下游 |
| **历史 item 被丢** | 第二轮调用与结果双双消失，模型重复执行同一操作 | tasks 6.5；spec 显式场景 |
| `to_chat_request_json` 签名变更 | 编译期可见，但 CodeGraph 漏报调用点（见 §5） | 用 rg 结果为准，10 处逐一改 |
| lark 文法降级后模型输出不合规 | 单测无法覆盖 | tasks 10.4 端到端观察；Codex 侧有容错重试 |
| `sequence_number` 若被 Codex 依赖 | 吞/扩事件后序号不连续 | design D9.2 记为待确认，端到端验证 |

## 5. CodeGraph 证据

```
codegraph status
  → Files 125 / Nodes 2362 / Edges 6271，索引可用

codegraph impact shorten_tool_name
  → 9 个受影响符号，全部在 src/anthropic/converter.rs 内
    （map_tool_name / convert_tools / convert_assistant_message + 4 个测试）
  → 结论：提 pub(crate) 不改实现，影响面封闭在该文件

codegraph callers map_tool_name
  → 4 处：convert_tools:831、convert_assistant_message:1049 + 2 测试

codegraph callers to_chat_request_json
  → 仅 4 处，全在 src/openai/responses.rs
  → **不完整**，见 §6

codegraph callers convert_items
  → 1 处：parse_input:79
```

## 6. rg / 源码补盲

CodeGraph 漏报了跨模块调用点，以 rg 为准：

```
rg -n "to_chat_request_json" src/
  → 10 处（含定义）：
    responses.rs:15 定义
    responses.rs:319/350/544/561/566/571/582  测试 7 处
    handlers.rs:728  测试（CodeGraph 未报）
    handlers.rs:832  **生产调用点（CodeGraph 未报）**
```

补盲发现的两项事实：

1. **历史工具名与 tools 定义共用同一个 `tool_name_map`**
   `converter.rs:863`（tools 定义）与 `:1078`（历史 assistant 的 tool_use）都走
   `map_tool_name`，共享 `tool_name_map`。
   → 只要归一层在进入 `prepare` **之前**把展平名拼好，两处自动得到一致的缩短结果，
   无需额外协调。这是 D8「历史 `function_call` 无条件拼展平名」能成立的前提。

2. **缩短的确定性已有测试锁定**
   `converter.rs:1439-1460` 的 `test_shorten_tool_name_deterministic` 与
   `test_shorten_tool_name_uniqueness` 已断言同输入同输出、不同输入不碰撞。
   → D3「归一层自行预测缩短结果」的假设成立，无需新增确定性测试。

配置/CI/部署补盲（均无需改动）：

```
rg -n "tool" config.example.json        → 仅 webSearchEmulation，与本次无关
rg -ln "openai|responses" .github/workflows/ Dockerfile* docker-compose*  → 无命中
rg -ln "additional_tools|custom_tool_call|namespace" spec/ docs/
  → 仅本 change 新增的分析文档；spec/ 未涉及
```

凭据安全：

```
ls config.json credentials.json  → 不存在（仅 *.example.json）
.gitignore:2,3,9,14              → /config.json、/credentials.json、/credentials.*、.codegraph/ 均已忽略
git status --short                → 仅本 change 文件
```

## 7. 任务到执行步骤映射

| 任务组 | 执行要点 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1 可见性与类型 | `shorten_tool_name` / `TOOL_NAME_MAX_LEN` 提 `pub(crate)`，**不改实现** | `cargo build` 无新警告 | 若需改实现才能复用 → 停，重新评估 D3 |
| 2 归一层骨架 | 新增 `responses_tools.rs`；`strip_encrypted` 递归；`upstream_visible_name` | `cargo test openai::responses_tools`；断言与 `map_tool_name` 结果一致 | 预测结果与实际缩短不一致 → 停 |
| 3 additional_tools | 签名改 `(Value, ToolRewriteMap)`；改 10 处调用点 | `cargo test openai::responses`；4 份抓包形状为输入 | — |
| 4 custom 降级 | schema + description 前插 + lark 追加；集合 key 用缩短名 | 同上 | — |
| 5 namespace 展平 | `__` 拼接；逆映射；两类冲突 400 | 同上；冲突各一测试断言 400 与双方名字 | — |
| 6 历史改写 | 归并到现有 `function_call` / `function_call_output` 分支 | `cargo test openai::responses` | — |
| 7 非流式还原 | 五分支提取；namespace 还原；`ToolRewriteMap` 传入 handler | `cargo test openai::handlers`；**7.6 / 7.7 锁定测试** | 锁定测试无法构造 → 停 |
| 8 流式还原 | 缓冲状态机；吞增量；完成时发 2 个事件 | `cargo test openai::responses_stream`；断言增量不透传 | — |
| 9 可观测性 | 3 处 warn | 单测：丢弃不影响其余工具；`text.format` 不报错 | — |
| 10 收尾 | 全量测试 + 端到端 + 清理 | `cargo test --bin kiro-rs`；`openspec validate --all` | 端到端未通 → 不得声称完成 |

实现顺序即任务组顺序（存在依赖：2 → 3 → 4/5 → 6 → 7 → 8）。

## 8. 必跑验证

```
cargo build                          # 每组任务后
cargo test --bin kiro-rs             # 收尾（本仓库无 lib target）
openspec validate --all
git status --short                   # 完成前，防凭据与 .codegraph/ 误入
```

零回归底线：`/v1/messages`、`/v1/chat/completions` 既有测试不得转红。

端到端（tasks 10.4，归档前必须补）：真实 Codex 客户端打通一次「执行命令」，
确认模型能实际调用 `exec` 并拿到输出。单测无法覆盖「客户端是否接受还原后的 item 形状」。

## 9. README / AGENTS / spec / openspec/specs 同步判断

| 入口 | 是否需改 | 理由 |
| --- | --- | --- |
| `README.md` | **否** | 不影响启动、构建、部署、测试命令与 API 入口清单（`/v1/responses` 已存在） |
| `AGENTS.md` | **否** | 不改 AI 纪律、高风险矩阵、验证命令 |
| `spec/` | **否** | 长期事实未涉及工具透传细节（rg 确认无命中）；本次是既有端点的行为修正，非新增能力 |
| `openspec/specs/openai-responses/` | **是，但由归档流程处理** | 本 change 的 spec delta 含 1 个 MODIFIED + 7 个 ADDED，归档时经 `openspec-archive-change` 收敛 |
| `docs/` | **已完成** | 新增 `codex-responses-lite-wire-analysis.md`；旧 `codex-tool-passthrough-optimization-design.md` 已删（未入库，无 git 痕迹） |
| 配置 schema | **否** | 无新增配置项 |
| Docker / CI | **否** | rg 确认无命中 |

## 10. 停止条件

实现过程中遇到以下任一情况，停止并回报：

1. 归一层预测的缩短名与 `map_tool_name` 实际结果不一致（推翻 D3）
2. 锁定测试（7.6 / 7.7 / 8.5）无法构造或无法转红
3. 上游对展平后的工具列表返回 400 `Improperly formed request`
   （则 design D7 的 schema 剥离范围须扩大，重新评估是否纳入 `W19` 对齐）
4. 端到端验证中模型仍无法调用 `exec`
   （则 D9 的还原形状判断有误，须重抓 wire 证据核对客户端实际期望）
5. 发现需要修改 `prepare` 签名或 `normalize_json_schema` 才能完成
   （超出本 change 范围，须先更新 proposal）
6. 工作区出现真实 `config.json` / `credentials.json` / token / Cookie / `.codegraph/`

## 11. 待清理项（tasks 10.5）

| 项 | 状态 |
| --- | --- |
| `src/openai/handlers.rs` 临时探针 | **已清理**（`git checkout`，`grep "wire capture"` 返回 0） |
| `wire-capture/` 抓包目录 | **已清理**（`rm -rf`） |
| `kiro_release/kiro-rs.exe` | **待用户处理**：当前是 debug 探针版，原版备份在 `kiro-rs.exe.bak-wire`；替换需停止运行中进程 |
| `kiro_release/kiro-rs.exe.debug_wire-capture` | 探针版留存，可删 |
