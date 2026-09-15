# 冒烟记录：关窗常驻与两阶段退出（任务 6.3）

日期：2026-09-15
环境：Xvfb（:99 / :98 / :97），无窗口管理器、无 SNI 宿主；
`VK_ICD_FILENAMES` 强制 GL 后端（llvmpipe），脚本见 `/tmp/smoke6/`。
二进制：`target/release/kiro-desktop`（本 change 构建产物）。

关窗事件用 xlib 直发 `WM_DELETE_WINDOW` 到带 `WM_PROTOCOLS` 的应用
窗口（`wmfind.py`）。首轮曾误发到外层容器窗口（`0x400000`，无
协议），导致「常驻」断言假阳性；修正选择逻辑后重跑。

## 场景 1：`close_to_tray` 缺省开（常驻）

```text
PASS: 进程存活（装配期）
PASS: 内嵌服务器响应 /v1/models (HTTP 401)
PASS: 托盘无宿主降级日志在场
窗口: 0x200001
PASS: 关窗后进程存活（常驻）
PASS: 关窗后服务器仍响应 (HTTP 401)
PASS: 关窗后窗口未销毁（带协议的窗口仍在）
PASS: 关窗未触发退出流程
PASS: Alt+F4 后进程已退出（两阶段退出生效）
PASS: 日志含两阶段退出完整轨迹
PASS=9 FAIL=0
```

推理链：OFF 场景证明拦截回调生效且偏好被读到（见下）；本场景进程
存活即拦截返回了 `false`（返回 `true` 会销毁窗口，零窗口即退出，
change 2 spike 已证）。日志无关窗后的「退出阶段 1」，排除缺省值
读成关的可能，唯一路径是 `minimize_window()`。Xvfb 无 WM 响应
`WM_CHANGE_STATE`，视觉最小化无法断言（bridge-plan 已记）。

## 场景 2：`close_to_tray=0`（预写 preferences，两阶段退出）

```text
preference close_to_tray=0 set
PASS: 进程存活（装配期）
PASS: close_to_tray=off 关窗触发两阶段退出（进程已退出）
PASS: 日志含阶段 1
PASS: 日志含阶段 2
PASS=4 FAIL=0
```

## 关键日志轨迹（场景 1）

```text
WARN kiro_desktop::tray: 托盘创建失败，降级为无托盘（常驻与退出仍可用）:
  failed to register to the StatusNotifierWatcher: ...ServiceUnknown...
INFO kiro_desktop::quit: 退出阶段 1 开始：等待内嵌服务器收敛
INFO kiro_desktop::quit: 退出阶段 1 完成，进入阶段 2
INFO kiro_desktop: 退出阶段 2：quit
```

## 限制

托盘单击唤回与菜单交互在 Xvfb 无 SNI 宿主下不可达（托盘降级），
真实注册能力已由 `spike-tray-register.md` 双路径取证；桌面下的
实际交互留手测。
