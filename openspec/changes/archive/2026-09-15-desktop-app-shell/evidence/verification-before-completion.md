# 验证记录: desktop-app-shell

日期：2026-09-11
分支：`dev-desktop`（提交 `ef0db36`）

## Verification

| 命令 | 结果 | 结论 |
| --- | --- | --- |
| `/tmp/gpui-spike` 编译 spike（`gpui-kit = "0.6"`） | `cargo build` 3m14s，0 错误 | 任务 1.1 通过（证据 `spike-gpui-build.txt` + lock） |
| 环境探测：`Xvfb`、Vulkan ICD（`lvp`/`llvmpipe`）、缺 6 个 `-dev` 包 | 无 root，`apt-get download` + sysroot 解法可行 | 任务 1.2 通过（方法固化进 `desktop/README.md`） |
| `cd desktop && RUSTFLAGS="-D warnings" cargo check --release --all-targets --locked` | Finished，0 warning | 任务 4.4 编译门禁通过 |
| `cd desktop && cargo test --release` | 9 passed（paths 3 + lock 4 + bridge 1 + views 1） | 任务 4.4 测试通过 |
| `xvfb-run -a target/release/kiro-desktop --config ... --credentials ...`（timeout 10s） | 存活满 10 秒；日志含「已获取单实例锁」「主窗口已创建」「核心装配完成，凭据数: 0」 | 任务 4.3 通过 |
| 零窗口存活 spike（`spike-zero-window`） | 销毁最后一个窗口后 `run()` 立即返回，心跳停止 | 任务 4.1 结论：**Linux 零窗口不存活**（`spike-zero-window.md`） |
| tray-icon 事件送达探测 | 无法执行：0.20-0.24 全系无 ksni feature，Linux 是 gtk Only；无头环境无 GTK3 dev 树与 StatusNotifier 守护 | 任务 4.2 结论：**设计假设不成立**，延到 change 6（`tray-icon-research.md`） |
| 根仓 `cargo check --release --all-targets` | 0 warning（与 change 1 后基线一致） | 任务 4.5 根侧告警不变 |
| `git status --short -- src/ Cargo.toml Cargo.lock` | 空 | 根仓源码零改动 |
| `openspec validate desktop-app-shell` | valid | 工件合规 |
| `git status --short`（提交前） | 无凭据、无 `.codegraph/`、无 `*.log` 误入 | 安全检查通过 |

未运行项（SKIPPED）：

- 有显示环境下的窗口画面人工确认：本机无头，验收口径按 design.md D6
  降为「Xvfb 存活 + 日志证据」。剩余风险：布局细节（sidebar 折叠动画、
  四视图切换的点击响应）未经人工目视，change 4 实现数据视图时补
  `#[gpui_kit::test]` headless 测试
- macOS / Windows 编译与运行：本期只做 Linux，三平台放 change 7 CI
- 单实例锁跨进程验证只写了同进程 + 重获测试；跨进程场景由
  `flock(LOCK_EX | LOCK_NB)` 语义保证，未实机双开（双开需图形会话）

## Documentation Sync

| 入口 | 是否同步 | 处理 |
| --- | --- | --- |
| `spec/structure.md` | 是 | 补 `desktop/` 目录树，随本提交 |
| `spec/design.md` | 是 | 「项目只有 binary target」过时（change 1 起为双目标），一并修正并补桌面版一行 |
| `docs/tooling-sources.md` | 是 | 补 `gpui-kit` 依赖登记（注明仅 `desktop/` workspace） |
| `docs/desktop-gpui-embedded-design.md` | 是 | §6.1 回填「无 ksni feature」、§6.5 回填两个 spike 结论 |
| README | 否 | CLI 使用方式不变 |
| AGENTS.md | 否 | 无影响 |

## Residual Risk

- change 2 未归档、未推送：归档走 `openspec-archive-change`，推送时机由
  用户决定（远端 `dev-desktop` 仍停在 `42ec15a`，本地领先 2 个提交）
- 常驻模型（§6.3）的「销毁 + 重建」在 Linux 上需兜底方案（占位窗口或
  拦截不销毁），change 6 立项时按平台分支钉死；macOS/Windows 行为未测
- `tray-icon` 的 Linux 方案（GTK3 vs ksni crate）延到 change 6 实测
- `desktop/Cargo.lock` 与根锁的版本一致性靠 review（§3.1 已论证的取舍）
- `AllAssets` 嵌入全部 1830 个图标导致二进制体积与编译时间上升
  （release 编译约 8 分钟、二进制 44MB）；若成为问题，change 7 可换
  自定义图标集
