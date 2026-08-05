# Bridge Plan: model-resolution-identity-dark-ui

日期：2026-07-24
状态：READY（工件齐全、validate 通过、工作区无真实密钥入库风险；实现前检查点）
分支：dev
设计输入：docs/model-alias-and-catalog-routing-optimization-design.md
OpenSpec status：4/4 artifacts complete；isComplete=true；非 blocked

## 范围

- **model-resolution（新）**：统一 resolve_model（thinking 后缀、兼容别名、Claude 归一、catalog 透传策略、拒绝原因）
- **model-catalog（改）**：/v1/models 仅暴露可 resolve 的 public ids；透传开启时可含 catalog 上游 id；Admin catalog/models 附 resolvable/testable 元数据
- **credential-model-test（改）**：test 走 resolve；返回 resolvedModel/resolveKind；auto/透传可进 generate；unmapped 不再“凭据无效”
- **model-aware-routing（改）**：凭据 model set 过滤使用 resolved upstream id（alias 后 / passthrough 原 id）
- **admin-runtime-settings（改）**：新增 GET/PUT /api/admin/settings/client-identity（kiroVersion/systemVersion/nodeVersion 热更+落盘）
- **admin-ui-model-ops（改）**：测试模型选择主题化、优先 testable、dark 可读
- **admin-ui-dark-theme（新）**：主题化 Select 替换原生 select；dark 验收清单

## 非目标

- 保证所有透传上游模型一定 generate 成功
- 把 gpt-5.6-sol 强制别名到 Claude
- Admin Cookie / 密码登录
- CW / AmazonQ 多 URL fallback
- 重写 priority/balanced 负载均衡算法
- 多 API Key 配额面板
- 改 SSE 流解析 / 工具调用转换核心（除非透传路径暴露阻塞 bug）
- 不提交 config.json / credentials.json / 真实 token / Cookie / .codegraph/
- 不 push / merge / PR / archive（除非用户另行要求）

## 关键设计决策（执行约束）

| 决策 | 结论 | 实现约束 |
| --- | --- | --- |
| D1 resolve_model 三层管线 | thinking -> alias -> normalize -> catalog/policy | test/convert/routing/list 共用；禁止再复制 if-contains |
| D2 auto | -> defaultChatModel（默认 claude-sonnet-4.6） | 不默认原样透传 auto |
| D3 catalog 透传 | 默认 true，但必须 catalog 命中 | 陌生 id 拒绝；不伪装 Claude |
| D4 OpenAI 别名 | gpt-4o/gpt-4/... -> claude-sonnet-4.5 | 同步改 test_map_model_unsupported 等旧测 |
| D5 列表分层 | Admin raw+元数据；/v1/models public；UI testable | 消除看得见点不了 |
| D6 错误语义 | unmapped != InvalidCredential 前缀“凭据无效” | 可新错误类型或改 Display 分支 |
| D7 client-identity | 独立 settings 资源 | 复用 update_config_with + save_config；非空校验 |
| D8 Dark Select | 主题化组件替换原生 select | 至少三处：test/settings/add-credential |
| D9 路由 | 用 resolved upstream id 过滤 | alias 后 id；passthrough 原 id |

### 一致性检查（proposal / design / tasks / specs）

| 能力 | proposal | specs | tasks |
| --- | --- | --- | --- |
| model-resolution（NEW） | 有 | specs/model-resolution/spec.md | 1.x |
| admin-ui-dark-theme（NEW） | 有 | specs/admin-ui-dark-theme/spec.md | 5.x |
| model-catalog（MODIFIED） | 有 | specs/model-catalog/spec.md | 2.1-2.2, 2.4 |
| credential-model-test（MODIFIED） | 有 | specs/credential-model-test/spec.md | 3.x（与 1.4 衔接） |
| admin-ui-model-ops（MODIFIED） | 有 | specs/admin-ui-model-ops/spec.md | 5.2 + 2.2 元数据 |
| admin-runtime-settings（MODIFIED） | 有 | specs/admin-runtime-settings/spec.md | 4.x |
| model-aware-routing（MODIFIED） | 有 | specs/model-aware-routing/spec.md | 2.3-2.4 |
| 文档/验证收尾 | 有 | — | 6.x |

结论：范围一致；openspec validate model-resolution-identity-dark-ui --strict 通过；openspec validate --all 12/12；可进入实现。

## 高风险项

