# Capability: public-api-catalog

## Purpose

Maintain a single static registry of all client-facing (Public Client API) endpoints as the canonical source of truth for path, method, auth kind, status, and stream capability. Route mounting, startup logging, and Admin display all derive from this registry, and a bidirectional drift guard keeps the `live` set exactly in sync with the real Axum router.

## Requirements

### Requirement: Public API 端点注册表为单一事实源

The system MUST maintain a single static registry of all client-facing (Public Client API) endpoints. Each registry entry MUST carry at least a stable id, a protocol family, an HTTP method, a canonical path, an alias list, an auth kind, a status, a stream capability flag, a human-readable summary, and a client hints list. Route mounting decisions, startup logging of client-facing endpoints, and Admin display MUST derive from this registry rather than from independently maintained lists.

#### Scenario: 启动日志来自注册表

- **WHEN** 服务启动并打印可用对外 API 列表
- **THEN** 打印内容 MUST 由注册表中 status 为 live 的条目生成，MUST NOT 是独立手写的第二份清单

#### Scenario: id 与路径组合唯一

- **WHEN** 读取注册表
- **THEN** 所有条目的 id MUST 互不相同，且 (method, path) 组合 MUST 互不相同

### Requirement: 上游端点与对外端点概念分离

The registry and all identifiers introduced for it MUST refer only to client-to-proxy endpoints (Public Client API). The existing upstream Kiro endpoint setting (`/api/admin/settings/endpoint`) MUST remain unchanged in semantics and purpose, and MUST NOT be reused to configure or describe client-facing endpoints. New JSON fields and route names MUST use a `publicApi` / `public-api` style prefix instead of a bare `endpoint` term.

#### Scenario: 上游设置不受影响

- **WHEN** 本能力引入注册表与 Admin 只读接口
- **THEN** `/api/admin/settings/endpoint` 的请求/响应契约与行为 MUST 保持不变

#### Scenario: 命名不使用裸 endpoint

- **WHEN** 新增对外端点相关的 Admin 路由或响应字段
- **THEN** 标识符 MUST 采用 publicApi / public-api / publicEndpoints 形式，MUST NOT 命名为裸 endpoint / endpoints

### Requirement: status 语义与 live 集合一致性

Each entry MUST declare a status of `live`, `beta`, or `planned`. The set of entries with status `live` MUST be exactly mountable on the real router: every live entry's (method, path) MUST resolve to an existing route. Every entry with status `planned` MUST NOT be mounted, and a request to it MUST result in HTTP 404. A `planned` entry MUST NOT be presented anywhere as usable.

#### Scenario: live 条目可被路由命中

- **WHEN** 对注册表中任一 status 为 live 的 (method, path) 向真实 Router 发起请求
- **THEN** 响应状态码 MUST NOT 为 404（鉴权失败返回的 401 视为路由已存在，属于命中）

#### Scenario: planned 条目不可被路由命中

- **WHEN** 对注册表中任一 status 为 planned 的 (method, path) 向真实 Router 发起请求
- **THEN** 响应状态码 MUST 为 404

#### Scenario: planned 不做占位实现

- **WHEN** 某端点尚未实现且登记为 planned
- **THEN** 系统 MUST NOT 为其挂载返回 501 或其他非 404 状态的占位 handler

### Requirement: 首版路径别名为空

The registry MUST expose an alias list field for each entry to allow future compatibility aliases. In this change every entry's alias list MUST be empty, and no alias route may be mounted. Canonical paths MUST remain the `/v1/...` and `/cc/v1/...` forms already served.

#### Scenario: alias 字段存在但为空

- **WHEN** 读取任一注册表条目的 aliases
- **THEN** 该列表 MUST 为空

#### Scenario: 不挂载别名路由

- **WHEN** 客户端请求无前缀路径（如 `/messages`）或其他别名形式
- **THEN** 响应 MUST 为 404

### Requirement: 注册表登记的初始端点集合

The registry MUST include the currently served Anthropic and models endpoints with status `live`. Endpoints of other protocols that are not yet mounted MUST be registered with status `planned`; their concrete status is owned by the change that implements them, not by this capability.

#### Scenario: 既有对外端点登记为 live

- **WHEN** 读取注册表
- **THEN** `GET /v1/models`、`POST /v1/messages`、`POST /v1/messages/count_tokens`、`POST /cc/v1/messages`、`POST /cc/v1/messages/count_tokens` MUST 均存在且 status 为 live

#### Scenario: 流式能力标注

- **WHEN** 读取 `POST /cc/v1/messages` 条目
- **THEN** 其 stream 标志 MUST 为 true，且 summary 或 client hints MUST 说明它与 `/v1/messages` 的流式行为差异（缓冲 vs 增量）
