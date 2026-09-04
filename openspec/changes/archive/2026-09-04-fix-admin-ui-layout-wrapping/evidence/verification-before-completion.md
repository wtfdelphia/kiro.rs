# verification-before-completion：fix-admin-ui-layout-wrapping

日期：2026-09-04　范围：第一轮 + 第二轮（收纳与批量工具栏行）

## Verification

| 命令 | 结果 |
| --- | --- |
| `npx vitest run`（admin-ui） | 17 文件 169 项全绿（第一轮基线 166，第二轮净增 3：工具栏归属 2、溢出菜单 1；清除已禁用 4 项改造不增减数量） |
| `npx tsc --noEmit` | 通过 |
| `pnpm build` | 成功，产物 `index-HfGrMh8b.js` / `index-DW96CStD.css` |
| `cargo check --release --all-targets` | 零告警（本轮未改 Rust，随全量回放确认） |
| `npx openspec validate fix-admin-ui-layout-wrapping --strict` | valid |
| playwright 复测（改前改后各一轮，本地 8473 + 真实部署 18990 双源交叉） | 改前：1920px 选中态标题行 152px、右组 2 行，375px 行高 460px、右组 8 行；改后：≥1280px 标题行单行 64px、批量工具栏单行，375px 无横向滚动。截图存 `evidence/toolbar-refactor/` |
| 部署后线上复测（2026-09-04，172.20.66.24:18990，tasks 8.6） | 二进制产物与本地构建一致（`index-HfGrMh8b.js`）；pm2 进程健康、无 error 日志；≥1280px 全选后标题行单行 64px、工具栏行单行 54px，768 与 1024px 标题行 108px、工具栏单行，375px 无横向滚动。截图存 `evidence/toolbar-refactor/deployed/` |
| humanizer-zh 关键词扫描（本轮改写的 6 份 Markdown） | 零命中 |
| `git status --short` 敏感文件核查 | 无 `config.json` / `credentials.*` / `.codegraph/` 进入候选；测量用临时 key 文件已删除 |

## Documentation Sync

| 入口 | 是否同步 | 理由 |
| --- | --- | --- |
| README.md | 否 | 本次为纯前端布局调整，不改构建、启动、部署、API 入口 |
| AGENTS.md / CLAUDE.md | 否 | 未改 AI 协作纪律与验证命令 |
| spec/（长期事实） | 否 | 布局约束属 change 级 spec，归档时由 `openspec archive` 落主 spec；`rg "响应式|布局|视口" spec/` 零命中 |
| docs/tooling-sources.md | 否 | 未引入新工具链依赖 |

## Residual Risk

- ~~tasks 8.6 未执行~~：2026-09-04 部署复测完成，结论与本地测量一致
- 定时刷新控件组（302px）留在标题行右组，768 到 1024px 下标题行折成 2 行（108px），design Open Questions 已记录
- 320px 下仍有 11px 横向滚动，低于规范最小支持宽度 375px，契约明确不作保证
- ~~截图基于本地构建产物~~：已补真实部署截图与逐视口几何

## 结论

代码、文档、部署三方验证完成，本 change 达到归档条件。
