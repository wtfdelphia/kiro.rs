## Context

设计输入：docs/model-alias-and-catalog-routing-optimization-design.md（2026-07-24）。
前置能力：model-catalog、credential-model-test、admin-ui-model-ops、admin-runtime-settings、model-aware-routing。

**当前状态（kiro-rs）：**

- ListAvailableModels + 双层缓存 + Admin 刷新/查看/test 已有
- map_model 仅识别 sonnet/opus/haiku 关键字与版本片段；auto/gpt-*/非 Claude catalog id 返回 None
- test_credential 与 convert_request 直接依赖 map_model；unmapped 走 InvalidCredential，文案前缀“凭据无效”
- Admin credentials/{id}/models 与 models/catalog 返回 raw 列表；GET /v1/models 过滤 unmapped
- select_next_credential 按缓存 model set 过滤（冷启动乐观）
- Config 含 kiroVersion/systemVersion/nodeVersion，进入 UA/请求指纹；仅启动加载
- Admin 已热更 proxy/endpoint/auth；无 client-identity
- Admin UI 测试/设置/添加凭据使用原生 select；dark 下 option 白底不可读

**参考（Kiro-Go）：**

- MapModel：别名表 + dash→dot + claude-* 透传；未知可原样返回
- /models 额外挂 auto/gpt-4o/gpt-4
- kiroVersion/systemVersion 进入 headers；Settings 热更面更广

## Goals / Non-Goals

**Goals:**

- 统一 resolve_model，消除“列表看得见、test 一点就 400”的双真源冲突
- auto 与历史 OpenAI 别名可解析；catalog 命中上游 id 可透传
- unmapped 错误可诊断且不误报凭据无效
- Admin 可热更 kiroVersion/systemVersion/nodeVersion 并落盘
- Admin dark 模式下拉可读；原生 select 替换为主题化控件
- 每阶段可独立验证；密钥不回传、不入库

**Non-Goals:**

- 不保证所有透传模型上游一定成功 generate
- 不把 gpt-5.6-sol 默认伪装为 Claude
- 不改 Admin Cookie/密码登录
- 不扩展多端点 URL fallback
- 不重写负载均衡算法本身
- 不在本 change 做多 API Key 配额

## Decisions

### D1: 三层解析管线 resolve_model

- **选择**：thinking 后缀剥离 → 兼容别名表 → Claude 版本归一 → catalog/policy 透传或拒绝
- **原因**：替代散落 if-contains；test/messages/routing 共用
- **备选**：继续扩展 map_model 大 if（难维护，拒绝）

### D2: auto 默认映射 defaultChatModel

- **选择**：auto → config.modelResolution.defaultChatModel（默认 claude-sonnet-4.6）
- **原因**：auto 是代理选择器；catalog 有 auto 不等于上游 generate 稳定
- **备选**：凭据最优 Claude（可二期）；原样透传（需 live 证明）

### D3: catalog 透传默认开启但需命中缓存

- **选择**：allowCatalogPassthrough 默认 true；仅当 id 出现在 global/per-cred catalog 时放行
- **原因**：gpt-5.6-sol 等是真实上游 id；陌生 id 仍拒绝
- **备选**：默认关闭（更保守，运维体验差）

### D4: 历史 OpenAI 别名表驱动

- **选择**：gpt-4o/gpt-4/gpt-4-turbo/gpt-3.5-turbo → claude-sonnet-4.5；同步更新旧单测
- **原因**：对齐 Kiro-Go 与常见客户端

### D5: 列表分层

- **选择**：Admin raw + resolvable/resolveTo/testable；/v1/models 仅 public 可解析；测试 UI 仅 testable
- **原因**：运维可看全量，客户端/测试不点死

### D6: 错误语义

- **选择**：model_unmapped / model_not_in_catalog 等；message 不用“凭据无效”前缀
- **原因**：live 中 unmapped 被误读为凭据损坏

### D7: Client Identity 独立 settings 资源

- **选择**：GET/PUT /api/admin/settings/client-identity
- **原因**：与 proxy/endpoint/auth 对称；复用 update_config_with + save_config
- **校验**：非空 trim、长度上限；不强制 systemVersion 枚举

### D8: Dark UI 用主题化 Select

- **选择**：Radix Select/Dropdown 组件 + bg-popover token；替换三处原生 select
- **原因**：原生 option 跨浏览器无法可靠主题化
- **热修备选**：color-scheme:dark + 去 transparent（仅缓解）

### D9: 路由使用 resolved upstream id

- **选择**：alias 路径用映射后 id 过滤 model set；passthrough 用原 id
- **原因**：与 generate 实际发送 id 一致

## Risks / Trade-offs

| 风险 | 缓解 |
| --- | --- |
| 透传非 Claude 可能影响 thinking/工具转换 | 先保证 test + 简单 chat；工具/thinking 边界在任务中标注 |
| 别名误导用户以为在用 GPT | /v1/models 对 compat 别名 owned_by=kiro-proxy（可选） |
| 放宽 map 破坏旧单测 | 任务中显式改测试与文档 |
| 错误版本 UA 导致上游失败 | UI 警告；校验非空；可回滚 config |
| Select 组件引入增加 UI 依赖面 | 优先复用已有 @radix-ui/react-dropdown-menu 或补 select |
| 二进制与源码版本漂移 | 验证清单要求本机构建/替换后 smoke |

## Migration / Compatibility

- 默认 Claude 路径行为保持
- 既有 map_model 可保留为 Claude 归一实现细节或由 resolve_model 调用
- config 缺省 modelResolution/client identity 字段时使用代码默认
- Admin UI 未升级时后端仍可工作；dark 修复在 UI 包

## Validation Strategy

- 单元：resolve_model 别名/auto/透传 hit-miss/thinking/Claude 回归
- Admin：test 默认成功；auto/gpt-5.6-sol 不再本地 unmapped（透传开启且 catalog 命中）
- settings：client-identity 读写落盘热更；401/400
- UI：dark 下测试模型下拉展开可读；pnpm build
- Live smoke（密钥不入库）：test auto / gpt-5.6-sol / claude-sonnet-4.6
- openspec validate --all
