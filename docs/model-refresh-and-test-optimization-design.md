# 刷新模型与测试模型能力优化方案（参考 Kiro-Go）

> 状态：设计文档（未实现）
> 日期：2026-07-22
> 范围：对比 Kiro-Go 与当前 kiro-rs 的「模型列表刷新 / 按模型路由 / 账号模型探测」实现，给出可落地的优化方案
> 分析手段：双项目 CodeGraph 索引 + 源码精读（ListAvailableModels、Admin API、账号池路由、Admin UI、`/v1/models`）

---

## 1. Context and motivation

### 1.1 背景

kiro-rs 与 Kiro-Go 都是 Anthropic/Claude 兼容代理。模型相关能力不止是「对外暴露模型名」，还包括：

1. **从上游拉取真实可用模型**（`ListAvailableModels`）
2. **缓存并驱动账号路由**（某账号是否支持某模型）
3. **Admin 手动刷新 / 查看**
4. **对指定账号做真实推理探测**（test）
5. **对外 `GET /v1/models` 与映射逻辑保持一致**

当前 kiro-rs 在「凭据 CRUD / Token 刷新 / 余额」上已较成熟，但模型侧仍以**静态表**为主；Kiro-Go 已形成「上游拉取 → 双层缓存 → 按模型路由 → Admin 刷新/测试」闭环。

### 1.2 问题陈述

| 问题 | kiro-rs 现状 | 影响 |
| --- | --- | --- |
| 模型列表静态 | `get_models()` 硬编码；`models_list.txt` 未进入运行时 | 上游新增/下线模型时，客户端列表与真实能力脱节 |
| 无账号级模型缓存 | 仅 `supports_opus()` 按订阅粗过滤 | Free/Enterprise 差异、部分账号无某模型时仍可能被选中，导致失败重试浪费 |
| 无 Admin 刷新入口 | 只有 Token force-refresh、balance | 运维无法主动同步模型目录，也无法按账号排查模型可用性 |
| 无账号测试探测 | 无 `/credentials/{id}/test` | 添加/启用账号后只能等真实流量验证；排障成本高 |
| `/v1/models` 与 `map_model` 双源 | 列表与映射各自维护 | 新模型要改多处，易漏 thinking 变体 / max_tokens |

### 1.3 Goals

- **G1**：引入 Kiro 上游 `ListAvailableModels` 客户端，能按凭据拉取真实模型目录。
- **G2**：建立**双层模型缓存**：全局聚合（服务 `GET /v1/models`）+ 凭据级集合（服务路由过滤）。
- **G3**：Admin 提供单凭据 / 全量刷新、查看缓存；可选真实推理 test。
- **G4**：`select_next_credential` 从「仅 opus 订阅过滤」升级为「模型集合过滤 + 冷启动乐观放行」。
- **G5**：`GET /v1/models` 优先走缓存，失败/空缓存回退静态 fallback（保持兼容）。
- **G6**：分阶段交付，每阶段可独立验证；不破坏现有 Anthropic 协议与 SSE 路径。

### 1.4 Non-goals（首期不做）

- 不重写整个负载均衡算法（priority / balanced 语义保持）。
- 不把模型缓存持久化到 DB；首期内存 + 可选 JSON 文件即可。
- 不强制 Admin UI 完整复刻 Kiro-Go 视觉；先 API，再最小 UI。
- 不把 `map_model` 改为完全动态（上游 modelId 仍需本地 alias/thinking 规则）。
- 不做跨凭据模型能力计费/配额预测。
- 不引入真实密钥入库的 e2e 测试。

---

## 2. 现状深度分析

### 2.1 CodeGraph 摘要

| 项目 | 索引规模 | 与模型能力直接相关的核心符号 |
| --- | --- | --- |
| **Kiro-Go** | ~111 files / 1,539 nodes | `ListAvailableModels`、`refreshModelsCache`、`fetchAndCacheAccountModels`、`apiRefreshAccountModels`、`apiRefreshAllAccountsModels`、`apiTestAccount`、`SetModelList` / `GetNextForModelExcluding`、`buildAnthropicModelsResponse` |
| **kiro-rs** | ~93 files / 1,502 nodes | 静态 `get_models`、`map_model`、`get_context_window_size`、`select_next_credential`（opus 过滤）、`force_refresh_token`、`get_usage_limits`；**无 ListAvailableModels / test account** |

