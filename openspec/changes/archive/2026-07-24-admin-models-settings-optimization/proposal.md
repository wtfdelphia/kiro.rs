## Why

model-catalog 与 admin-ui-model-ops 已落地，但运维与客户端仍断点明显：GET /v1/models 在 catalog 空时假静态、动态/静态 modelId 方言混用导致「列表有但不可用」；Admin 查看/刷新模型与测试对话框未共享缓存数据；卡片「重置失败」只清计数不查余额；全局代理、默认端点、客户端 API Key 校验仍只能改 config.json 重启。设计输入见 docs/admin-models-settings-optimization-design.md（对标 Kiro-Go Settings/ApiKeys）。

## What Changes

- **模型列表正确性**：catalog 输出归一 canonical modelId；默认不暴露 map_model 失败项；静态 fallback 契约（均可 map）；空缓存时 fallback + 可选后台预热（不阻塞 /v1/models）
- **Admin 模型联动**：凭据状态暴露 modelCount/modelsUpdatedAt；测试对话框 Select+手输，数据来自 GET .../models；查看模型可「用于测试」；刷新后 invalidate 列表
- **余额协作**：卡片主操作由「重置失败」改为「刷新余额」（force 绕过 balance 缓存）；原 reset 降级为失败/禁用态恢复；与顶栏「查询信息」互补
- **运行时设置（对标 Kiro-Go）**：Admin 热更新出站代理、defaultEndpoint、requireApiKey/apiKey（mask，不回传明文）；写盘 + 内存生效
- **非 BREAKING**：默认仍兼容静态 fallback 与单 apiKey 必校验；requireApiKey=false 为显式配置；不改 SSE 主协议

## Capabilities

### New Capabilities

- `admin-runtime-settings`：Admin 运行时配置面——出站代理、Kiro 默认端点、客户端 API Key 验证开关与密钥轮换（脱敏），热更新并持久化到 config.json

### Modified Capabilities

- `model-catalog`：强化 GET /v1/models 的 id 可用性契约、可选诊断字段/Admin 全局 catalog、启动/空缓存预热策略
- `admin-ui-model-ops`：模型缓存与测试下拉联动、modelCount 展示、卡片余额刷新入口与重置失败降级
- `credential-model-test`：测试入口消费凭据模型列表（默认/可选模型选择语义与 UI 对齐，后端仍支持省略 model）

## Impact

- **代码**：src/anthropic/handlers.rs（get_models/models_from_catalog）、src/kiro/token_manager.rs（预热/元数据）、src/admin/*（settings、balance force、status 字段）、src/anthropic/middleware.rs 与 src/model/config.rs（requireApiKey/热更新）、admin-ui 卡片/测试对话框/设置页
- **API**：扩展 CredentialStatusItem 可选字段；balance 支持 force；新增 /api/admin/settings/{proxy,endpoint,auth} 与可选 GET /api/admin/models/catalog
- **配置 schema**：config.json 新增 requireApiKey（可选，默认 true 保持现状）
- **文档**：README 配置/Admin 段；设计 docs/admin-models-settings-optimization-design.md
- **风险类型**：Admin、模型映射/目录、认证中间件、配置 schema、跨模块（anthropic + kiro + admin + admin-ui）
- **非目标（本 change）**：多 API Key 配额面板、CodeWhisperer/AmazonQ 多 URL 自动 fallback、模型 catalog 强制 DB 持久化、重写负载均衡
