# Bridge Plan: desktop-settings-server-view

门禁：`openspec-superpowers-bridge`。

- change：`desktop-settings-server-view`
- 分支：`dev-desktop`（HEAD `1226274`，工作区干净）
- 状态：`openspec validate` 通过，state 非 blocked

## 一、范围 / 非目标

范围：`ServerControl` 状态机与内嵌服务器、两阶段退出协调、装配句柄
挂接、服务与设置两个真实视图（含 JSON 导入导出）、根仓 `flush_stats`
与 server settings 增量。

非目标：托盘常驻与 `closeToTray`（change 6）；打包与三平台流水线
（change 7）；macOS 应用菜单 Quit 的平台拦截（随 change 7 处理）；
设置修改对「已启动服务器」的免重启热生效（host/port 明确为重启生效，
其余分区本就热更新）。

## 二、关键设计决策（design.md 摘要）

- D1 `ServerControl`：状态机在 tokio 侧，GPUI 侧方法只发信号不 await；
  restart 收敛在 tokio 侧
- D2 装配：`build_routes` 一次，桌面 `AdminService` 挂 auth/ws 句柄，
  修复 `update_auth_settings` / `update_ws_settings` 的挂接缺口
- D3 两阶段退出：`SHUTDOWN_TIMEOUT` 200ms 约束决定一切等待必须发生在
  `cx.quit()` 之前；回执 + `timer` select，上限 12 秒
- D4/D5 视图：服务视图消费状态机事件；设置视图全走现有 `AdminService`
- D6 根仓：仅两个增量方法，不动现有行为

## 三、风险与对策

| 风险 | 对策 |
| --- | --- |
| `on_window_should_close` 返回 false 后窗口状态残留 | 返回 false 即启动阶段 1，阶段 2 随进程退出兜底；headless 测试覆盖拦截返回值 |
| drain 永不收敛挂住退出 | 12 秒上限 + warn 日志，与 CLI `drain_backstop` 同语义 |
| 端口 0 / 实际地址不一致 | 记录 `listener.local_addr()`，状态事件带真实地址 |
| 两份 `AdminService` 余额缓存 | 沿用 change 4 取舍（同一锚点、TTL 300 秒），记录在案 |

## 四、验证口径

- 根仓：`cargo check --release --all-targets` 零告警 + `cargo test --release`
- 桌面：`RUSTFLAGS="-D warnings" cargo check --release --all-targets
  --locked` + `cargo test --release`
- 冒烟：Xvfb + 临时数据目录，`curl` 内嵌服务器、端口重启、退出收敛计时

## 五、规格映射

`skip_specs: true`。内嵌服务器与退出协调是桌面进程治理，设置面复用
现有服务端行为，不改变任何已规格化的需求。
