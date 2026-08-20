# Bridge Plan: add-stream-error-propagation

> 生成时间：2026-08-20（openspec-superpowers-bridge）
> change 状态：planning 工件全部 done，非 blocked（`openspec status --change ... --json`）
> 分支：`dev`；工作区仅本 change 工件与分析文档两个未跟踪项，无真实凭据/缓存

## 1. 范围、非目标、关键设计决策

**范围**：Kiro 上游流内硬错误（`Event::Error`；除 `ContentLengthExceededException`
外的 `Event::Exception`）在三条协议链路上的可见性：

| 链路 | 消费点（已逐一核实） |
| --- | --- |
| Anthropic 直流式 `/v1/messages` | `handle_stream_request`（handlers.rs:527）→ `StreamContext::process_kiro_event`（anthropic/stream.rs:634） |
| Anthropic 缓冲流式 `/cc/v1/messages` | `handle_stream_request_buffered`（handlers.rs:1107）→ `BufferedStreamProcessor::process_and_buffer`（stream.rs:1200）→ `finish_and_get_all_events`（stream.rs:1219） |
| Anthropic 非流式（两入口共用） | `handle_non_stream_request`（handlers.rs:664，自有聚合 match，:824 附近） |
| OpenAI Chat 流式 | handlers.rs:238 → `process_kiro_event`（openai/stream.rs:143）；收尾 `finish()`（stream.rs:325） |
| OpenAI Chat 非流式 | 共享 `aggregate()`（openai/handlers.rs:315，调用点 :297） |
| Responses SSE | `ResponsesEventSource`（handlers.rs:1407）→ `process_kiro_event`（responses_stream.rs:168）；既有 `fail()`（responses_stream.rs:577） |
| Responses WS ingress | 同一 `ResponsesEventSource`（ws_ingress.rs:588 起），接线后自动覆盖 |
| Responses 非流式 | `handle_responses_non_stream`（handlers.rs:1536）→ 共享 `aggregate()`（调用点 :1557） |

**非目标**（proposal 已冻结）：透明故障转移（建议 2）、配额重置窗口（建议 3）、
provider 状态码级重试、请求级错误映射、成功路径形态、count_tokens、配置开关。

**关键设计决策**：

1. 共享分类层 `classify_stream_fault` 输出 `StreamFault{code, message}`；
   `ContentLengthExceededException` 保留既有 length/max_tokens 语义（已被测试与
   spec 覆盖，不动）。
2. 首个硬错误生效：各处理器置 `stream_failed`，后续错误仅日志。
3. **缓冲流式路径统一走 SSE 错误事件**（不返回 502）：客户端请求的是 stream=true，
   与直流式保持同一语义，spec 无需为缓冲路径开例外。
4. Responses 复用既有 `fail()` → `response.failed`，只接触发源接线；
   `finish()` 需校验 `failed` 时不再产出 `response.completed`（实现时确认，
   现状 finish 未见 failed 检查，responses_stream.rs:538-560）。
5. 非流式 fault → HTTP 502 + 各自协议错误信封（Anthropic 信封 / OpenAI 信封）。
6. 诊断摘要记错误 code + 次数，不含原始消息（遵循 kiro-eventstream-diagnostics
   安全边界），为建议 2 提供遥测。

## 2. 高风险项

| # | 风险 | 缓解 |
| --- | --- | --- |
| R1 | 收尾路径改动影响成功路径（假回归） | 成功路径 parity 由既有测试把关（如 `test_thinking_only_sets_max_tokens_stop_reason`、openai finish 测试、Responses 序列测试）；任务 6.1 全量 cargo test |
| R2 | `generate_final_events` 返回空破坏「错误后仍要关块」的客户端解析 | 错误分支先产出未闭合块的 `content_block_stop` 再发 error 事件（design §2.2） |
| R3 | Responses `finish()` 在 failed 后仍发 completed | 实现时显式检查 `self.failed`；任务 4.2 测试断言无 completed |
| R4 | 缓冲路径 input_tokens 更正逻辑与错误收尾交互 | 缓冲路径测试覆盖（任务 2.2）；更正逻辑只改 message_start，不动错误事件 |
| R5 | OpenAI 错误 chunk 形状不合客户端预期 | 采用官方惯例：`choices: []` + `error` 对象 + `[DONE]`（参照 Qoder2Api writeStreamAPIError 与 OpenAI 文档形态） |
| R6 | 零新增告警门槛（含 `--no-default-features` CI 腿） | 新增符号均有真实调用点（三协议 + 测试）；任务 6.2 本地两组合都跑 |
| R7 | 错误消息透引入敏感内容 | 仅透传上游 code+message 并加固定前缀；诊断摘要不含消息；任务 5.2 安全测试 |

## 3. CodeGraph 证据

索引：`codegraph status` → ✓ Index is up to date（150 文件 / 2937 节点）。

```text
$ codegraph impact "process_kiro_event" -d 2
src/openai/responses_stream.rs  method process_kiro_event:168
src/openai/stream.rs            method process_kiro_event:143
src/anthropic/stream.rs         method process_kiro_event:634
src/anthropic/stream.rs         method process_and_buffer:1200   ← 缓冲路径消费点

$ codegraph impact "generate_final_events" -d 2
src/anthropic/stream.rs  method generate_final_events:1065 / finish_and_get_all_events:1219 / generate_final_events:451
```

结论：三协议各一个事件处理入口 + 一个缓冲消费点，与源码精读一致；
`process_and_buffer`/`finish_and_get_all_events` 是 codegraph 帮助发现的
设计文档初稿遗漏点，已回写 design.md §2.2 与 tasks.md 2.2。

## 4. rg / 源码补盲（CodeGraph 不覆盖面）

