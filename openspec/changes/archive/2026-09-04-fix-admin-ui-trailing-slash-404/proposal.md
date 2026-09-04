## Why

访问 `/admin/` 返回 404，只有不带尾斜杠的 `/admin` 能打开 Admin UI。浏览器地址栏自己补斜杠是常事，反向代理规范化路径也会补，撞上就是一片空白。

根因在 `src/main.rs` 的挂载方式。`nest("/admin", router)` 把内层的 `/` 折成 `/admin`，另一条注册成 `/admin/{*file}`，而 matchit 的 catch-all 至少要吃掉一个字符，`/admin/` 这个空段路径两条都落不到。内层 `fallback` 也接不住——404 在外层 nest 就产生了，请求根本没进内层 router。

线上实测：`/admin` 返回 200，`/admin/` 返回 404 和 axum 的默认空响应体。

## What Changes

- 新增 `mount_admin_ui(app)` 封装挂载动作，内部在 `nest("/admin", _)` 之外显式补一条 `/admin/` 路由指向首页 handler
- `src/main.rs` 改为调用 `mount_admin_ui`，不再直接 `nest`
- `create_admin_ui_router` 收回为模块私有，避免绕过封装重新引入缺陷
- 新增 `src/admin_ui/router_test.rs`，按生产挂载方式打真实 Router，覆盖五条路径契约

不改 `static_handler` 的 SPA 回落与安全检查逻辑。`/{*file}` 那条 catch-all 保留原样，尾斜杠是它天然覆盖不到的空段情形，不是它的逻辑问题。

试过两条更省事的路子，都不行。`.route("/{file}", _)` 与 `.route("/{*file}", _)` 共存被 matchit 拒绝（`Insertion failed due to conflict`）。内层 `.fallback(_)` 对 `/admin/` 无效，原因见上。

## Capabilities

### New Capabilities

- `admin-ui-routing`: Admin UI 的静态资源与 SPA 路径契约。约束首页在带与不带尾斜杠时都可达、前端路由回落到首页、缺失的资源文件保持 404、穿越路径被拒

### Modified Capabilities

无。既有 spec 不涉及 Admin UI 的路径解析。

## Impact

代码：

- `src/admin_ui/router.rs`：新增 `mount_admin_ui`，`create_admin_ui_router` 降为私有
- `src/admin_ui/mod.rs`：导出改为 `mount_admin_ui`，注册测试模块
- `src/main.rs:220-227`：挂载方式收敛到一次调用
- `src/admin_ui/router_test.rs`：新增

行为变化只有一处：`/admin/` 从 404 变 200 并返回 `index.html`。其他路径的状态码与响应体都不动。

多挂的这条路由路径写死，复用现成的 `index_handler`，读的还是编译期嵌进来的资源，碰不到别的东西。
