# Bridge Plan: desktop-app-shell

日期：2026-09-11
分支：`dev-desktop`（`9323be7` 之后）
前置文档：`docs/desktop-gpui-embedded-design.md` v2.1 §3.1/§4/§6.5/§9

## 范围与非目标

范围：`desktop/` 独立 workspace 应用骨架（双运行时、事件桥、数据目录、
单实例锁、文件日志、窗口 + sidebar + 四占位视图）+ §6.5 两个 spike。

非目标：SQLite/keyring（change 3）、托盘与常驻实现（change 6）、视图数据
交互（change 4/5）、两阶段退出（change 5）、桌面 CI（change 7）、根仓
任何改动。

## 关键设计决策（见 design.md）

独立 workspace 不进根（D1）；双运行时 + `CoreEvent` 单向桥（D2）；本期
存储固定 JSON store（D3）；flock 单实例锁，失败即退出（D4）；占位视图
不做数据交互（D5）；无显示环境用 Xvfb + 日志证据验收（D6）。

## 高风险项

1. `gpui-pre` 0.3.4 编译/运行兼容：**已前置排掉**。`/tmp/gpui-spike` 最小
   依赖 `gpui-kit = "0.6"`，本机 `cargo build` 3m14s 通过（0 错误，
   证据 `evidence/spike-gpui-build.txt` + lock）。
2. 无 root 缺 `-dev` 包：sysroot 方案已验证可行（`apt-get download` +
   `dpkg-deb -x` + `.pc` 前缀改写）。精确包清单：`libxkbcommon-x11-dev`、
   `libasound2-dev`、`libzstd-dev`、`libx11-xcb-dev`、`libxcb-xkb-dev`、
   `libvulkan-dev`（vulkan 头文件可选，`gpui-pre` 的 `blade` 渲染需要）。
3. 无 `DISPLAY` 运行时行为未知：Xvfb + `lvp` Vulkan 软渲染兜底；跑不起来
   则按 design.md 风险表止损口径降级。
4. `admin-ui/dist` 编译期依赖：本机已存在（`ls admin-ui/dist` 确认）；
   桌面构建前需保证存在，写入 `desktop/README.md`。

## CodeGraph 证据

| 命令 | 结论 |
| --- | --- |
| `codegraph status` | 索引 up to date（189 文件 / 3587 节点）；本变更不触碰根仓源码，不需要 impact 分析 |

## rg / 源码补盲

- `rg workspace Cargo.toml`：根 `Cargo.toml` 无 `[workspace]` 段，
  `desktop/` 作为嵌套独立 workspace 不会被根侧命令带入（§3.1 假设成立）。
- `.gitignore`：无 `desktop` 条目，`git check-ignore desktop/` 不命中，
  产物可入库；`desktop/Cargo.lock` 按设计入库。
- `git status`：工作区干净，无凭据与 `.codegraph/` 误入风险；本变更新增
  文件全在 `desktop/` 与 `openspec/changes/desktop-app-shell/`。
- workflows：`warning-gate.yaml` 等五条均在 `.github/workflows/`，以根
  workspace 为作用域，`desktop/` 独立布局不触碰（本变更不改）。
- 版本钉定：spike lock 解析 `gpui-kit` 0.6.1 / `gpui-pre` 0.3.4，与可行
  性文档调研结论一致；`desktop/Cargo.lock` 直接复用 spike 的解析结果
  起点。

## 任务到执行步骤映射

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1-1.2 | 已完成（spike 与环境探测） | 证据存档 | — |
| 2.1-2.2 | 建 `desktop/` workspace，path 依赖 `kiro-rs` | `cargo check`（desktop） | gpui-kit API 与设计文档示意严重不符（如 `application()` 签名变化）则先对齐 API 再继续 |
| 3.1-3.6 | 骨架各模块逐个实现，每步编译 | `cargo check` + 单测 | 单步编译错误超过两个文件范围则回退该步 |
| 3.7 | `ui/` 布局，参照 gpui-kit sidebar 文档 | 编译 + Xvfb 启动 | — |
| 4.1-4.2 | 两个 spike：写探测代码，Xvfb 下运行取证 | 结论写入证据目录 | 无显示环境下托盘探测不可行时，记录环境限制，结论标注「待有显示环境复验」 |
| 4.3-4.5 | 启动验收 + 双侧门禁 + validate | 全绿 | 任一不绿则修复，不得掩盖 |

## 必跑验证

1. `cd desktop && PKG_CONFIG_PATH=... cargo check --release --all-targets
   --locked`（附加 `-D warnings`）
2. `cd desktop && cargo test --release`（paths/lock/bridge 单测）
3. Xvfb 启动：进程存活 ≥5 秒，日志含窗口创建与 `Bootstrapped` 事件
4. 根侧：`cargo check --release --all-targets` 告警数 = 0（不变）
5. `openspec validate desktop-app-shell`
6. `git status --short` 无凭据与 `.codegraph/` 误入

## 文档同步判断

- `docs/tooling-sources.md`：需补 `gpui-kit` / `gpui-pre` 依赖登记（新增
  crate，按该文件「Cargo 依赖登记」表格式），随本提交
- `spec/structure.md`：`desktop/` 目录树补一行，随本提交
- `spec/design.md`：构建与测试策略一节提到「项目只有 binary target」，
  change 1 后已过时（现为 lib + bin），本变更一并修正
- README：CLI 使用方式不变，无需更新
- AGENTS.md：无影响

## 停止条件

- `gpui-kit` 的公开 API 与设计文档 §4.1 骨架示意不兼容，且无法在骨架
  范围内适配
- Xvfb 下窗口创建持续失败且日志无可用线索
- 根仓告警门禁被本变更打破
- 无显示环境导致 §6.5 spike 完全无法执行：记录限制后继续其余任务，
  不整体阻塞
