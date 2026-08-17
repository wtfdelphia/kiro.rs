## Context

后端契约已冻结（见 `add-responses-websocket-ingress`）：

- `GET /api/admin/settings/websocket` → `WsSettingsResponse`：enabled / mode / maxConnections / clientFirstMessageTimeoutSeconds / interTurnIdleTimeoutSeconds / maxMessageBytes / upstreamReadTimeoutSeconds / activeConnections（camelCase）。
- `PUT /api/admin/settings/websocket` → `UpdateWsSettingsRequest`：全部字段可选的部分更新；未知 mode 返回 400；先写内存热生效，再落盘，落盘失败返回「已在内存生效但未落盘」错误文案。
- enabled=false 时新 upgrade 被 503 拒绝，存量会话自然终态（`src/model/config.rs:214` 注释）。
- inter_turn_idle_timeout_seconds=0 表示不启用该超时（tasks 6.6 语义）。

Admin UI 现状：设置面板为单面板多分区 + 统一保存按钮（`settings-panel.tsx`，286 行），已含 proxy / endpoint / auth / 身份指纹分区，组件复用 Radix 封装（Input/Select/Switch）与 TanStack Query，成功反馈为 toast「设置已保存并热更新」。

## Goals / Non-Goals

**Goals:**

- 设置面板内完成 WS 配置的读、改、存闭环，语义与后端部分更新/热生效一致。
- 字段约束在前端拦截，避免无意义 4xx；后端错误原样透出。

**Non-Goals:**

- 不改后端 API、配置 schema 或 WS ingress 行为。
- 不做活跃连接数的轮询/实时刷新（打开面板与保存后刷新即可）。
- 不做 passthrough 模式的额外配置面（它是预留项）。

## Decisions

- **D1：并入现有 SettingsPanel，不新建面板。** 与 proxy/endpoint/auth 分区同级的一个 section，复用统一保存按钮与 toast 流程。备选：独立 WS 面板——拒绝，设置项总量不大，拆面板增加导航成本。
- **D2：API 层加 `getWebsocketSettings` / `updateWebsocketSettings`** 于 `admin-ui/src/api/settings.ts`，类型入 `admin-ui/src/types/`，命名与字段对齐后端 camelCase。
- **D3：max_message_bytes 以 MB 呈现。** Input 数字（step 1，min 1），加载时 `bytes / 1048576`，提交时乘回。经 API 直设的非整 MB 值在 UI 显示为四舍五入值，下次保存会取整——可接受，注释说明。备选：裸字节输入——拒绝，33554432 这类数值对运维不友好。
- **D4：mode 用 Select 固定两项**，passthrough 选项文案标注「预留，turn 将以 501 响应」。后端对未知 mode 有 400 兜底，前端不需要自由文本。
- **D5：前端校验只做保存拦截**（非负整数、MB ≥ 1），不做跨字段约束；后端无额外数值校验，保持两侧一致。
- **D6：活跃连接数只读展示**，加载时机 = 面板打开 + 保存成功后重新 GET；不加轮询。

## Risks / Trade-offs

- [MB 取整导致与 API 直设值漂移] → 设计内接受；UI 注释标明单位与取整行为。
- [保存是顺序 PUT 链（proxy→endpoint→auth→身份→websearch），任一失败即断链并统一报错] → WS 作为新环节加入同一条链，行为与现状一致；错误消息经 `extractErrorMessage` 原样透出，可区分是哪个分区失败。
- [vitest 覆盖面有限（现仅 kam-import-dialog 一例）] → 纯换算/校验逻辑抽成可测纯函数并配单测；UI 交互以 `pnpm build` + 手动回归把关。

## Migration Plan

纯前端增量，随 admin-ui 构建嵌入发布；无数据迁移。回滚即回退二进制内嵌资源。
