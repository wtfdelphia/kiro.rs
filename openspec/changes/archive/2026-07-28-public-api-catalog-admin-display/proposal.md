## Why

对外端点事实目前分散在三处且互相漂移：`src/anthropic/router.rs:49-74` 的真实挂载、`src/main.rs:214-235` 手写的启动日志列表、以及 `docs/` 下四份并行设计稿。改一处漏两处。

Admin 侧（`src/admin/router.rs`）覆盖凭据运维、模型目录、运行时设置（proxy / endpoint / auth / client-identity），但**没有任何「对外 API 怎么接」的展示**：用户拿不到 Base URL、path、鉴权头，只能靠猜或读源码。

同时 `/api/admin/settings/endpoint` 指的是**上游 Kiro 端点（ide）**，与「对外 Public API 端点」概念同名不同义，是当前最高频的配置误解来源。

后续要新增 OpenAI Chat Completions 与 Responses 端点（见 Phase B/C change），需要先有一个不会撒谎的端点注册表，否则新端点上线时漂移面从 3 处扩到 5 处。

设计输入：`docs/multi-protocol-api-design.md`（跨 Phase 设计定稿，由四份并行稿合并而成），经 Kiro-Go（`proxy/handler.go`、`translator.go`、`responses_*.go`）与 sub2api（`backend/internal/service/gateway_websearch_emulation.go`、`openai_alpha_search.go`）源码对照复核。

## What Changes

- **新增 Public API Catalog**：`src/public_api/` 模块，canonical 端点清单作为路径 / 方法 / 鉴权 / 状态的单一事实源
- **状态不撒谎**：`status = live | beta | planned`；`live` 集合必须与真实 Axum 路由表一致，由单测强制
- **新增 Admin 只读接口** `GET /api/admin/public-api`：返回服务概要（listenHost / port / requireApiKey / apiKey mask / authHeaders）与按协议族分组的端点清单，含 curl 示例
- **启动日志改为遍历 catalog**：消除 `src/main.rs` 手写的第三份列表
- **Admin UI 新增「API 端点」面板**：Base URL、分组端点卡、客户端配方（含 `OPENAI_BASE_URL` 需带 `/v1`、`ANTHROPIC_BASE_URL` 不带的提示）、一键复制
- **OpenAI 端点先登记为 planned**：`/v1/chat/completions`、`/v1/responses` 进 catalog 但不挂载，请求返回 404；Admin 显式标注未启用（Phase A 时点状态；后续 Phase B/C 已分别翻转为 `live`，两者的状态契约归各自能力持有，本能力的 spec 不再断言其具体状态）
- **不改任何现有对外行为**：Anthropic `/v1`、`/cc/v1` 路由、handler、SSE 事件、鉴权模型零改动

## Capabilities

### New Capabilities

- `public-api-catalog`：对外端点注册表的数据模型、canonical 清单、status 语义、`live ⊆ routes` 防漂移契约
- `admin-public-api-view`：`GET /api/admin/public-api` 的响应契约、密钥掩码约束、Admin UI 展示与文案约束

### Modified Capabilities

无。本 change 不修改既有能力的行为契约。

## Impact

- **代码**：新增 `src/public_api/{mod,catalog,dto}.rs`；`src/main.rs`（`mod public_api;` + 启动日志）；`src/admin/{router,handlers,service,types}.rs`（新增只读接口）；`admin-ui/src/api/public-api.ts`、`admin-ui/src/components/public-api-panel.tsx`、`admin-ui/src/components/dashboard.tsx`（入口按钮）、`admin-ui/src/types/api.ts`
- **API**：新增 `GET /api/admin/public-api`（受 `admin_auth_middleware` 保护）。无对外 public API 变更
- **配置**：预留 `publicBaseUrl` 字段（未配置时响应中为 `null`，前端回落 `window.location.origin`）。本 change 不新增必填配置
- **依赖**：新增 dev-dependency `tower`（0.5.2，features: util），用于单测中对真实 Router 发请求以支撑防漂移断言。它已是 axum 的传递依赖，不增加运行时依赖；已登记于 `docs/tooling-sources.md`
- **风险类型**：Admin API（走 OpenSpec）、admin-ui（`pnpm build` 门禁）
- **非目标**：
  - 不实现 OpenAI Chat Completions / Responses 协议（Phase B/C 独立 change）
  - 不做路径别名（`/messages`、`/chat/completions` 等），catalog `aliases` 字段首版全为空数组
  - 不改 `/api/admin/settings/endpoint`（上游 ide 端点）的语义或用途
  - 不返回完整 client apiKey，示例统一用占位符 `API_KEY`
  - 不改 `/v1/models` 的鉴权口径（保持受 `require_api_key` 约束，比 Kiro-Go 严；面板须标注需鉴权）
