## Why

`add-responses-websocket-ingress` 已落地后端全链路：`websocket` 配置块、热加载运行时句柄、`GET/PUT /api/admin/settings/websocket`（部分更新 + 落盘 + 热生效）。但 Admin UI 没有任何 WebSocket 设置入口——运维者要开关 WS ingress 或调参，只能手改 config.json 或直接调 Admin API，与 proxy / endpoint / auth / 身份指纹等既有设置项"面板内可视化热更新"的体验不一致。

## What Changes

- Admin UI 设置面板新增 WebSocket 分区：总开关（enabled）、传输模式选择（http_bridge / passthrough，后者标注预留）、连接与超时参数（max_connections、client_first_message_timeout_seconds、inter_turn_idle_timeout_seconds、max_message_bytes、upstream_read_timeout_seconds）编辑，以及只读的活跃连接数展示。
- 保存沿用既有统一保存语义：PUT 部分更新、写盘并热生效、toast 反馈；错误消息原样透出（含「已生效未落盘」区分）。
- 新增 admin-ui API 客户端函数与类型（`GET/PUT /settings/websocket`）。

## Capabilities

### New Capabilities

- `admin-ui-websocket-settings`: Admin UI 设置面板中 WebSocket ingress 的可视化配置与热更新操作面。

### Modified Capabilities

（无——后端 Admin API 契约由 `add-responses-websocket-ingress` 的 admin-runtime-settings delta 覆盖，本变更不改动 API 行为）

## Impact

- 代码：`admin-ui/src/api/settings.ts`、`admin-ui/src/types/`、`admin-ui/src/components/settings-panel.tsx`；后端零改动。
- 验证：`pnpm build`、`pnpm test`（admin-ui）；真实回归经 Admin UI 切换 enabled 并核对 WS upgrade 准入行为（关闭时新连接 503）。
- 依赖：无新增依赖（复用 Radix Switch/Select/Input 与 TanStack Query）。
