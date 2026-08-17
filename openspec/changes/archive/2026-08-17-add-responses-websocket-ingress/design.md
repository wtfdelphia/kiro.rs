# Design: add-responses-websocket-ingress

完整设计论证见 `docs/websocket-support-optimization-design.md`（v2）。本文是
OpenSpec 实现视角的浓缩：当前实现、目标设计、数据流、异常路径、回滚、验证策略。

## 1. 当前实现

- `POST /v1/responses`（`src/openai/handlers.rs:1120 post_responses`）：
  `ResponsesRequest` 归一（`previous_response_id` 显式拒绝，`src/openai/responses.rs:25`）
  → `to_chat_request_json` → `prepare`（模型映射 / thinking / tool 改写）→
  `KiroProvider.call_api_stream`（HTTP + AWS event-stream）→ `EventStreamDecoder`
  → `ResponsesStreamContext` 状态机（`src/openai/responses_stream.rs:55`）→
  `ResponsesSseEvent::to_sse_string()` SSE 输出（`create_responses_sse_stream`，
  `handlers.rs:1323`）。
- 路由装配 `src/openai/mod.rs:28 create_openai_routes`：`/v1/responses`、
  `/v1/chat/completions`，各挂 `auth_middleware`（注意 merge 不传播 layer）。
- 鉴权：`x-api-key` / `Authorization: Bearer`（`src/common/auth.rs:14`），
  常量时间比较。
- 热更新事实：`AppState.auth: Arc<RwLock<AuthRuntime>>`
  （`src/anthropic/middleware.rs:28`）+ Admin `update_auth_settings`
  （`src/admin/service.rs:1300`）三段式：写内存句柄 + `update_config_with` +
  `save_config`。中心配置 `src/model/config.rs:23 Config`（serde，load/save）。
- 无 WS 路由、axum 未开 `ws` feature、无文件监听式热加载。

## 2. 目标设计

### 2.1 模块划分

```
src/openai/
  ws_transport.rs   # WsTransportMode、WsTransport trait、模式路由 resolve_mode()
  ws_ingress.rs     # GET /v1/responses handler、握手准入、首帧契约、HttpBridge 会话循环
  ws_error.rs       # WsTurnError 分类（stage + wrote_downstream）、关闭码映射
  responses_stream.rs（重构）# ResponsesEventSource 与 sink 解耦
  handlers.rs（重构）# prepare 流程抽为 handler 无关函数
```

### 2.2 关键抽象

- `WsTransportMode { HttpBridge, Passthrough }`：serde snake_case；未知值加载时
  回落 `HttpBridge` 并 warn。
- `WsTransport` trait：`run_session(socket, WsSessionContext)`（实现期偏差回写：
  首帧契约在 http_bridge 会话循环内处理，passthrough 又在 upgrade 前被拒，
  无需把 `first_frame` 作为 trait 参数）。
  P0 只实现 `HttpBridgeTransport`；`PassthroughTransport` 为预留分支，选中时
  **upgrade 之前**返回 501 JSON 错误（不接受连接后再关）。
- `ResponsesEventSource`：输入 `reqwest::Response` + `ResponsesStreamContext`，
  输出 `Vec<ResponsesSseEvent>` 批次（保留 keepalive / decoder / finish/fail 语义）。
  SSE sink 序列化 `to_sse_string()`（现状不变）；WS sink 将事件 JSON（SSE data
  部分）作为 text 帧发送；Keepalive 不下发 WS（协议级 ping/pong 替代）。
- `WsSettings`（中心 `Config` 新增 `websocket` 块，`#[serde(default)]`）：
  `enabled` / `mode` / `max_connections` / `client_first_message_timeout_seconds` /
  `inter_turn_idle_timeout_seconds`（0=关闭）/ `max_message_bytes` /
  `upstream_read_timeout_seconds`。`AppState.ws: Arc<RwLock<WsSettings>>`。
  **JSON 命名约定**：中心 `Config` 与 admin types 均 `rename_all = "camelCase"`
  （`src/model/config.rs:22`、`src/admin/types.rs:11`），config.json 与 Admin API
  字段为 camelCase（`maxConnections` 等），Rust 字段保持 snake_case。
- 准入计数器：`AtomicUsize` CAS + RAII 守卫（Drop 归还）（**不用** Semaphore，
  其容量运行时不可变，挡不住 `max_connections` 热改）。实现期偏差回写：
  准入是即时拒绝语义（无排队等待路径），原稿的 `Notify` 无真实使用点，未引入。

### 2.3 握手与会话时序

