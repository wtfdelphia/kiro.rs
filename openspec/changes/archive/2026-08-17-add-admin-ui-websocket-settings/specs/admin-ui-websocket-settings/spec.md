## Purpose

在 Admin UI 设置面板中提供 WebSocket ingress 的可视化配置：让运维者无需手改配置文件或直接调用 Admin API，即可开关 WS 接入、调整连接与超时参数，并即时看到活跃连接数。所有修改沿用后端既有的部分更新、写盘与热生效语义。

## ADDED Requirements

### Requirement: WebSocket 分区呈现当前配置

设置面板 MUST 包含 WebSocket 分区，打开面板时 MUST 从 `GET /api/admin/settings/websocket` 加载当前值并填充各字段；活跃连接数 MUST 以只读形式展示，不可编辑。

#### Scenario: 打开面板加载当前值

- **WHEN** 用户打开设置面板
- **THEN** WebSocket 分区各字段 MUST 显示后端当前值（enabled、mode、max_connections、两项超时、max_message_bytes、upstream_read_timeout_seconds）
- **AND** 活跃连接数 MUST 显示为只读

#### Scenario: 加载失败不破坏面板

- **WHEN** WebSocket 设置读取失败
- **THEN** 面板其余分区 MUST 仍可使用
- **AND** WebSocket 分区 MUST 呈现错误提示而非空白

### Requirement: 字段编辑与约束

enabled MUST 为开关控件；mode MUST 为 http_bridge / passthrough 二选一的选择控件，且 passthrough 选项 MUST 标注为预留能力；数值字段 MUST 仅接受非负整数；turn 间空闲超时 MUST 允许 0 并说明 0 表示不启用该超时。max_message_bytes MUST 以 MB 为单位呈现并在提交时换算为字节。

#### Scenario: passthrough 标注预留

- **WHEN** 用户查看 mode 选项
- **THEN** passthrough MUST 带有「预留」标注，说明当前选择会以 501 响应 turn

#### Scenario: 非法数值被拦截

- **WHEN** 用户在数值字段输入负数或非整数
- **THEN** 保存 MUST 被拦截并提示字段错误
- **AND** MUST NOT 发起 PUT 请求

#### Scenario: MB 与字节换算

- **WHEN** 后端返回 max_message_bytes=33554432
- **THEN** UI MUST 显示为 32 MB
- **AND** 用户改为 64 MB 保存时，PUT 请求体 MUST 携带 67108864

### Requirement: 保存走部分更新与热生效反馈

保存 MUST 以 PUT 部分更新提交 WebSocket 分区字段；成功时 MUST 给出与既有分区一致的「已保存并热更新」反馈；失败时 MUST 原样透出后端错误消息（包括「已在内存生效但未落盘」的区分）。

#### Scenario: 保存成功热生效

- **WHEN** 用户修改字段并保存，后端返回成功
- **THEN** UI MUST 显示保存成功反馈
- **AND** MUST NOT 提示需要重启

#### Scenario: 后端错误原样透出

- **WHEN** 后端返回「WebSocket 设置已在内存生效但未落盘」类错误
- **THEN** 错误提示 MUST 包含该消息内容，不得吞掉或替换为通用文案

### Requirement: 关闭总开关的运维语义可见

总开关关闭的后果 MUST 在 UI 上有说明：新的 WebSocket upgrade 将被拒绝，存量会话不受中断。避免运维者误以为关闭会切断进行中的会话。

#### Scenario: 关闭开关附带后果说明

- **WHEN** 用户查看 enabled 开关
- **THEN** 附近 MUST 有文案说明关闭仅拒绝新连接、存量会话自然终态
