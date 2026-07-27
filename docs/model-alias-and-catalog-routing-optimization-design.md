# 模型别名与 Catalog 路由优化设计

> 状态：设计文档（分析 + 方案）；OpenSpec change 已建立：openspec/changes/model-resolution-identity-dark-ui
> 日期：2026-07-24
> 范围：
> 1. 验证 18990 最新改动生效面；复现 auto/gpt-5.6-sol test 失败；对照 Kiro-Go
> 2. 分析 systemVersion / kiroVersion 是否支持动态修改
> 3. 排查 Admin 黑夜模式（测试模型下拉白底不可见）并给出全按钮/控件审计与优化方案
> 4. 汇总可落地设计优化方案（实现前仍需 OpenSpec change）

分析手段：live HTTP + kiro-rs/Kiro-Go 源码精读 + 近期 commit/OpenSpec 对照

---

## 1. Context and motivation

### 1.1 背景

近期提交：

| Commit | 能力 |
| --- | --- |
| 028ed87 | ListAvailableModels、双层缓存、model set 路由、Admin test |
| 35ad9fd | Admin UI 模型刷新/查看/测试、占位 profileArn 恢复 |
| fabc03b | runtime settings、catalog 过滤 unmapped、测试 UI 模型选择、requireApiKey |

本地实例：

- 进程：C:/Users/wtf5058/Downloads/kiro_release/kiro-rs.exe (pid 62224)
- 监听：127.0.0.1:18990
- 配置：同目录 config.json；requireApiKey=true；apiKey 与 adminApiKey 分离

用户对 auto / gpt-5.6-sol 的 test 结果为：

`json
{"error":{"type":"invalid_request","message":"凭据无效: 模型不支持或无法映射: auto"}}
{"error":{"type":"invalid_request","message":"凭据无效: 模型不支持或无法映射: gpt-5.6-sol"}}
`

这不是凭据损坏，而是模型解析门禁在打上游前拒绝。

### 1.2 问题陈述

| # | 现象 | 根因 | 影响 |
| --- | --- | --- | --- |
| P1 | Admin 列表有 auto/gpt-5.6-sol，test 400 | 列表 raw catalog；test 严格 map_model | 看得见点不了 |
| P2 | auto/gpt-* 无法映射 | map_model 只认 Claude 关键字 | 兼容客户端差 |
| P3 | 文案写凭据无效 | InvalidCredential 复用 unmapped | 排障误判 |
| P4 | client key 打 Admin 401 | 双 key 设计 | 误以为功能未生效 |
| P5 | /v1/models 只剩 Claude | models_from_catalog 过滤 unmapped | catalog 18，对外缩水 |

### 1.3 Goals

- G1：说清生效面与缺口
- G2：解释失败并对照 Kiro-Go
- G3：给出解析/列表/测试/路由方案
- G4：验收可测，避免 silent 乱映射

### 1.4 Non-goals

- 本会话不改 map_model 代码或发版
- 不承诺所有非 Claude 上游模型一定可用
- 不默认把 gpt-5.6-sol 伪装成 Claude
- 不改 Admin Cookie 登录
- 不扩展 CW/AmazonQ 多端点 fallback

---

## 2. Live verification (2026-07-24)

鉴权：

