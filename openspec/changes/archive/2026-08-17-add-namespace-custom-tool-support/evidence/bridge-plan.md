# Bridge Plan: add-namespace-custom-tool-support

> 日期：2026-08-17　门禁：openspec-superpowers-bridge

## 1. 范围、非目标、关键设计决策

**范围**：Responses 端点工具归一中，namespace 内层 `custom` 子工具的展平降级与两级映射注册；响应侧组合还原行为以测试固化。

**非目标**：namespace 内层其他未知形状（继续丢弃留痕）；顶层 custom/function 既有路径；Anthropic 端点；配置面。

**关键决策**（详见 design.md）：

- D1 复用顶层 freeform 降级（`freeform_description` + `freeform_schema`），展平名同时注册 `rewrite.namespaces` 与 `rewrite.freeform`。
- D2 冲突检查与 function 内层共用 `top_level` / `flattened_seen` 两段路径。
- D3 丢弃告警收窄到 function/custom 之外的形状。
- D4 输出侧（流式 `close_freeform_tool_item` / 非流式 `build_tool_call_item`）预期零改动，测试验证组合。

**OpenSpec 状态**：`openspec status --change add-namespace-custom-tool-support --json` → isPlanningComplete=true，4/4 工件 done，无 blocked。

## 2. 高风险项

| 风险 | 等级 | 处置 |
| --- | --- | --- |
| 目标文件带用户在制品改动（见 §6），误覆盖或误提交他人 WIP | 高 | 只读本文件现状后叠加最小改动；提交只挑本变更文件；不 revert 非本任务改动 |
| 同模块存在活跃变更 `add-responses-websocket-ingress`（`ws_ingress.rs` 等未跟踪文件） | 中 | 编辑严格限于 namespace/custom 逻辑，不触碰 ws_* 相关代码路径 |
| 响应侧组合是按两级已验证映射推导，真实客户端回传形状未完全观测 | 中 | tasks 2.5/2.6 双路径测试 + 3.4 真实流量回归（claude-tap 在链） |
| 告警门禁：零新增编译告警是硬性要求 | 中 | 以 `cargo check --release --all-targets` 为准绳，告警数对比基线 |

## 3. CodeGraph 证据

- `codegraph status`：150 files / 2,937 nodes / 8,878 edges；Pending 3 added + 12 modified（索引略滞后于脏工作区，结论以源码核对为准）。
- `codegraph impact "normalize_tools"`：23 个受影响符号——responses_tools.rs 本体与 16 个既有测试、responses.rs 的 `to_chat_request_json`/`chat`（调用方）、handlers.rs 的 `post_responses`（入口）。结论：改动面收敛在 openai 模块内。
- `codegraph callers "build_tool_call_item"`：10 个调用方，含测试 `test_plain_tool_stays_function_call` / `test_freeform_tool_becomes_custom_tool_call` / `test_freeform_tool_raw_source_passthrough`。结论：非流式组合测试有现成范式可仿写。
- `codegraph callers "close_freeform_tool_item"`：唯一调用方 `close_tool_item`（responses_stream.rs:453）。结论：流式 freeform 收口单点，组合行为只需验证该路径。

## 4. rg / 源码补盲

- `rg -n "namespace" README.md`：L43 与 L672-679 已描述 custom/namespace 方言与冲突语义 → 实现后需同步（见 §7）。
- `docs/codex-responses-lite-wire-analysis.md`：L240 记录 Codex 工具种类（Function/Namespace/ToolSearch/WebSearch/Freeform），§5.1 给出 freeform 权威事件序列，§5.2 论证 `(namespace, name)` 匹配——分析类文档，非规范源，不改；可选补一条内层 custom 观测注记。
- `rg -ln "responses_tools|namespace" .github/`：无命中，CI 除通用告警门禁外无特殊接线。
- `git check-ignore config.json credentials.json`：均被忽略，敏感文件无入库风险。
- 源码核对：丢弃分支位于 responses_tools.rs:155-161；**现有测试无一条覆盖「内层非 function 丢弃」路径**，tasks 2.4 顺带补齐该盲区；测试构造范式为 `collaboration_tool()` helper + `rewrite.namespaces` 断言。
- 配置面/Docker：无 schema 变化，无镜像/编排影响。

