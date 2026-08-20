# Spec Compliance Report: add-stream-error-propagation

审查时间：2026-08-20（实现后、归档前）
审查范围：工作区未提交 diff（`git diff` 7 个已修改文件 + 新增
`src/kiro/stream_fault.rs` 与本 change 目录）。

复核记录：同日按 skill 重跑一轮合规审查（diff 重扫 + hunk 级复核），
结论维持 PASS；复核增量检查：

- 授权边界扫描：diff 不含 `Cargo.toml`、`Cargo.lock`、`admin-ui/`、
  `.github/`、`scripts/`、Docker 相关文件，未触碰停止条件所列非授权范围。
- hunk 级抽查：`src/anthropic/stream.rs` 的 `SseStateManager` 改动为
  `generate_final_events` 内关块循环原样提取为 `close_open_blocks()`
  （design §2.2 要求错误分支复用，行为不变）；`src/anthropic/handlers.rs`
  tests 头部 +2 行为新测试所需导入（`crc32`、`HashMap`）。均无越界重构。
- 验证结果与首轮一致（复核期 `cargo test` 824 全绿、`openspec validate --all`
  25 项通过、`git status` 干净、密钥扫描 0 命中）。

## 六维审查

| 维度 | 结论 | 说明 |
| --- | --- | --- |
| Scope | PASS | 改动仅覆盖 proposal Impact 列出的模块：`src/kiro/`（新分类层 + 诊断扩展）、`src/anthropic/{stream,handlers}.rs`、`src/openai/{stream,handlers,responses_stream}.rs`。无非目标触碰（provider 重试、count_tokens、配置 schema、成功路径字节形态均未动）。proposal Impact 提到的 `src/openai/responses.rs` 实际未改——该文件是请求侧 Responses→Chat 转换模块，无上游事件聚合路径，design §1.2 表格该行定位有误，design §2.3 已正确定位到 handlers.rs 共享 `aggregate()`，见发现项 F1 |
| Design | PASS | 与 design.md 逐节核对：§2.1 `StreamFault{code,message}` + `classify_stream_fault`（ContentLength 排除）+ 首个生效（`stream_failed`/`failed` 标志）✓；§2.2 Anthropic 先关块再发 SSE `error`（api_error，code 编入 message），`generate_final_events` 失败时返回空，缓冲路径复用同一 StreamContext ✓；§2.3 OpenAI 错误 chunk（`choices:[]` + `error{message,type:server_error,code}`）+ `[DONE]`，finish 抑制 ✓；§2.4 Responses 复用 `fail()` → `response.failed`，`finish()` 增加 `failed` 早退（R3），WS/SSE 共用事件源 ✓；§2.5 诊断只记 code+次数不记消息 ✓ |
| Scenarios | PASS | `stream-error-propagation` 全部 7 个 Requirement 的场景均有实现与测试对应（映射见下表）；`openai-responses` MODIFIED 新增场景「流内错误事件构成上游失败」由 responses_stream 与共用事件源测试覆盖 |
| Project Rules | PASS | OpenSpec change 先行 ✓；零新增告警（两组合本地实测）✓；无真实凭据（全部合成帧测试）✓；提交信息纪律将在提交阶段走 caveman-commit |
| Verification | PASS | 仅报告本会话真实运行命令：`cargo test --release` 824 passed / 0 failed；`cargo check --release --all-targets` 与 `--no-default-features` 同参数组合均 0 warning；`openspec validate --all` 25 项通过。无 SKIPPED |
| README/AGENTS Sync | PASS | 不影响启动、构建、部署、测试命令与 API 入口清单（错误路径行为修复），proposal Impact 已说明无需同步，审查确认属实 |

### Scenario → 证据映射（stream-error-propagation）

