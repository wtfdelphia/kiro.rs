# API 级回归证据（task 4.2 后端契约部分）

> 日期：2026-08-17　对象：pm2 运行实例（/home/openclaw/kiro.rs，172.20.66.24:18990）

脚本经 Admin API 与裸 socket WS 握手验证，密钥从部署配置读取、未打印：

| 步骤 | 操作 | 结果 |
| --- | --- | --- |
| 1 | `GET /api/admin/settings/websocket` | 200，8 个 camelCase 字段与 UI 类型逐一对齐（含 activeConnections） |
| 2 | WS upgrade（enabled=true） | `HTTP/1.1 101 Switching Protocols` |
| 3 | `PUT enabled=false` | 200「WebSocket 设置已更新并落盘」 |
| 4 | WS upgrade（disabled） | `HTTP/1.1 503 Service Unavailable`（符合 spec：仅拦新连接） |
| 5 | `PUT enabled=true` | 200「已更新并落盘」 |
| 6 | WS upgrade（恢复后） | `101 Switching Protocols` |
| 7 | 终态核对 | enabled=true、mode=http_bridge、activeConnections=0，无状态残留 |

UI 侧（面板字段加载、保存链、MB 换算呈现）随新内嵌资源部署后目检确认；
构建门禁已过：`pnpm build`（tsc -b && vite build）与 `pnpm test`（21/21）。
