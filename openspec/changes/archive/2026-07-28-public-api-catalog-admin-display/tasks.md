## 1. Public API Catalog 模块

- [x] 1.1 新增 `src/public_api/mod.rs`、`catalog.rs`：`EndpointStatus` / `AuthKind` / `PublicEndpoint` 与静态 `catalog()`
- [x] 1.2 登记 5 条 live（`/v1/models`、`/v1/messages`、`/v1/messages/count_tokens`、`/cc/v1/messages`、`/cc/v1/messages/count_tokens`）
- [x] 1.3 登记 3 条 planned（`/v1/chat/completions`、`/v1/responses`、`GET /v1/responses/{id}`），aliases 全为空
  - 时点注记：本任务完成时为 3 条 planned；Phase B/C 已把前两条翻转为 `live`，现存 planned 仅 `GET /v1/responses/{id}`（`catalog.rs:192`，实测 404）
- [x] 1.4 `client_hints` 写入：OpenAI Base URL 需带 `/v1`、Models 需鉴权（D6）、`/cc/v1` 缓冲流差异、model 回显原值（D9）
- [x] 1.5 `src/main.rs` 加 `mod public_api;`
- [x] 1.6 单测：id 唯一、`(method, path)` 唯一、live 项字段非空 → verify：`cargo test public_api`

## 2. 防漂移契约

- [x] 2.1 单测 `live ⊆ routes`：遍历 live 条目打真实 Router，断言非 404（401 算命中）
- [x] 2.2 单测 `planned ∉ routes`：遍历 planned 条目打真实 Router，断言 404
- [x] 2.3 verify：`cargo test` 两条均通过；手工把某 planned 改 live 确认测试转红后改回

## 3. Admin 只读接口

- [x] 3.1 `src/public_api/dto.rs`：`PublicApiResponse` / `ServerSummary` / `FamilyGroup` / `EndpointDto` + curl 示例生成（key 用 `API_KEY` 占位）
- [x] 3.2 `src/admin/` 新增 `GET /api/admin/public-api` handler + 路由挂载（受 `admin_auth_middleware`）
- [x] 3.3 apiKey 掩码沿用 `main.rs:212` 现有规则；`suggestedBaseUrl` 未配置时为 `null`
- [x] 3.4 单测：未认证 401；正则断言响应不含完整 apiKey；示例含 `API_KEY` 占位符
- [x] 3.5 verify：`cargo test admin` + 本地 curl 带/不带 admin key

## 4. 启动日志改造

- [x] 4.1 `src/main.rs:214-218` 改为遍历 catalog live 条目打印 `method path`
- [x] 4.2 Admin 段落（`:220-234`）保留手写，加注释说明 catalog 只覆盖 Public Client API
- [x] 4.3 verify：启动服务观察输出与 catalog 一致

## 5. Admin UI 面板

- [x] 5.1 `admin-ui/src/types/api.ts` 补 DTO 类型
- [x] 5.2 `admin-ui/src/api/public-api.ts` 调 `GET /api/admin/public-api`
- [x] 5.3 `admin-ui/src/components/public-api-panel.tsx`：服务概要 + 分组端点卡（status badge / stream 徽章 / 复制）+ 客户端配方表 + 注意区
- [x] 5.4 `dashboard.tsx` 顶栏加「API 端点」按钮（与运行时设置并列）
- [x] 5.5 文案：区分对外 API 与上游 endpoint；planned 标未启用；OpenAI Base URL 带 `/v1`；Models 需鉴权；`/cc/v1` 流式差异
- [x] 5.6 复用 card/badge/dialog，遵循 `settings-panel.tsx` 暗色配色
- [x] 5.7 verify：`pnpm build`（tsc + vite）

## 6. 门禁与收尾

- [x] 6.1 `openspec validate --all`
- [x] 6.2 `cargo build` + `cargo test`（全量，确认 Anthropic 侧零回归）
- [x] 6.3 `git status --short` 确认无 config.json / credentials 误入
- [x] 6.4 README 同步对外端点与启动日志说明（AGENTS.md 同步纪律）
- [x] 6.5 evidence/ 落盘真实命令输出