**Kiro-Go `ListAvailableModels` callers（CodeGraph）：**

1. `refreshModelsCache`（全量聚合）
2. `fetchAndCacheAccountModels`（单账号）
3. `apiGetAccountModels`（Admin 实时查看并回写缓存）

**kiro-rs `map_model` impact（CodeGraph）：** 影响 converter 内转换链与 `handlers` 的 messages 路径（约 17 符号），说明「对外模型名」与「上游 modelId」耦合在 converter；刷新模型方案应**少动消息主路径**，优先在 admin / token_manager / 新 models 模块落点。

### 2.2 Kiro-Go：刷新模型能力

#### 2.2.1 上游协议

`proxy/kiro_api.go`：

```text
GET https://codewhisperer.us-east-1.amazonaws.com/ListAvailableModels
  ?origin=AI_EDITOR&maxResults=50
  &profileArn=<optional>
Headers: setKiroHeaders(account)  // Authorization Bearer + Kiro UA 等
```

响应核心字段（`ModelInfo`）：

| 字段 | 含义 |
| --- | --- |
| `modelId` | 上游模型 ID（路由与缓存主键） |
| `modelName` / `description` | 展示 |
| `supportedInputTypes` | 推导 vision / image |
| `rateMultiplier` | 倍率（可选透出） |
| `tokenLimits.maxInputTokens` / `maxOutputTokens` | 限额 |

#### 2.2.2 双层缓存

```text
                    ListAvailableModels(account)
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
   pool.modelLists[accountID]          Handler.cachedModels
   set(modelId)                        mergeUniqueModels(全局聚合)
              │                               │
              ▼                               ▼
   GetNextForModel* 路由过滤           GET /v1/models 展示
```

关键策略：

- **冷启动乐观**：账号尚无 model list 时 `accountHasModel` 返回 true（不阻塞启动流量）。
- **合并去重**：`mergeUniqueModels` 按 modelId 小写去重，补齐 name/description/tokenLimits/inputTypes。
- **thinking 变体**：`buildAnthropicModelsResponse` 对每个真实 modelId 再追加 `modelId + thinkingSuffix`。
- **fallback**：缓存为空时使用硬编码 Claude 列表 + 别名（auto / gpt-4o 等）。

#### 2.2.3 刷新触发面

| 触发 | 行为 |
| --- | --- |
| `GET /models` 缓存空 | `refreshModelsCache()` 同步拉全量启用账号 |
| Admin `POST .../accounts/{id}/models/refresh` | 单账号 fetch + 写双层缓存 |
| Admin `POST .../accounts/models/refresh` | 复用全量 `refreshModelsCache` |
| 账号创建 / 重新启用 / 批量启用 | 异步 `go fetchAndCacheAccountModels` |
| Admin 查看账号模型 | 可实时 List 并回写缓存 |

#### 2.2.4 按模型路由

`pool/account.go`：

- `GetNextForModelExcluding(model, excluded)`：轮询时跳过不支持该 model 的账号。
- 调用点遍布 chat / responses 主路径（`handler.go`、`responses_handler.go`）。
- model 参数应为**去掉 thinking 后缀后的实际 modelId**。

### 2.3 Kiro-Go：测试模型 / 测试账号能力

`POST /admin/api/accounts/{id}/test` → `apiTestAccount`：

1. 查找账号，`ensureValidToken`
2. body 可选 `{ "model": "..." }`，默认 `claude-sonnet-4`
3. 解析 thinking 后缀 → 构造最小非流式请求（`say ok`，`max_tokens=5`）
4. 经完整 `OpenAIToKiro` + `CallKiroAPI` 走真实上游
5. 返回 `{ success, reply, model }` 或 error

