# Verification Before Completion: desktop-tray-resident

门禁：`verification-before-completion`。全部为本会话真实运行。

## Verification

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `desktop/` `RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked` | Finished，0 warning | 桌面门禁零告警 |
| 根仓 `cargo check --release --all-targets` | Finished，0 warning | 根门禁零告警（零根仓代码改动，仅确认） |
| `desktop/` `cargo test --release` | 64 passed, 0 failed | 全量通过 |
| `openspec validate desktop-tray-resident` | valid | 工件合规 |
| `dbus-run-session` 无宿主探针 | `DEGRADED: ...ServiceUnknown` | 降级路径取证 |
| `dbus-run-session` 假 watcher 探针 | `REGISTER OK` + watcher 收到注册 | 真实注册取证 |
| Xvfb 冒烟 ON（常驻，9 断言） | 9/9 | 关窗常驻 + 唤回前置 + 两阶段退出 |
| Xvfb 冒烟 OFF（`close_to_tray=0`，4 断言） | 4/4 | 关窗走两阶段退出分支 |

告警基线对比：本 change 前后根仓与桌面门禁均 0 告警，无新增。

## Documentation Sync

| 入口 | 是否同步 | 说明 |
| --- | --- | --- |
| `desktop/README.md` | 是 | 进度 change 5→6，补托盘/常驻能力 |
| `docs/desktop-gpui-embedded-design.md` §6 | 是 | 回填 0.25 ksni 定案与最小化常驻修订 |
| 根 `README.md` | 否 | 桌面小节指向 `desktop/README.md`，无需改 |
| `AGENTS.md` / `spec/` / `openspec/specs` | 否 | `skip_specs: true`，无主 specs；启动/构建命令未变 |

## Residual Risk

- Xvfb 无窗口管理器，`minimize_window()` 视觉效果无法断言，以
  「窗口未销毁 + 进程存活」为判据（已记 `smoke-xvfb-resident.md`）
- 真实桌面托盘交互（单击唤回、菜单）本环境不可达，留手测；注册
  能力已由探针双路径覆盖
- macOS / Windows 自启入口仅编译验证（本环境单 target），内容为
  纯函数 + 单测
- 尚未提交 / push / PR / archive

## git status

提交前 `git status --short` 无 `config.json` / `credentials.*` /
`.codegraph/` / 真实密钥混入；改动全部在 `desktop/`、`docs/` 与
change 工件内。