- **CI**：`.github/workflows/warning-gate.yaml:69,76` 两腿
  `cargo check --release --all-targets --locked`（default 与 `--no-default-features`，
  附 `-D warnings`）。feature 面核实：`Cargo.toml` 仅 `native-tls`（TLS 后端），
  不裁剪模块，新代码两组合都须编译干净。
- **debug 工具**：`src/debug.rs:154-205` 对 `Event::Error`/`Exception` 已有穷尽
  match（打印器），不受影响、无需改动。
- **诊断**：`src/kiro/model/events/diagnostics.rs:14-28,79,187`——摘要结构已有
  `unknown_event_count/types` 先例，新增错误 code 计数字段遵循同一形态；
  `observe` 已接收 Error/Exception（diagnostics.rs:137 不做工具统计但计入观察）。
- **配置/Docker/示例凭据**：本变更不触碰配置 schema、Dockerfile、compose、
  `*.example.json`，rg 确认无相关引用需要改。
- **README**：仅 :35 泛述 SSE 支持、:29 提及上游截断类 Issue（与本变更方向一致，
  不冲突），无错误行为的既有承诺，无需同步。
- **工作区安全**：`git status --short` 仅未跟踪的文档与 change 目录；
  无 `config.json`/`credentials.*` 出现在未跟踪/修改列表。

## 5. 任务 → 执行步骤映射

| 任务 | 执行步骤 | 验证与停止点 |
| --- | --- | --- |
| 1.1 分类层 | 新建 `src/kiro/stream_fault.rs`（或并入 events 模块）+ mod 导出 | 编译过；被 ≥3 协议路径引用 |
| 1.2 分类测试 | 同文件 `#[cfg(test)]`：Error/普通 Exception/ContentLength/其他 | `cargo test stream_fault` 绿 |
| 2.1 Anthropic 流式错误分支 | anthropic/stream.rs：错误事件（先关块）+ stream_failed；`generate_final_events` 失败时返回空 | 单元级事件序列断言 |
| 2.2 Anthropic 测试 | 直流式 + 缓冲路径（BufferedStreamProcessor）合成事件测试；内容前/后出错 | 断言 error 事件在、end_turn/message_stop 不在；既有测试不回归 |
| 2.3 Anthropic 非流式 | handle_non_stream_request 聚合记 fault → 502 信封 | 非流式测试绿 |
| 3.1 OpenAI Chat 流式 | openai/stream.rs：error chunk（choices:[] + error 对象）+ Done；finish() 抑制 | chunk 序列断言 |
| 3.2 Chat 流式测试 | 合成事件：错误形状、[DONE] 结尾、无 finish_reason chunk | 既有 finish 测试不回归 |
| 3.3 共享 aggregate 扩展 | Aggregated 增 fault；调用点 :297 → 502 信封 | Chat 非流式测试绿 |
| 4.1 Responses 流式接线 | responses_stream.rs：硬错误 → fail()；finish() 检查 failed 不发 completed | 事件序列断言 |
| 4.2 Responses SSE/WS 测试 | 事件源级测试：response.failed 在、completed 不在；WS 走同一 source | 既有 WS/序列测试不回归 |
| 4.3 Responses 非流式 | 复用 3.3，调用点 :1557 → 502 信封 | 非流式测试绿 |
| 5.1 诊断遥测 | diagnostics.rs：错误 code 列表 + 计数进 summary/log_summary | 摘要含 code、不含消息 |
| 5.2 安全测试 | 构造敏感串 fault，断言渲染输出只含 code+message | 断言绿 |
| 6.1 全量测试 | `cargo test` | 全绿，停止点：任何失败先修后继续 |
| 6.2 告警门禁 | `cargo check --release --all-targets`；再跑 `--no-default-features` 对齐 CI | 两组合零新增告警 |
| 6.3 openspec | `openspec validate --all` | 通过 |

执行顺序建议：1 → 2 → 3 → 4 → 5 → 6（分类层是后续依赖；3.3 先于 4.3 因共享 aggregate）。

## 6. 必跑验证（真实执行后才可声称完成）

```bash
cargo test                                    # 全量测试（含新增与既有 parity）
cargo check --release --all-targets           # 告警门禁准绳，零新增
cargo check --release --all-targets --no-default-features   # 对齐 CI 第二腿
openspec validate --all                       # 规格一致性
git status --short                            # 防止密钥/.codegraph 误入
```

高风险矩阵对应行：协议/SSE → `cargo test` 相关模块（已含）。本变更不涉及真实
上游调用，无需 curl 实测；若实现后需要端到端确认，仅可用合成/mock 上游，
不得动用真实凭据。

## 7. README / AGENTS / spec 同步判断

- **README**：无需同步（不影响启动、构建、部署、API 入口清单；既有 SSE 描述
  不含错误行为承诺）。
- **AGENTS.md**：无需同步（AI 纪律与验证命令不变）。
- **spec/（长期事实）**：无需同步（模块边界与数据流描述仍准确）。
- **openspec/specs/**：本 change 归档时由 `openspec archive` 自动合入
  （新增 `stream-error-propagation`，更新 `openai-responses`），实现阶段不手改主 specs。

## 8. 停止条件

- 成功路径既有测试出现回归且无法在不违背 spec 的前提下修复 → 停下重新评估设计。
- 发现 spec 未覆盖的协议错误渲染歧义（如客户端对 SSE error 事件的处理冲突）→
  先修订 spec 再继续。
- `cargo check` 出现无法零成本消除的新告警 → 按 AGENTS.md 最小范围 allow 并注释，
  否则停下。
- 工作区出现真实凭据文件 → 立即停止并报告。
- 实现中发现 Responses `fail()` 语义与 WS 关闭码存在未记录耦合 → 回到 design
  补异常路径，再实施。