| Requirement / Scenario | 实现与测试证据 |
| --- | --- |
| 统一分类：Error 分类为硬错误 | `src/kiro/stream_fault.rs` `classify_stream_fault`；测试 `error_event_is_hard_fault` |
| 统一分类：无映射异常分类为硬错误 | 同上；测试 `generic_exception_is_hard_fault` |
| 统一分类：ContentLength 保留语义 | 分类层排除 + 三协议保留映射；测试 `content_length_exception_is_not_fault`、`test_content_length_exception_keeps_max_tokens_semantics`、`test_content_length_exception_keeps_length_semantics`、`test_content_length_exception_keeps_completed_semantics`、`test_aggregate_content_length_is_not_fault`、`content_length_exception_not_counted_as_stream_error` |
| 统一分类：首个硬错误生效 | anthropic `stream_failed`、openai `stream_failed`、responses `failed` 早退；测试 `test_stream_fault_first_wins_and_suppresses_later_events`、`test_fault_first_wins_and_suppresses_later_content`、`test_first_fault_wins_subsequent_faults_suppressed` |
| Anthropic：内容之后出错 | `emit_stream_fault`（先 `close_open_blocks` 再 error 事件）；测试 `test_stream_fault_after_content_emits_error_and_suppresses_final`（含缓冲路径 `test_buffered_stream_fault_suppresses_final`） |
| Anthropic：内容之前出错 | 测试 `test_stream_fault_before_any_content` |
| Anthropic：非流式错误信封 | `aggregate_non_stream_events` fault → 502 + `ErrorResponse::new("api_error", ...)`（handlers.rs:695-704）；聚合级测试 3 个（handlers.rs:1503/1514/1529），502/api_error 为 `ErrorResponse` 既有形状 |
| OpenAI Chat：流式错误 chunk | `error_chunk` + `OpenAiSseChunk::Done`；测试 `test_fault_emits_error_chunk_then_done`、`test_fault_suppresses_normal_finish` |
| OpenAI Chat：非流式错误信封 | `aggregate()` fault 字段 → handlers.rs:300 `OpenAiError::Upstream` → 502 + api_error 信封；测试 `test_aggregate_detects_hard_fault` 等，信封映射由 `src/openai/error.rs` 既有测试 `test_status_and_type_mapping` 覆盖 |
| Responses：SSE 路径失败事件 | responses_stream.rs `process_kiro_event` 硬错误分支 + `finish()` failed 早退；测试 `test_hard_error_after_content_emits_failed_without_completed`、`test_hard_error_before_content_emits_failed`、`test_unmapped_exception_is_hard_error` |
| Responses：WS ingress 同语义 | 共用 `ResponsesEventSource`；事件源级测试 `in_stream_fault_propagates_through_shared_source`（handlers.rs ws_parity_tests，原始 AWS event-stream 帧驱动 feed→finish 全链） |
| Responses：非流式错误信封 | handlers.rs:1666-1669 fault → `OpenAiError::Upstream` → 502；共享 `aggregate()` 测试覆盖 |
| 成功路径不变 | 全量 824 测试通过（含既有成功路径 parity 测试：`ws_frames_match_sse_data_lines`、openai finish 系列等） |
| 安全：渲染不泄漏敏感信息 | 渲染仅由 `client_message()`（固定前缀 + code + message）构成；测试 `test_fault_rendering_exposes_only_code_and_message`（OpenAI 与 Responses 各一，断言 error 对象字段白名单）、`test_fault_rendering_is_determined_by_code_and_message_only`（Anthropic 结构等式）、`client_message_composes_only_code_and_message`、`client_message_falls_back_when_message_empty` |
| 遥测：诊断摘要记录分类 | `EventStreamDiagnostics.stream_error_codes`；测试 `counts_stream_error_codes_without_messages`、`content_length_exception_not_counted_as_stream_error`（均断言序列化输出不含原始消息） |
| 无真实凭据可验证 | 全部测试使用合成 Event / 合成 AWS event-stream 帧（crc32 帧编码器），零网络依赖 |

## 发现项

- **F1（低，无需处理）**：design.md §1.2 将 Responses 非流式聚合定位到
  `src/openai/responses.rs`，实际该文件为请求侧转换模块；真实聚合调用点在
  `src/openai/handlers.rs` `handle_responses_non_stream`（design §2.3 已正确记录
  handlers.rs:1557）。实现按 §2.3 执行，无行为影响。
- **F2（低，已知）**：tasks.md 2.2 文本写 `BufferedStreamProcessor`，实际类型名
  `BufferedStreamContext`；实现与测试针对真实类型，不影响覆盖。
- **F3（低，可接受剩余风险）**：三协议非流式的「fault → 502 信封」映射未做
  端到端 handler 级测试（handler 需要真实上游 reqwest 响应，代码库无该测试缝隙）；
  按代码库既有粒度以共享聚合函数测试 + 错误信封映射既有测试组合覆盖。

## 证据路径

- 桥接计划：`openspec/changes/add-stream-error-propagation/evidence/bridge-plan.md`
- 本报告：`openspec/changes/add-stream-error-propagation/evidence/spec-compliance-report.md`

## 剩余风险

- 客户端对协议错误事件（SSE error / error chunk / response.failed）的处理能力
  未在真实客户端验证（design §5 已给出按协议回退渲染层的预案）。
- 上游错误消息本身若含敏感内容会原样进入客户端错误 message——这是 design §4
  的既定决策（仅原样编入，不附加上下文），诊断摘要侧已隔离。

## 总体状态

**PASS**（发现项均为低级别文档/粒度事项，无阻塞）
