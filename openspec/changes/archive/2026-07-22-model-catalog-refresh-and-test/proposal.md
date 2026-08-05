## Why

kiro-rs 目前对模型能力以静态表为主：GET /v1/models 硬编码，select_next_credential 仅按订阅粗过滤 opus，Admin 只能强制刷新 Token / 查余额。Kiro 上游已提供 ListAvailableModels，Kiro-Go 已形成「拉取 → 双层缓存 → 按模型路由 → Admin 刷新/测试」闭环。上游模型目录变更时，静态列表与账号能力会脱节，导致客户端展示不准、错误账号被选中与排障困难。

## What Changes

- 新增上游 ListAvailableModels 客户端（对齐 CodeWhisperer REST + profileArn/proxy/headers 语义）
- 新增双层模型缓存：凭据级 model set + 全局聚合 catalog
- Admin 新增：单凭据/全量 models refresh、查看缓存模型、凭据真实推理 test
- 凭据选择升级：在现有 priority/balanced 与 supports_opus 之上，按 model set 过滤（冷启动无缓存乐观放行）
- GET /v1/models 优先读全局缓存并生成 thinking 变体；缓存空时回退现有静态列表（行为兼容）
- 生命周期：添加/启用凭据后异步刷新该凭据模型缓存
- **非 BREAKING**：缓存未就绪时行为近似现状；不改变 SSE 协议与现有 credentials 文件主 schema

## Capabilities

### New Capabilities

- model-catalog: 上游模型目录拉取、双层缓存、Admin 刷新/查看、动态 GET /v1/models（含 fallback 与 thinking 变体）
- model-aware-routing: 按请求模型过滤可用凭据（冷启动乐观、与 opus 订阅过滤共存）
- credential-model-test: Admin 对指定凭据发起最小真实推理探测

### Modified Capabilities

- （无）现有 openspec/specs/ 能力不修改需求文本；本变更为新增能力

## Impact

- **代码**：src/kiro/（新 models API + catalog、token_manager 选择/生命周期）、src/admin/*（路由/类型/service）、src/anthropic/handlers.rs（get_models 读缓存）、可选 admin-ui
- **API**：新增 Admin endpoints；GET /v1/models 内容可从静态变为动态（结构字段保持兼容）
- **依赖**：无新外部 crate 强制要求；复用现有 reqwest/headers/proxy 模式
- **运维**：可主动同步模型目录与冒烟探测；可能产生少量 ListAvailableModels / 短 test 上游调用
- **文档**：README Admin API 表；设计输入见 docs/model-refresh-and-test-optimization-design.md
- **风险类型**：Admin、模型映射/目录、跨模块（kiro + admin + anthropic 列表）
