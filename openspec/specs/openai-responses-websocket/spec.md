## Purpose

Serve `GET /v1/responses` as an OpenAI Responses WebSocket ingress. Clients keep one
WebSocket connection across multiple turns; each `response.create` turn is executed by
bridging into the existing upstream HTTP/SSE pipeline (http_bridge mode). The transport
mode is resolved at handshake and frozen for the connection's lifetime; passthrough
(WS→WS relay) is reserved by the mode router but not implemented in this change.

## Requirements

### Requirement: WS 端点与握手准入

The system MUST serve `GET /v1/responses` with `Upgrade: websocket` under the same
client authentication as `POST /v1/responses`. All rejection paths MUST happen before
the WebSocket upgrade completes. Non-upgrade requests MUST receive HTTP 426 with a JSON
error body. When the WebSocket feature is disabled or the admission limit is reached,
the request MUST be rejected with an appropriate status and a `Retry-After` header
without upgrading.

#### Scenario: 非 upgrade 请求被拒绝

- **WHEN** 客户端对 `GET /v1/responses` 发起普通 HTTP 请求（无 `Upgrade: websocket`）
- **THEN** 响应 MUST 为 426，body 为 JSON 错误，且 MUST NOT 升级为 WebSocket

#### Scenario: 未鉴权请求在升级前被拒绝

- **WHEN** `requireApiKey` 开启且请求未携带有效 client apiKey
- **THEN** 响应 MUST 为 401，且 MUST NOT 升级为 WebSocket、MUST NOT 触达上游

#### Scenario: 连接数超限被拒绝

- **WHEN** 活跃 WS 连接数已达到 `websocket.max_connections`
- **THEN** 新请求 MUST 收到 429 与 `Retry-After` 头，且 MUST NOT 升级

#### Scenario: 功能关闭时拒绝新连接

- **WHEN** `websocket.enabled` 为 false
- **THEN** 新请求 MUST 收到 503 与 `Retry-After` 头，且 MUST NOT 升级

### Requirement: 首帧契约

After the upgrade, the first client frame MUST arrive within the configured first-message
timeout, MUST be valid JSON, and MUST carry a non-empty `model`. A missing `type` MUST be
treated as `response.create`. Violations MUST be reported by writing one `error` event JSON
frame first, then closing with WebSocket status 1008 (Policy Violation).

#### Scenario: 首帧超时

- **WHEN** 升级后在 `client_first_message_timeout_seconds` 内未收到首帧
- **THEN** 连接 MUST 以 1008 关闭

#### Scenario: 首帧非法 JSON

- **WHEN** 首帧不是合法 JSON
- **THEN** 服务端 MUST 先写一条 `error` 事件帧，再以 1008 关闭

#### Scenario: 首帧缺少 model

- **WHEN** 首帧 JSON 缺少非空 `model` 字段
- **THEN** 服务端 MUST 先写一条 `error` 事件帧，再以 1008 关闭

#### Scenario: type 缺省按 response.create 处理

- **WHEN** 首帧 JSON 无 `type` 字段但包含 `model` 与 `input`
- **THEN** 服务端 MUST 按 `response.create` 处理该帧

### Requirement: 多 turn 会话循环与连接存活

Each `response.create` frame MUST execute one turn. After a terminal event
(`response.completed` / `response.failed` / `response.incomplete` / `response.cancelled`),
the connection MUST remain open and wait for the next client frame. A turn failure MUST
NOT close the connection once any event has been written downstream; the failure MUST be
expressed as an `error` or `response.failed` event. An overlapping `response.create`
while a turn is still active MUST be rejected with an `error` event without closing the
connection. A `response.cancel` frame MUST stop the active turn and be answered with a
`response.cancelled` event. A `session.update` frame MUST update the session-level model
used by subsequent `response.create` frames that omit `model`.

#### Scenario: 多 turn 复用同一连接

- **WHEN** 客户端在同一连接上先后发送两个 `response.create`
- **THEN** 两个 turn MUST 各自产出完整事件序列直至终态，且连接在第一个终态后 MUST NOT 关闭

#### Scenario: turn 失败不毁掉会话

- **WHEN** 某 turn 已向客户端写出事件后上游失败
- **THEN** 服务端 MUST 发送 `error` 或 `response.failed` 事件，且连接 MUST 保持打开

#### Scenario: 重叠 create 被拒绝

- **WHEN** 上一个 turn 未到终态时客户端再发 `response.create`
- **THEN** 服务端 MUST 回一条 `error` 事件，且 MUST NOT 关闭连接或并发执行第二个 turn

#### Scenario: cancel 停止当前 turn

- **WHEN** turn 进行中客户端发送 `response.cancel`
- **THEN** 服务端 MUST 停止该 turn 的事件输出并回 `response.cancelled` 事件

### Requirement: http_bridge 事件等价

