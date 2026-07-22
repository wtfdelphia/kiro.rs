## 1. 上游客户端与类型

- [x] 1.1 新增 UpstreamModelInfo / 响应类型与 JSON fixture 解析单测
- [x] 1.2 实现 list_available_models（host/query/headers/proxy/profileArn）与非 200 错误路径单测
- [x] 1.3 导出模块并接入 kiro 模块树

## 2. ModelCatalog 与选号

- [x] 2.1 实现 per-credential + global catalog 存储与 merge_unique_models
- [x] 2.2 实现 refresh_models_for / refresh_models_all（ensure token + 写缓存 + 失败保留旧缓存）
- [x] 2.3 更新 select_next_credential：model set 过滤 + 冷启动乐观 + 保留 supports_opus
- [x] 2.4 删除凭据时清理缓存；添加/启用后异步 refresh
- [x] 2.5 选号与 merge 单测

## 3. Admin API

- [x] 3.1 增加 types：refresh/list/test 请求响应
- [x] 3.2 路由与 handlers：models/refresh（单/全）、models 查看、credentials/{id}/test
- [x] 3.3 service 层错误映射（404/400/502）与单测

## 4. GET /v1/models

- [x] 4.1 get_models 注入 catalog 读口；非空动态构建 + thinking 变体
- [x] 4.2 空缓存静态 fallback 与关键 id 兼容断言/测试

## 5. 凭据 Test

- [x] 5.1 实现最小非流式 Kiro generate 探测（默认模型、小 max_tokens）
- [x] 5.2 非法 model / token 失败 / 上游失败路径与密钥不泄露检查

## 6. 文档与可选 UI

- [x] 6.1 README Admin API 表补充新 endpoints
- [x] 6.2 （可选）admin-ui：刷新模型 / 测试按钮与展示 （SKIP：API-first，UI 非首期）

## 7. 验证

- [x] 7.1 cargo test 相关模块真实执行并记录结果
- [x] 7.2 openspec validate --all
- [x] 7.3 git status --short 确认无密钥与 .codegraph 误入
