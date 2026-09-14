# Proposal: desktop-settings-server-view

桌面版（路线 B，`docs/desktop-gpui-embedded-design.md` §12 里程碑 5）：
设置面板、服务器启停视图、内嵌服务器生命周期与两阶段退出。完成后桌面
应用具备完整功能闭环：装配、存储、凭据管理、运行时设置、内嵌服务、
安全退出。

## Why

- 桌面应用至今没有启动内嵌服务器的入口，「桌面版 = 代理本体 + 管理界面」
  的一体化定位不成立；设置面板也还是占位。
- GPUI 的 `App::shutdown()` 对 `on_app_quit` 回调只等 200 毫秒
  （`gpui-pre` 0.3.4 `app.rs` 的 `SHUTDOWN_TIMEOUT`），直接 quit 会把
  在途请求与活跃 WS 直接掐掉。设计文档 §5.2 的两阶段退出就是为此准备的，
  必须随服务器一起落地。
- change 4 的 `AdminService` 构造时没挂鉴权与 WS 运行时句柄，设置类
  写操作（`update_auth_settings` / `update_ws_settings`）要么不热更新
  运行中的服务器，要么直接报「运行时未挂接」。服务器启动需要构建
  `AppState`，正好把这些句柄一次性接上。

## What Changes

- 根仓：`MultiTokenManager` 新增 `flush_stats()`（退出前强制落盘防抖
  未刷的统计）。其余服务端代码零改动。
- 桌面 `core/server.rs`：`ServerControl` 状态机（Stopped / Starting /
  Running(addr) / Stopping / Failed(msg)），tokio 侧 bind +
  `axum::serve` + graceful shutdown；停止信号经 oneshot 驱动，复用
  CLI 的 ws_shutdown 广播与 10 秒 drain 兜底语义；状态变化与收敛完成
  经 `CoreEvent::ServerStatus` / `ServerDrained` 回传。
- 桌面 `core/`：`CoreHandle` 扩展 server 控制与存储句柄；装配任务在
  `bootstrap` 后一次性 `build_routes`，桌面 `AdminService` 挂上
  `AppState` 的 auth / ws 句柄；装配成功即按配置 `host:port` 自动启动，
  端口冲突就是 bind 失败，进 `Failed` 态。
- 桌面 `quit.rs`：`RequestQuit` action 与两阶段退出协调。Cmd+Q 键绑定
  与窗口关闭拦截（`on_window_should_close`）统一进阶段 1；阶段 1 等
  `ServerDrained`（上限 12 秒）并完成统计落盘、WAL checkpoint，阶段 2
  才 `cx.quit()`。
- 桌面 `ui/server.rs`：服务器视图。状态徽标、当前地址、启动/停止/重启
  按钮、失败原因展示；修改 `host` / `port` 落配置后需重启生效。
- 桌面 `ui/settings.rs`：设置面板。鉴权（requireApiKey + apiKey）、
  代理、默认端点、负载均衡、WebSocket 五个分区，全部经现有
  `AdminService` 设置方法；JSON 导出（凭据/配置，文件保存对话框）与
  手动导入（文件选择对话框，不改名源文件）。
- 事件桥：`CoreEvent` 扩 `ServerStatus(ServerStatus)` / `ServerDrained`；
  标题栏徽标接真实服务器状态，退出中显示退出指示。

## Capabilities

桌面端交互与进程治理，服务端需求行为不变（`flush_stats` 是新增的
落盘触发点，不改变防抖策略与文件内容）。

### New Capabilities

无（桌面交互面，无可被服务端规格化的需求行为）。

### Modified Capabilities

无。

## Impact

- 代码：根仓 `src/kiro/token_manager.rs` 一个 pub 方法；桌面
  `core/`、`quit.rs`、`ui/`、`main.rs`、`bridge.rs`、`store/json_io.rs`
  签名微调。
- 依赖：无新增。`rusqlite` checkpoint 走已有连接；文件对话框用
  `gpui-pre` 的 `prompt_for_new_path` / `prompt_for_paths`。
- 测试：根仓 `flush_stats` 单测；桌面 `ServerControl` 真实 tokio 下
  的启停/端口冲突/收敛测试、两阶段退出纯逻辑测试、设置与服务视图
  headless 渲染测试。冒烟：Xvfb 启动后 `curl` 命中内嵌服务器，退出
  收敛计时。
- 已知取舍：macOS 应用菜单的 Quit 走平台直通路径，本期只在窗口级拦截
  键绑定与关窗，菜单项绕过阶段 1 的缺口留到 change 7（macOS 打包）
  一并处理；`closeToTray` 与开机自启是 change 6 的设置项，本期不出现在
  设置面板。
- 后续：change 6 托盘常驻复用关窗拦截钩子；change 5 的
  `ServerControl` 状态供托盘四态图标直接消费。
