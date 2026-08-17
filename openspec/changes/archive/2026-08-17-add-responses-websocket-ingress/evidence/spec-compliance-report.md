# spec-compliance-check 报告：add-responses-websocket-ingress

日期：2026-08-14　审查范围：本 change 的实现 diff（工作区尚有 `analyze-kiro-eventstream-diagnostics`
change 的未提交改动，已在范围核对中识别并排除，不属于本审查对象）。

## 六维评估

| 维度 | 状态 | 结论 |
| --- | --- | --- |
| Scope | PASS | 改动限于 proposal Impact 清单：`src/openai/`（3 个新 ws 模块 + handlers/responses_stream 重构 + mod 路由）、`src/model/config.rs`、`src/anthropic/middleware.rs`（AppState 句柄）、`src/admin/`（4 文件）、`src/public_api/`、`src/main.rs`、README、config.example.json、Cargo.toml（axum ws feature + tokio-tungstenite dev-dep）。未触碰 Anthropic `/v1/messages`、`/cc/v1`、Chat Completions 对外行为。 |
| Design | PASS | 与 change design.md 一致；两处实现偏差已回写 design.md 与 `docs/websocket-support-optimization-design.md` 附录 C（run_session 无 first_frame 参数；准入计数器无 Notify，CAS+RAII 守卫）。 |
| Scenarios | PASS | 三个 delta spec 的全部 Scenario 有实现+测试锚点，逐项见下表。 |
| Project Rules | PASS | `cargo check --release --all-targets` 零告警（AGENTS.md 硬门槛）；未提交任何真实凭据（e2e 用显式 fake key）；OpenSpec change 先行；Skills 门禁按序执行。 |
| Verification | PASS | 本会话真实运行：cargo check（0 告警）、cargo test（全绿，见下）、openspec validate --all（23 通过）、release 二进制 e2e（10/10）。无 SKIPPED 隐瞒；9.4 Codex CLI 实测无法执行，原因见 evidence/e2e-ws-smoke.md 与 tasks.md 9.4。 |
| README/AGENTS Sync | PASS | README 端点表 + Admin 设置表已同步；启动日志由 catalog 派生自动含 GET /v1/responses；config.example.json 加 websocket 块；AGENTS.md 无需变更（未改验证命令/纪律本身）。 |

## Scenario → 证据映射

### openai-responses-websocket

| Requirement / Scenario | 实现位置 | 测试证据 |
| --- | --- | --- |
| 握手准入：非 upgrade → 426 | `ws_ingress::get_responses_ws`（from_request_parts 失败分支） | `openai::tests::test_responses_get_without_upgrade_gets_426`；e2e `non_upgrade_get_426` |
| 握手准入：未鉴权 upgrade 前 401 | 既有 auth_middleware layer（openai/mod.rs 挂载） | `test_responses_get_requires_auth_before_upgrade`；e2e `auth_missing_key_401` |
| 握手准入：超限 429+Retry-After | `WsAdmission::try_acquire` CAS | `ws_integration::capacity_full_rejected_429_and_released_on_disconnect` |
| 握手准入：enabled=false 503+Retry-After | handler 快照检查 | `disabled_rejected_503_before_upgrade`；e2e `disabled_new_conn_503` |
| 首帧超时 → error+1008 | `run_http_bridge_session` 首帧窗口 | `first_frame_timeout_closes_1008` |
| 首帧非法 JSON → error+1008 | `dispatch_frame` | `invalid_json_first_frame_error_then_1008` |
| 首帧缺 model → error+1008 | `dispatch_frame` | `missing_model_first_frame_error_then_1008` |
| type 缺省按 response.create | `dispatch_frame` unwrap_or | `missing_type_defaults_to_response_create` |
| 多 turn 复用连接 | turn 循环回 wait_client_frame | `multi_turn_reuses_connection` |
| turn 失败不毁会话（wrote_downstream 分界） | `run_turn`/`pump_upstream` | `turn_failure_keeps_connection_alive`；e2e turn=error 后连接存活 |
| 重叠 create → error 不关连接 | `handle_in_turn_frame` | `overlapping_create_rejected_without_closing` |
| cancel → response.cancelled | `handle_in_turn_frame` + `ResponsesStreamContext::cancel` | `cancel_stops_turn_and_connection_survives` |
| SSE/WS 事件序列等价 | 同一 `ResponsesEventSource` 双 sink | `ws_parity_tests::ws_frames_match_sse_data_lines` |
| model 回显客户端请求名 | `start_responses_stream_turn` echo_model（D9 复用） | `session_update_model_override_used_by_create`（断言 response.model） |
| passthrough 握手前 501 | `resolve_transport` | `passthrough_rejected_501_before_upgrade` |
| 未知 mode 回落 http_bridge + warn | `WsTransportMode` 手动 Deserialize | `model::config::tests::unknown_ws_mode_falls_back_to_http_bridge` |
| mode 建连冻结 | `WsSessionContext.mode` 握手时写入 | 结构性保证（P0 仅 http_bridge 可运行，热改 mode 无可观察行为差异；记为剩余风险） |
| 超大帧拒绝 | codec max_message_size（握手冻结）+ 每帧快照复查 | `oversized_frame_rejected` |
| turn 间空闲超时 → 1001 | 会话循环 idle 边界 | `inter_turn_idle_timeout_closes_1001` |
| 优雅 shutdown → 1001 | broadcast + main.rs graceful_shutdown | `graceful_shutdown_closes_active_ws_1001` |
| enabled=false 不杀存量 | 准入只拦新连接 | `hot_disable_does_not_kill_existing_session` |
| max_connections 热缩减 | CAS 用最新快照上限 | `ws_transport::tests::admission_hot_shrink_only_affects_new_connections` |

