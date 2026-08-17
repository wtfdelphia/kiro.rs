# Proposal: add-responses-websocket-ingress

为 kiro-rs 增加 **OpenAI Responses WebSocket ingress**：让客户端可以按 OpenAI
Responses WebSocket 协议（`GET /v1/responses` + `Upgrade: websocket`）接入，
代理内部把每个 turn 翻译成对既有上游 HTTP/SSE 链路的一次调用（sub2api 术语里的
**http_bridge** 模式），从而复用现有转换核与事件状态机。

同时冻结**透传（passthrough）扩展框架**（模式路由 + 传输抽象缝，当前仅预留）与
**配置化开关 + 热加载**（复用既有 Admin 热更新模式，不重启生效）。

## Why

- Codex CLI / 新式 Agent 客户端正在向 WebSocket 迁移；Codex `models.json` 存在
  `prefer_websockets` 元数据（`docs/codex-responses-lite-wire-analysis.md:253`），
  kiro.rs 目前未消费，ws 模式客户端无法接入。
- kiro.rs 的 Responses 支持只有 HTTP/SSE 一条传输层（`POST /v1/responses`），
  详见 `docs/multi-protocol-api-design.md`。
- 设计依据：`docs/websocket-support-optimization-design.md`（v2，基于 sub2api
  CodeGraph 分析 + 双方源码精读）。sub2api 用 `http_bridge` 把「客户端 WS、
  上游无 WS」这一约束成熟落地，kiro.rs 的约束与其完全同构。
- Kiro 上游是 HTTP + AWS event-stream，**没有上游 WS**，因此本期只能做
  http_bridge；但透传是真实未来需求（上游 WS 化 / 级联外部 WS 上游 /
  Realtime 音频等协议保真），需要现在就冻结抽象缝，避免未来重构会话层。
- 项目既有热更新模式（`Arc<RwLock<AuthRuntime>>` + Admin API + `save_config`
  落盘，见 `src/admin/service.rs:1300`）不覆盖 WS 行为参数；WS 开关必须配置化
  且可热加载，否则调参需重启。

## What Changes

### 新增能力：openai-responses-websocket（新 spec）

- `GET /v1/responses` WebSocket ingress：握手准入、首帧契约、多 turn 会话循环、
  http_bridge turn 执行、错误事件化、关闭码语义、连接保护。
- 事件源重构：`create_responses_sse_stream`（`src/openai/handlers.rs:1323`）
  拆为 `ResponsesEventSource` + SSE/WS 双 sink，SSE 行为不变，parity 测试把关。
- 模式路由与透传预留：`WsTransportMode`（http_bridge / passthrough）+
  `WsTransport` trait 缝；passthrough 本期返回 501，未知 mode 回落并告警。
- 配置化开关 + 热加载：`websocket` 配置块 + `AppState.ws` 运行时句柄，
  语义矩阵见 design.md §6。

### 扩展能力：admin-runtime-settings（MODIFIED/ADDED）

- 新增 `GET /api/admin/settings/websocket` 与 `PUT /api/admin/settings/websocket`：
  读取（含活跃连接数）/ 部分更新 / 落盘 / 热生效。

### 扩展能力：public-api-catalog（ADDED）

- 注册表登记 `GET /v1/responses`（upgrade websocket）为 live 条目，
  启动日志与 Admin 展示自动派生，防止端点事实漂移（P4 复发防护）。

## Impact

- **Specs**: 新增 `openai-responses-websocket`；扩展 `admin-runtime-settings`、
  `public-api-catalog`。
- **源码**: `src/openai/`（handlers / responses_stream / 新 ws_ingress、ws_transport、
  ws_error 模块）、`src/model/config.rs`、`src/anthropic/middleware.rs`（AppState）、
  `src/admin/`（service / handlers / types / router）、`src/public_api/`。
- **依赖**: `Cargo.toml` 的 axum 增加 `ws` feature（引入 tokio-tungstenite）。
- **配置 schema**: config.json 新增 `websocket` 块（`#[serde(default)]` 兼容旧文件）。
- **文档**: README 端点列表同步。
- **不影响**: Anthropic `/v1/messages`、`/cc/v1`、Chat Completions、`POST /v1/responses`
  的对外行为（SSE parity 为合并前置）。

## Non-Goals（本期明确不做）

- **透传 WS 的实现**：仅冻结模式路由与 trait 缝；passthrough 返回 501。
  真正实现另立 change（前提：上游具备 WS 能力或需级联外部 WS 上游）。
- 上游 WS 连接池 / ctx_pool、Redis 粘连、多账号调度：单上游平台、单实例，无对应物。
- `previous_response_id` 续链：维持显式拒绝（`src/openai/responses.rs:25`）；
  本地 TurnStateStore 属 P1，另行评估。
- permessage-deflate 压缩：axum 内置 ws 不暴露压缩选项，收益有限，保持零额外依赖面。
- Admin UI 的 WS 设置页面：后续项，不阻塞 P0。
- 文件监听式热加载：项目无 `notify` 类机制，热加载通道仅 Admin API。

## Assumptions

- 客户端遵循 OpenAI Responses WebSocket 事件形状（以 sub2api 已验证行为为基线，
  v1/v2 beta 暂按同一事件集处理，见设计文档 §5）。
- 单实例部署、单 API Key 鉴权（`require_api_key` 可关）。
- Kiro 上游维持 HTTP + AWS event-stream，无上游 WS 可用。

## Success Criteria

- `GET /v1/responses` 能通过 ws 客户端完成一次完整多轮对话（websocat + Codex CLI 实测）。
- `POST /v1/responses` SSE 行为零变化（parity 测试全绿）。
- 热加载生效：Admin 改 `enabled`/`mode`/`max_connections` 后新连接立即按新语义，
  存量连接不受 `enabled=false` 影响；重启恢复落盘值。
- `mode=passthrough` 握手前返回 501；未知 mode 回落 http_bridge 并告警。
- `cargo check --release --all-targets` 零新增告警（AGENTS.md 硬门槛）。
- `openspec validate --all` 通过。

## Risks

对应设计文档 §8 R1–R9，重点：

- **协议漂移**：OpenAI Responses WS 随 Codex 版本演进（v1/v2 beta）。缓解：以
  sub2api 验证行为为基线，日志记录 beta 头。
- **`previous_response_id` 拒绝**可能影响部分客户端。缓解：P0 先观察，P1 评估本地续链。
- **热加载语义误解**（期望 `enabled=false` 掐断存量会话）。缓解：语义矩阵写入
  Admin 响应（返回活跃连接数）与 README。
- **SSE/WS 双 sink 重构引入行为漂移**。缓解：parity 测试作为合并前置，SSE 测试保持绿。
- **透传预留值与配置笔误**。缓解：未知 mode 回落并告警，passthrough 显式 501。