语义是 **「真实链路冒烟」**，不是 mock health check。成本极低但能验证：token、profile、proxy、模型权限、上游可达性。

UI 侧：AccountsPanel 已有「刷新全部模型」；单账号详情可 refresh models。test 接口以后端为主（前端接线可后补）。

### 2.4 kiro-rs：模型相关现状

#### 2.4.1 对外模型列表（静态）

`src/anthropic/handlers.rs` · `get_models`：

- 返回硬编码 `Vec<Model>`（sonnet-5 / opus-4.8 / … / haiku + thinking 变体）
- 字段：id / display_name / max_tokens / owned_by 等
- **不访问上游、不读缓存、不读 `models_list.txt`**

`models_list.txt` 与静态列表内容对齐，更像人工同步的清单，不是运行时源。

#### 2.4.2 模型映射

`src/anthropic/converter.rs` · `map_model`：

- 字符串包含规则（sonnet-5 / 4.6 / 4.5、opus 4.5–4.8、haiku）
- 输出 Kiro modelId（如 `claude-sonnet-4.6`）
- `get_context_window_size` 复用同一映射（1M vs 200K）

**特点：** 请求能否发出取决于 map_model，与「某凭据是否真有该模型」无关。

#### 2.4.3 凭据选择（弱模型感知）

`src/kiro/token_manager.rs` · `select_next_credential(model)`：

- 仅当 model 名含 `opus` 时过滤 `!supports_opus()`
- `supports_opus`：订阅标题含 FREE 则否；未知订阅则放行
- **无** 凭据级 model set，**无** GetNextForModel 等价物

#### 2.4.4 Admin 已有能力（可复用）

| 能力 | 路径 | 可复用点 |
| --- | --- | --- |
| 强制刷新 Token | `POST /api/admin/credentials/{id}/refresh` | test/refresh 前 ensure token |
| 余额 / 订阅 | `GET .../balance` → `get_usage_limits` | 已有 CodeWhisperer 风格 HTTP 客户端与 profileArn 附加 |
| profile 解析 | `src/kiro/profile.rs` | ListAvailableModels 同样需要 profileArn query |
| 凭据状态列表 | `GET /credentials` | 可扩展返回 `modelCount` / `modelsCachedAt` |

**缺口：** 无 `ListAvailableModels` 客户端；无 models refresh/test 路由；UI 无对应按钮。

### 2.5 能力对照矩阵

| 能力 | Kiro-Go | kiro-rs | 差距等级 |
| --- | --- | --- | --- |
| 上游 ListAvailableModels | 有 | 无 | 高 |
| 全局模型缓存 | 有 | 无（静态） | 高 |
| 账号级模型集合 | pool.modelLists | 无 | 高 |
| 按模型路由 | GetNextForModel* | 仅 opus 订阅 | 高 |
| Admin 单账号刷新模型 | 有 | 无 | 高 |
| Admin 全量刷新模型 | 有 | 无 | 高 |
| 查看账号模型 | 实时 + 缓存 API | 无 | 中 |
| 账号真实 test | apiTestAccount | 无 | 高 |
| GET /v1/models 动态 | 缓存 + fallback | 纯静态 | 高 |
| thinking 变体自动生成 | 有 | 静态手写 | 中 |
| 新账号/启用自动刷新模型 | 有（async） | 无 | 中 |
| 上游日志 | AppendUpstreamLog | 可借鉴 tracing | 低 |
| 模型映射 alias | MapModel + 硬编码 | map_model | 已有，需对齐动态列表 |

---

## 3. Implementation considerations

### 3.1 约束

1. **Surgical**：改动集中在 `src/kiro/`（新 models 客户端 + token_manager 缓存）+ `src/admin/` + 可选 `admin-ui` + `get_models` 读缓存；消息 SSE 主路径尽量只换「选凭据」过滤条件。
2. **兼容**：缓存未就绪时行为 ≈ 现状（乐观选凭据 + 静态 models 列表）。
3. **安全**：响应与日志禁止 access/refresh token；upstream body 可 debug 级截断。
4. **性能**：ListAvailableModels 不宜每个 chat 请求调用；依赖缓存 + 显式刷新 + 生命周期触发。
5. **OpenSpec**：该变更为跨模块（kiro client / admin / 路由过滤 / 对外 models），实现前应建 OpenSpec change（本文件为设计输入）。

