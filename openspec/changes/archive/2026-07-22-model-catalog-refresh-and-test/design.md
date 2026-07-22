## Context

参考文档：docs/model-refresh-and-test-optimization-design.md（2026-07-22）。

**当前状态（kiro-rs）：**

- get_models 返回硬编码模型列表；models_list.txt 不参与运行时
- map_model / get_context_window_size 负责请求侧映射，与账号真实可用模型无关
- select_next_credential 仅在 model 名含 opus 时用 supports_opus() 过滤
- Admin 已有 token force-refresh、balance（getUsageLimits）、profile 解析；无 ListAvailableModels / models refresh / test

**参考实现（Kiro-Go）：**

- ListAvailableModels → pool.modelLists + Handler.cachedModels
- GetNextForModelExcluding 按模型路由；无列表时乐观放行
- Admin：POST .../models/refresh、POST .../test；创建/启用账号异步刷模型

**约束：** Surgical 改动；缓存未就绪兼容现状；禁止密钥入库；实现前本 change 工件齐全。

## Goals / Non-Goals

**Goals:**

- 按凭据拉取真实模型目录并缓存
- 全局 catalog 驱动 GET /v1/models（空则静态 fallback）
- 选号按 model set 过滤（冷启动乐观）
- Admin 可刷新/查看/test
- 可验证：单元 + mock 集成 + 明确验收场景

**Non-Goals:**

- 重写 priority/balanced 负载均衡语义
- 首期强制 DB 持久化模型缓存（可选 JSON 旁路可后置）
- 完整复刻 Kiro-Go Admin UI 视觉
- 用动态目录完全取代 map_model alias 规则
- 跨凭据计费预测、Docker/CI 大改

## Decisions

### D1: 双层缓存而非仅全局列表

- **选择**：凭据级 HashSet<modelId> + 全局聚合 Vec<UpstreamModelInfo>
- **原因**：路由需要账号维度；对外列表需要并集
- **备选**：仅全局列表 → 无法避免选中不支持该模型的账号

### D2: 冷启动乐观放行

- **选择**：凭据无 model_set 或 set 为空时视为「未就绪」，不因缺缓存拒绝
- **原因**：与 Kiro-Go accountHasModel 一致，避免启动阻塞
- **缓解**：添加/启用后异步刷新；可选启动预热

### D3: ListAvailableModels host 固定 us-east-1

- **选择**：https://codewhisperer.us-east-1.amazonaws.com（与 profile.rs / Kiro-Go 一致）
- **原因**：避免首期纠缠 api_region 分片不确定性
- **备选**：跟随凭据 api_region — 留作后续配置化

### D4: /v1/models 不在请求路径同步全量拉取

- **选择**：读缓存；空则静态 fallback；刷新由 Admin/生命周期/后台预热触发
- **原因**：避免客户端超时；Go 在缓存空时同步 refresh 的风险更高

### D5: Test 走最小真实 Kiro 非流式路径

- **选择**：ensure_token → 构造极短 generate（默认 claude-sonnet-4.6，小 max_tokens）
- **原因**：覆盖 token/profile/proxy/模型权限；比仅 List/Usage 更有信号
- **备选**：只做 ListAvailableModels 健康检查 — 可作为轻量模式后置

### D6: supports_opus 保留为附加过滤

- **选择**：model_set 为主过滤；opus+FREE 仍拒绝
- **原因**：订阅元数据可在 model 缓存缺失时兜底

### D7: 模块落点

- 新：src/kiro/models_api.rs（或等价）、available_models 类型、catalog 状态
- 改：	oken_manager（缓存/选择/生命周期）、admin/*、anthropic/handlers::get_models
- 消息 SSE 转换核心尽量不动

## Risks / Trade-offs

- **[Risk] 同步刷新阻塞** → Admin 可同步；chat 与 /v1/models 不强制同步全量拉取
- **[Risk] 乐观窗口选错号** → 异步预热 + 现有失败计数/切换
- **[Risk] 上游 modelId 与 map_model 不一致** → 保留 map_model；监控未映射 id
- **[Risk] Test 消耗配额** → 极小 max_tokens；默认模型可配置
- **[Risk] 并发写缓存** → Mutex/RwLock；按凭据更新再 merge 全局
- **[Risk] 改动扩散** → 禁止顺手重构 converter/SSE；PR 聚焦新文件与薄接入

## Migration Plan

1. 部署后旧客户端：缓存空时 /v1/models 与今日静态一致
2. 运维调用全量 refresh 后列表变为动态
3. 回滚：关闭动态读路径或回退版本；静态 fallback 始终可用
4. 无需迁移 credentials.json 主字段（catalog 内存或旁路文件）

## Open Questions

- 是否首期持久化 model_catalog.json（建议 P1，非阻塞）
- thinking 后缀是否配置化（默认 -thinking，与现有列表一致）
- 全量 refresh 默认并发度（建议串行或 2–3，避免触发上游限流）
- Admin UI 是否纳入本 change 首 PR 还是 API-only 首发（建议 API 先，UI 可选任务）

## 验证策略

- cargo test：解析 fixture、merge、选择过滤、Admin 错误映射
- mock HTTP：ListAvailableModels 200/403；test 成功/失败
- 手工（密钥不入库）：refresh → /v1/models；test 默认模型
- openspec validate --all；完成前 git status --short
