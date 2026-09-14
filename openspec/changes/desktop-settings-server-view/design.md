# Design: desktop-settings-server-view

桌面版功能闭环的收尾：内嵌服务器生命周期、两阶段退出、设置与服务两个
真实视图。逐决策给出现状、目标实现与取舍。

## D1 ServerControl 状态机

现状：桌面应用没有服务器；CLI 的启停逻辑（`src/main.rs`）与信号处理
耦合，不可复用。

实现（`desktop/crates/kiro-desktop/src/core/server.rs`）：

```text
Stopped ──start──► Starting ──bind 成功──► Running(addr) ──stop──► Stopping ──serve 结束──► Stopped
                       │
                       └──bind 失败──► Failed(错误信息) ──start──►（可重试）
```

- `ServerControl { rt: Handle, inner: Arc<Mutex<Inner>> }`，`Inner` 持
  `status` / `stop_tx: Option<oneshot::Sender<()>>` / `restart_pending` /
  `pending_stop` / `stop_requested` / `loop_active`。
  所有方法只持短锁 + `rt.spawn`，不 await，GPUI 线程可直接调用。
- `start()`：仅 `Stopped` / `Failed` 有效。tokio 侧 bind；失败发
  `ServerStatus(Failed(msg))`（端口冲突就是这一类，不做预检，与设计
  文档 §5.1 一致）；成功则记录 `local_addr`（port 为 0 时取实际端口），
  发 `Running`，随后 `axum::serve(listener, router).with_graceful_shutdown`。
- graceful shutdown future 等 `stop_tx` 的 oneshot：收到后广播
  `ws_shutdown`（活跃 WS 以 1001 关闭），进入 `Stopping`。serve 与
  10 秒 `drain_backstop`（从 `src/main.rs` 平移的兜底逻辑）
  `tokio::select!`，超时记 warn 后强制结束，语义与 CLI 一致。
- 服务循环（`spawn_service_loop`）反复跑 `serve_once`，直到返回
  `None`。收敛广播 `drained` 只在循环真正退出时发一次：重启间隙
  不发，因此「改端口重启」紧接退出时，退出协调不会被重启间隙的
  中间态骗到而提前放行 `cx.quit()`（修复前的竞态窗口）。
- 重启路径锁内直接 `Stopping → Starting`，不经过 `Stopped` 中间态；
  `loop_active` 标志覆盖整个循环生命周期，`start()` 在循环存活时
  拒绝二次启动，`request_stop()` 以它区分「真停止」与「重启中」。
- `request_stop()` 返回 `oneshot::Receiver<()>`（drain 完成回执），
  供两阶段退出等待；`Stopped` / `Failed` 且循环未存活时立即给出
  已完成的回执，循环存活时置 `pending_stop` + `stop_requested` 等
  收敛广播。停止优先于重启：收敛窗口内先停后重启时按停止收敛。

## D2 装配接线与 AdminService 句柄挂接

现状：装配任务（`main.rs`）在 `bootstrap` 成功后构造桌面
`AdminService`，`client_auth` 传 `None`、WS 句柄未挂。
`update_auth_settings` 因此不热更新运行中的鉴权，`update_ws_settings`
直接报「WebSocket 运行时未挂接」。

修法：装配任务里 `bootstrap` 后立即 `build_routes(&b)`，拿到
`(router, app_state)`。桌面 `AdminService` 构造改为：

```rust
AdminService::new_with_runtime(
    b.token_manager.clone(),
    b.endpoint_names.clone(),
    Some(app_state.auth.clone()),
    Some(b.kiro_provider.clone()),
)
.with_ws_runtime(app_state.ws_settings.clone(), app_state.ws_admission.clone())
```

`router` 与 `app_state` 一并放进 `CoreHandle`（`ServerControl::start`
消费 router；`ws_shutdown` 广播句柄来自 `app_state`）。HTTP Admin 路由
若配置了非空 `adminApiKey`，由 `build_routes` 内部自建服务实例，与桌面
实例各持一份余额缓存（change 4 已记录的取舍）。

装配成功后按配置 `host:port` 自动 `start()`（设计文档 §5.1 默认行为）。

## D3 两阶段退出

背景：`gpui-pre` 0.3.4 的 `App::shutdown()` 对 `on_app_quit` 回调只等
`SHUTDOWN_TIMEOUT` = 200ms（`app.rs:78`），超时继续退出。任何「在
quit 回调里等服务器收敛」的方案都不成立。

实现（`desktop/crates/kiro-desktop/src/quit.rs`）：