In http_bridge mode, events delivered as WebSocket text frames MUST carry exactly the JSON
payloads that `POST /v1/responses` would emit as SSE `data:` lines for the same normalized
request (same event sequence, same content), except that SSE keepalive comments have no
WebSocket counterpart. The `model` field in emitted events MUST echo the client-requested
model name.

#### Scenario: 与 SSE 的事件序列等价

- **WHEN** 同一归一化请求分别经 `POST /v1/responses`（SSE）与 WS ingress（http_bridge）执行
- **THEN** WS 收到的事件 JSON 序列 MUST 与 SSE data 行序列逐项一致（不含 keepalive）

#### Scenario: 模型名回显

- **WHEN** 客户端请求模型为别名或映射源模型
- **THEN** WS 事件中的 `model` MUST 为客户端请求的模型名，MUST NOT 泄漏上游模型名

### Requirement: 模式路由与透传预留

The transport mode MUST be resolved from the latest `WsSettings` snapshot at handshake and
frozen for the connection's lifetime. `mode=http_bridge` MUST select the bridging
transport. `mode=passthrough` MUST be rejected before the upgrade with HTTP 501 and a JSON
error in this change (implementation is reserved for a future change). Unknown mode values
in configuration MUST fall back to `http_bridge` with a warning at load time.

#### Scenario: passthrough 预留分支显式拒绝

- **WHEN** `websocket.mode` 为 `passthrough` 且客户端请求 upgrade
- **THEN** 响应 MUST 为 501 JSON 错误，且 MUST NOT 升级

#### Scenario: 未知 mode 回落

- **WHEN** 配置中 `websocket.mode` 为无法识别的值
- **THEN** 加载时 MUST 回落为 `http_bridge` 并输出 warn 日志

#### Scenario: mode 建连冻结

- **WHEN** 连接建立后 Admin 热更新 `websocket.mode`
- **THEN** 已建立连接 MUST 继续使用建连时解析的模式

### Requirement: 连接保护与关闭码

The ingress MUST enforce a maximum WebSocket message size, an inter-turn idle timeout
(disabled when configured as 0), a per-turn upstream read timeout, and graceful shutdown
semantics. The upstream read timeout MUST count only the absence of upstream data; client
frames MUST NOT reset it. The first-message timeout MUST be clamped to a minimum of one
second so a misconfigured 0 cannot reject new connections instantly. Close codes MUST follow:
1008 for protocol violations, 1011 for internal errors, 1013 for capacity loss, 1001 for
shutdown or cancellation. On graceful shutdown, all active WebSocket connections MUST be
closed with 1001. Graceful shutdown MUST be bounded by a drain deadline: if in-flight
requests have not converged when the deadline elapses, the process MUST force-exit.

#### Scenario: 超大帧被拒绝

- **WHEN** 客户端帧超过 `websocket.max_message_bytes`
- **THEN** 连接 MUST 被关闭，且服务端 MUST NOT 继续处理该帧

#### Scenario: turn 间空闲超时

- **WHEN** 上一 turn 终态后超过 `inter_turn_idle_timeout_seconds`（非 0）未收到新帧
- **THEN** 连接 MUST 以 1001 关闭

#### Scenario: 优雅关闭

- **WHEN** 服务收到 shutdown 信号
- **THEN** 所有活跃 WS 连接 MUST 以 1001 关闭

#### Scenario: 上游读超时不被客户端帧重置

- **WHEN** 上游流停滞，且客户端在 `upstream_read_timeout_seconds` 内持续发送帧（如 `session.update`）
- **THEN** 当前 turn MUST 仍按上游读超时以 `response.failed` 事件终结，连接 MUST 存活

#### Scenario: 首帧超时 0 值保护

- **WHEN** `client_first_message_timeout_seconds` 被配置为 0
- **THEN** 新连接 MUST 按 1 秒下限等待首帧，MUST NOT 升级后立即关闭

#### Scenario: 优雅关闭 drain 兜底

- **WHEN** shutdown 信号触发后，超过 drain 兜底时限在途请求仍未收敛
- **THEN** 进程 MUST 强制结束，MUST NOT 无限挂起

### Requirement: 热加载运行时语义

Changes to `WsSettings` MUST take effect without restart. `enabled=false` MUST reject new
upgrades but MUST NOT terminate established sessions. `max_connections` changes MUST apply
to subsequent admission decisions without affecting established connections. Timeout and
limit fields MUST be re-read from the latest snapshot at each first-frame or turn boundary.

#### Scenario: enabled=false 不杀存量会话

- **WHEN** 存在活跃 WS 会话时 Admin 将 `enabled` 置为 false
- **THEN** 存量会话 MUST 继续运行至自然终态，而新 upgrade 请求 MUST 被 503 拒绝

#### Scenario: max_connections 热缩减

- **WHEN** Admin 将 `max_connections` 调低到小于当前活跃连接数
- **THEN** 存量连接 MUST 不被断开，新连接 MUST 按新上限被拒绝，直至活跃数自然回落