| 风险 | 等级 | 说明 | 缓解 / 停止条件 |
| --- | --- | --- | --- |
| 模型映射语义变化 | 高 | 影响 messages + test + /v1/models | 先单测 resolve；Claude 回归必须绿 |
| 透传非 Claude | 高 | thinking/工具转换可能假设 Claude | 首期保证 test + 简单 chat；工具复杂路径发现阻塞则停并回写 design |
| 错误类型改动 | 中 | InvalidCredential Display 统一前缀“凭据无效” | unmapped 必须分流；单测钉文案 |
| 配置 schema | 中 | modelResolution + client identity | 缺省兼容旧 config；example/README 同步 |
| Admin 热更版本 UA | 中 | 错误版本可能导致上游 4xx | 校验非空/长度；UI 警告；可回滚 config |
| Admin UI dark | 中 | 原生 option 跨浏览器 | 主题化 Select 优先；color-scheme 仅热修 |
| 旧单测债 | 中 | dated snapshot / gpt-4=None | 任务 1.5 显式改测，不隐藏失败 |
| 密钥与本地运行配置 | 高（纪律） | 真机 Downloads/kiro_release 有 config/credentials | 仓库内永不提交；live smoke 密钥不入库 |

## CodeGraph 证据

命令与结论（本会话）：

```text
codegraph status
# Project kiro.rs; 103 files / 1743 nodes / 4541 edges; index up to date

codegraph context map_model
# Entry: src/anthropic/converter.rs:80
# Related: get_context_window_size, convert_request, models_from_catalog, test_credential

codegraph callers map_model
# get_context_window_size, convert_request, models_from_catalog, test_credential (+ map 单测)

codegraph impact map_model
# 26 symbols — converter/handlers/admin service 主路径与相关测试

codegraph context test_credential
# service.rs:590 + handlers.rs:279；直接 use map_model

codegraph context select_next_credential
# token_manager.rs:885；model set 过滤 + opus 订阅 + LB；acquire_context 调用

codegraph impact select_next_credential
# 14 symbols — acquire_context 与 model-set 单测

codegraph context update_config_with
# token_manager.rs:788；callers: update_proxy/endpoint/auth_settings

codegraph context CredentialTestDialog
# admin-ui credential-test-dialog.tsx；getCredentialModels + testCredential
```

落点结论：

1. 解析真源应落在 src/anthropic/converter.rs（或同模块新类型）并 pub use；map_model 可降为 Claude 归一实现细节
2. test 改 AdminService::test_credential + TestCredentialResponse 字段
3. 列表改 models_from_catalog / get_models；Admin get_credential_models / get_global_models_catalog 响应形状
4. 路由改 select_next_credential 入参语义（调用方传入 resolved id）
5. client-identity 仿 update_auth_settings 模式：types + service + handlers + router + settings-panel + settings API
6. UI 优先 credential-test-dialog.tsx / settings-panel.tsx / add-credential-dialog.tsx + 新 components/ui/select*

## rg / 源码补盲

| 主题 | 命令/发现 | 结论 |
| --- | --- | --- |
| map 调用面 | rg map_model src | converter + handlers + admin test + pub use |
| 错误前缀 | AdminServiceError::InvalidCredential Display=凭据无效 | unmapped 不能继续无脑走该变体 |
| catalog 过滤 | models_from_catalog 用 map_model.is_none() skip | 改 resolve 后同步 |
| 路由过滤 | set_contains_model + select_next_credential | 入参应为 resolved id |
| 版本字段 | Config 已有 kiro/system/node；UA 在 token_manager/profile/models_api/ide | 热更只需写 Config |
| settings 路由 | /settings/proxy|endpoint|auth 已有；无 client-identity | 新增对称路由 |
| Admin 原生 select | test/settings/add-credential 三处；test/settings 用 bg-transparent | dark 白底根因 |
| 文档默认值 | README kiroVersion 示例 0.9.2 vs 代码默认 0.11.107 | 6.1 对齐 |
| modelResolution | 仅 design/OpenSpec，源码尚无 | 实现时加 Config 字段 |
| 密钥文件 | 仓库根仅 *.example.json；无 config.json/credentials.json | 可实现；真机 smoke 用外部目录 |

## 任务到执行步骤映射