### 3.2 设计原则

- **上游为真源，本地为投影**：聚合列表来自 ListAvailableModels；alias/thinking/fallback 仍由本地规则生成。
- **冷启动不阻塞**：无缓存时放行（与 Go 一致），避免启动死锁。
- **刷新失败不拖垮写路径**：异步刷新 warn 即可；同步 Admin 刷新返回明确错误。
- **test 走真实链路、最小 token**：默认短 prompt + 小 max_tokens。
- **与现有 balance/profile 客户端风格一致**：复用 headers、UA、proxy、profileArn 解析模式。

### 3.3 推荐方案 vs 备选

| 方案 | 描述 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- | --- |
| **A. 对齐 Kiro-Go 双层缓存 + Admin + test** | 完整移植语义 | 能力闭环、可运维 | 实现量中等 | **推荐** |
| B. 仅动态 `/v1/models`，不做按模型路由 | 只改展示 | 改动小 | 选错账号问题仍在 | 不推荐作为终点 |
| C. 配置文件驱动模型表 | 运维手改 models.json | 无上游依赖 | 永远滞后、无账号差异 | 仅作 fallback |

**推荐 A**，分阶段落地（见 §7）。

---

## 4. High-level behavior

### 4.1 刷新模型（目标态）

```text
触发（手动 / 生命周期 / 缓存 miss）
    │
    ▼
ensure_token(credential)
    │
    ▼
GET ListAvailableModels (+ profileArn)
    │
    ├─ 成功 → 写 credential.model_set
    │         merge → global model_catalog
    │         返回 count / models
    └─ 失败 → 保留旧缓存；Admin 返回 5xx + 错误摘要
```

### 4.2 对外 GET /v1/models（目标态）

```text
读 global model_catalog
    │
    ├─ 非空 → 映射为 Anthropic Model[]
    │         + 本地 thinking 变体
    │         + 可选 alias
    └─ 空 → 现有静态 fallback（与今日一致）
```

### 4.3 按模型选凭据（目标态）

```text
map_model(request.model) → kiro_model_id
    │
    ▼
select_next_credential(Some(kiro_model_id))
    │
    ├─ 凭据无 model_set 或 set 为空 → 乐观放行（再应用 supports_opus 若需要）
    ├─ set 含 model → 候选
    └─ set 不含 → 跳过
```

说明：`supports_opus` 可保留为**额外保险**（订阅元数据），但主过滤应是 model_set。

### 4.4 测试模型 / 测试凭据（目标态）

```text
POST /api/admin/credentials/{id}/test
body: { "model"?: string }  // 默认 claude-sonnet-4.6 或配置默认
    │
    ▼
ensure_token → 构造最小 Anthropic Messages 或直接 Kiro generate
    │
    ▼
非流式短回复
    │
    ▼
{ success, model, reply?, latency_ms, error? }
```

可选增强：test 成功后顺带 refresh 该凭据 model_set（一次操作完成「验活 + 同步模型」）。

---

## 5. Domain design

### 5.1 新模块建议

| 模块 | 建议路径 | 职责 |
| --- | --- | --- |
| 上游客户端 | `src/kiro/models_api.rs`（或 `list_models.rs`） | `list_available_models(creds, token, proxy)` |
| 类型 | `src/kiro/model/available_models.rs` | `UpstreamModelInfo`、序列化 |
| 缓存 | `MultiTokenManager` 内或独立 `ModelCatalog` | 全局 Vec + per-id HashSet |
| Admin API | `src/admin/{router,handlers,service,types}.rs` | refresh / list / test |
| 对外列表 | `src/anthropic/handlers.rs#get_models` | 读 catalog + fallback |
| UI（二期） | `admin-ui` credentials 操作列 | 按钮：刷新模型 / 测试 |

