## ADDED Requirements

### Requirement: Admin 可读写 WebSocket 运行时设置

Admin API MUST allow authenticated operators to read and update the WebSocket ingress
settings (`enabled`, `mode`, `max_connections`, first-message timeout, inter-turn idle
timeout, max message bytes, upstream read timeout) without a process restart. Updates MUST
be partial (unspecified fields keep their current values), MUST be persisted to the server
config file, and MUST take effect for subsequent connections immediately. The read endpoint
MUST additionally report the current number of active WebSocket connections. Every update
MUST be logged with old and new values. If persistence fails, the in-memory value MUST
still take effect and the error MUST distinguish "applied but not persisted".

#### Scenario: 读取 WebSocket 设置

- **WHEN** GET `/api/admin/settings/websocket` 且 Admin 已认证
- **THEN** 返回当前全部 WebSocket 设置字段与当前活跃 WS 连接数

#### Scenario: 部分更新并热生效

- **WHEN** PUT 仅携带 `{"enabled": false}`
- **THEN** 其余字段 MUST 保持不变，配置 MUST 落盘，新的 WS upgrade 请求 MUST 立即被拒绝

#### Scenario: 落盘失败区分语义

- **WHEN** 更新成功写入内存但 `save_config` 失败
- **THEN** 内存值 MUST 已生效，响应 MUST 明确区分「已生效未落盘」错误

#### Scenario: 热更新留痕

- **WHEN** 任一 WebSocket 设置被更新
- **THEN** 日志 MUST 记录变更字段的旧值与新值