## 5. 任务到执行步骤表

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1-1.2 namespace 分支接受 custom 内层 | 改 responses_tools.rs:144-206 的 namespace 分支：custom 子工具走 freeform 降级并双映射注册 | tasks 2.1/2.2 测试绿 | 与 spec 场景冲突时停下对规格 |
| 1.3 冲突检查复用 | 同分支内先走既有两段冲突检查再注册 | tasks 2.3 测试绿 | 400 消息格式变化即违规 |
| 1.4 告警面收窄 | 修改 L156-161 条件为「非 function 且非 custom」；custom 缺名走带 namespace 的 warn | tasks 2.4 测试绿 | 误伤 function 内层即回退 |
| 2.1-2.4 responses_tools 单测 | 仿 `collaboration_tool()` 增加 custom 内层 helper 与 4 组断言 | `cargo test responses_tools` | 无 |
| 2.5 非流式组合测试 | 仿 handlers.rs 既有 freeform 测试，构造 namespaces+freeform 双注册的 rewrite，断言 custom-tool-call + namespace | `cargo test handlers` | 暴露缺口→最小修复并在 tasks 记录 |
| 2.6 流式组合测试 | responses_stream 既有 freeform 测试基础上加 namespace 断言（含增量不透传） | `cargo test responses_stream` | 同上 |
| 2.7 回放往返 | convert_items 测试：带 namespace 的 custom_tool_call → 展平名 assistant 调用 | `cargo test responses` | 无 |
| 3.1-3.3 门禁验证 | 依次 cargo test、cargo check --release --all-targets、openspec validate --all | 命令输出 | 任一失败即停 |
| 3.4 真实流量回归 | Codex app 发含 namespace+custom 的请求；查 pm2 日志与 claude-tap trace | 无丢弃告警、展平名送达、回传形状正确 | 上游拒绝则回设计评审 |

## 6. 工作区现状（执行前必读）

- 脏文件中与本变更重叠：`src/openai/handlers.rs`、`src/openai/responses_stream.rs`、`README.md`（均含用户在制品）；`src/openai/responses_tools.rs` 与 `src/openai/responses.rs` 相对 HEAD 干净。
- 同模块活跃变更 `add-responses-websocket-ingress` 的未跟踪文件（`ws_error.rs`/`ws_ingress.rs`/`ws_transport.rs`）不碰。
- 纪律：不 revert 他人改动；提交仅包含本变更相关文件；config.json/credentials.json 已确认被 .gitignore 忽略。

## 7. 必跑验证

1. `cargo test`（至少 openai 相关模块全绿）
2. `cargo check --release --all-targets`：零新增告警（对变更基线）
3. `openspec validate --all`
4. 真实流量回归（tasks 3.4）：pm2 日志无「内层工具形状非 function」custom 丢弃告警；`工具已送达上游` 含展平名；有回传时核对 custom-tool-call + namespace 形状

## 8. README / AGENTS / spec 同步判断

- README：**需同步**。L43 方言清单与 L672-679 namespace 行为表补「内层 custom 同样降级」一句；实现完成后随代码一起改。
- AGENTS.md：不需要（无纪律/验证命令变化）。
- `spec/`（长期事实）：不需要（模块边界与数据流未变）。
- `openspec/specs/openai-responses/spec.md`：归档时由 delta 合入，实现阶段不手改。

## 9. 停止条件

- 工件缺失/矛盾/状态 blocked（当前无）。
- 发现未写入规格的高风险影响（当前无新增）。
- 工作区出现会被提交的真实凭据（已核验：config.json/credentials.json 被忽略）。
- 无法确定验证命令或剩余风险（当前均可执行）。
- 触发任一 §2 高风险项且无缓解路径时，停止实现并回到设计评审。
