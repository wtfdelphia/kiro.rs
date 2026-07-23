## Why

`2026-07-22-model-catalog-refresh-and-test` 已落地后端能力：

- `POST /api/admin/credentials/models/refresh`
- `POST /api/admin/credentials/{id}/models/refresh`
- `GET /api/admin/credentials/{id}/models`
- `POST /api/admin/credentials/{id}/test`

并对本地 `127.0.0.1:18990` 验证：路由存在、鉴权生效、错误路径可诊断。

但 Admin UI 首期刻意 SKIP 了模型入口。结果是：

- 运维只能 curl 才能刷新模型目录 / 做凭据推理探测
- 动态 `/v1/models` 缓存常年空，长期落在静态回退列表
- 账号 suspended / profile 问题等诊断无法在页面完成闭环

本次只补 **页面可见可用入口**。

后续实况发现：BuilderId 固定 profileArn 占位会导致 ListAvailableModels/generate 上游 403；在本 change 内增补最小后端修复，使模型/测试入口对可用账号真正可用。

## What Changes

- Admin UI API 客户端补充 models refresh / models list / credential test 封装与类型
- Dashboard 增加「刷新全部模型」入口，展示全量刷新统计（refreshed/failed/globalCount）与失败摘要
- Credential 卡片增加：
  - 查看模型（缓存列表 + lastError/updatedAt）
  - 刷新本凭据模型
  - 测试本凭据（默认模型；可选指定 model）
- 操作结果通过 toast / 对话框展示可诊断信息；失败不得展示密钥
- 列表刷新后保持现有 credentials 轮询/失效行为

**非 BREAKING**：仅前端可见性与可用性；Admin HTTP 契约沿用已发布 types。

## Capabilities

### New Capabilities

- `admin-ui-model-ops`: Admin 前端对模型目录与凭据测试能力的可见入口、调用、结果展示与错误诊断

### Modified Capabilities

- （无强制修改主规格文本）`model-catalog` / `credential-model-test` 后端需求保持不变；本变更补齐其 UI 消费面

## Impact

- **代码**：`admin-ui/src/api/credentials.ts`、`types/api.ts`、`hooks/use-credentials.ts`、`components/dashboard.tsx`、`components/credential-card.tsx`，可能新增轻量 dialog 组件
- **API**：不新增后端路由；只消费既有 Admin endpoints
- **构建**：`admin-ui` `pnpm build`；嵌入二进制需重建 admin-ui
- **运维**：页面可主动同步模型目录并做冒烟探测；可能产生 ListAvailableModels / 短 test 上游调用
- **文档**：README Admin 段落可补充「UI 入口」一句（若有对应说明位）
- **风险类型**：Admin UI；无协议/凭据 schema 变更
