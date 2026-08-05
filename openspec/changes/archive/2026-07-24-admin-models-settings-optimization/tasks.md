## 1. 模型列表正确性（Phase A 后端）

- [x] 1.1 `models_from_catalog`：默认过滤 map_model 失败的 modelId；thinking 变体基于 canonical；补充单测
- [x] 1.2 静态 fallback 列表契约测试：每个 id（含 thinking 基座）map_model 为 Some；必要时调整静态 id 与 map 规则一致
- [x] 1.3 启动后台模型预热（启用凭据、限并发）；失败仅 log；/v1/models 不阻塞同步全量 refresh
- [x] 1.4 （可选）GET /api/admin/models/catalog 返回全局 count/models/updatedAt

## 2. 凭据状态元数据与余额 force（Phase A/B 后端）

- [x] 2.1 CredentialStatusItem 增加 modelCount / modelsUpdatedAt / modelsLastError（读缓存元数据）
- [x] 2.2 balance 支持 force 查询参数或 POST balance/refresh，跳过 TTL 缓存
- [x] 2.3 相关 admin/token_manager 单测（缓存命中 vs force；modelCount 字段）

## 3. Admin UI 模型联动与测试选择（Phase A 前端）

- [x] 3.1 CredentialTestDialog：打开时拉取 getCredentialModels；Select + 自定义 Input；默认 sonnet/首项/省略
- [x] 3.2 CredentialModelsDialog：列表项支持「用此模型测试」打开测试并预填
- [x] 3.3 刷新模型成功后 invalidate credentials（更新 modelCount）并刷新打开中的 models 视图
- [x] 3.4 卡片展示 modelCount 徽章（无缓存不崩溃）

## 4. 余额按钮协作（Phase B 前端）

- [x] 4.1 卡片「重置失败」改为「刷新余额」（force）；展示订阅/剩余
- [x] 4.2 原 reset 降级为条件显示（有失败计数或已禁用）
- [x] 4.3 顶栏「查询信息」保留批量语义；文案可微调为批量余额/订阅

## 5. 运行时设置后端（Phase C/D）

- [x] 5.1 Config 增加 requireApiKey（默认 true）；load/save 兼容旧文件
- [x] 5.2 GET/PUT /api/admin/settings/proxy：校验 URL；落盘；更新 MultiTokenManager 全局 proxy
- [x] 5.3 GET/PUT /api/admin/settings/endpoint：白名单已注册端点；落盘 defaultEndpoint
- [x] 5.4 GET/PUT /api/admin/settings/auth：mask 读；requireApiKey/apiKey 热更新 AppState；true+空 key fail-closed
- [x] 5.5 middleware 单测：requireApiKey 四象限；settings 非法输入 400；未认证 401

## 6. 运行时设置 UI 与文档（Phase C/D）

- [x] 6.1 Admin UI Settings 面板：代理、默认端点、API Key 开关与轮换（二次确认关闭鉴权）
- [x] 6.2 更新 config.example.json 与 README 配置/Admin 段
- [x] 6.3 `pnpm --dir admin-ui build` 通过

## 7. 验证与收尾

- [x] 7.1 `cargo test` 覆盖 handlers/map_model/admin/settings 相关模块
- [x] 7.2 `openspec validate --all` 通过
- [x] 7.3 `git status --short` 无密钥与 .codegraph 误入
- [x] 7.4 完成前 verification-before-completion 证据（实现阶段填写）
