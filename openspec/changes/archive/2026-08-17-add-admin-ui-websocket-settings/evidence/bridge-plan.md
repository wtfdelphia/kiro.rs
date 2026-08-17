# Bridge Plan: add-admin-ui-websocket-settings

> 日期：2026-08-17　门禁：openspec-superpowers-bridge

## 1. 范围、非目标、关键设计决策

**范围**：Admin UI 设置面板新增 WebSocket 分区（读、改、存、只读活跃连接数），复用既有分区模式与保存链；前端增量，后端零改动。

**非目标**：不改后端 API / 配置 schema / WS ingress 行为；不做活跃连接数轮询；不为 passthrough 预留项做额外配置面。

**关键决策**（详见 design.md）：D1 并入现有 SettingsPanel；D2 API 函数对 `getWebsocketSettings`/`updateWebsocketSettings`；D3 max_message_bytes 以 MB 呈现双向换算；D4 mode 用 Select 固定两项（passthrough 标注预留）；D5 前端校验仅保存拦截；D6 活跃连接数打开面板 + 保存后刷新。

**OpenSpec 状态**：4/4 工件 done，`openspec validate --all` 25 项通过，无 blocked。

## 2. 高风险项

| 风险 | 等级 | 处置 |
| --- | --- | --- |
| MB/字节换算取整漂移（API 直设非整 MB 值） | 低 | 设计内接受，UI 注释标明；单测覆盖换算边界 |
| 保存链顺序 PUT，WS 环节失败会中断后续环节 | 低 | 与现状一致；错误经 extractErrorMessage 透出可定位分区 |
| admin-ui 经 rust-embed 内嵌（`src/admin_ui/router.rs:14` `#[folder = "admin-ui/dist"]`），UI 改动须 `pnpm build` 后重编 Rust 二进制才进发布 | 中 | tasks 4.1 覆盖构建；部署提示写入最终报告 |
| 告警门禁只约束 Rust 代码 | 无 | 本变更纯前端，不触碰告警基线 |

## 3. CodeGraph 证据

- `codegraph status`：索引以 Rust 为主（typescript 未入图），前端影响面分析不适用；本变更以 rg + 源码精读替代。
- 结论：无 Rust 符号受影响（后端零改动）。

## 4. rg / 源码补盲

- websearch 分区是最近范式：`settings-panel.tsx` L21/26（API 导入）、L53（state）、L67（load 并发 Promise 链）、L83（填充）、L117（保存链 PUT）、L257-258（Switch 渲染）——WS 分区逐步对齐。
- API 层范式：`admin-ui/src/api/settings.ts` 的 get/update 函数对（typed、axios 拦截器统一错误）；类型集中在 `admin-ui/src/types/api.ts`。
- 测试设施：`pnpm test` = `vitest run`，无独立 vitest 配置（走默认）；既有一例 `kam-import-dialog.test.ts` 证明 runner 可用。
- 发布链路：admin-ui/dist 编译期内嵌，UI 变更需 pnpm build + cargo 重编才生效于部署。
- README：L195 已列 WS Admin API，L670 已述热加载参数；L353-372 配置表无 websocket 块行——4.3 判定：README 可在 L670 附近补一句「可在 Admin UI 设置面板操作」（可选小改），其余不动。
- 工作区：`git status --short -- admin-ui/` 干净；websearch 分区已提交（c801204），无 WIP 冲突。
- 敏感文件：本变更不触碰 config/credentials；无入库风险。

## 5. 任务到执行步骤表

| 任务 | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1-1.2 API 层与类型 | types/api.ts 加 WebsocketSettings/UpdateWebsocketSettingsRequest；settings.ts 加两函数 | tsc 通过 | 后端字段名不一致时停下核对 |
| 2.1 分区加载与渲染 | settings-panel.tsx：state、load 链、字段渲染（Switch/Select/Input/只读连接数） | pnpm build | 组件缺失时停下 |
| 2.2 MB 换算 | 纯函数 bytesToMb / mbToBytes + 加载/提交接入 | 3.2 单测 | 无 |
| 2.3 语义文案 | enabled 后果说明、0=不启用超时说明 | 目检 | 无 |
| 2.4 保存链 | handleSave 链加 updateWebsocketSettings 环节 | pnpm build + 手动回归 | 无 |
| 3.1-3.2 校验与单测 | 校验纯函数 + vitest 用例（负数/非整数/MB 边界） | pnpm test | 无 |
| 4.1 构建门禁 | pnpm build && pnpm test | 输出绿 | 失败即停 |
| 4.2 真实回归 | Admin UI 打开面板核对；enabled 切换后 ws upgrade 准入验证（websocat/curl 503 与恢复） | 证据入 evidence/ | 上游/服务异常即停 |
| 4.3 README | L670 附近补 Admin UI 操作入口一句（若判断无需则说明原因） | diff 检查 | 无 |

## 6. 必跑验证

1. `cd admin-ui && pnpm build`（tsc -b && vite build）通过
2. `cd admin-ui && pnpm test` 通过
3. 真实回归：面板字段加载正确；enabled=false 保存后新 WS upgrade 被 503，enabled=true 恢复；活跃连接数展示
4. Rust 侧零改动确认：不跑 cargo check（无 Rust 变更）；若实现期意外触碰 Rust，补 `cargo check --release --all-targets` 零新增告警

## 7. README / AGENTS / spec 同步判断

- README：L670 附近可选补一句 Admin UI 操作入口（task 4.3 判定）；其余已备。
- AGENTS.md：不需要（无纪律/验证命令变化）。
- `spec/`：不需要（模块边界未变）。
- `openspec/specs/`：新 capability 归档时由 delta 生成主 spec，实现期不手改。

## 8. 停止条件

- 工件缺失/矛盾/blocked（当前无）。
- 实现中发现后端契约与设计不符（当前已逐字段核对）。
- 工作区出现会被提交的真实凭据（当前无）。
- 无法执行真实回归（Admin 服务不可达）时，写明原因与剩余风险。