### 5.2 上游客户端行为

对齐 Go：

1. Base：`https://codewhisperer.us-east-1.amazonaws.com`（与 profile.rs 一致；是否走凭据 api_region 见 §5.6）
2. Path：`/ListAvailableModels?origin=AI_EDITOR&maxResults=50`
3. Query：有效 profileArn 时附加
4. Headers：与 `get_usage_limits` / `ListAvailableProfiles` 同风格（Bearer、UA、optout 等）
5. Proxy：凭据级 `effective_proxy`
6. 非 200：返回 status + body 摘要
7. 200：解析 `models[]`

### 5.3 缓存数据结构（逻辑）

```text
ModelCatalog {
  global: Vec<UpstreamModelInfo>,      // 聚合去重
  updated_at: Option<DateTime>,
  per_credential: HashMap<u64, CredentialModelCache>,
}

CredentialModelCache {
  model_ids: HashSet<String>,          // lower-case keys
  raw: Option<Vec<UpstreamModelInfo>>, // Admin 详情可选
  updated_at: Option<DateTime>,
  last_error: Option<String>,
}
```

持久化（可选，P1）：`data/model_catalog.json` 或与 balance cache 同目录；进程启动预热，减少冷启动乐观窗口。

### 5.4 Admin API 设计

前缀沿用现有 `/api/admin`。

| Method | Path | 说明 |
| --- | --- | --- |
| POST | `/credentials/{id}/models/refresh` | 单凭据拉取并写缓存 |
| POST | `/credentials/models/refresh` | 所有**未禁用**凭据刷新（串行或有限并发） |
| GET | `/credentials/{id}/models` | 返回缓存；`?live=1` 实时拉取并更新 |
| POST | `/credentials/{id}/test` | 真实最小推理探测 |
| GET | `/models/catalog` | 全局聚合缓存摘要（运维） |

**响应示例（刷新）：**

```json
{
  "success": true,
  "credentialId": 3,
  "count": 12,
  "models": ["claude-sonnet-4.6", "claude-opus-4.6", "..."],
  "updatedAt": "2026-07-22T12:00:00Z"
}
```

**全量刷新：**

```json
{
  "success": true,
  "refreshed": 5,
  "failed": 1,
  "globalCount": 14,
  "errors": [{ "credentialId": 2, "error": "HTTP 403: ..." }]
}
```

> 注：Go 的全量接口目前 `failed: 0` 写死，kiro-rs 建议如实统计失败，便于运维。

**测试：**

```json
// request
{ "model": "claude-sonnet-4.6" }

// response
{
  "success": true,
  "model": "claude-sonnet-4.6",
  "reply": "ok",
  "latencyMs": 842
}
```

### 5.5 GET /v1/models 映射规则

1. 取全局缓存 modelId 列表。
2. 每个 modelId 生成 Anthropic `Model`：
   - `id`: 可用上游 id，或同时暴露「客户端友好 id」（若需兼容现有静态 id，见下）
   - `display_name`: modelName 或本地表
   - `max_tokens`: tokenLimits.maxOutputTokens 或本地表
3. 追加 thinking 变体：`{id}-thinking` 或配置 suffix（与 converter 解析规则一致）。
4. 可选 alias：仅在配置开启时附加（默认可不做 Go 的 gpt-4 别名，避免污染 Claude 客户端）。
5. 缓存空：完整返回今日静态列表（行为不变）。

**兼容注意：** 当前静态 id 含日期型（`claude-sonnet-4-5-20250929`）与短 id 混用；上游多为 `claude-sonnet-4.5` 点分形式。动态列表应以**上游 modelId** 为准，静态 fallback 保留旧 id；`map_model` 已能把多种写法归一到点分 id。

### 5.6 Region / Endpoint

- profile / ListAvailableModels 在 Go 与 kiro-rs profile 路径均固定 `codewhisperer.us-east-1.amazonaws.com`。
- 首期对齐：**ListAvailableModels 固定 us-east-1 host**，不跟随 api_region（与 profile 一致）。
- 若后续确认区域分片，再配置化 host 模板。

