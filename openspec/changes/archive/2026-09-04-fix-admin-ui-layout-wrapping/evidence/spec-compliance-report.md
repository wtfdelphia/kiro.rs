# spec-compliance-check 报告：fix-admin-ui-layout-wrapping

日期：2026-09-04（第二轮增补后）　审查范围：第一轮已审部分 + 第二轮收纳与批量工具栏行

## 六维审查

| 维度 | 结论 | 依据 |
| --- | --- | --- |
| Scope | PASS | 第二轮只动 `admin-ui/src/components/dashboard.tsx`、`dashboard.test.tsx`、新增 `ui/dropdown-menu.tsx` 与本 change 的 OpenSpec 工件。工作区里 `src/admin_ui/*`、`src/main.rs` 的改动属另一个 change（`fix-admin-ui-trailing-slash-404`），本次未触碰。`AGENTS.md` 的改动是会话前已存在的他方改动，未参与 |
| Design | PASS | 实现与 design.md 第二轮决策一致：5 低频项进溢出菜单（保留可用性判据）、批量四项加批量余额/订阅进 `role="toolbar"` 行、标题行右组剩定时刷新、添加凭据、更多操作三项 |
| Scenarios | PASS | 见下方对照表 |
| Project Rules | PASS | 未引入新依赖；`@radix-ui/react-dropdown-menu` 是 `package.json` 既有直接依赖；未新增 CI 环节 |
| Verification | PASS | 全部命令本会话实跑，结果见下 |
| README/AGENTS Sync | PASS | 无需同步，理由同第一轮：纯前端布局，长期事实层未记录布局约束；`rg "响应式|布局|viewport|视口" spec/` 仍零命中 |

## Requirement 与 Scenario 对照

| Requirement | Scenario | 实现与证据 |
| --- | --- | --- |
| 批量操作与标题行全局操作分行 | 桌面宽度下全选后标题行单行 | `dashboard.tsx` 工具栏行；`probe_toolbar.py` 实测 1280 到 2560px 工具栏 1 行、标题行 64px；截图 `evidence/toolbar-refactor/after_1920.png` |
| 同上 | 选中凭据后新增控件不进入标题行 | `dashboard.test.tsx`「全选后四项批量操作全部落在工具栏行」用例 |
| 低频操作收进溢出菜单 | 低频操作从标题行移入菜单 | `dashboard.test.tsx`「更多操作溢出菜单」用例 |
| 同上 | 菜单项可用性判据不丢失 | 「全量无已禁用时菜单项禁用且不带计数」用例 |
| 布局约束需有自动化回归护栏 | 折行约束被断言等四条 | 既有断言 + 本轮新增归属断言，回退源码可使其失败 |

第一轮的四项 Requirement（最小支持宽度、折行、弹窗高度上限、不可断长文本）实现未变，仍由既有测试守护。

## 验证命令与结果（本会话实跑）

- `npx vitest run`：17 文件 169 项全绿（第一轮基线 166，本轮净增 3）
- `pnpm build`：成功，产物 `index-HfGrMh8b.js`
- `cargo check --release --all-targets`：零告警（本轮未改 Rust，随第一轮回放）
- `npx openspec validate fix-admin-ui-layout-wrapping --strict`：valid
- playwright 复测（改前改后各一轮，本地 8473 端口 + 真实部署 18990 双源交叉）：≥1280px 选中态标题行单行 64px、工具栏单行；375px 无横向滚动；截图存 `evidence/toolbar-refactor/`

## 发现项

无 CRITICAL。

1. 320px 下仍有 11px 横向滚动。320px 低于规范声明的最小支持宽度 375px，契约明确不作保证，接受。
2. 8.6（部署后线上复测第二轮效果）待部署执行，属预期遗留项。

## 剩余风险

- 定时刷新控件组（302px）留在标题行右组，768 到 1024px 下标题行折成 2 行（108px）。design Open Questions 已记录，后续若反馈明显再评估移入工具栏行。
- 「更多操作」菜单项数量变化时测试的 `MENU_ITEMS` 清单需手工同步，这是静态清单的固有代价，与既有断言风格一致。

## 总体状态

PASS。8.6 部署复测完成后本 change 可进入归档流程。
