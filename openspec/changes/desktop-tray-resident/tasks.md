# Tasks: desktop-tray-resident

## 1. 存储：preferences 表

- [x] 1.1 `store/schema.rs`：schema v2 迁移，`preferences
      (key TEXT PRIMARY KEY, value TEXT NOT NULL)`
- [x] 1.2 `store/mod.rs`：`get_preference` / `set_preference`
      （bool 存取，读缺省不写库）
- [x] 1.3 单测：缺省值（未写库时）、写入读出、迁移幂等

## 2. 托盘模块

- [x] 2.1 `Cargo.toml`：`tray-icon = "0.25"`，`default-features = false`
      + `features = ["ksni"]`；`Cargo.lock` 更新
- [x] 2.2 `tray.rs`：`TrayState`（图标四态生成、菜单构建与重建）；
      `TrayIcon::new` 失败降级（记日志，返回 `None`）
- [x] 2.3 事件轮询循环（200ms）：`TrayIconEvent` 单击 → 唤回；
      `MenuEvent` → 显示主窗口 / 启停服务器 / 自启勾选 / 退出
- [x] 2.4 `CoreHandle` 挂托盘句柄；装配完成后创建，状态事件驱动
      菜单重建与图标切换
- [x] 2.5 单测：四态图标 RGBA（22x22、非空、格式）；
      `dbus-run-session` 下真实注册成功与无宿主降级

## 3. 常驻关窗拦截

- [x] 3.1 `main.rs` 关窗拦截：退出中放行；`close_to_tray` 开则
      `minimize_window()` + 返回 `false`；关则进阶段 1
- [x] 3.2 唤回：`activate_window()`（托盘单击与菜单共用）
- [x] 3.3 `auto_start_server` 关时装配后跳过自动启动

## 4. 开机自启

- [x] 4.1 `autostart.rs`：三平台入口的写与删（Linux desktop entry；
      macOS plist 与 Windows 注册表仅编译验证）
- [x] 4.2 内容生成纯函数抽离 + 单测（desktop entry / plist 字段）

## 5. 设置视图

- [x] 5.1 `ui/settings.rs`：「行为」分区（关窗常驻 / 开机自启 /
      自动启动服务器三个 Switch），写开关经 `AdminService` 之外的
      本地路径（preferences 直读），自启开关联动平台入口
- [x] 5.2 headless 渲染测试：行为分区在场

## 6. 验证

- [x] 6.1 桌面门禁零告警 + 全量测试；根仓门禁确认（零根仓改动则
      只跑 `cargo check` 确认）
- [x] 6.2 `openspec validate desktop-tray-resident` 通过
- [x] 6.3 Xvfb 冒烟：关窗常驻 → 进程/窗口/服务器存活 → 唤回动作 →
      托盘「退出」或 Alt+F4 两阶段退出；无 SNI 宿主降级日志可证
- [x] 6.4 回填 `docs/desktop-gpui-embedded-design.md` §6（0.25 ksni、
      最小化常驻、spike 结论落定）