### 5.7 生命周期挂钩

| 事件 | 行为 |
| --- | --- |
| 添加凭据成功且已启用 | 异步 refresh models（失败仅 log） |
| 凭据从禁用→启用 | 异步 refresh models |
| 删除凭据 | 移除 per-credential 缓存；可选重建 global |
| 进程启动 | 可选：对启用凭据后台预热（限并发 2–3） |
| GET /v1/models 缓存空 | 可选后台 refresh（避免阻塞 HTTP；首期可只返回 fallback） |

与 Go 差异建议：`GET /v1/models` **不要同步**拉全量（避免客户端超时）；用 fallback + 后台预热更稳。

### 5.8 Test 实现路径选择

| 路径 | 说明 | 推荐 |
| --- | --- | --- |
| 经 Anthropic handlers 内部调用 | 覆盖中间件/转换，但耦合重 | 否 |
| 直接构造 Kiro payload + provider 单次非流式 | 与生产 generate 一致、可控 | **是** |
| 仅 ListAvailableModels / getUsageLimits | 不能证明推理通路 | 仅作轻量 check 备选 |

默认 model：`claude-sonnet-4.6`（与当前主力一致）；请求体可覆盖。若 map 失败返回 400。

---

## 6. Error handling and UX

| 场景 | HTTP | 行为 |
| --- | --- | --- |
| 凭据不存在 | 404 | `Credential not found` |
| Token 刷新失败 | 502/500 | 明确 token 错误，不写空缓存覆盖 |
| ListAvailableModels 非 200 | 502 | 保留旧缓存；返回上游摘要 |
| 全量刷新部分失败 | 200 | success=true 但 failed>0 + errors[] |
| Test 上游失败 | 502 | success=false + error |
| Test 模型不支持 / map 失败 | 400 | 本地错误，不打上游 |
| 缓存 miss 读 models | 200 | fallback 静态列表 |

Admin UI（二期）：

- 凭据行：`刷新模型`、`测试`
- 顶栏：`刷新全部模型`
- 展示：模型数量徽章；详情抽屉列出 modelId
- 测试对话框：选 model + 显示 reply/latency

---

## 7. Implementation outline（分阶段）

### Phase 0 — 基线与契约（0.5d）

- 冻结 API 草案（本文件 §5.4）
- 建 OpenSpec change：`model-catalog-refresh-and-test`
- 确认默认 test model、thinking suffix、是否持久化缓存

### Phase 1 — 上游客户端 + 类型 + 单测（1–2d）

1. 新增 `UpstreamModelInfo` 与 JSON 解析测试（固定 fixture，不联网）。
2. 实现 `list_available_models`（复用 http client / headers / proxy 模式）。
3. 单元测试：URL 构造、profileArn 编码、非 200 错误、空 models。

**成功标准：** `cargo test` 覆盖解析与 URL；可用 mock server 或 wiremock 风格测试（若项目已有同类模式则跟随）。

### Phase 2 — ModelCatalog + TokenManager 集成（1–2d）

1. 在 `MultiTokenManager`（或并列组件）维护 catalog。
2. `refresh_models_for(id)` / `refresh_models_all()`。
3. `select_next_credential` 增加 model_set 过滤 + 冷启动乐观。
4. 删除凭据时清理缓存。
5. 添加/启用后 spawn 异步刷新。

**成功标准：** 单元测试覆盖：有缓存时过滤；无缓存时放行；opus+FREE 仍拒绝。

### Phase 3 — Admin API（1d）

1. 路由 / handler / service / types。
2. 单测 service 层（mock token_manager）。
3. 文档片段更新 README Admin 表。

### Phase 4 — GET /v1/models 读缓存（0.5–1d）

1. `get_models` 注入 catalog 读口（State）。
2. 动态构建 + thinking 变体 + 静态 fallback。
3. 保持响应字段兼容现有客户端。

### Phase 5 — Test 凭据（1d）

1. `POST /credentials/{id}/test`。
2. 最小 Kiro 非流式调用；超时可配置（默认 30–60s）。
3. 单测：默认 model、非法 model、token 失败。

