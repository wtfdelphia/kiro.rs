## 1. API 层与类型

- [x] 1.1 `admin-ui/src/types/` 新增 `WebsocketSettings`（含 activeConnections 只读字段）与 `UpdateWebsocketSettingsRequest` 类型，字段对齐后端 camelCase
- [x] 1.2 `admin-ui/src/api/settings.ts` 新增 `getWebsocketSettings` / `updateWebsocketSettings`，错误走既有 axios 拦截器

## 2. 设置面板 WebSocket 分区

- [x] 2.1 `settings-panel.tsx` 加载时 GET 并填充：enabled（Switch）、mode（Select，passthrough 标注预留）、四个数值字段（Input）、活跃连接数只读展示
- [x] 2.2 max_message_bytes 以 MB 呈现：加载换算（/1048576）、提交换算（×1048576），注释说明取整行为
- [x] 2.3 enabled 开关附文案：关闭仅拒绝新 upgrade，存量会话自然终态；turn 间空闲超时注明 0 = 不启用
- [x] 2.4 保存链加入 WS 环节（部分 PUT），成功沿用「设置已保存并热更新」toast，失败经 `extractErrorMessage` 原样透出（含「已生效未落盘」）

## 3. 校验与测试

- [x] 3.1 保存前校验：数值字段非负整数、MB ≥ 1，非法时拦截并字段级提示，不发 PUT
- [x] 3.2 MB/字节换算与校验逻辑抽为纯函数，配 vitest 单测（对齐既有 `pnpm test` 设施）

## 4. 验证与收尾

- [x] 4.1 `pnpm build`（tsc -b && vite build）与 `pnpm test` 通过
- [x] 4.2 真实回归：Admin UI 打开设置面板核对字段加载；切换 enabled 保存后验证 WS upgrade 准入变化（关闭时新连接 503、开启恢复）；活跃连接数显示正确
      已于部署新二进制后（pm2 pid 383415，08:51 UTC 重启）完成 14/14 项回归，见 `evidence/ui-regression.log` 与截图 `evidence/ui-websocket-section.png`
- [x] 4.3 README 如有 Admin UI 设置项清单则同步（无清单则不改）
