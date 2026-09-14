# Tasks: desktop-settings-server-view

## 1. 根仓增量

- [x] 1.1 `MultiTokenManager::flush_stats()`：无条件落盘统计（防抖未刷部分）
- [x] 1.2 `AdminService::get_server_settings` / `update_server_settings`：
      host/port 校验 + `update_config_with` + `save_config`
- [x] 1.3 根仓单测：`flush_stats` 幂等、server settings 校验与落盘

## 2. ServerControl

- [x] 2.1 `core/server.rs`：`ServerStatus` 枚举与 `ServerControl` 状态机
      （start / request_stop / restart / status）
- [x] 2.2 tokio 侧 bind + `axum::serve` + graceful shutdown；ws_shutdown
      广播 + 10 秒 `drain_backstop` 兜底（语义平移自 `src/main.rs`）
- [x] 2.3 状态变化发 `CoreEvent::ServerStatus`；收敛完成发 `ServerDrained`
- [x] 2.4 测试：真实 tokio 下启动命中、端口冲突进 Failed、stop 回执、
      restart 重绑

## 3. 装配接线

- [x] 3.1 装配任务 `build_routes`，桌面 `AdminService` 挂 auth / ws 句柄；
      `CoreHandle` 携带 `ServerControl` 与存储句柄
- [x] 3.2 装配成功按配置 `host:port` 自动启动
- [x] 3.3 `CoreEvent` 扩 `ServerStatus` / `ServerDrained` / `ShutdownPrepared`

## 4. 两阶段退出

- [x] 4.1 `quit.rs`：`RequestQuit` action（`actions!` 宏）+
      `QuitCoordinator`（quitting 状态、去重、12 秒上限）
- [x] 4.2 `cmd-q` / `alt-f4` 键绑定；`on_window_should_close` 拦截
- [x] 4.3 阶段 1 落盘：`flush_stats` + WAL checkpoint；阶段 2 经
      `ShutdownPrepared` 进 `cx.quit()`
- [x] 4.4 标题栏退出中徽标

## 5. 视图

- [x] 5.1 `ui/server.rs`：状态卡 + 启停/重启按钮 + host/port 修改重启生效
- [x] 5.2 `ui/settings.rs`：鉴权 / 代理 / 端点 / 负载均衡 / WebSocket 五分区
- [x] 5.3 数据分区：JSON 导出（保存对话框）与手动导入（选择对话框，
      不改名源文件）
- [x] 5.4 标题栏服务器徽标接真实状态；视图事件订阅刷新
- [x] 5.5 headless 渲染测试：服务视图五态文案与地址展示、设置六分区
      渲染（`gpui-kit/test-support` 的 `find()` 观察 + a11y 标签断言）

## 6. 验证

- [x] 6.1 根仓门禁零告警 + 全量测试
- [x] 6.2 桌面门禁零告警 + 全量测试
- [x] 6.3 `openspec validate desktop-settings-server-view` 通过
- [x] 6.4 Xvfb 冒烟：启动 → `curl /v1/models` → 服务视图改端口 8181
      「应用并重启」→ 新地址命中且旧端口释放、SQLite config 落盘 →
      Alt+F4 真实两阶段退出（落盘 + WAL checkpoint → Stopping → Stopped
      → ServerDrained → quit）