### Phase 6 — Admin UI（可选，1d）

1. API client + hooks。
2. 按钮与 toast；详情展示模型列表。

### Phase 7 — 验证与收尾

- `cargo test` 相关模块
- `openspec validate`
- 手工：刷新 → `/v1/models` 变化；test 成功/失败路径
- 不提交真实凭据与 `.codegraph/`

---

## 8. Testing approach

### 8.1 单元

- JSON fixture → `UpstreamModelInfo`
- `merge_unique_models` 去重与字段合并
- `account_has_model` 冷启动 / 命中 / 未命中
- `select_next_credential` 与 model_set / opus 组合
- Admin 错误映射（404/502/400）

### 8.2 集成（无真实密钥）

- mock HTTP：ListAvailableModels 200 / 403 / 超时
- mock generate：test 接口 success / upstream error

### 8.3 手工（本地真实环境，密钥不入库）

1. 导入 1 个可用凭据 → 自动或手动 models/refresh → count > 0
2. `GET /v1/models` 含上游 id
3. 禁用仅支持子集的账号后，请求稀有模型应落到正确凭据（若可构造）
4. test 默认 model 返回 reply
5. 错误 refreshToken 的 test 返回清晰失败

### 8.4 回归

- 现有 `map_model` / converter / token_manager / admin balance 测试全绿
- 缓存为空时 `/v1/models` 与今日静态列表一致（快照或关键 id 断言）

---

## 9. Acceptance criteria

1. **Given** 至少一个启用凭据且 token 有效，**when** `POST /api/admin/credentials/{id}/models/refresh`，**then** 返回 count≥1 且后续 `GET .../models` 可见相同集合。
2. **Given** 全局缓存非空，**when** `GET /v1/models`，**then** data 包含缓存中的 modelId 及其 thinking 变体（若启用规则）。
3. **Given** 全局缓存为空，**when** `GET /v1/models`，**then** 响应与当前静态 fallback 行为兼容（关键模型仍存在）。
4. **Given** 凭据 A 缓存不含 model X、凭据 B 含 X，**when** 请求 model X，**then** 选择逻辑跳过 A（在两凭据均可用时）。
5. **Given** 凭据尚无 model 缓存，**when** 任意模型请求，**then** 不因缺缓存而拒绝（乐观放行）。
6. **Given** 有效凭据，**when** `POST .../test` 默认 model，**then** success=true 且 reply 非空或可解释的上游内容。
7. **Given** 无效 token，**when** refresh models 或 test，**then** 4xx/5xx 且不静默写空列表覆盖旧缓存。
8. **Given** 全量刷新部分失败，**when** 调用 all refresh，**then** 响应包含 refreshed/failed 与错误明细。
9. 不记录、不提交真实 token；`git status` 无 credentials / `.codegraph` 误入。
10. 相关 `cargo test` 在本会话真实执行并通过（实现阶段）。

---

## 10. Future-proofing

- **分页**：上游 `maxResults=50`；若出现 nextToken，客户端预留循环拉取。
- **多区域 host**：配置模板化 base URL。
- **模型能力位**：inputTypes → 对外 supports_image / modalities（对齐 Go buildModelInfo）。
- **缓存 TTL**：定时后台刷新（如 6h）+ 手动刷新。
- **与 OpenSpec 模型映射变更联动**：动态目录可提示「map_model 未覆盖的新 modelId」。
- **导出**：Admin 导出全局 catalog 供离线对照。

---

## 11. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| 同步刷新阻塞请求 | Admin 可同步；对外 /v1/models 与 chat 不走同步全量拉取 |
| 乐观放行导致短窗口选错号 | 启动预热 + 添加后异步刷新；失败重试仍走现有 failure 计数 |
| 上游 modelId 与 map_model 不一致 | 保留 map_model；动态列表以上游为准；监控未映射 id |
| Test 消耗配额 | 默认 max_tokens 极小；UI 二次确认（可选） |
| 并发刷新写缓存 | catalog 用 Mutex/RwLock；按凭据粒度更新再 merge global |
| 影响面扩散 | 禁止顺手重构 converter/SSE；PR  diff 聚焦新文件 + 薄接入点 |

