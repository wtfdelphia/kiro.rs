# Proposal: desktop-credentials-view

桌面版（路线 B，`docs/desktop-gpui-embedded-design.md` §8）的第四步：把
change 2 的「凭据」占位视图换成真实数据视图。凭据列表（筛选/分页）+
启停/优先级/删除/测试/余额 + 添加、批量导入、KAM 导入对话框 + Builder ID /
IAM SSO 在线登录。数据面复用 `AdminService`，不动服务端，不动存储层。

## Why

- change 2 的四个视图全是占位面板，桌面端至今没有任何可用功能；凭据管理
  是单用户代理工具的核心操作面，顺序上必须最先落地。
- change 3 的 `SqliteStore` 已经能安全落盘，但没有界面入口：添加凭据、
  看余额、测可用性都只能手改 JSON 或走 CLI。
- `AdminService` 已封装全部凭据业务（查询/增删/测试/余额/在线登录），
  admin-ui 的 React 前端消费的就是它；桌面视图直连同一个服务面，零重复
  实现。

## What Changes

- 桌面侧新增 `CoreHandle`：`Arc<AdminService>` + tokio handle + 结果事件
  通道。装配事件 `CoreEvent::Bootstrapped` 携带它，`AppView` 持有并下发。
- 「凭据」视图：`DataTable`（gpui-kit `TableDelegate`）展示
  `query_credentials` 结果；筛选栏（认证方式/禁用状态/订阅等级/邮箱）；
  分页；行内操作（启停开关、优先级、测试、余额、刷新模型、删除）。
- 对话框：添加凭据（social / api_key / idc 表单）、批量导入（JSON 数组
  粘贴）、KAM 文档导入（粘贴原文）、删除确认、Builder ID 登录（开浏览器
  + 轮询）、IAM SSO 登录（startUrl → 浏览器 → 粘贴回调）。
- 同步查询在 GPUI 线程直调（`AdminService` 同步方法，锁短无网络）；异步
  操作（测试/余额/添加/导入/SSO）经 tokio handle 派发，结果走事件通道，
  视图收事件后刷新表格并弹通知。
- 桌面端自持一个 `AdminService` 实例（`new_with_runtime`，不带
  admin_api_key 门槛：门槛只管 HTTP Admin API，管不到本地界面）。
- 根仓零改动：`AdminService::new_with_runtime` 与全部所需方法已是 pub，
  `Bootstrapped` 字段足够构造服务。

## Capabilities

本变更为桌面端新增交互界面，服务端行为与需求不变。

### New Capabilities

无（桌面交互面，无可被服务端规格化的需求行为；实现细节见 design）。

### Modified Capabilities

无。

## Impact

- 代码：全部新增在 `desktop/crates/kiro-desktop/`（`src/core/` 句柄与事件、
  `src/ui/credentials/` 视图与对话框）；`main.rs` / `bridge.rs` 只改装配
  传递。根仓零改动。
- 依赖：`desktop/` 新增 `open`（已在锁内，拉起系统浏览器）；dev-deps 加
  `gpui-pre`（`test-support`，headless 测试）。根仓依赖零变化。
- 测试：headless（`#[gpui::test]` + `TestAppContext`）覆盖表格渲染、启停/
  优先级/删除走真实 `AdminService` + tempdir `SqliteStore`、表单校验；
  SSO 与上游操作不碰网络，留手动全流程。
- 已知取舍：桌面 `AdminService` 与 change 5 内嵌服务器的实例各持一份余额
  缓存（同一文件锚点，TTL 300 秒，可接受）；总览视图仍是占位（不在本
  change 范围）。
- 后续：change 5 的设置/服务视图复用 `CoreHandle` 模式。
