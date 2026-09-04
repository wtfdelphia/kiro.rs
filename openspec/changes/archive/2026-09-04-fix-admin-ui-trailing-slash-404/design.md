## Context

动机与根因推导见 `proposal.md` 的 Why。这里只记影响改法选择的两个框架约束。

matchit 的路由树里，`nest("/admin", _)` 注册出 `/admin` 与 `/admin/{*file}` 两条。catch-all 段至少要消费一个字符，`/admin/` 的路径段是空字符串，两条都匹配不上。404 由外层路由树直接产生，请求进不了内层 router，所以内层加什么兜底都无效，这一点在 task 2.2 实测过。

`/{*file}` 那条 catch-all 同时承担静态资源与 SPA 回落，不能动它；也不能在旁边补一条单段 `/{file}`，matchit 认为两者在同前缀下冲突，注册阶段就报 `Insertion failed due to conflict`，task 2.1 实测过。能动的只剩挂载层：在 `nest` 之外显式多挂一条精确路径。

## Goals / Non-Goals

Goals：

- `/admin/` 与 `/admin` 返回同样的首页
- 挂载动作收进单一入口，测试按生产挂载方式打真实 Router
- 其余路径的状态码与响应体不变

Non-Goals：

- 不改 `static_handler` 的 SPA 回落与 `..` 检查逻辑
- 不为 `/admin//`、`/admin/.` 这类畸形路径立专项契约，它们走 catch-all 的现有行为
- 不引入重定向（301/302 到无斜杠形式）。显式 200 比多一跳跳转简单，也避免代理层把跳转再改回去

## Decisions

### 在挂载层显式补一条 `/admin/`，而不是动内层路由

内层的两条修法都试过且都不通（见 Context）。剩下的选项只有挂载层：`mount_admin_ui` 在 `nest("/admin", _)` 之后追加 `.route("/admin/", get(index_handler))`。这条路径精确匹配空段情形，与 nest 出来的两条互不重叠，matchit 不报冲突。

代价是挂载处比裸 `nest` 多一行，且这条知识必须跟着挂载点走。所以把它封装进 `mount_admin_ui`，`main.rs` 只调用这一个函数；`create_admin_ui_router` 降为模块私有，防止别处再拿裸 router 去 `nest` 而复现缺陷。

### 测试走 `mount_admin_ui`，不打裸的内层 router

缺陷出在 nest 的前缀折叠上，直接对内层 router 发请求测不出这条补丁。测试统一用 `mount_admin_ui(Router::new())` 构造，五条用例（带斜杠、不带斜杠、SPA 回落、缺失资源 404、穿越路径拒绝）全部打在挂载后的路径上。

## Risks / Trade-offs

- 路径写死的 `/admin/` 与 nest 的 `/admin` 前缀耦合。如果将来把 Admin UI 挪到别的前缀，要同步改 `mount_admin_ui` 里的两处。缓解：两处都在同一个函数体内，改动面天然收敛
- 「前缀变化时两处要一起改」没有测试钉住。接受：函数体共四行，肉眼可查

## 验证策略

- `cargo test admin_ui`：五条用例覆盖 spec 的四个 Requirement
- 回退实验：摘掉 `/admin/` 那条路由，只有尾斜杠用例变红，证明补丁与用例一一对应
- 线上复测：部署到 172.20.66.24 后 `curl /admin/` 返回 200 且正文是完整 `index.html`（task 3.5）