- /v1/* 用 config.apiKey
- /api/admin/* 用 config.adminApiKey（与客户端 key 不同）

| 检查项 | 请求 | 结果 | 结论 |
| --- | --- | --- | --- |
| 服务存活 | TCP 18990 | kiro-rs Listen | 运行中 |
| settings/auth | GET /api/admin/settings/auth | 200 requireApiKey=true mask sk-k***3456 | 已生效 |
| settings/endpoint | GET /api/admin/settings/endpoint | 200 defaultEndpoint=ide | 已生效 |
| 凭据列表 | GET /api/admin/credentials | 200 一条可用 IDC | 正常 |
| 全局 catalog | GET /api/admin/models/catalog | 200 count=18 含 auto/gpt-5.6-sol/claude-sonnet-5 | 缓存生效 |
| 凭据 models | GET /api/admin/credentials/1/models | 200 18 项 | 生效 |
| 对外 models | GET /v1/models + client key | 200 仅 Claude+thinking | 过滤 unmapped 生效 |
| test 默认 | POST .../1/test {} | 200 model=claude-sonnet-4.6 reply=ok ~4.4s | 真实推理正常 |
| test Claude | model=claude-sonnet-4.6 | 200 reply=ok ~5.3s | 可映射正常 |
| test auto | model=auto | 400 无法映射 | 未兼容 |
| test gpt-5.6-sol | model=gpt-5.6-sol | 400 无法映射 | 未兼容 |
| test gpt-4o | model=gpt-4o | 400 无法映射 | Go 可别名，rs 拒绝 |
| client key 打 Admin | /api/admin/* | 401 | 双 key 边界符合设计 |

### 2.1 生效摘要

已生效：Admin 鉴权、凭据/模型缓存、全局 catalog、/v1/models 过滤、Claude 真实 test、settings auth/endpoint。

缺口：非 Claude catalog 模型不可 chat/test；兼容别名缺失；UI raw 下拉可点出 400；错误文案误导为凭据无效。

---

## 3. Root cause analysis

### 3.1 调用链

`	ext
POST /api/admin/credentials/{id}/test
  -> test_credential
     -> requested = body.model || claude-sonnet-4.6
     -> map_model(requested)
        None => InvalidCredential(模型不支持或无法映射) // 400
        Some => run_minimal_generate(mapped_id) // 真实上游
`

map_model（src/anthropic/converter.rs）只处理 sonnet/opus/haiku 关键字与版本片段，其余返回 None。

| 输入 | map_model | 结果 |
| --- | --- | --- |
| claude-sonnet-4.6 | claude-sonnet-4.6 | 成功 |
| claude-sonnet-5 | claude-sonnet-5 | 可映射 |
| claude-sonnet-4 | None | 失败（Go 可透传） |
| auto | None | 失败 |
| gpt-5.6-sol | None | 失败 |
| gpt-4o | None | 失败（Go 别名 sonnet-4.5） |

### 3.2 双真源冲突

| 表面 | 数据源 | 过滤 |
| --- | --- | --- |
| Admin credentials/{id}/models | raw cache | 不过滤 |
| Admin models/catalog | raw catalog | 不过滤 |
| GET /v1/models | catalog | 过滤 map_model |
| test / messages | 请求 model | 强制 map_model |

### 3.3 对照 Kiro-Go

| 维度 | Kiro-Go | kiro-rs |
| --- | --- | --- |
| 解析 | 别名 + dash 转 dot + claude 透传；未知原样返回 | 严格 Claude；未知拒绝 |
| 兼容别名 | gpt-4o/gpt-4/gpt-3.5-turbo -> sonnet-4.5 | 无；单测要求 gpt-4=None |
| /models | catalog + thinking + 挂 auto/gpt-4o/gpt-4 | 仅 mappable Claude |
| test 默认 | claude-sonnet-4 | claude-sonnet-4.6 |
| 未知策略 | 交上游 | 本地 400 |

一句话：Go 兼容入口 + 上游裁决；rs 本地白名单 + 先拒后调。catalog 扩展后 rs 白名单落后。

### 3.4 假设排序

1. 主因：map_model 拒绝 auto/gpt-5.6-sol（已证实）
2. 次因：Admin 列表暴露 raw 不可映射 id（已证实）
3. 干扰：client key 不是 admin key（已证实）
4. 非主因：token/profile/上游 —— 默认 test 已 200
5. 产品债：真实上游模型要透传，不是伪装 Claude

---

## 4. High-level behavior

统一 Model Resolution Pipeline：

`	ext
input model
  -> strip thinking suffix
  -> alias table
  -> version normalize
  -> catalog/policy
  -> MappedClaude | PassthroughUpstream | Reject
  -> routing + generate 使用 resolved upstream id
`

原则：单解析函数复用；列表分 raw/public/testable；拒绝原因可诊断；陌生 id 默认拒绝，catalog 命中可配置透传。

---

## 5. Design decisions

### D1 三层解析

- L1 Alias：gpt-4o -> claude-sonnet-4.5；auto -> defaultChatModel
- L2 Normalize：claude-sonnet-4-6 -> claude-sonnet-4.6
- L3 Policy：catalog 命中 gpt-5.6-sol -> passthrough

### D2 auto 语义

1. 默认映射 defaultChatModel（建议 claude-sonnet-4.6）
2. 可选：凭据可用 Claude 优先级选择
3. 不默认原样透传，除非 live 验证上游稳定接受

### D3 gpt-5.6-sol 类优先透传

- allowCatalogPassthrough=true 且 catalog 命中 -> PassthroughUpstream
- 未命中 -> 400 model_not_available
- 不要默认映射到 Claude

### D4 历史 OpenAI 别名表

`	ext
gpt-4-turbo, gpt-4o, gpt-4, gpt-3.5-turbo -> claude-sonnet-4.5
claude-3-5-sonnet* -> claude-sonnet-4.5
`

同步修改 test_map_model_unsupported 等旧预期。

### D5 列表契约拆分

| API | 返回 |
| --- | --- |
| Admin credentials/{id}/models | raw + resolvable/resolveTo/testable |
| Admin models/catalog | raw + 解析元数据 |
| GET /v1/models | public 可调用 ids + 可选 compat 别名 |
| Test 下拉 | 仅 testable=true |

### D6 错误文案

- model_unmapped
- model_not_in_catalog
- model_not_supported_by_credential

message 不再使用凭据无效前缀。

### D7 路由

alias 用映射后 id 过滤；passthrough 用原 id 过滤。

### D8 配置

`json
{
  "modelResolution": {
    "defaultChatModel": "claude-sonnet-4.6",
    "allowCatalogPassthrough": true,
    "exposeCompatAliasesInModels": true,
    "compatAliases": {
      "gpt-4o": "claude-sonnet-4.5",
      "gpt-4": "claude-sonnet-4.5",
      "auto": "claude-sonnet-4.6"
    }
  }
}
`

---

## 6. API / UX

成功示例：

`json
{"success":true,"model":"gpt-5.6-sol","resolvedModel":"gpt-5.6-sol","resolveKind":"passthrough","reply":"ok","latencyMs":1234}
`

失败示例：

`json
{"error":{"type":"invalid_request","message":"模型无法解析: gpt-5.6-sol（不在兼容别名表，且未允许 catalog 透传或不在可用模型缓存中）"}}
`

Admin UI：测试下拉仅 testable；自定义输入不可解析时禁用；列表徽章 mapped/passthrough/unusable。

/v1/models：Claude+thinking；允许透传的上游 id；可选 compat 别名 owned_by=kiro-proxy。

---

## 7. Implementation outline

### Phase A 语义修复

1. 抽取 resolve_model
2. 别名表 + auto 默认
3. test/convert 改用 resolve
4. 错误文案去误导
5. 单测补齐

### Phase B Catalog 透传

1. allowCatalogPassthrough
2. /v1/models 暴露可透传 id
3. UI 仅 testable
4. live smoke gpt-5.6-sol

### Phase C 元数据与设置

1. Admin models 返回解析元数据
2. settings 热更新 defaultChatModel/passthrough
3. README/config.example 同步

### Phase D 技术债

1. 修复过时 map 单测
2. 静态 fallback id 风格统一
3. 评估裸 claude-sonnet-4 策略

---

## 8. Testing approach

- 单元：auto/gpt-4o/gpt-5.6-sol hit-miss/thinking/Claude 回归
- 集成：默认 test 成功；unmapped 新文案；UI 与 testable 一致
- Live：auto / gpt-5.6-sol / claude-sonnet-4.6 test；/v1/models；/v1/messages
- 对照：同输入比较 Go 与 rs 的 resolved/失败类别

---

## 9. Acceptance criteria

1. 默认 test 保持 success=true
2. auto 可解析，不再 unmapped 400
3. catalog 内 gpt-5.6-sol 允许透传时进入真实 generate
4. gpt-4o 解析到明确 Claude 目标
5. /v1/models 无永远 400 的假可用 id，或显式不可用
6. unmapped 不再写凭据无效
7. client key 访问 Admin 仍 401
8. 实现前 OpenSpec；完成后 cargo test + openspec validate + smoke 证据

---

## 10. Risks and open questions

| 风险 | 建议 |
| --- | --- |
| 上游 auto 未必可 generate | 先探测，失败仅别名 |
| 非 Claude 透传影响 thinking/工具 | 先限 test + 简单 chat |
| 别名误导用户 | compat owned_by=kiro-proxy |
| 放宽 map 破坏旧测 | 同步改测试文档 |
| 二进制落后源码 | 发版写 commit |

待拍板：

1. auto 固定默认 Claude 还是凭据最优 Claude？
2. catalog 透传默认开还是关？
3. /v1/models 是否暴露 compat 别名？

---

## 11. Recommended next step

1. OpenSpec change: model-resolution-alias-and-passthrough
2. 先 Phase A（别名 + auto + 错误文案）
3. 再 Phase B（透传 + UI testable）
4. 发布后替换 Downloads/kiro_release/kiro-rs.exe 复测

---

## 12. Evidence index

- 运行：kiro-rs @ 127.0.0.1:18990，配置目录 Downloads/kiro_release
- Live：catalog 18；默认/sonnet-4.6 test 成功；auto/gpt-5.6-sol/gpt-4o unmapped 400
- 代码：src/anthropic/converter.rs map_model；src/admin/service.rs test_credential；src/anthropic/handlers.rs models_from_catalog
- 对照：Kiro-Go proxy/translator.go MapModel；handler.go models 追加 auto/gpt-4o/gpt-4 与 apiTestAccount
- 变更：028ed87 / 35ad9fd / fabc03b

---

## 13. systemVersion / kiroVersion 动态修改能力

### 13.1 现状结论

**当前不支持通过 Admin UI / Admin API 动态修改。**

| 字段 | 配置文件可写 | 启动加载 | 运行时内存可变基础设施 | Admin 热更新 API | Admin UI |
| --- | --- | --- | --- | --- | --- |
| kiroVersion | 是 (config.json) | 是 | 有 update_config_with，但无调用方 | 无 | 无 |
| systemVersion | 是 | 是 | 同上 | 无 | 无 |
| nodeVersion | 是 | 是 | 同上 | 无 | 无 |
| 全局 proxy | 是 | 是 | 是 | GET/PUT /api/admin/settings/proxy | 有 |
| defaultEndpoint | 是 | 是 | 是 | GET/PUT /api/admin/settings/endpoint | 有 |
| requireApiKey/apiKey | 是 | 是 | 是 | GET/PUT /api/admin/settings/auth | 有 |

默认值（代码 src/model/config.rs）：

- kiroVersion 默认 0.11.107（README 示例可能仍写 0.9.2）
- systemVersion 默认在 darwin#24.6.0 / win32#10.0.22631 中随机
- nodeVersion 默认 22.22.0

### 13.2 运行时使用位置

二者会进入上游请求指纹，不是装饰字段：

| 用途 | 位置 | 作用 |
| --- | --- | --- |
| User-Agent / x-amz-user-agent | token_manager 刷新 Token | KiroIDE-{kiroVersion}-{machineId} 等 |
| UA 拼装 | profile.rs、models_api.rs、endpoint/ide.rs | ListAvailableModels / generate / profile 解析 |
| OS 标识 | system_version 作为 os_name | 与 nodeVersion 一并写入客户端标识串 |

MultiTokenManager::config() 每次 clone 当前 Mutex 中的 Config。若通过 update_config_with 改版本，后续请求无需重启即可带上新请求头。
但今天没有任何 Admin 路径写这两个字段；改 config.json 后也必须重启（或另做 reload）才会进入内存。

### 13.3 与 Kiro-Go 对照

- Go 配置同样有 kiroVersion / systemVersion，并进入 ClientConfig 与 kiro_headers
- Go Admin Settings 主要热更 proxy / thinking / endpoint / 密码等；版本字段仍偏配置文件侧
- rs 已有 proxy/endpoint/auth 热更，版本字段是明确缺口，不是已支持未暴露

### 13.4 是否应该支持动态修改

建议支持 Admin 热更新 + 落盘。理由：

1. 上游/风控可能校验 KiroIDE 版本；运维需要快速对齐官方客户端版本
2. systemVersion 影响 OS 指纹；跨平台排障需要可配
3. 基础设施已具备：update_config_with + save_config + settings 模式可复用

风险与约束：

1. 错误版本可能导致上游 4xx/行为差异，UI 需提示会影响全部上游请求头
2. 不建议每请求随机 systemVersion（现默认仅在缺省时随机一次）
3. adminApiKey 本身仍不热更（安全边界保持）
4. 改版本不自动轮换 machineId

### 13.5 设计方案（Client Identity 设置）

新增：

```text
GET  /api/admin/settings/client-identity
PUT  /api/admin/settings/client-identity
```

字段示例：

```json
{
  "kiroVersion": "0.11.107",
  "systemVersion": "win32#10.0.22631",
  "nodeVersion": "22.22.0"
}
```

校验：非空 trim；长度上限；systemVersion 建议 platform#version 形态但不强制枚举。

行为：

1. PUT 校验通过后 update_config_with 写三字段并 save_config 落盘
2. 后续 Token 刷新 / models / generate 自动读新 Config
3. 不强制全局刷新 Token；下一次需要 UA 的请求自然使用新值
4. Admin UI Settings 增加客户端标识分组

验收：

1. PUT 后 GET 立即返回新值
2. 不重启，下一次 models refresh 请求头含新 kiroVersion
3. 非法空值 400；未认证 401
4. config.json 可见持久化结果（密钥不入库测试）

---

## 14. Admin 黑夜模式：测试模型下拉白底问题

### 14.1 现象

Admin 黑夜模式下，凭据测试对话框的模型下拉展开后整片白底，选项文字看不清或像没有列表。

### 14.2 根因

测试对话框使用原生 select/option，class 含 bg-transparent。

1. bg-transparent 在 dark 主题下，关闭态或许尚可，但展开的 option 列表由浏览器原生绘制，不吃 Tailwind 的 CSS 变量
2. Windows / Chromium 在 dark 页面上常见 option 白底 + 浅色字或异常继承色，导致看不见
3. 项目没有 shadcn Select 组件（admin-ui/src/components/ui/ 无 select.tsx），尽管依赖里已有 @radix-ui/react-dropdown-menu
4. 同类原生 select 还出现在：
   - settings-panel.tsx 默认端点（同样 bg-transparent）
   - add-credential-dialog.tsx 认证方式（bg-background，关闭态较好，展开 option 仍可能原生白底）

这不是业务数据为空：live 已证明 models 接口返回 18 项；是渲染/对比度问题。

### 14.3 控件黑夜模式审计（当前）

| 控件/区域 | 实现 | Dark 评价 | 风险 |
| --- | --- | --- | --- |
| Button（default/outline/secondary/ghost） | CSS 变量 | 良好 | 低 |
| Input | bg-background text-foreground | 良好 | 低 |
| Dialog / Card | bg-background | 良好 | 低 |
| Switch / Checkbox | Radix + token | 大体良好 | 低 |
| Badge success/warning | 固定绿/黄 + text-white | 可接受 | 中 |
| 测试模型下拉 | 原生 select + bg-transparent | 差 | 高（用户报告） |
| 设置端点下拉 | 原生 select + bg-transparent | 差 | 高 |
| 添加凭据 authMethod 下拉 | 原生 select + bg-background | 中 | 中 |
| batch-verify 状态条 | 有 dark 变体 | 良好 | 低 |
| kam-import 部分 text-gray-400/500 | 硬编码 gray | 中 | 中 |
| Settings 二次确认 amber | 有 dark 变体 | 良好 | 低 |
| 主题切换 | documentElement classList dark | 工作正常 | 低 |

结论：按钮体系大体支持黑夜模式；原生 select 是最大缺口，不是所有按钮都坏了。

### 14.4 优化方案

#### 方案 A（推荐，根治）

引入统一 Select 组件（Radix DropdownMenu 或 @radix-ui/react-select）：

1. Trigger 使用 bg-background text-foreground border-input
2. Content/Item 使用 bg-popover text-popover-foreground（dark token 已在 index.css 定义）
3. 替换三处原生 select：credential-test-dialog、settings-panel 端点、add-credential-dialog 认证方式
4. 长列表支持 max-height + overflow；二期可加搜索；与 testable 标记联动

#### 方案 B（最小补丁，临时）

保留原生 select，但：

1. 去掉 bg-transparent，改为 bg-background text-foreground
2. .dark select { color-scheme: dark; }
3. option 尽力设置 background/color（跨浏览器不完美）

只能缓解，不能承诺根治。

#### 方案 C（体验增强）

测试模型改为 Input 搜索 + 可滚动列表按钮，完全主题化，不依赖原生 option。可与方案 A 合并。

### 14.5 全量 Dark 验收清单

1. 顶栏：主题切换、设置、刷新、批量按钮
2. 凭据卡片：测试/模型/余额/禁用/删除等
3. 测试对话框：关闭态 + 展开列表 + 手动输入 + 结果区
4. 模型查看对话框
5. 设置面板：三 section + 端点下拉展开
6. 添加凭据 / 批量导入 / 在线登录 / KAM 导入
7. Toast（sonner）dark 对比度
8. 空态 / 错误红字 / 警告 amber

验收标准：任意可点击控件文字可读；下拉展开层非刺眼白底；不依赖系统 color-scheme 才碰巧可读。

### 14.6 实现分期

| Phase | 内容 | 优先级 |
| --- | --- | --- |
| UI-A | 全局 color-scheme + 原生 select 去 transparent（热修） | P0 |
| UI-B | 统一 Select 组件替换三处下拉 | P0/P1 |
| UI-C | 测试模型列表搜索 / testable 徽章 | P1 |
| UI-D | 清扫硬编码 gray / 徽章暗色微调 | P2 |
| ID-A | client-identity settings API + UI（kiro/system/node version） | P1 |

可与模型解析 change 分开，也可放进同一 OpenSpec 的 UI/settings 任务组。

---

## 15. 更新后的总体实施顺序

1. UI-A/B：黑夜模式下拉可读（用户可见缺陷，改动面小）
2. 模型解析 Phase A：auto/别名/错误文案
3. Client Identity：kiroVersion/systemVersion 热更新
4. 模型解析 Phase B：catalog 透传 + testable
5. 文档：README 版本默认值与 config.example 对齐

---

## 15.1 实施状态对照（model-resolution-identity-dark-ui）

本设计已通过 OpenSpec change `model-resolution-identity-dark-ui` 落地为以下实现点：

| 范围 | 实现状态 | 说明 |
| --- | --- | --- |
| 统一模型解析 | 已实现 | `resolve_model` 统一处理 thinking 后缀、`auto`、OpenAI 兼容别名、Claude 归一与 catalog 透传 |
| `auto` | 已实现 | 默认解析到 `modelResolution.defaultChatModel`，缺省 `claude-sonnet-4.6` |
| `gpt-4o` / `gpt-4` 等别名 | 已实现 | 内置映射到明确 Claude 上游 id，避免本地 unmapped |
| `gpt-5.6-sol` catalog 透传 | 已实现 | `allowCatalogPassthrough=true` 且命中 global/per-credential catalog 时原样进入 generate 路径 |
| Admin test 语义 | 已实现 | test 返回 `resolvedModel` / `resolveKind`；模型解析失败使用模型错误语义，不再冠以“凭据无效” |
| 列表分层 | 已实现 | Admin models/catalog 保留 raw `models`，新增 `modelItems`（`resolvable` / `resolveTo` / `resolveKind` / `testable`）；`/v1/models` 只返回可解析项 |
| model-aware routing | 已实现 | chat 转换先解析为上游 modelId；provider 从请求体提取 resolved `modelId` 后进行凭据 model set 过滤 |
| Client Identity 热更 | 已实现 | `GET/PUT /api/admin/settings/client-identity` 管理 `kiroVersion` / `systemVersion` / `nodeVersion`，保存到 config 并热生效 |
| Admin dark select | 已实现 | 新增主题化 Select，替换测试模型、设置端点、添加凭据认证方式下拉；设置 `color-scheme` |

验证命令见 OpenSpec change 的任务记录与最终报告；live smoke 仍建议使用本地实际凭据在替换二进制后复测默认、`auto`、`gpt-5.6-sol`、`claude-sonnet-4.6`。

---

## 16. 一句话结论

最新模型目录、Admin 设置、凭据 test 主路径已生效；auto / gpt-5.6-sol 报错是模型解析白名单落后于上游 catalog，叠加 raw 列表与可调用集合不一致，以及错误文案误报凭据无效。systemVersion/kiroVersion 目前只能改 config.json 后重启，Admin 不支持动态修改（尽管内存 Config 具备热更基础设施）。Admin 黑夜模式下测试模型下拉白底不可读，根因是原生 select/option 未使用主题 token；Button/Input 等大多已支持 dark。优化方向：统一 alias + normalize + catalog passthrough；补 client-identity 热更；用主题化 Select（或临时 color-scheme）修全站原生下拉。


