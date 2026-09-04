## 1. 复现

- [x] 1.1 写 `src/admin_ui/router_test.rs`，按生产方式 nest 到 `/admin` 后打真实 Router。裸打内层 router 测不出前缀折叠，必须走挂载后的路径
- [x] 1.2 五条用例里 `/admin/` 一条红（404 vs 200），其余四条（`/admin`、SPA 回落、缺失资源 404、穿越路径）绿，确认缺陷范围只在尾斜杠

## 2. 修复

- [x] 2.1 试 `.route("/{file}", _)` 补空段：matchit 拒绝与 `/{*file}` 共存，报 `Insertion failed due to conflict with previously registered route`
- [x] 2.2 试内层 `.fallback(_)`：`/admin/` 仍 404，因为 404 在外层 nest 就产生了，请求进不到内层
- [x] 2.3 新增 `mount_admin_ui(app)`，在 `nest("/admin", _)` 之外显式挂一条 `/admin/` 指向 `index_handler`；`create_admin_ui_router` 收回私有，防止绕过封装
- [x] 2.4 `src/main.rs` 改用 `mount_admin_ui`，两次 nest 收敛成一句

## 3. 验证

- [x] 3.1 `cargo test admin_ui` 五条全绿
- [x] 3.2 回退 `/admin/` 那条路由，确认 `test_admin_root_with_trailing_slash_serves_index` 变红，其余四条不动
- [x] 3.3 `cargo test` 全量 870 passed
- [x] 3.4 `cargo check --release --all-targets` 零告警
- [x] 3.5 部署后线上复测 `/admin/` 返回 200 且页面正常渲染（2026-09-04，172.20.66.24:18990，`/admin/` 与 `/admin` 均 200，正文为完整 `index.html`）
