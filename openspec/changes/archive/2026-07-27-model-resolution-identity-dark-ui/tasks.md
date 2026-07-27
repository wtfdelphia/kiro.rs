## 1. 模型解析核心（后端）

- [x] 1.1 抽取 resolve_model（thinking 后缀、别名表、Claude 归一、拒绝原因结构 ResolvedModel）
- [x] 1.2 实现 auto → defaultChatModel；OpenAI 兼容别名（gpt-4o/gpt-4/gpt-4-turbo/gpt-3.5-turbo 等）
- [x] 1.3 实现 allowCatalogPassthrough：命中 global/per-cred catalog 的上游 id 可透传
- [x] 1.4 convert_request / test_credential / 相关入口改用 resolve_model；保留 Claude 单测回归
- [x] 1.5 unmapped 错误不再使用“凭据无效”前缀；补充/修正 map/resolve 单测（含旧 gpt-4 预期）

## 2. 列表分层与路由

- [x] 2.1 GET /v1/models：仅暴露 resolve 可接受的 public ids；透传开启时包含 catalog 可透传 id；compat 别名策略按配置
- [x] 2.2 Admin credentials/{id}/models 与 models/catalog：返回 raw 同时带 resolvable/resolveTo/testable（或等价元数据）
- [x] 2.3 model-aware routing：使用 resolved upstream id 过滤 model set（alias 后 id / passthrough 原 id）
- [x] 2.4 相关单元测试：public 列表不含永远 unmapped 项；routing 使用 resolved id

## 3. 凭据测试 API 语义

- [x] 3.1 test 请求解析走 resolve_model；默认模型行为保持可测成功
- [x] 3.2 成功响应增加 resolvedModel / resolveKind（或文档等价字段）
- [x] 3.3 auto 与 catalog 命中的 gpt-5.6-sol 在策略开启时进入 generate 路径（非本地 unmapped）
- [x] 3.4 失败响应可诊断且无密钥；单测覆盖 unmapped 文案

## 4. Client Identity 运行时设置

- [x] 4.1 Config 确认/文档化 kiroVersion、systemVersion、nodeVersion 读写；缺省兼容
- [x] 4.2 GET/PUT /api/admin/settings/client-identity：校验、update_config_with、save_config、热生效
- [x] 4.3 未认证 401；空值/过长 400；单测
- [x] 4.4 Admin UI Settings 增加客户端标识分组（三字段 + 警告文案）

## 5. Admin Dark UI 与主题化 Select

- [x] 5.1 引入主题化 Select 组件（bg-background / bg-popover 等 token）
- [x] 5.2 替换 credential-test-dialog 模型下拉；仅展示 testable（若后端已提供）或至少 dark 可读
- [x] 5.3 替换 settings-panel 端点下拉、add-credential-dialog 认证方式下拉
- [x] 5.4 全局 dark color-scheme 或等价热修；去掉原生 select 的 bg-transparent
- [x] 5.5 手工/截图验收 dark：测试展开列表、设置、添加凭据、顶栏按钮；`pnpm --dir admin-ui build`

## 6. 文档与验证收尾

- [x] 6.1 更新 README / config.example：modelResolution 与 client-identity 字段说明
- [x] 6.2 对照 docs/model-alias-and-catalog-routing-optimization-design.md 勾选范围
- [x] 6.3 `cargo test` 相关模块 + `openspec validate --all`
- [x] 6.4 可选 live smoke（密钥不入库）：test 默认/auto/gpt-5.6-sol/claude-sonnet-4.6
- [x] 6.5 `git status --short` 无密钥与 .codegraph 误入