| Task | 主要落点 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1 resolve 结构 | converter.rs（+ 可选 types） | 编译 + 新单测骨架 | 与 map_model 语义冲突无法表达 |
| 1.2 auto/别名 | alias 表 + defaultChatModel | cargo test resolve/alias | 产品改 auto 语义未写入 spec |
| 1.3 catalog 透传 | resolve + catalog 查询钩子 | hit/miss 单测 | catalog 访问需跨层环依赖且无法解 |
| 1.4 入口替换 | convert_request、test_credential | Claude 回归 + test 路径 | 大面积 convert 测试红且非本任务债无法隔离 |
| 1.5 错误/旧测 | error.rs 或 test 分支 + map 测 | unmapped 文案断言；gpt-4 新预期 | — |
| 2.1 /v1/models | handlers.rs | models_from_catalog 测；透传 id 出现 | — |
| 2.2 Admin 元数据 | types + service models/catalog | 响应含 testable 字段；无密钥 | 破坏现有 UI 解析且无兼容策略 |
| 2.3 路由 resolved id | acquire_context 调用链 / select_next | model-set 单测 + 别名场景 | 调用方拿不到 resolved id |
| 2.4 列表/路由测 | handlers + token_manager tests | 目标测绿 | — |
| 3.1-3.4 test API | service/types/tests | 默认成功；auto/gpt-5.6-sol 非本地 unmapped；文案 | live 上游失败可接受，本地 unmapped 不可接受 |
| 4.1-4.3 client-identity API | config/types/service/handlers/router | 401/400/热更单测 | 写盘失败却 200 |
| 4.4 Settings UI | settings-panel + api/settings.ts | build；表单读写 | — |
| 5.1-5.5 Dark Select | ui/select + 三对话框 + index.css | pnpm --dir admin-ui build；dark 手工 | 无可用 Radix 且原生无法可读则停 |
| 6.1-6.5 文档验证 | README/example + validate | openspec validate --all；相关 cargo test；git status | 验证失败未记录原因 |

建议实现顺序（与 design 对齐）：

1. 5.x 热修/主题化 Select（可与 1.x 并行，UI 独立）
2. 1.x -> 3.x 解析与 test
3. 2.x 列表与路由
4. 4.x client-identity
5. 6.x 文档与总验证

## 必跑验证

| 阶段 | 命令 / 动作 | 通过标准 |
| --- | --- | --- |
| 工件 | openspec validate model-resolution-identity-dark-ui --strict | valid |
| 工件全集 | openspec validate --all | 全绿 |
| 解析/列表 | cargo test map_model / resolve 相关 / models_from_catalog / static_fallback | 目标测绿；旧债单独标注 |
| Admin test | cargo test test_credential 及相关 admin | unmapped 文案与 400 |
| 路由 | cargo test select_next_credential / set_contains | 别名/透传语义符合 |
| settings | client-identity 相关测 | 401/400/成功落盘路径 |
| UI | pnpm --dir admin-ui build | 通过 |
| 纪律 | git status --short | 无 config/credentials/token/.codegraph |
| 可选 live | Admin test 默认/auto/gpt-5.6-sol/claude-sonnet-4.6 | 默认 Claude 成功；auto/透传非本地 unmapped |

未跑 live 时最终报告必须写明原因与剩余风险（账号/二进制版本）。

## README / AGENTS / spec 同步判断

| 文档 | 是否需要 | 原因 |
| --- | --- | --- |
| README.md | 是 | client-identity API、modelResolution、kiroVersion 默认值对齐、test 模型语义 |
| config.example.json | 是 | 可选示例字段（modelResolution / 版本） |
| docs/model-alias-and-catalog-routing-optimization-design.md | 已有；实现后可补已实现状态 | 设计真源 |
| AGENTS.md | 否（除非验证命令/纪律变化） | 本 change 不改 AI 纪律入口 |
| spec/ 长期 | 否直接手改；归档时经 openspec archive/sync | 单次变更在 openspec/changes |
| openspec/specs/* | 实现完成归档时同步 | 现在只写 delta |

## 工作区与密钥检查

```text
git status --short
# ?? docs/model-alias-and-catalog-routing-optimization-design.md
# ?? openspec/changes/model-resolution-identity-dark-ui/

分支：dev
仓库根：无 config.json / credentials.json
存在：config.example.json、credentials.example.*.json、.codegraph/（本地索引，不入库）
```

结论：无会被提交的真实凭据文件；可开始实现。

## 停止条件

遇到以下情况必须停止实现并回报用户：

1. OpenSpec 工件与实现过程出现未记录的高风险范围扩张（SSE/工具协议大改、多端点 fallback 等）
2. 透传非 Claude 导致工具/thinking 主路径系统性失败且无法在本 change 最小修复
3. client-identity 热更无法保证“校验失败不改内存/磁盘”
4. Admin dark Select 无法在目标浏览器达到可读验收
5. 工作区出现待提交的真实 config/credentials/token
6. 验证命令无法确定或关键测试持续失败且原因未定位

## 实现入口（READY）

- change：model-resolution-identity-dark-ui
- 下一步技能：openspec-apply-change（按 tasks 顺序落地）
- 推荐首刀：
  1. resolve_model + 单测（tasks 1.1-1.2）
  2. 或并行 UI Select 热修（tasks 5.1/5.4）改善 dark 体验