```text
阶段 1（退出协调，发生在 cx.quit() 之前，上限 12 秒）
  退出请求（RequestQuit action / 窗口关闭）
    → 置 quitting 状态（标题栏徽标「正在退出」，重复请求去重）
    → ServerControl::request_stop() 拿 drain 回执
    → cx.spawn 等待回执，上限 = drain 兜底 10 秒 + 2 秒余量，
      用 cx.background_executor().timer() 与回执 select
    → flush_stats() 强制落盘统计；SQLite WAL checkpoint
阶段 2
    → 发 CoreEvent::ShutdownPrepared，主事件循环调 cx.quit()
```

- `RequestQuit` 用 gpui 的 `actions!` 宏定义；`cx.bind_keys` 把
  `cmd-q` / `alt-f4` 绑到它，覆盖任何默认退出路径。窗口关闭经
  `window.on_window_should_close` 拦截：未退出时启动阶段 1 并返回
  `false`（窗口保留，等阶段 1 完成随进程退出）。change 6 会把这个钩子
  改成「常驻时最小化」，本期先直连阶段 1。
- 阶段 1 完成经事件桥通知主循环而不是异步任务里直接 `cx.quit()`：
  主事件循环持有 `&mut App`，路径唯一，避免多入口竞争。
- 服务器未启动时，回执立即完成，阶段 1 只剩落盘，直接进阶段 2。
- 超时路径：回执等不到也进阶段 2，日志记录（与 `drain_backstop` 的
  强制结束语义一致）。

## D4 服务视图（`ui/server.rs`）

- 状态卡：徽标（四态 + Failed 原因）、当前地址、启动/停止/重启按钮按
  状态机可用性置灰。
- `host` / `port` 输入：经新增的 `AdminService::get_server_settings` /
  `update_server_settings`（根仓小增量：校验 + `update_config_with` +
  `save_config`；写成功提示「重启后生效」并自动触发 restart）。
- 状态经 `CoreEvent::ServerStatus` 推送，视图收事件重拉快照。

## D5 设置视图（`ui/settings.rs`）

分区与数据面（全部复用 change 4 的 `CoreHandle`，同步读在 GPUI 直调、
写操作经 `exec`）：

| 分区 | 读 | 写 |
| --- | --- | --- |
| 鉴权 | `get_auth_settings` | `update_auth_settings`（requireApiKey 开关 + apiKey 输入，脱敏展示） |
| 代理 | `get_proxy_settings` | `update_proxy_settings` |
| 默认端点 | `get_endpoint_settings` | `update_endpoint_settings`（注册端点单选） |
| 负载均衡 | `get_load_balancing_mode` | `set_load_balancing_mode`（priority / balanced 单选） |
| WebSocket | `get_ws_settings` | `update_ws_settings`（enabled 开关 + 数值项，展示活跃连接数） |
| 数据 | `credential_count` / `has_config` | JSON 导出（`prompt_for_new_path` + `json_io::export_*`）；手动导入（`prompt_for_paths` + `json_io::import_*`，不改名源文件，与首启导入区分） |

`closeToTray` 与开机自启是 change 6 的设置项，本期不放。

## D6 根仓增量

- `MultiTokenManager::flush_stats()`：`save_stats_debounced` 的防抖可能
  把最后一批统计留在内存，退出前需要强制落盘。实现为 `save_stats` 的
  无条件公开包装。
- `AdminService::get_server_settings` / `update_server_settings`：host
  非空、port 合法的校验，落盘走 `save_config`（SQLite / JSON 后端透明）。
  与其他设置方法同款错误分类。

## D7 事件桥扩展

`CoreEvent` 增：`ServerStatus(ServerStatus)`（含 Failed 原因）、
`ServerDrained`（保留，供未来托盘/其他消费者）、`ShutdownPrepared`。
`ServerStatus` 单独成文件（`core/server.rs`）并 derive `Clone` /
`Debug` / `PartialEq`，供视图与测试直接断言。

## 验证策略

- 根仓：`flush_stats` 与 server settings 的单测；
  `cargo check --release --all-targets` 零告警 + 全量 `cargo test`。
- 桌面：`ServerControl` 真实 tokio 测试（启动命中、端口冲突进 Failed、
  stop 回执与 drain 收敛、restart 重绑）；设置与服务视图的 headless
  渲染测试；两阶段退出协调的超时/回执路径单测。
- 冒烟（Xvfb）：临时数据目录启动 → `curl` 内嵌服务器 `/v1/models` →
  改端口重启 → 再次命中 → 退出收敛日志计时。

## 回滚

根仓两个增量方法独立可回滚；桌面侧单提交回滚不影响 change 4。
