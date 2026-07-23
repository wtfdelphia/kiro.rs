## Context

**背景（已验证）：**

- 后端 model-catalog / credential-model-test 已实现并归档
- 本地运行二进制包含 Admin 路由；curl 可调用
- Admin UI 当前已有：添加凭据、批量/KAM 导入、在线授权、余额、Token 刷新、禁用/优先级/删除
- Admin UI **缺失**：模型目录刷新（单/全）、模型列表查看、凭据真实推理 test
- 归档 tasks 明确写过：`6.2 admin-ui 刷新模型/测试按钮 SKIP：API-first，UI 非首期`

**前端现状：**

- `admin-ui/src/api/credentials.ts` 无 models/test 封装
- `dashboard.tsx` 工具栏无「刷新模型」
- `credential-card.tsx` 操作区无「模型 / 测试」

**约束：**

- Surgical：只改 Admin UI 消费面；不改 Rust Admin 契约（除非发现前端必须的无害类型对齐）
- 不引入真实密钥展示；错误摘要截断
- 上游账号 suspended 时 UI 应显示可诊断错误，而不是静默成功

## Goals / Non-Goals

**Goals:**

- 所有已落地的模型/测试 Admin API 在页面上有明确入口
- 单凭据与全量刷新均可从 UI 触发
- 可查看凭据缓存模型列表与 lastError
- 可对凭据发起默认/指定模型 test，并展示 success/reply/latencyMs 或错误
- 失败路径可诊断、不泄露密钥
- `pnpm build` 通过

**Non-Goals:**

- 重做 Admin 信息架构或路由系统
- 修改 ListAvailableModels / 选号 / `/v1/models` 后端逻辑
- 实现完整「模型目录全局浏览器」独立页面（卡片 + 全量刷新足够）
- 批量对全部凭据并发 test（可后续；首期单卡 + 可选顺序批量若成本低再加）
- 修改 online-auth / import 既有对话框（已有入口）

## Decisions

### D1: 入口放在 Dashboard + CredentialCard，不新开路由

- **选择**：工具栏「刷新全部模型」；卡片「查看模型 / 刷新模型 / 测试」
- **原因**：现有 UI 是单页凭据管理；与余额/刷新 Token 模式一致
- **备选**：独立 Models 页 → 过度导航，首期不必要

### D2: 查看模型用轻量 Dialog

- **选择**：点击「查看模型」打开 Dialog，展示 models 列表、updatedAt、lastError；提供「重新刷新」「实时拉取(?live=1)」按钮（若 live 查询已存在则用 query）
- **原因**：列表可能较长；卡片内嵌会拥挤
- **备选**：仅 toast 展示前 N 个 → 信息不够

### D3: 测试用 Dialog 支持可选 model 输入

- **选择**：默认空 model（后端默认 claude-sonnet-4.6）；可选手填 model
- **原因**：对齐 `TestCredentialRequest.model`
- **展示**：success + model + latencyMs + reply 预览；失败显示 error.message

### D4: API 封装与 hooks

- **选择**：在 `api/credentials.ts` 增加函数；`types/api.ts` 增加响应类型；hooks 增加 mutation/query 可选
- **原因**：与现有 forceRefresh/balance 模式一致
- **错误处理**：axios 错误走 `extractErrorMessage`

### D5: 全量刷新反馈

- **选择**：Dashboard 按钮触发 `POST /credentials/models/refresh`；toast 摘要 + 可选 Dialog 展示 errors[]
- **原因**：部分失败时需要明细（credentialId + error）

### D6: 不在本 change 改后端

- **选择**：严格前端；若类型字段名与 camelCase 不一致，以前端对齐后端 serde camelCase 为准
- **例外**：仅当后端明显缺 UI 必需且无契约风险的小修时另开讨论；默认不做



### D7: 修复错误固定 profileArn 导致模型/测试 403（本会话增补）

**实况结论（2026-07-23 本地探测）：**

- 网络/代理不是主因；getUsageLimits 与「无 profileArn」的 ListAvailableModels/generate 均 200
- 当前凭据持久化了 BuilderId 固定 ARN .../profile/AAAACCCCXXXX
- 带该 ARN 的 ListAvailableModels / generateAssistantResponse → 403 User is not authorized
- 去掉 ARN 后同一 Token 可拉模型并完成 generate

**决策：**

1. 已知固定 ARN（BuilderId/Social 占位）视为 **不可信缓存**，不得作为“已解析成功”短路
2. 不再把固定 ARN **无条件持久化**到 credentials.json
3. ListAvailableModels：带 ARN 收到 403 unauthorized 时 **无 ARN 重试**；成功则可清除坏占位 ARN
4. generate/test：对 User is not authorized 且请求含 profileArn 时，清除占位/失败 ARN 并 **无 ARN 重试一次**
5. 保持 UI 入口与 Admin 契约不变；此为使模型运维入口真正可用的后端最小修复

**Non-Goals 调整：** 原 D6「不改后端」对本问题开例外；不重写 ListAvailableProfiles 全协议，只避免有害固定 ARN 阻塞路径。

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| 上游 403 suspended 被当成 UI bug | 错误文案完整展示上游摘要；成功/失败分色 |
| 全量刷新耗时长 | 按钮 loading 态；禁用重复点击 |
| test 触发真实上游费用/风控 | 按钮二次确认或明确「真实推理探测」文案；默认小 max_tokens 由后端保证 |
| 模型列表很大 | Dialog 可滚动；仅展示 id 字符串列表 |
| 嵌入二进制未重建导致旧 UI | tasks 要求 `pnpm build`；README/验证说明需重建 admin-ui 后 cargo build |

## Migration Plan

1. 实现前端 API/类型/UI
2. `pnpm build` 验证
3. 本地连已运行 Admin 服务做手动验收（可用 mock/错误路径）
4. 运维侧：更新运行中的 kiro-rs 需重新 build 嵌入 UI 的二进制

回滚：还原 admin-ui 改动并重建；后端不受影响。

## Open Questions

- 是否需要「批量测试选中凭据」：首期不做，tasks 标可选后续
- `GET .../models?live=true` 是否在 UI 暴露：若 handlers 已支持 live query，Dialog 提供开关；否则仅缓存读取 + refresh

## Verification Strategy

- `pnpm --dir admin-ui build`（或等价）必须通过
- 手动/脚本：登录 Admin → 全量刷新按钮可见可点 → 卡片三入口可见
- 错误路径：无可用上游时 UI 显示 failed 摘要（不要求真实 200 test 成功，因账号状态不可控）
- 不引入密钥到仓库；`git status` 检查