```
GET /v1/responses
 ├─ 非 upgrade → 426 JSON
 ├─ auth_middleware（既有）→ 401
 ├─ enabled=false → 503 + Retry-After（upgrade 前）
 ├─ 准入计数 ≥ max_connections → 429 + Retry-After
 ├─ resolve_mode(snapshot) → passthrough → 501（upgrade 前）
 ├─ WebSocketUpgrade（max_message_size 取快照）
 └─ 会话循环（模式冻结）
     ├─ 首帧超时窗口 → 违规先写 error 事件再 1008 关闭
     ├─ response.create → prepare → call_api_stream → 事件写回 WS
     ├─ response.cancel → 停止当前 turn，回 response.cancelled
     ├─ session.update → 记录 session 级 model 覆盖
     ├─ 终态后等待下一帧；turn 间空闲超时
     └─ shutdown → 1001 GoingAway
```

## 3. 数据流与影响面

- 新链路：WS text 帧 → 首帧校验 → `prepare`（复用）→ 上游（复用）→
  `ResponsesStreamContext`（复用）→ WS sink。
- 重构影响面：`create_responses_sse_stream` 的调用方只有 `handle_responses_stream`；
  抽出事件源后 SSE 输出必须逐事件 parity（同一输入，WS 帧 JSON 序列 == SSE data
  行序列）。
- `post_responses` 内「解析 → websearch 分支 → prepare → provider」抽为
  handler 无关函数，HTTP/WS 共用；web_search 代执行分支在 WS 下同样适用。
- 不影响：`/v1/messages`、`/cc/v1`、`/v1/chat/completions`、Admin 既有端点。

## 4. 异常路径

| 场景 | 行为 |
| --- | --- |
| 首帧超时 / 非法 JSON / 缺 model | 先写 `error` 事件 JSON，再 1008 PolicyViolation 关闭 |
| 重叠 `response.create`（前 turn 未终态） | 回 `error` 事件（不关连接） |
| 上游失败且**未写出任何事件** | 可换凭据重试一次（复用 MultiTokenManager 错误处理）；仍失败 → `response.failed` 事件，连接存活 |
| 上游失败且**已写出事件** | 只发 `error` / `response.failed` 事件，连接存活（`wrote_downstream` 为唯一分界） |
| 客户端中途断开 | 终止 turn，上游流 drop（reqwest stream drop 即取消）；计数归还 |
| 帧超 `max_message_bytes` | 1009/1008 关闭（按 tungstenite 语义） |
| turn 间空闲超时 | 1001 关闭 |
| 进程优雅关闭 | 全部活跃 WS 以 1001 GoingAway 关闭 |
| 容量丢失（热缩容不触发，仅自然回落） | —（存量连接不受 max_connections 热改影响） |

## 5. 回滚

- 运行时：`websocket.enabled=false` 热回滚（只拦新连接，不杀存量），对齐 sub2api
  `force_http` 语义。
- 配置：旧 config.json 无 `websocket` 块时按 `#[serde(default)]` 取默认值
  （enabled=true、mode=http_bridge）；若需彻底关闭，Admin PUT 或改文件后重启。
- 代码：axum `ws` feature 为增量依赖，revert 提交即可完全退回；SSE 链路经
  parity 测试保证无行为变化。

## 6. 热加载语义矩阵

| 字段 | 新连接 | 存量连接 |
| --- | --- | --- |
| `enabled=false` | upgrade 前 503 | 不影响，自然终态 |
| `mode` | 新握手按新值解析 | 冻结（建连时确定） |
| `max_connections` | 按新上限准入 | 不影响 |
| 超时 / 帧上限 | 下一次首帧/turn 边界读最新快照 | 同左 |

Admin 端点：`GET /api/admin/settings/websocket`（当前值 + 活跃连接数）、
`PUT /api/admin/settings/websocket`（部分更新，字段合并照 `update_auth_settings`
的 `unwrap_or(current)`；`save_config` 失败时内存值已生效但错误信息区分
「已生效未落盘」；每次更新 tracing::info 记录旧→新值）。

## 7. 验证策略

- 零告警硬门槛：`cargo check --release --all-targets`。
- parity 测试：同一请求输入，SSE data 序列 == WS 帧 JSON 序列。
- 单测：首帧超时 / 非法 JSON / 缺 model / 重叠 create / cancel / idle 超时 /
  未知 mode 回落 / passthrough 501 / 热加载语义（enabled 不杀存量、mode 冻结、
  max_connections 热缩减）/ Admin GET/PUT（部分更新、落盘、活跃连接数）。
- 集成：`tokio-tungstenite` 测试客户端打真实 Router（tower oneshot + upgrade）。
- 端到端：本地 websocat 手工一轮完整对话；Codex CLI 指向本代理 ws 端点实测
  （结果写入 `evidence/`）。
- 端点目录：`src/public_api/` 测试断言含 `GET /v1/responses` live 条目且
  (method, path) 唯一性不破。
- 流程门禁：spec-compliance-check、openspec-verify-change、
  verification-before-completion（见 AGENTS.md Skills 门禁）。
