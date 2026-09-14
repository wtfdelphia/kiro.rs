# 取证记录：tray-icon 0.25 ksni 真实注册与降级（任务 2.5）

日期：2026-09-15
方法：探针 `src/bin/spike-tray-register.rs`（无窗口，纯 `TrayIconBuilder::build`）
假宿主：`/tmp/smoke6/fake_sni_watcher.py`（dbus-python 暴露
`org.kde.StatusNotifierWatcher`，`IsStatusNotifierHostRegistered=true`）

## 场景 1：无 SNI 宿主（空 D-Bus 会话）

```text
$ dbus-run-session -- target/release/spike-tray-register
DEGRADED: failed to register to the StatusNotifierWatcher: org.freedesktop.DBus.Error.ServiceUnknown: The name org.kde.StatusNotifierWatcher was not provided by any .service files
```

`build()` 返回 `Err`，不 panic、不挂起；主程序 `Tray::init` 把它降级为
无图标实例（日志「托盘创建失败，降级为无托盘」），与 design D1 一致。

## 场景 2：SNI 宿主在场（真实注册）

```text
$ dbus-run-session -- bash -c '/usr/bin/python3 fake_sni_watcher.py & \
    sleep 1.5; target/release/spike-tray-register'
REGISTER OK
```

watcher 侧日志：

```text
WATCHER: registered org.kde.StatusNotifierItem-2823203-1
```

ksni 自管工作线程完成 D-Bus 服务注册（`org.kde.StatusNotifierItem-<pid>-<n>`）
并调用宿主 `RegisterStatusNotifierItem`，全程无事件循环要求，验证
design D1「创建对调用线程无事件循环约束」。

## 结论

任务 2.5 两条路径取证完成：真实注册成功、无宿主降级可控。
Xvfb 冒烟无 SNI 宿主，走降级路径属预期（场景 1 同款）。
