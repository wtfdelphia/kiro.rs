# 模型查询联动与 Admin 设置优化方案（参考 Kiro-Go）

> 状态：设计文档（未实现）
> 日期：2026-07-23
> 范围：
> 1. 修复 / 优化 GET /v1/models 空列表与「列表有但不可用」
> 2. Admin「模型列表」与「查看模型 / 刷新模型」数据联动
> 3. 凭据「测试」模型选择：可选列表 + 可手动输入
> 4. 「重置失败」按钮语义重构为「余额刷新」，并与「查询信息」协作
> 5. 参考 Kiro-Go：API Key 验证开关与配置页、Kiro 端点设置、出站代理设置的落地优化
>
> 分析手段：kiro-rs + Kiro-Go 双项目 CodeGraph 索引 + 源码精读

---

## 1. Context and motivation

### 1.1 背景

model-catalog-refresh-and-test 与 admin-ui-model-ops-entrypoints 已归档实现。后端具备 ListAvailableModels、双层缓存、GET /v1/models 读缓存、按模型路由、凭据 test；Admin UI 有查看/刷新/测试入口，但测试仅手输模型，模型列表未驱动全局/测试选项。客户端 API Key、全局代理、默认端点仍依赖 config.json 手改 + 重启；Kiro-Go 已有 Admin 热更新。

注：docs/model-refresh-and-test-optimization-design.md 页眉仍写未实现，以代码与 openspec archive 为准。本文件聚焦已实现后的缺口与设置面扩展。

### 1.2 问题陈述

| # | 问题 | 根因 | 影响 |
| --- | --- | --- | --- |
| P1 | /v1/models 无模型或模型不可用 | catalog 未预热走静态 fallback；动态透传上游 id 与静态连字符 id 混用；map_model 未覆盖则失败 | 假可用 / 映射失败 |
| P2 | 模型列表未与查看/刷新关联 | 刷新写缓存但 UI 无 modelCount/全局视图；test 不读 models API | 运维心智割裂 |
| P3 | 测试只能手输模型 | credential-test-dialog 仅 Input | 易输错 |
| P4 | 重置失败语义弱 | 只清计数 re-enable，不访问上游 | 更需余额刷新 |
| P5 | 无 Admin 运行时配置 | 无 settings API；apiKey 总是校验 | 改配置需重启 |

### 1.3 Goals

- G1：/v1/models 返回可 map 且可上游使用的 id；空缓存可观测
- G2：查看/刷新/测试下拉共享模型缓存
- G3：测试 Select + 手动输入
- G4：卡片「重置失败」→「刷新余额」，与「查询信息」互补
- G5：Admin 设置：API Key 开关、默认端点、出站代理热更新
- G6：文档落 docs/；实现前 OpenSpec

### 1.4 Non-goals

- 不首期复刻 Go 多 API Key 配额面板
- 不首期做 CW/AmazonQ 三端点 fallback（rs 仅 ide）
- 不强制 catalog 持久化 DB
- 不重写负载均衡
- 不改 Admin Cookie 鉴权

---

## 2. CodeGraph 对照

| 项目 | 规模 | 核心符号 |
| --- | --- | --- |
| kiro-rs | 99 files / 1655 nodes | get_models, models_from_catalog, map_model, list_available_models, refresh_models_for, select_next_credential, reset_and_enable, get_balance, test_credential, Config |
| Kiro-Go | 本地 codegraph 可用 | ListAvailableModels, refreshModelsCache, authenticate/RequireApiKey, GetPreferredEndpoint, GetProxyURL, SettingsPanel |

### 当前数据流

```tex
ListAvailableModels
    |-- per_credential.model_ids --> select_next 过滤
    |-- global catalog --> GET /v1/models
Admin 查看模型 --> GET .../models
Test dialog --> 仅手输（未连接 models）
```

### 能力矩阵（摘要）

- 模型上游/双层缓存：两边都有；rs 需 id 归一与预热
- /v1/models：Go 可 miss 同步 refresh；rs 空则静态
- 全局 proxy 热改：Go 有；rs 无
- 端点：Go 三 URL fallback；rs 仅 ide trai
- API Key 开关：Go RequireApiKey；rs 总是校验
- 重置失败 vs 余额：rs 应改卡片主操作为刷新余额

---

## 3. 根因

### 3.1 /v1/models

get_models：catalog 非空用 models_from_catalog，否则 static_fallback_models（非空硬编码）。

「没有模型」常是：鉴权/路径错误、客户端解析失败、或「有列表但不可用」被说成没有。

「不能用」：map_model None；model_set 无候选；上游拒绝；id 方言（4-6 vs 4.6）。

### 3.2 重置失败

POST .../reset = 清 failure 并启用，不查余额。查询信息 = 批量 balance。应拆：卡片刷新余额；reset 降级。

---

## 4. 设计原则

1. Surgical
2. 上游真源、本地投影
3. 可观测 source/updatedA
4. 热更新失败保留旧值
5. 密钥不回传
6. 高风险走 OpenSpec

推荐方案 A：列表正确性 → UI 联动 → 设置热更。

---

## 5. 目标行为

### 5.1 模型闭环

启动预热（并发 2）→ Admin 刷新写缓存 → /v1/models 优先 catalog 并归一 id → map_model → 按 model_set 选凭据。

空缓存：fallback + 后台 refresh（S1，推荐）。

### 5.2 Admin 联动

查看/刷新/测试共享 GET .../models；卡片 modelCount；可选 GET /api/admin/models/catalog。

### 5.3 余额

