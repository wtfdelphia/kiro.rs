## Purpose

定义 Admin UI 静态资源服务的路径契约：首页在带与不带尾斜杠时都可达、前端路由路径回落到首页、缺失的资源文件保持 404、穿越路径被拒。不管地址栏里那个斜杠在不在，界面都得打开；同时别让 SPA 回落把资源缺失盖过去。

## ADDED Requirements

### Requirement: 首页在带与不带尾斜杠时都可达

Admin UI 的挂载点 SHALL 同时响应 `/admin` 与 `/admin/`，两者都返回 `index.html` 且状态码为 200。

MUST NOT 依赖客户端重定向或反向代理规范化来达成这一点。

#### Scenario: 不带尾斜杠

- **WHEN** 请求 `GET /admin`
- **THEN** 返回 200，响应体是 `index.html`

#### Scenario: 带尾斜杠

- **WHEN** 请求 `GET /admin/`
- **THEN** 返回 200，响应体是 `index.html`

### Requirement: 前端路由路径回落到首页

请求路径的末段不含扩展名且嵌入资源中无对应文件时，服务 SHALL 返回 `index.html`，由前端路由接管。

#### Scenario: SPA 路径

- **WHEN** 请求 `GET /admin/credentials`
- **THEN** 返回 200，响应体是 `index.html`

### Requirement: 缺失的资源文件保持 404

请求路径的末段含扩展名且嵌入资源中无对应文件时，服务 SHALL 返回 404，MUST NOT 回落到 `index.html`。

回落会让浏览器把 HTML 当作 JS 或 CSS 解析，报出与真实原因无关的语法错误。

#### Scenario: 缺失的 JS 文件

- **WHEN** 请求 `GET /admin/assets/does-not-exist.js`
- **THEN** 返回 404

### Requirement: 穿越路径被拒

请求路径含 `..` 时服务 MUST NOT 返回任何文件内容。

#### Scenario: 向上穿越

- **WHEN** 请求 `GET /admin/../../etc/passwd`
- **THEN** 不返回 200
