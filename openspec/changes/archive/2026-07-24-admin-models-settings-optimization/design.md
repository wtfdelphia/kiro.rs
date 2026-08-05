## Context

设计输入：docs/admin-models-settings-optimization-design.md（2026-07-23）。
前置能力（已归档）：model-catalog-refresh-and-test、admin-ui-model-ops-entrypoints。

**当前状态（kiro-rs）：**

- ListAvailableModels + 双层缓存 + Admin 刷新/查看/test 已有
- GET /v1/models：catalog 非空用 models_from_catalog（透传上游 id）；空则 static_fallback_models（连字符 id 混用）
- map_model 为请求侧真源；列表 id 与 map 失配会导致「看得见点不了」
- Admin UI：查看/刷新模型与测试对话框数据未打通；测试仅手输
- POST .../reset：清 failure 并 re-enable；GET balance 有 TTL 缓存
- Config：apiKey / proxyUrl / defaultEndpoint 仅启动加载；无 Admin 热更新；客户端 key 总是校验

**参考（Kiro-Go）：**

- SettingsPanel：/admin/api/proxy、/endpoint、settings 热改
- RequireApiKey 总开关 + 多 Key（本 change 只做开关 + 单 key 轮换）
- 端点含义为三 URL 族 fallback；rs 为 KiroEndpoint trait（当前仅 ide）——不可硬塞三 URL

## Goals / Non-Goals

**Goals:**

- /v1/models 暴露的 id 默认可被 map_model 接受（catalog 路径过滤 unmapped；静态契约）
- Admin 模型查看/刷新/测试共享凭据模型缓存；卡片展示 modelCount
- 卡片主操作「刷新余额」（force）；reset 降级
- Admin 热更新 proxy / defaultEndpoint / requireApiKey+apiKey（脱敏）
- 每阶段可独立验证；密钥不回传、不入库

**Non-Goals:**

- 多 API Key 配额与 ApiKeys 面板
- 多上游 endpoint 自动 fallback（ide 以外适配器）
- 模型 catalog DB 持久化
- 重写 priority/balanced
- Admin Cookie 密码鉴权

## Decisions

### D1: catalog 输出过滤 unmapped + canonical 点分

- **选择**：models_from_catalog 对 map_model 失败的 modelId 默认跳过；thinking 仍基于 canonical
- **原因**：避免客户端展示无法请求的模型
- **备选**：双写连字符别名（可选，仅 map 同结果时）

### D2: 空缓存 S1（fallback + 后台预热）

- **选择**：/v1/models 不阻塞同步全量 List；空则静态 fallback，并可后台限并发预热
- **原因**：客户端超时风险低于 Go 同步 refresh

### D3: 测试 UI 读 GET .../models，后端契约不变

- **选择**：Select 选项来自凭据缓存；省略 model 仍用服务端默认
- **原因**：Surgical；不改 test API 形状

### D4: 刷新余额 vs 重置失败

- **选择**：主按钮 force balance；reset 保留 API，UI 降级
- **原因**：运维高频需求是 usage/订阅；清计数是故障恢复

### D5: 设置热更新写盘 + 内存

- **选择**：PUT settings 校验 → Config::save → 更新 token_manager proxy / AppState auth / defaultEndpoint 引用
- **失败**：校验失败 400 且不改内存；写盘失败返回错误并尽量回滚内存

### D6: requireApiKey 默认 true

- **选择**：缺省与现网「总是校验」一致；false 显式关闭
- **true + 空 apiKey**：fail-closed 401（对齐 Go）

### D7: 端点首期仅白名单已注册名

- **选择**：defaultEndpoint 必须在 IdeEndpoint 等已注册表内（现 ide）
- **endpointFallback**：可存配置预留，本 change 不实现多 URL 切换

### D8: 模块落点

- anthropic/handlers + converter 契约测试
- token_manager 元数据/预热
- admin service/router/types：settings + balance force + status 字段
- middleware：require_api_key 热读
- admin-ui：test dialog、card、dashboard、settings 面板

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| 过滤后列表变短 | Admin 查看仍可显示上游 raw models；日志 warn |
| 关闭 API Key 鉴权裸奔 | UI 二次确认 + 文档警告 |
| proxy 热更与 in-flight 请求 | 仅影响新 client；文档说明 |
| 误把 Go 三端点塞进 rs | design 明确差异；Non-goal |
| 配置写盘失败半更新 | 先校验再写；失败返回明确错误 |
| 预热打爆上游 | 并发上限 2 |

## Migration

- 旧 config 无 requireApiKey → 视为 true
- 旧 Admin UI 无 modelCount 字段 → 可选字段，前端兼容 undefined
- balance 无 force 参数 → 默认 false（旧缓存语义）
