# Tasks: add-responses-websocket-ingress

实施前须先完成 openspec-superpowers-bridge（bridge plan 写入 `evidence/`）。
每项任务的完成判据见行内说明；验证纪律遵循 AGENTS.md。

## 1. 依赖与配置块

- [x] 1.1 `Cargo.toml` 的 axum 增加 `ws` feature；`cargo check --release --all-targets` 通过且零新增告警
- [x] 1.2 `src/model/config.rs` 的 `Config` 新增 `websocket: WsSettings` 字段（`#[serde(default)]`，含 enabled/mode/max_connections/client_first_message_timeout_seconds/inter_turn_idle_timeout_seconds/max_message_bytes/upstream_read_timeout_seconds）；旧 config.json（无该块）加载成功的单测
- [x] 1.3 `WsTransportMode` 反序列化：未知值回落 `http_bridge` 并 warn 的单测

## 2. 事件源重构与 SSE/WS parity

- [x] 2.1 从 `create_responses_sse_stream`（`src/openai/handlers.rs`）抽出 `ResponsesEventSource`（产出 `Vec<ResponsesSseEvent>` 批次，保留 keepalive/decoder/finish/fail 语义）
- [x] 2.2 SSE sink 接回事件源，`POST /v1/responses` 既有测试全部保持绿
- [x] 2.3 parity 测试：同一请求输入下，WS sink 收到的事件 JSON 序列 == SSE data 行序列

## 3. prepare 流程抽取

- [x] 3.1 把 `post_responses` 的「解析 → websearch 分支 → prepare → provider」抽为 handler 无关函数，`POST /v1/responses` 行为不变（既有测试绿）

## 4. 模式路由与传输抽象缝

- [x] 4.1 新增 `src/openai/ws_transport.rs`：`WsTransportMode`、`WsTransport` trait、`resolve_mode(&WsSettings)`；模式在握手时解析并随 `WsSessionContext` 冻结
- [x] 4.2 `HttpBridgeTransport` 骨架（会话循环在任务 6 实现）
- [x] 4.3 passthrough 预留分支：`mode=passthrough` 在 upgrade 前返回 501 JSON 错误的集成测试

## 5. 错误分类模块

- [x] 5.1 新增 `src/openai/ws_error.rs`：`WsTurnError { stage, cause, wrote_downstream }`、`ClientClose { status, reason }`；`wrote_downstream=false` 且上游阶段失败才可重试的判定单测
- [x] 5.2 关闭码映射：1008 协议违规 / 1011 内部错误 / 1013 容量 / 1001 关闭与取消的常量与转换单测

## 6. WS ingress handler 与会话循环

- [x] 6.1 `src/openai/ws_ingress.rs`：`GET /v1/responses` 挂载到 `create_openai_routes`（与 POST 同路径按方法分派，auth/cors/body-limit 按既有注释挂齐）
- [x] 6.2 握手准入：非 upgrade → 426；鉴权失败 401；`enabled=false` → 503+Retry-After；准入计数满 → 429+Retry-After（均在 upgrade 前）
- [x] 6.3 首帧契约：超时（默认 30s）/ 非法 JSON / 缺 model → 先写 error 事件再 1008；type 缺省补 `response.create`
- [x] 6.4 turn 循环：response.create → prepare → call_api_stream → 事件经 WS sink 写回；终态后回到等待下一帧（多 turn 复用连接的集成测试）
- [x] 6.5 重叠 `response.create` → error 事件不关连接；`response.cancel` → 停止当前 turn 并回 response.cancelled；`session.update` → 记录 session 级 model 覆盖
- [x] 6.6 连接保护：帧上限（默认 32MB）、turn 间空闲超时（默认 30min，0=关闭）、优雅 shutdown 以 1001 关闭活跃 WS
- [x] 6.7 客户端中途断开：终止 turn、上游流 drop、准入计数归还（单测）

## 7. 热加载与 Admin 端点

- [x] 7.1 `AppState.ws: Arc<RwLock<WsSettings>>` 句柄；会话循环在首帧/turn 边界读最新快照
- [x] 7.2 准入计数器用 AtomicUsize+Notify 实现；`max_connections` 热缩减对新连接生效、不影响存量的单测
- [x] 7.3 `src/admin/`：`GET /api/admin/settings/websocket`（当前值 + 活跃连接数）、`PUT /api/admin/settings/websocket`（部分更新，合并语义照 `update_auth_settings`；写内存 + `update_config_with` + `save_config`；落盘失败时错误区分「已生效未落盘」）
- [x] 7.4 热更新 tracing::info 记录旧→新值；`enabled=false` 不掐断存量会话的集成测试
- [x] 7.5 重启恢复：落盘值重新加载为启动值的单测

## 8. 端点目录与文档

- [x] 8.1 `src/public_api/` 登记 `GET /v1/responses`（live、stream=true、upgrade websocket hint），(method, path) 唯一性测试不破；`dto.rs` 断言更新
- [x] 8.2 README 端点列表同步（启动日志若由目录生成则自动）
- [x] 8.3 `docs/websocket-support-optimization-design.md` 中实现期发现的偏差回写（若有）

## 9. 验证与证据

- [x] 9.1 `cargo check --release --all-targets` 零新增告警（报告告警数）
- [x] 9.2 `cargo test` 全绿（含 parity、热加载、准入、首帧契约、模式路由用例）
- [x] 9.3 本地 websocat/wscat 端到端一轮完整对话（证据写 `evidence/`）
- [x] 9.4 Codex CLI 指向本代理 ws 端点实测一轮真实会话（证据写 `evidence/`；无法执行时写明原因与剩余风险）
- [x] 9.5 spec-compliance-check 报告（`evidence/spec-compliance-report.md`）
- [x] 9.6 openspec-verify-change + verification-before-completion（`evidence/`），`openspec validate --all` 通过
- [x] 9.7 `git status --short` 确认无密钥、无 `.codegraph/` 误入

## 10. 审查后加固（code review findings）

- [x] 10.1 `pump_upstream` 上游读超时锚定最近一次上游 chunk 到达时间：客户端帧（session.update / 重叠 create 等）不得重置计时；集成测试证明客户端持续发帧时卡死 turn 仍按超时终结、连接存活且可再跑一轮 turn
- [x] 10.2 优雅关闭 drain 兜底（`SHUTDOWN_DRAIN_TIMEOUT=10s`）：信号触发后超限未收敛即强制结束，进程不得无限挂起；`drain_backstop` 单测（start_paused 时间控制）
- [x] 10.3 `client_first_message_timeout_seconds` 加 0 保护下限 1s（对齐 `upstream_read_timeout_seconds` 的保护方式）；集成测试证明 0 值误配下新连接仍可完成首个 turn