| 操作 | 范围 |
| --- | --- |
| 刷新余额 | 单卡 force balance |
| 查询信息 | 当前页批量 |
| 恢复/清失败 | 原 reset，条件显示 |

### 5.4 设置

| 区块 | 目标 API |
| --- | --- |
| 代理 | GET/PUT /api/admin/settings/proxy |
| 端点 | GET/PUT /api/admin/settings/endpoint |
| 鉴权 | GET/PUT /api/admin/settings/auth |

---

## 6. Domain design

### 6.1 /v1/models

1. catalog 以上游 modelId 为 canonical（点分）
2. 可选双写连字符别名（map 同结果）
3. thinking: {id}-thinking
4. 默认不暴露 map_model 失败的 id
5. 静态 fallback 契约：每个 id map_model Some
6. 诊断：Admin catalog API 或可选扩展字段 source/coun

### 6.2 UI 联动

CredentialStatusItem 增加 modelCount / modelsUpdatedAt / modelsLastError。

Test dialog：open 拉 models；Select + 自定义 Input；默认 sonnet/第一项/服务端默认 claude-sonnet-4.6。

查看模型项：「用此模型测试」。

### 6.3 余额按钮

balance?force=true 跳过 TTL。卡片主按钮改「刷新余额」。reset 仅失败/禁用时突出。

### 6.4 API Key 开关（对齐 Go 语义）

```json
{ "apiKey": "sk-...", "requireApiKey": true, "adminApiKey": "sk-admin-..." }
```

| requireApiKey | apiKey | 行为 |
| --- | --- | --- |
| false | * | 不校验客户端 key |
| true | 非空 | 常量时间比较 |
| true | 空 | fail-closed 401 |

Admin 只返回 mask。AppState 热更新 + Config::save。多 Key 后置。

### 6.5 端点（概念差异）

Go：三 URL 族 + fallback。
rs：KiroEndpoint trait，当前仅 ide。
首期只配 defaultEndpoint 白名单；endpointFallback 预留。多端点 fallback 另 change。

### 6.6 出站代理

GET/PUT settings/proxy：proxyUrl + 可选认证；空 URL 清除；校验 http/https/socks5。
更新 token_manager 全局 proxy；凭据级仍优先；direct 旁路语义保留。

---

## 7. 错误与 UX

- catalog 空：200 fallback + Admin 提示未刷新
- test 无法 map：400
- force balance 失败：502 保留旧展示
- proxy 非法：400 不改内存
- require=true 清空 key：拒绝或确认后 fail-closed
- 未知 endpoint：400

---

## 8. 分阶段实施

### Phase A（P0，1–2d）模型正确性与联动

models_from_catalog 过滤 unmapped；预热；modelCount；Test Select；invalidate。

验证：cargo test handlers/token_manager；UI 手测。

### Phase B（P0，0.5d）余额按钮

force balance；按钮改名；reset 条件显示。

### Phase C（P1，1d）代理热更新

settings/proxy + UI。

### Phase D（P1，1–1.5d）端点 + API Key 开关

settings/endpoint、settings/auth；middleware 矩阵；README/example 同步。

### Phase E（可选）

多 API Key；多上游 endpoint fallback。

---

## 9. 测试

- 单元：catalog 过滤、static map 契约、balance force、auth 四象限、proxy parse、endpoint 白名单
- 集成：refresh → /v1/models；test 选缓存模型；proxy 写盘
- 手工：冷启动、刷新后可 chat、下拉一致、余额协作、鉴权开关
- 安全：无真密钥；响应无完整 apiKey

---

## 10. 验收标准

Phase A：/v1/models id 可 map；测试可选可输；查看刷新同源；test 绿。
Phase B：主按钮刷新余额；reset 仍可达；查询信息有效。
Phase C/D：代理热更生效；endpoint 合法；requireApiKey 矩阵；文档同步。

---

## 11. 风险

| 风险 | 缓解 |
| --- | --- |
| 过滤后列表变短 | Admin 看 raw；warn 日志 |
| proxy 热更与 in-flight | 仅新请求；文档说明 |
| 关闭鉴权误操作 | 二次确认 |
| 硬塞 Go 三端点 | 概念分离；Phase E |
| 预热限流 | 并发 2 |

---

## 12. OpenSpec

建议两个 change：

1. fix-models-list-and-admin-linkage（A+B）
2. admin-runtime-settings-proxy-endpoint-auth（C+D）

流程：propose → bridge → apply → compliance → verify → verification-before-completion。

---

## 13. 源码索引

### kiro-rs

- src/anthropic/handlers.rs — get_models
- src/anthropic/converter.rs — map_model
- src/kiro/models_api.rs, model/available_models.rs
- src/kiro/token_manager.rs — catalog / select / refresh
- src/admin/* — models/test/reset/balance
- src/model/config.rs
- src/kiro/endpoint/*
- admin-ui credential-card / test-dialog / models-dialog / dashboard

### Kiro-Go

- proxy/kiro_api.go, handler.go, auth.go, kiro.go
- config/config.go, apikeys.go
- web SettingsPanel.jsx, ApiKeysPanel.jsx

### 相关文档

- docs/model-refresh-and-test-optimization-design.md
- docs/add-account-optimization-design.md
- openspec/specs/model-catalog, model-aware-routing
- archive: 2026-07-22-model-catalog-refresh-and-test, 2026-07-23-admin-ui-model-ops-entrypoints

---

## 14. 结论

已有模型目录后端，缺口在：对外列表正确性、Admin 模型数据联动、运行时设置热更。
重置失败应降级；刷新余额与查询信息互补。
交付顺序：A → B → C → D；符合 Surgical 与 OpenSpec 纪律。