### admin-runtime-settings（WebSocket 设置端点）

| Scenario | 测试证据 |
| --- | --- |
| 读取设置 + 活跃连接数 | `admin::service::tests::test_ws_settings_get_returns_defaults_and_active_count`；e2e `admin_get_ws_settings` |
| 部分更新并热生效 + 落盘 | `test_ws_settings_partial_update_merges_and_persists`；e2e disable→503→reenable |
| 落盘失败区分「已生效未落盘」 | `test_ws_settings_applied_but_not_persisted_distinguished` |
| 热更新留痕（旧→新） | `update_ws_settings` tracing::info changes 字段（实现保证；日志断言未单测，tracing 基线无断言设施） |
| 未知 mode 拒绝 400 | `test_ws_settings_update_mode_and_unknown_mode_rejected` |
| 重启恢复落盘值 | `model::config::tests::ws_settings_roundtrip_restart_recovery` |

### public-api-catalog（GET 条目）

| Scenario | 测试证据 |
| --- | --- |
| GET 条目 live + stream=true + WS hint | `catalog::tests::test_responses_websocket_entry_live_with_upgrade_hint` |
| live 条目可路由（426 非 404） | `public_api::routes_test::test_live_endpoints_are_mounted` |
| POST 条目不受影响 | catalog POST 条目未改；既有断言全绿 |
| (method, path) 唯一性 | `catalog::tests::test_method_path_unique`（GET/POST 同 path 不同 method 合法） |

## 验证命令与结果（本会话真实运行）

```text
cargo check --release --all-targets   → Finished, 0 warnings（终态）
cargo test --release                  → 783 passed; 0 failed（含 21 个新增 WS 相关测试）
openspec validate --all               → 23 passed, 0 failed
release 二进制 e2e（python websockets 16.0，替代未安装的 websocat）→ 10/10 通过
```

## 发现项

1. （INFO）工作区含 `analyze-kiro-eventstream-diagnostics` change 的未提交改动
   （src/anthropic/handlers.rs、src/anthropic/stream.rs、src/kiro/model/events/*、
   src/openai/stream.rs 及新 diagnostics/metering/reasoning 模块等）。本 change 的
   handlers.rs/responses_stream.rs 重构建立在其之上，未回退、未混改其逻辑；归档/提交时需按 change 拆分归属。
2. （WARN，可接受）mode 建连冻结无行为级集成测试：P0 只有 http_bridge 可运行，
   冻结语义由结构保证（`WsSessionContext.mode` 握手写入后不再读设置）。未来实现
   passthrough 时需补「热改 mode 不影响存量连接」的行为测试。
3. （WARN，可接受）热更新 tracing 留痕未做日志断言（项目无 tracing 断言设施），
   由代码路径保证。
4. （INFO）e2e 环境无真实上游凭据：turn 以 error 事件收尾（上游 400），意外完整演练了
   「未写出事件→换凭据重试一次→error 事件→连接存活」链路（server.log 留痕）。

## 剩余风险

- Codex CLI 真机会话未实测（无可用凭据），见 9.4；协议形状以 sub2api 验证行为为基线，
  v1/v2 beta 按同一事件集处理（design §5）。
- 上游读超时以「任意活动重置窗口」实现（宽松），客户端活跃可延长死流存活时间；
  影响面小，已在代码注释说明。

## 总体状态

**PASS（含 2 项可接受 WARN）**
