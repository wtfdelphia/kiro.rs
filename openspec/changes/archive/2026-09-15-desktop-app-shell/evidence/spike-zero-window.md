# Spike 记录：零窗口时 GPUI 应用循环是否存活（设计文档 §6.5 之一）

日期：2026-09-11
环境：本机无头（Xvfb 显示、llvmpipe Vulkan 软渲染），`gpui-pre` 0.3.4

## 方法

`src/bin/spike-zero-window.rs`：开窗 → 每秒心跳 → 第 3 秒
`window.remove_window()` 销毁唯一窗口 → 继续心跳到第 8 秒。

## 结果（Linux）

```text
[spike] 窗口已打开: 4294967297
[spike] tick 1
[spike] tick 2
[spike] tick 3
[spike] tick 3: 窗口已销毁 (remove_window ok=true)
[spike] run() 返回，应用已退出
EXIT=0
```

销毁最后一个窗口后，心跳停止、`run()` 立即返回。**零窗口不存活**：
`gpui-pre` 0.3.4 在 Linux 上，窗口数为零即退出应用循环。

## 对常驻模型的影响（§6.3 需回填）

1. 设计文档 §6.3 的「关窗拦截 → `remove_window()` 销毁 → 进程存活」在
   Linux 上不成立：销毁最后一个窗口的瞬间应用就退出了，没有窗口可重建。
2. 可行的常驻路径（change 6 立项时按实测选定）：
   a. **隐藏窗口兜底**：关窗拦截后不销毁唯一窗口，而是移屏外/最小化或
      建一个 1x1 不可见占位窗口保住应用循环，主窗口照常销毁重建
   b. **拦截不销毁**：`on_window_should_close` 返回 `false` 后仅隐藏/
      清空主窗口内容（若平台窗口有隐藏能力，§6.3 已确认
      `PlatformWindow` 无隐藏接口，需另寻窗口级隐藏或重建路径）
   c. macOS/Windows 行为未测（本机仅 Linux）；Zed 在 macOS 上零窗口
      存活，三平台结论可能不同，常驻实现必须按平台分支。
3. 无论走哪条路径，「退出语义统一走两阶段退出」不变；常驻只影响
   「关窗」这一个入口的处置。

## 复现

```bash
cd desktop
xvfb-run -a cargo run --release --bin spike-zero-window
```
