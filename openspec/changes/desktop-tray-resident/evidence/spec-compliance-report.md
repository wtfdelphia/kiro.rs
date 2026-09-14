# Spec Compliance Report: desktop-tray-resident

门禁：`spec-compliance-check`。审查对象为本 change 全部未提交改动
（`git status` 9 改 3 增，全部在 `desktop/` 工作区、`docs/` 与 change
工件内）。

## 六维结论

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | PASS | 改动仅覆盖 proposal 范围：托盘、常驻、开机自启、行为偏好、设置分区。未触碰非目标（占位窗口、位图资源、打包） |
| Design | PASS | D1–D6 落实；D3「托盘挂 `CoreHandle`」因 `TrayIcon` 非 `Send` 编译不成立，修正为驻留主线程事件循环，design.md 已同步修正记录 |
| Scenarios | PASS | `skip_specs: true`，无 Requirement；tasks 19 项全勾，逐项对应实现与测试 / 冒烟证据 |
| Project Rules | PASS | 零新增告警（两侧门禁复跑）；真实凭据零入库；单提交纪律待提交时执行；文档过 humanizer |
| Verification | PASS | 门禁、64 测试、双分支冒烟、D-Bus 双路径取证均为本会话真实运行；无 SKIPPED |
| README/AGENTS Sync | PASS | `desktop/README.md` 进度与能力更新；设计文档 §6 回填；根 `README.md` 指向子文档无需改 |

## 发现项

1. 无 CRITICAL。
2. WARN（接受）：Xvfb 无窗口管理器，`minimize_window()` 的视觉
   效果无法断言，以「窗口未销毁 + 进程存活」为判据；真实桌面托盘
   交互（单击唤回、菜单）留手测。均已记录于
   `evidence/smoke-xvfb-resident.md`。
3. WARN（接受）：macOS / Windows 自启入口仅编译验证（本环境单
   target），入口内容为纯函数 + 单测覆盖。

## 证据索引

- 门禁：桌面 `RUSTFLAGS="-D warnings" cargo check --release
  --all-targets --locked` 零告警；根仓 `cargo check --release
  --all-targets` 零告警（零根仓代码改动）
- 测试：`cargo test --release`（桌面 workspace）64 passed 0 failed
- 托盘注册：`evidence/spike-tray-register.md`
- 冒烟：`evidence/smoke-xvfb-resident.md`（ON 9/9、OFF 4/4）
- `openspec validate desktop-tray-resident`：valid

## 总体状态

**PASS**。剩余风险：真实桌面托盘交互未验证（环境限制，已记在案）。