---

## 12. 关键文件清单（实现时）

### 预计新增

- `src/kiro/models_api.rs`（或等价）
- `src/kiro/model/available_models.rs`
- 可选 `src/kiro/model_catalog.rs`
- 测试 fixture：`tests/fixtures/list_available_models.json`

### 预计修改

- `src/kiro/mod.rs` / `model/mod.rs` — 模块导出
- `src/kiro/token_manager.rs` — 缓存、选择过滤、生命周期
- `src/admin/router.rs` / `handlers.rs` / `service.rs` / `types.rs`
- `src/anthropic/handlers.rs` — `get_models` 读缓存
- `src/main.rs` — 状态注入（若 catalog 独立）
- `README.md` — Admin API 表
- 可选 `admin-ui/src/**`

### 明确不改（首期）

- SSE 流解析、工具调用转换核心
- Docker / CI（除非增加测试步骤）
- 凭据文件格式（除非增加非敏感 model cache 旁路文件）

---

## 13. 与「添加账号优化」文档的关系

已有 `docs/add-account-optimization-design.md` 覆盖导入 / 身份 / SSO。本文件专注 **模型目录与探测**。交叉点：

- 添加账号成功后的 **异步 models refresh** 应接入添加流程（两文档 Phase 对齐）。
- userId upsert 不阻塞本方案；catalog 以 credential id 为键即可。

建议 OpenSpec 变更拆分：

1. `add-account-optimization`（若尚未建）
2. `model-catalog-refresh-and-test`（本文）

可并行设计，实现上优先 **Phase1–3 模型刷新**，test 紧随，UI 最后。

---

## 14. 附录：关键源码锚点

### Kiro-Go

| 主题 | 位置 |
| --- | --- |
| ListAvailableModels | `proxy/kiro_api.go:93-131` |
| ModelInfo | `proxy/kiro_api.go:564+` |
| 缓存刷新 / Admin refresh | `proxy/handler.go:560-672` |
| /v1/models 构建 | `proxy/handler.go:450-558` |
| 账号池模型集合与路由 | `pool/account.go:145-257` |
| 测试账号 | `proxy/handler.go:3393-3458` |
| 创建/启用触发 | `proxy/handler.go:2443+`, `2566+`, `2728+` |
| 查看账号模型 | `proxy/handler.go:3640-3687` |
| UI 全量刷新 | `web/src/components/AccountsPanel.jsx:72+` |

### kiro-rs

| 主题 | 位置 |
| --- | --- |
| 静态 get_models | `src/anthropic/handlers.rs:70+` |
| map_model / 上下文窗口 | `src/anthropic/converter.rs:78-122` |
| 选号与 opus 过滤 | `src/kiro/token_manager.rs:801-848` |
| supports_opus | `src/kiro/model/credentials.rs:266-279` |
| getUsageLimits | `src/kiro/token_manager.rs:323+` |
| profile / CodeWhisperer host | `src/kiro/profile.rs` |
| Admin 路由 | `src/admin/router.rs` |
| 静态清单文件 | `models_list.txt` |

---

## 15. 结论

kiro-rs 在凭据与 Token 体系上已具备承接「刷新模型 / 测试模型」的底座（proxy、profileArn、usage limits、Admin 框架齐全），缺口集中在：

1. 缺少 **ListAvailableModels** 与 **双层缓存**
2. 缺少 **按模型路由**
3. 缺少 **Admin 刷新 / 测试** 与 **动态 /v1/models**

对齐 Kiro-Go 语义、按 Phase 1→5 落地，可在不扰动 SSE 主协议的前提下补齐运维闭环，并为后续多订阅/多套餐账号池提供正确的模型维度调度能力。

**下一步（实现前）：** 用 OpenSpec `openspec-new-change` / `openspec-propose` 建立 change，再经 `openspec-superpowers-bridge` 后开工。

