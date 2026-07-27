## Why

Admin 与客户端已能看到上游 catalog 中的 auto / gpt-5.6-sol 等模型，但请求侧 map_model 仅接受 Claude 关键字，导致 test/chat 在本地以“凭据无效: 模型不支持或无法映射”拒绝；同时 systemVersion/kiroVersion 只能改 config.json 重启，Admin 黑夜模式原生 select 下拉白底不可读。设计输入见 docs/model-alias-and-catalog-routing-optimization-design.md（live 验证 + Kiro-Go 对照）。

## What Changes

- **统一模型解析**：引入 resolve_model（alias + normalize + catalog passthrough policy），替换 test/convert 等入口对严格 map_model 的直接依赖
- **兼容别名与 auto**：对齐 Kiro-Go 的 gpt-4o/gpt-4 等别名；auto 映射到 defaultChatModel（默认可为 claude-sonnet-4.6）
- **catalog 透传**：对 ListAvailableModels 缓存命中的上游 id（如 gpt-5.6-sol）允许透传，不伪装成 Claude
- **列表分层**：Admin raw 列表可带 resolvable/testable 元数据；/v1/models 仅暴露可解析 public ids；测试下拉只给 testable
- **错误语义**：unmapped 不再使用“凭据无效”前缀
- **Client Identity 热更**：Admin GET/PUT settings/client-identity 管理 kiroVersion/systemVersion/nodeVersion（落盘 + 热更新）
- **Admin Dark UI**：主题化 Select 替换原生 select（测试模型、端点、认证方式等），修复黑夜模式白底
- **非 BREAKING 默认路径**：默认 Claude test 与现有 Claude 映射保持可用；透传与别名通过明确策略启用；不改 SSE 主协议

## Capabilities

### New Capabilities

- `model-resolution`：统一模型解析管线（thinking 后缀剥离、兼容别名、版本归一、catalog 透传策略、拒绝原因）
- `admin-ui-dark-theme`：Admin UI 主题化下拉与 dark 模式可读性契约（覆盖测试/设置/添加凭据等原生 select）

### Modified Capabilities

- `model-catalog`：public /v1/models 与 Admin catalog 在透传策略下的暴露规则；可选解析元数据
- `credential-model-test`：test 走 resolve_model；返回 resolvedModel/resolveKind；错误文案与可测列表语义
- `admin-ui-model-ops`：测试模型选择使用主题化控件；优先展示 testable 模型
- `admin-runtime-settings`：新增 client-identity（kiroVersion/systemVersion/nodeVersion）读写热更
- `model-aware-routing`：路由过滤使用 resolved upstream id（alias 后 / passthrough 原 id）

## Impact

- **代码**：src/anthropic/converter.rs（map/resolve）、src/anthropic/handlers.rs（/v1/models）、src/admin/service.rs|types.rs|router.rs|handlers.rs|error.rs、src/kiro/token_manager.rs|provider.rs（config 热更与路由）、src/model/config.rs、admin-ui credential-test-dialog/settings-panel/add-credential-dialog 与 ui/select、index.css
- **API**：test 响应扩展；Admin models 元数据可选扩展；新增 /api/admin/settings/client-identity；配置 schema 增加 modelResolution 与 client identity 字段
- **文档**：README 配置/Admin；设计 docs/model-alias-and-catalog-routing-optimization-design.md
- **风险类型**：模型映射、Admin、配置 schema、跨模块（anthropic + kiro + admin + admin-ui）
- **非目标**：多 API Key 配额面板、CW/AmazonQ 多端点 fallback、Admin Cookie 登录、把 gpt-5.6-sol 强制别名到 Claude、保证所有非 Claude 上游模型一定 generate 成功
