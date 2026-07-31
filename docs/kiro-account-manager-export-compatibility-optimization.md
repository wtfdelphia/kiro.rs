# kiro-account-manager 导出文件兼容性分析与优化方案

> **状态：已实现并通过真实数据验证**（见第 12 章）
> 分析日期：2026-07-30 ｜ 实现与验证日期：2026-07-30 / 2026-07-31
> OpenSpec change：`openspec/changes/kam-external-idp-import-compat/`（83/83 tasks，四份 evidence 报告）
> 分析基线：kiro-rs `79086d4`（当前 HEAD `e775835` 仅改 `.github/workflows`，源码等价）；kiro-account-manager `c5c4776`（manifest 版本 1.9.2，`git describe` = `v1.9.2-62-gc5c4776`，即 tag 之后 62 个提交，非发布态）
> 分析方法：双项目 CodeGraph 索引、调用链/影响面分析、源码精读
> 相关文档：`docs/add-account-optimization-design.md`（2026-07-21，同为未实现设计，已含 KAM 导入入口描述）。两份方案涉及同一入口，实施前需合并或明确分工，避免计划分叉。
>
> **阅读提示**：第 1-11 章是**实现前**的分析与设计，描述的缺陷现已修复，保留作为决策记录。
> 当前实际行为见第 12 章。第 4 章兼容矩阵的「不支持」列已在实现后失效。

## 1. 结论

当前项目**不能完整支持** kiro-account-manager（下称 KAM）导出的所有账号。结论必须按导入入口和认证类型分别理解：

- 通过 Admin UI 的“KAM 账号导入”，当前 KAM 的平铺数组、平铺单对象、旧版 `credentials` 嵌套对象以及 `{ version, accounts }` 包装格式都能被解析。
- Google/GitHub Social、BuilderId 和 Enterprise IdC 的关键字段可进入后端，刷新主链路基本可用。
- Microsoft Entra ID / Azure AD 的 `external_idp` **不可正常使用**，且失败形态按入口不同分三种（详见 4.2）：
  - UI 导入 + 机密客户端（有 `clientSecret`）：被重算为 `idc`，刷新打 AWS OIDC，且被额外打上 `provider = BuilderId`。
  - UI 导入 + 公共客户端（无 `clientSecret`）：前端**直接硬失败**，报“idc 模式需要同时提供 clientId 和 clientSecret”，记录进不了后端。
  - 直接作为 `credentials.json`：显式 `authMethod = external_idp` 保留下来，但 `refresh_routes_to_idc` 只认 `idc/builder-id/iam`，因此落到 **Social** 端点，不是 IdC。
  三种路径的共同根因相同：后端没有 `tokenEndpoint`、`issuerUrl`、`scopes` 字段，也没有 Microsoft OAuth2 刷新分支。
- 直接把 KAM 文件作为 `credentials.json` 时，仅平铺单对象/数组与当前原生 schema 相容。`{ version, accounts }` 和旧版嵌套格式不是受支持的启动格式。平铺 `external_idp` 即使携带尚未过期的 `accessToken`，也只可能短暂工作，过期后仍无法正确刷新。

因此，当前状态应描述为：**部分格式可解析，Social 与 AWS IdC 主流账号可用，但未达到“各种登录格式完整支持”。**

## 2. 调查范围

本次将“正常使用”定义为同时满足以下条件：

1. 能识别 KAM 导出文件的容器格式。
2. 认证类型不会被错误推断。
3. 刷新所需字段可无损进入持久化模型。
4. access token 过期后能路由到正确的刷新端点。
5. 刷新后仍保留身份、`profileArn`、region 和 machine ID 等请求上下文。
6. 导入失败能按记录报告，且日志和响应不泄露 token/client secret。

KAM 当前账号模型包含三类认证：

| 认证族 | KAM 标识 | 典型 provider | 刷新端点 | 关键字段 |
| --- | --- | --- | --- | --- |
| Social OAuth | `social` | `Google`、`Github` | Kiro Desktop Auth `/refreshToken` | `refreshToken`、可选 `profileArn`、`machineId` |
| AWS IAM Identity Center | `IdC` | `BuilderId`、`Enterprise` | `oidc.<region>.amazonaws.com/token` | `refreshToken`、`clientId`、`clientSecret`、region；Enterprise 还需 `startUrl` |
| 外部身份提供商 | `external_idp` | Microsoft Entra ID / Azure AD（KAM 刻意置 `provider = null`） | 导出数据中的 Microsoft `tokenEndpoint`，或由 `issuerUrl` 派生 | `refreshToken`、`clientId`、可选 `clientSecret`、`tokenEndpoint/issuerUrl`、可选 `scopes`、真实 `profileArn` |

Social 端点在两侧形态不同，需要区分：KAM 把 host **硬编码**为 `https://prod.us-east-1.auth.desktop.kiro.dev`（`clients/kiro_auth_client.rs:25`、`auth/auth.rs:40`）；kiro.rs 才是 `prod.{region}.auth.desktop.kiro.dev` 的 region 模板（`token_manager.rs:167`）。互操作时不能假设 KAM 导出的 `region` 会影响其 Social 刷新行为。

KAM 的 API Key 或网关客户端密钥不属于账号导出登录格式，不纳入本次互操作范围。

## 3. 当前链路

### 3.1 KAM 导出端

KAM 的 `export_accounts` 直接序列化 `Account` 数组，无容器包装。`Account` 是 `rename_all = "camelCase"` 的平铺结构，共 34 个字段；本次互操作关心的认证相关字段：

```text
refreshToken / accessToken / expiresAt / authMethod / provider
clientId / clientSecret / region / startUrl / profileArn / machineId
tokenEndpoint / issuerUrl / scopes
```

`authMethod` 与 `provider` 都是 `Option<String>`，不是枚举，无编译期取值约束。实际写入的字面量为 `"IdC"`、`"social"`、`"external_idp"`；provider 为 `"Google"`、`"Github"`、`"BuilderId"`、`"Enterprise"`。`scopes` 是 `Option<String>`（空格分隔单串），不是 `Vec<String>`。`tokenEndpoint` / `issuerUrl` 带 `alias` 兼容 snake_case 来源。

消费端必须注意三个导出侧行为：

1. **导出无脱敏、无字段裁剪**：`refreshToken`、`accessToken`、`clientSecret`、账号 `password`、`proxyConfig.password` 全部明文落盘。
2. **只有 `email` 与 `proxyConfig` 带 `skip_serializing_if`**，其余可选字段一律输出显式 `null`，解析器必须容忍全字段 `null`。
3. **`export_accounts` 会先跑 `fix_account` 回填**：`provider` 为 null 时按 `startUrl`/`clientSecret`/邮箱域猜测，`authMethod` 为 null 时按 `clientId + clientSecret` 猜 `IdC`/`social`。因此导出文件里的 `authMethod` 通常非空，但可能是 KAM 自己猜的值而非登录时写入的值——这不改变“显式值优先”的设计方向，但说明该字段并非绝对权威。
4. **`enabled: bool`（default `true`）与 kiro.rs 的 `disabled: bool`（default `false`）语义相反**，直接换文件时 KAM 侧被禁用的账号会静默变为启用。

`external_idp` 的实现明确要求优先走 Microsoft OAuth2：若落入 AWS IdC，会向 `oidc.<region>.amazonaws.com` 发送 Microsoft client ID 并得到 `400 invalid_request`。

### 3.2 kiro-rs Admin UI 导入

```text
KAM JSON
  -> parseKamJson
  -> normalizeKamAccount（平铺转旧版 credentials 嵌套形态）
  -> 根据 clientId + clientSecret 猜测 social/idc
  -> POST /api/admin/credentials/import/batch
  -> AdminService::ingest_from_request
  -> MultiTokenManager::ingest_credential
  -> refresh_token（仅 social/idc 两路）
  -> credentials.json
```

关键缺口在类型判定处。注意下图**仅适用于 UI 导入路径**：`normalizeKamAccount` 确实读取并转发了 `authMethod`，但 `handleImport` 随后从零重算并覆盖它（`kam-import-dialog.tsx:240-242`）。

```text
导出 authMethod = external_idp
              |
              v
normalize 转发 authMethod，但 handleImport 重算覆盖
              |
              +-- clientId + clientSecret 都有 -> idc -> AWS OIDC（错误）
              |                                   并强制 provider = BuilderId
              |
              +-- 只有 clientId，无 clientSecret -> 前端硬失败，报
                                                   "idc 模式需要同时提供 clientId 和 clientSecret"
                                                   （记录不进后端，非误分类）
```

公共客户端这条是硬失败而非静默误分类，属于相对良性的现状：错误可见、可定位，但用户会被迫伪造 `clientSecret` 才能导入。

### 3.3 直接加载 credentials.json

`CredentialsConfig` 是无标签的 `KiroCredentials | Vec<KiroCredentials>`：

- 平铺单对象/数组可以反序列化。
- 包装对象和旧版嵌套对象没有显式 KAM 适配。**这不是“可能”而是确定行为**：`KiroCredentials` 的 25 个字段全部为 `Option<_>` 或带 `#[serde(default)]`（零必填），且全仓库无 `deny_unknown_fields`，因此 `{ version, accounts: [...] }` **必然**匹配 `Single(KiroCredentials::default())`。后果链已确认：`main.rs:49-58` 不报错 → `main.rs:76` 打印“已加载 1 个凭据配置” → 该凭据无 `refreshToken` 但保持启用（自动禁用只对 api_key 生效，`token_manager.rs:721-739`）→ `persist_credentials` 因 `is_multiple_format == false` 提前返回（`token_manager.rs:1194-1197`）。用户看到的是“加载成功但一个账号都不能用”。
- `authMethod = external_idp` 能作为字符串保留下来（`canonicalize_auth_method_value` 只归一 `builder-id`/`iam`/`apikey`，未知值原样透传），但 `refresh_routes_to_idc` 只在 `auth_method` 为 `None` 时才看 client 字段；显式 `external_idp` 既不等于 `idc` 也不等于 `builder-id`/`iam`，因此**落到 Social 端点**，与 UI 导入路径的误分方向相反。

## 4. 兼容矩阵

### 4.1 文件容器格式

> ⚠ 下表是**实现前**的状态。「静默误解析」问题已修复，四种容器现在在两条路径均支持。
> 当前状态见 12.8。

| KAM 文件形式 | Admin UI KAM 导入 | 直接用作 credentials.json | 结论 |
| --- | --- | --- | --- |
| 当前平铺数组 `[{ refreshToken, ... }]` | 支持 | 可加载 | KAM `export_accounts` 的**唯一**实际产物；认证字段仍受下表限制 |
| 当前平铺单对象 `{ refreshToken, ... }` | 支持 | 可加载 | KAM 后端不产生，前端 `ImportAccountModal` 会把裸对象包成数组；认证字段受下表限制 |
| 包装格式 `{ version, accounts: [...] }` | 支持 | 静默误解析 | KAM c5c4776 既不导出也不接受该格式（`import_from_json` 严格 `Vec<Account>`），属 kiro.rs 侧历史兼容 |
| 旧版 `{ credentials: { ... } }` | 支持 | 静默误解析 | 同上，历史格式；且该分支会丢 `label`（见 4.3） |
| 包装格式内旧版嵌套账号 | 支持 | 静默误解析 | 只能走 UI |

“静默误解析”而非“不支持”：如 3.3 所述，未知包装对象不会被拒绝，而是变成一条字段全空的启用凭据。这比明确报错更糟。

### 4.2 登录格式

> ⚠ 下表是**实现前**的状态，`external_idp` 的「不支持」已修复。当前状态与真实数据
> 验证结果见 12.8。

| 登录格式 | 解析 | 字段保留 | 正确刷新 | 总体状态 |
| --- | --- | --- | --- | --- |
| Google Social | 是 | 关键字段保留 | 是 | 支持 |
| GitHub Social | 是 | 关键字段保留 | 是 | 支持 |
| BuilderId IdC | 是 | `clientId/clientSecret/region/provider` 保留 | 是 | 支持；缺 provider 时默认 BuilderId |
| Enterprise IdC | 是 | `provider/startUrl/clientId/clientSecret/region` 保留 | 是 | 支持；依赖 provider 与 startUrl 正确 |
| Microsoft Entra/Azure `external_idp`（机密客户端，有 clientSecret） | 是 | endpoint/issuer/scopes 丢失 | 否。UI 导入误走 AWS IdC 并被打上 `provider = BuilderId` | 不支持 |
| Microsoft Entra/Azure `external_idp`（公共客户端，无 clientSecret） | 是 | 不适用，记录未进后端 | 否。UI 导入**硬失败**，报 "idc 模式需要同时提供 clientId 和 clientSecret" | 不支持 |

`external_idp` 走直接 `credentials.json` 路径时刷新方向不同：显式 `authMethod` 使其落到 **Social** 而非 IdC（见 3.3）。同一账号在两个入口得到两种错误端点，这是 5. 中“文件解析入口分裂”的直接证据。

“支持”表示代码路径和字段契约完整，不代表本次使用真实账号做过在线验活。

### 4.3 字段保真

| KAM 字段 | UI 导入现状 | 原生凭据模型 | 影响 |
| --- | --- | --- | --- |
| `refreshToken` | 保留 | 支持 | 正常 |
| `authMethod` | normalize 转发，但 `handleImport` 重算覆盖 | 任意字符串，仅识别 social/idc/api_key 语义 | 造成 external 错分 |
| `provider` | 保留；`idc` 且缺省时强制填 `BuilderId` | 支持 | Social/IdC 正常；external 因 KAM 侧本就为 `null`，被误填成 `BuilderId` |
| `clientId/clientSecret` | 保留；缺一即硬失败 | 支持 | IdC 正常；不能据此单独判别 external |
| `region` | 仅映射为 `authRegion`，native `region` 恒为 `None` | `region/authRegion/apiRegion` 均支持 | 刷新可用，但通用 `region` 作为单一来源被跳过，字段语义被压缩为「只服务刷新」。应写入通用 `region`，让 `effective_auth_region` 的既有回退链派生。**注意**：`effective_api_region`（`credentials.rs:229-233`）不回退到凭据级 `region` 是**有意设计**，不是缺陷——见下方注 |
| `profileArn` | 保留 | 支持 | external 必须保持真实值 |
| `startUrl` | 保留 | 支持 | Enterprise 正常 |
| `machineId` | 保留 | 支持 | 正常 |
| `userId/email` | 保留 | 支持 | 正常 |
| `label` | 仅平铺分支映射 nickname（`:66-70`）；旧版 `{credentials:{...}}` 嵌套分支原样返回（`:99`）→ 丢失 | 支持 nickname | 嵌套格式导入后账号无昵称 |
| `accessToken/expiresAt` | UI 导入丢弃并强制刷新；后端 `ingest_from_request` 也硬编码为 `None` | 模型支持 | 对 Social/IdC 可接受；external 无法刷新时成为阻断 |
| `tokenEndpoint` | 丢弃 | 不支持 | external 无法刷新 |
| `issuerUrl` | 丢弃 | 不支持 | external 无法派生刷新端点 |
| `scopes` | 丢弃 | 不支持 | 某些 Microsoft 租户刷新可能失败 |
| `enabled` | 丢弃 | 语义相反的 `disabled`（default `false`） | 直接换文件时 KAM 侧禁用账号静默变启用；导入路径需显式映射 `disabled = !enabled` |
| 账号 `password` | 丢弃 | 不支持 | 导出文件含明文密码，属敏感项，禁止预览/日志展示 |
| 账号级 `proxyConfig` | 丢弃 | 使用另一套 `proxyUrl/username/password` | 非登录格式核心，需明确是否迁移；含明文 `password`，禁止预览/日志展示 |

> **关于 `effective_api_region`（本文档初版判断有误，已修正）**
>
> 初版把「`effective_api_region` 不回退到凭据级 `region`」当成缺陷。这是错的。
> auth region 与 api region 的回退链**故意不同**：auth region 是刷新端点的 region，
> api region 是数据面端点的 region，一个部署可以合法地在 A 区认证、在 B 区调用。
> 三重反证：
>
> 1. `src/kiro/token_manager.rs:3256-3270` 的既有测试带解释注释明确断言该行为
>    （`// 凭据.region 不参与 api_region 回退链`）；
> 2. `README.md:456` 与 `:459` 分两行文档化两条链，前者含 `凭据.region`，后者刻意不含；
> 3. `src/model/config.rs:267-274` 的 Config 层同为两条互不引用的链。
>
> 因此 KAM 导入只需把 region 写入通用 `region` 字段（刷新经既有回退链取到），
> **不改任何 region 解析函数**。需要凭据级 api region 的用户在 Admin UI 显式设
> `apiRegion`。详见 `openspec/changes/kam-external-idp-import-compat/evidence/bridge-plan.md` 第 8.1 节。

## 5. 根因与影响面

1. **契约收窄只在前端**：TS 侧 `admin-ui/src/types/api.ts:75` 把 `authMethod` 限成 `'social' | 'idc' | 'api_key'`；但 Rust 侧 `src/admin/types.rs:148` 是裸 `String`，`#[serde(default)]` 兜 `"social"`，**无任何取值校验**。也就是说 `external_idp` 目前已能穿透 Admin API 并持久化，只是没人给它正确的刷新分支。这降低了实施成本：Phase 1 不需要放宽 Rust 契约，反而需要**补**校验。
2. **类型推断覆盖显式事实**：`handleImport` 重算 `authMethod`，只检查两个 client 字段。公共客户端本来就允许没有 `clientSecret`，该规则无法区分 external 与 Social，且会把只有 clientId 的记录直接判失败。
3. **持久化模型缺字段**：`KiroCredentials`（25 字段）、Admin API 和前端类型都没有 external endpoint 元数据。全仓库 `tokenEndpoint/issuerUrl` 仅出现在 `src/kiro/online_auth.rs` 的 AWS OIDC register-client **出站请求体**中，与凭据模型无关。
4. **刷新分派只有两路**：`refresh_token` 仅调用 `refresh_social_token` 或 `refresh_idc_token`（`token_manager.rs:147-152`），api_key 在入口 `bail`，无第三条分支。
5. **文件解析入口分裂**：UI 有 KAM normalize，启动加载器只认原生扁平 schema，同一个 external 账号从 UI 进会打 AWS OIDC、从文件进会打 Social。
6. **测试没有覆盖真实格式矩阵**：现有测试覆盖原生 social/idc 序列化和 Admin batch 请求，但没有以脱敏 KAM fixture 验证容器格式、认证分类与刷新路由。具体缺口见 9.3 现状清单。

`KiroCredentials` 在 `src/` 下有 10 个文件、184 处文本引用，集中在 `token_manager.rs`（81）、`model/credentials.rs`（49）、`profile.rs`（21）、`machine_id.rs`（15），其余散落于 `models_api.rs`、`admin/service.rs`、`main.rs`、`provider.rs`、`endpoint/mod.rs`。影响面覆盖 Token 管理、Profile ARN、Admin service、模型目录与 Provider 请求。实现属于凭据 schema、Token 刷新、Admin API 和跨模块行为变化，必须先建立 OpenSpec change，并在实现前运行 `openspec-superpowers-bridge`。

## 6. 优化目标

- 所有 KAM 导出容器格式通过同一个规范化契约得到一致结果。
- 显式 `authMethod` 优先于字段猜测；只在旧文件缺少标识时做保守推断。
- Social、BuilderId、Enterprise、external_idp 都能持久化并长期刷新。
- external token endpoint 必须经过严格 HTTPS + Microsoft 域名白名单校验，不能把导入文件变成任意 SSRF 入口。
- 每条无效记录返回明确错误，不再只在浏览器 console 静默跳过。
- 现有原生 `credentials.json`、Admin batch API 和三类已支持账号保持向后兼容。

## 7. 推荐设计

### 7.1 建立规范化认证类型

在 Rust 领域层集中规范化，不让 UI 与 Token 管理器各自猜测：

| 输入别名（大小写不敏感） | 规范值 |
| --- | --- |
| `social` | `social` |
| `idc`、`IdC`、`builder-id`、`iam` | `idc` |
| `external_idp`、`external-idp`、`externalidp`、`azure`、`azuread`、`azure_ad` | `external_idp` |
| `api_key`、`apikey` | `api_key` |

判别优先级：

1. 使用合法的显式 `authMethod`。
2. 缺失时，Microsoft 白名单内的 `tokenEndpoint/issuerUrl` 推断为 `external_idp`。
3. 缺失时，`clientId + clientSecret` 推断为 `idc`。
4. 其余含 refresh token 的记录推断为 `social`。
5. 显式但未知的值直接按记录报错，不静默降级。

external 分支必须排在 IdC 之前，因为 external 账号也可能同时拥有 client ID 和 client secret。

### 7.2 扩展凭据与 Admin 契约

对以下结构增加 camelCase 可选字段：

- `KiroCredentials`
- `AddCredentialRequest` / Admin UI `AddCredentialRequest`（TS 联合类型需加 `'external_idp'`；Rust 侧字段已是 `String`，改动重点是**加校验**而非放宽类型）
- KAM normalize 输入/输出类型
- 凭据快照中仅增加非敏感的“是否配置”状态；不得回传 endpoint 之外的 secret/token 明文

新增字段：

```text
tokenEndpoint: Option<String>
issuerUrl: Option<String>
scopes: Option<String>
```

认证类型校验规则：

- Social：要求 `refreshToken`。
- IdC：要求 `refreshToken + clientId + clientSecret`；Enterprise 应保留 `startUrl`。
- external_idp：要求 `refreshToken + clientId + (tokenEndpoint 或 issuerUrl)`；`clientSecret` 可选；`profileArn` 有值时必须原样保留；**不得因 provider 缺省而回填 `BuilderId`**（KAM 对 external 刻意置 `provider = null`）。
- API Key：沿用现有规则。

同时应收紧 `refresh_routes_to_idc` 的现状风险：它对未知 `auth_method` 静默 fallback 到 Social。规范化后，未知值应在**入口**被拒绝，而不是在刷新时被当成 Social。

### 7.3 增加 external_idp 刷新分支

刷新路由调整为：

```text
api_key       -> 拒绝 refresh
external_idp  -> Microsoft OAuth2 refresh_token grant
idc           -> AWS SSO OIDC JSON API
social        -> Kiro Desktop Auth JSON API
```

external 请求应使用 `application/x-www-form-urlencoded`，至少包含：

```text
grant_type=refresh_token
client_id=<clientId>
refresh_token=<refreshToken>
client_secret=<可选>
scope=<可选>
```

端点规则参考 KAM 已验证实现，但在 kiro-rs 中独立测试：

- 只允许 HTTPS。
- 用 URL 解析器取 hostname 后小写比较，不做字符串后缀拼接判断（KAM 的注释指明这是为了防 `https://evil.com\.login.microsoftonline.com` 这类反斜杠归一化绕过）。
- 白名单为精确匹配或子域匹配，KAM 现有清单共 4 项：`login.microsoftonline.com`、`login.microsoftonline.us`、`login.partner.microsoftonline.cn`、`login.chinacloudapi.cn`。
- 禁止 userinfo、反斜杠混淆、IP/localhost/link-local 和非标准绕过形式。KAM 侧 IP/localhost 只是**隐式**被域名白名单挡住（无显式 IP/loopback 判断）；kiro.rs 应补显式检查，不要照抄这一点。
- `issuerUrl` 只用于按固定规则派生 token endpoint，派生后再次走同一校验器。
- 错误响应不得包含 refresh token、access token 或完整 client secret。

### 7.4 统一 KAM 文件适配器

推荐在 Rust 服务端新增一个小型 KAM adapter，以 `serde_json::Value` 做结构判别，再输出规范化 `Vec<KiroCredentials>`。不要继续依赖 `#[serde(untagged)]` 猜测包装对象。适配器应支持：

1. 平铺单对象。
2. 平铺数组。
3. `{ version, accounts }`。
4. 单个/数组的旧版 `credentials` 嵌套对象。

Admin UI 将原始文档交给专用 KAM import API，服务端返回逐条预检/导入结果。现有 `/credentials/import/batch` 继续作为原生凭据 API，不必破坏。

启动加载器可复用同一个 adapter，但要遵循：

- 先明确识别原生格式，再识别 KAM wrapper/nested 格式。
- 不能把未知包装对象反序列化为空凭据；应 fail fast 并指出 JSON path。
- 首次把 KAM 格式规范化回写为原生格式前，使用现有持久化安全策略创建备份并原子替换。
- 文档明确推荐路径仍是 Admin 导入；直接替换文件属于离线迁移能力。

### 7.5 改进导入反馈

- 预览中展示“识别类型、provider、字段完整性”，不展示 token、client secret、账号 `password` 或 `proxyConfig.password`（KAM 导出这四类均为明文）。
- 缺字段、未知认证类型、非法 endpoint 都应形成逐条失败结果。
- 公共客户端不得再因缺 `clientSecret` 被判失败，用户不应被迫伪造 secret。
- 不再通过 `filter()` + `console.warn()` 静默丢弃无 refresh token 的记录。当前实现在**全部**记录无效时会 throw（`:141-143`），只有部分丢弃才静默——需要的是逐条可见，而非仅全失败可见。
- “验活成功”至少应区分“刷新成功”“余额成功”“profile 未解析”；避免余额接口偶然成功被等同为完整对话可用。
- duplicate/upsert 继续使用稳定 `userId` 优先、token hash 兜底的现有语义。

## 8. 分阶段实施

### Phase 0：规格与契约测试

1. 创建 OpenSpec change，覆盖凭据 schema、KAM 导入、external 刷新、Admin API 与配置加载兼容。归档中的 `2026-07-21-improve-credential-ingest`、`2026-07-21-profile-arn-resolution` 涉及相邻语义，需先读再写，避免重复或回退既有决策。
2. 从 KAM 当前模型制作完全虚构的脱敏 fixtures，不复制真实 token。fixture 必须包含“可选字段全为显式 `null`”的样本，因为 KAM 只有 `email`/`proxyConfig` 带 `skip_serializing_if`。
3. 固化容器格式 × 登录格式矩阵及错误场景。
4. 明确是否将直接 `credentials.json` 迁移纳入首期；推荐纳入，但与 Admin 导入分任务交付。
5. 为 admin-ui 引入测试运行器：`admin-ui/package.json` 当前**无 `test` script、无 vitest/jest**，`parseKamJson` / `normalizeKamAccount` 零自动化覆盖。这是 Phase 2 的必需前置，不是可选项。

### Phase 1：后端 external_idp 能力

1. 扩展凭据模型与 Admin request（新增三字段；Rust `auth_method` 从无校验 `String` 改为规范化 + 白名单校验）。
2. 增加认证规范化和字段校验，未知 `authMethod` 在入口拒绝而非 fallback 到 Social。
3. 实现 Microsoft endpoint 校验与 refresh 分支，含显式 IP/loopback 拒绝（KAM 侧仅隐式挡住）。
4. 保证持久化 round-trip 不丢 external 字段。
5. 补 external 的 Profile ARN 回归测试。**注意固定占位 ARN 问题已修复，此处不需要新功能**：`get_fixed_profile_arn` 已是 dead code（`profile.rs:93` 标 `#[allow(dead_code)]`，调用点全在 `#[cfg(test)]`），`profile.rs:221` 注释明确 "Fixed ARN table is documentation/history only — never short-circuit or persist"，且 `resolve_profile_arn`（`:206-213`）会主动 `clear_profile_arn` 清除已存的占位值。本项收敛为“加测试锁定该行为对 external 同样成立”。
6. region 改写入通用 `region` 字段（当前 UI 只写 `authRegion`），靠 `effective_auth_region` 的既有回退链派生。**不改任何 region 解析函数**——初版曾提议让 `effective_api_region` 也回退到凭据级 `region`，该判断已被证伪，见 §4.3 后的注。

### Phase 2：统一 KAM 导入

1. 增加服务端 KAM adapter 和专用导入 endpoint。
2. Admin UI 改为尊重显式 `authMethod` 并透传 external 字段；移除“缺 clientSecret 即失败”的硬规则。
3. 增加逐条预检、错误与分类展示。
4. 修掉两处已知映射缺口：嵌套分支的 `label → nickname` 丢失；`enabled` 未映射到 `disabled`。
5. 保持现有 batch API 向后兼容。

### Phase 3：启动文件兼容与文档

1. 启动加载器复用 adapter，支持 wrapper/nested，并让未知包装对象 fail fast（终结“加载 1 个空凭据”）。
2. 增加备份、原子迁移及错误定位。
3. 同步 README 中 KAM 支持范围、推荐导入入口和 external 安全限制。
4. 提供仅含占位值的 `credentials.example.*.json`，不增加任何真实凭据样例。**顺带修正已存在的问题**：`credentials.example.idc.json` 的 `profileArn` 恰好等于 `BUILDER_ID_PROFILE_ARN`（`profile.rs:23-24`），而该值被 `is_known_placeholder_profile_arn`（`:105-109`）判为不可信并主动清除，样例文件在教用户填一个代码会删掉的值。

## 9. 测试与验收

### 9.1 必需自动化测试

| 层 | 场景 | 验收点 |
| --- | --- | --- |
| KAM parser | 4 种容器 × Social/BuilderId/Enterprise/external | 记录数、类型、字段映射准确 |
| 类型规范化 | 全部别名、未知值、缺失值 | 显式值优先；未知值拒绝；fallback 顺序固定 |
| 模型 round-trip | external 完整字段序列化/反序列化 | endpoint/issuer/scopes/profileArn 不丢失 |
| 刷新路由 | external 同时带 client secret | 始终选 external，不落 IdC |
| external request | 公共客户端/机密客户端 | form 字段正确，client secret 可选 |
| endpoint 安全 | 合法 Microsoft 域与混淆/SSRF 样例 | 只接受白名单 HTTPS hostname |
| Admin import | 混合账号批次 | 逐条结果准确，单条失败不污染其他记录 |
| 启动加载 | 原生、平铺 KAM、wrapper、nested、未知包装 | 正确加载或可诊断拒绝，不生成空凭据 |
| 回归 | Social/BuilderId/Enterprise | 原刷新路由、region 和 profile 行为不变 |
| 安全 | 日志与 API error | 不包含 token/client secret/password 明文 |

### 9.1.1 现有覆盖与缺口

已有测试（可复用为回归基线）：

- `src/kiro/model/credentials.rs:400-431`：`test_credentials_config_single`、`test_credentials_config_multiple`、`test_credentials_config_priority_sorting`，另有 `test_from_json_with_unknown_keys`、`test_region_field_*`、`test_auth_api_region_*`、`test_provider_field_roundtrip`、`test_identity_fields_roundtrip`、`test_effective_proxy_*`。
- `src/kiro/token_manager.rs`：`test_validate_refresh_token_*`、`test_refresh_token_rejects_api_key_credential`、`test_add_credential_*`、`test_ingest_upsert_oauth_user_id_without_network`、region 优先级系列。
- `src/kiro/profile.rs:442-660`：`test_supports_profiles_social`、`test_decide_idc_never_force_refreshes`、`test_placeholder_arn_not_trusted` 等。
- `src/admin/types.rs:551-597`、`src/admin/service.rs:1534-1794`：batch 请求反序列化、身份字段、余额缓存绕过等。

必须补的具体缺口：

1. `CredentialsConfig` **无 wrapper-object 用例**，正是 3.3 那个静默误解析漏洞所在。
2. 无任何测试断言未知 `authMethod` 的路由归属（当前静默走 Social）。
3. `supports_profiles` 有 `external_idp` 分支（`profile.rs:49`）但**无对应测试**。
4. 前端 `parseKamJson` / `normalizeKamAccount` / `handleImport` 零覆盖，且无测试运行器。
5. 无 KAM fixture 的容器 × 登录格式矩阵测试（本次核心新增）。

### 9.2 建议验证命令

实现后至少运行：

```powershell
cargo test kiro::model::credentials
cargo test kiro::token_manager
cargo test admin
cargo test
pnpm --dir admin-ui build
openspec validate --all
git status --short
```

若项目新增前端单元测试脚本，还应运行 KAM parser 的定向测试。真实账号端到端验证只能使用本地临时凭据，并确认 `config.json`、`credentials.json`、`credentials.*`、token、Cookie 和 `.codegraph/` 不进入 Git 候选。

### 9.3 完成标准

- 兼容矩阵中的六种登录行全部达到“正确刷新”。
- 同一脱敏 fixture 从 Admin 导入与启动加载得到等价的规范化凭据（当前 external 在两个入口分别走 IdC 与 Social，这是最直接的验收信号）。
- external 公共客户端无需伪造 `clientSecret`。
- 所有 endpoint 绕过测试通过，含显式 IP/loopback 用例。
- 未知包装对象不再产生“已加载 1 个凭据配置”的空凭据。
- 全量 Rust 测试、Admin UI build 和 OpenSpec validate 通过。
- README 明确支持版本、入口和限制；无需用户阅读本设计文档才能正确导入。

## 10. 风险与取舍

| 风险 | 后果 | 控制措施 |
| --- | --- | --- |
| 信任导出文件中的 token endpoint | SSRF、凭据外传 | HTTPS + 精确 hostname 白名单；派生后复检 |
| 继续按 client 字段猜类型 | external 错分 | 显式 authMethod 优先，endpoint fallback 次之 |
| 过度信任导出的 `authMethod` | KAM `export_accounts` 的 `fix_account` 会在字段为 null 时自行猜测并回填，该值可能不是登录时写入的真值 | 仍以显式值优先（比字段猜测更可靠），但 external 分支必须同时校验 endpoint 存在，不能只凭 authMethod 就发请求 |
| 扩展 KiroCredentials | 影响面大 | OpenSpec + round-trip + Token/Profile/Admin 回归测试 |
| wrapper 被 Serde 当空凭据 | 启动后无可用账号且难诊断 | 结构化 `Value` 判别，未知包装 fail fast |
| 导入时强制刷新 | 大批量触发限流 | 保持低并发、逐条结果、可重试但不重试 invalid_grant |
| 迁移回写失败 | 凭据文件损坏 | 备份、临时文件、原子替换、失败不覆盖原文件 |
| 错误体回显上游响应 | token/租户信息泄露 | 结构化脱敏错误，禁止记录请求 form 和完整响应敏感字段 |

## 11. 源码证据

### kiro-account-manager（@ c5c4776）

- `src-tauri/src/core/account.rs:170-241`：`Account` 34 字段平铺 camelCase，含 `authMethod`、`tokenEndpoint`、`issuerUrl`、`scopes`（均为 `Option<String>`）；`enabled` 默认 `true`（`:237,:243`）；`proxyConfig` 于 `:239-240`，结构见 `:106-121`。`import_from_json` 严格 `Vec<Account>`（`:799`）。
- `src-tauri/src/commands/account_cmd.rs:901-975`：`export_accounts` 输出平铺 `Account` 数组，无包装、无脱敏；`fix_account`（`:911-956`）回填 null 的 `provider`/`authMethod`。`:863-864` 显示 external 刻意设 `provider = None`。
- `src-tauri/src/commands/common.rs:451-479`：external 先于 IdC（`:483`）分派，注释明写误走 `oidc.*.amazonaws.com` 会得 `400 invalid_request`；`:404-416` 对 external 别名做大小写不敏感匹配。
- `src-tauri/src/auth/providers/external_idp.rs:46-87`：HTTPS-only（`:56-58`）、`url::Url` 解析 hostname（`:53,:65-68`）、拒 userinfo（`:60-64`）、4 域白名单（`:72-77`）、子域匹配（`:80`）、反斜杠绕过回归测试（`:277-293`）；refresh form 于 `:139-153`，`scope`/`client_secret` 仅非空时追加。
- `src-tauri/src/clients/kiro_auth_client.rs:25`：Social host **硬编码** `us-east-1`。
- `src-tauri/src/clients/aws_sso_client.rs:65,:197`：IdC 为 `https://oidc.{region}.amazonaws.com/token`。

### kiro-rs（@ 79086d4）

- `admin-ui/src/components/kam-import-dialog.tsx:118-135`：四种容器格式判别；`:240-242` 重算并覆盖 `authMethod`；`:243-251` 缺 clientSecret 硬失败；`:257-270` 载荷未含 external 字段，`region` 仅映射 `authRegion`；`:66-70` vs `:99` 造成嵌套分支丢 `label`；`:139,:147,:179` 为静默过滤，`:141-143` 仅全失败时 throw。
- `admin-ui/src/types/api.ts:75`：请求类型 `authMethod?: 'social' | 'idc' | 'api_key'`，不含 external。
- `src/admin/types.rs:147-148,:214-216`：Rust `auth_method` 为裸 `String` + `default = "social"`，**无取值校验**。
- `src/admin/service.rs:335-410`：`ingest_from_request` 逐字段组装，无 external 元数据，`access_token`/`expires_at` 硬编码 `None`。
- `src/kiro/model/credentials.rs:14-129`：25 字段，零必填，无 `tokenEndpoint/issuerUrl/scopes`；`:151-158` untagged `CredentialsConfig`；`:136-144` `canonicalize_auth_method_value` 透传未知值；`:220-227` / `:229-233` 两条 region 回退链故意不同（见 §4.3 后的注）。
- `src/kiro/token_manager.rs:118-130`：`refresh_routes_to_idc` 仅认 `idc/builder-id/iam`，client 字段仅在 `auth_method` 为 `None` 时参与；`:133-153` 两路分派；`:1935-1943` 导入时强制刷新；`:1194-1197` 单凭据格式不回写。
- `src/kiro/profile.rs:40-52`：已有 external profile 语义（仅 gate `supports_profiles`）；`:23-24,:93,:105-109,:206-213,:221`：固定占位 ARN 已降级为 dead code 并会被主动清除。
- `src/main.rs:49-58,:76`：wrapper object 静默变成“已加载 1 个凭据配置”。

第 1-11 章记录的是实现前的分析，行号基于上述两个基线提交。实现已完成，见第 12 章。

## 12. 实现与验证结果

> 实现：2026-07-30，OpenSpec change `kam-external-idp-import-compat`（83/83 tasks）
> 真实数据验证：2026-07-31，构建产物 `kiro-rs.exe`（09:28）运行于 `127.0.0.1:18990`
> 验证样本：KAM 导出文件 `kiro-accounts-2-2026-07-31.json`（真实凭据，本节不含任何密钥值）

### 12.1 实现范围

| 新增文件 | 职责 |
| --- | --- |
| `src/kiro/external_idp.rs` | endpoint 白名单校验（6 步）+ OAuth2 refresh form 构造 |
| `src/kiro/kam_adapter.rs` | 四容器结构化判别 → 规范化 `Vec<KiroCredentials>` |
| `src/common/atomic_file.rs` | 原子写入 + 带时间戳备份 |

| 主要改动 | 内容 |
| --- | --- |
| `src/kiro/model/credentials.rs` | 新增 `tokenEndpoint`/`issuerUrl`/`scopes`；`AuthMethod` 枚举 + `parse_auth_method` + `classify_auth_method`；`load_detailed` 改为 `Value` 驱动；`migrate_to_native` |
| `src/kiro/token_manager.rs` | 刷新分派扩为四路（external 判定先于 IdC）；`refresh_external_token`；`persist_credentials` 改原子写 |
| `src/admin/types.rs` | `AddCredentialRequest` 加三字段 + `validate_shape` 按族校验 |
| `src/admin/{service,handlers,router}.rs` | `POST /api/admin/credentials/import/kam` |
| `admin-ui/` | 移除客户端类型重算与硬失败；改为服务端逐条预检驱动；引入 vitest |

自动化测试：后端 **693 passed / 0 failed**（基线 652，净增 41），前端 **11 passed**，
`openspec validate --all` 19/19。

### 12.2 真实文件的结构

平铺数组，2 条记录，**无 external_idp 账号**：

| 记录 | authMethod | provider | 关键字段 |
| --- | --- | --- | --- |
| [0] | `IdC` | `BuilderId` | refreshToken、clientId、clientSecret、region、machineId、userId、email |
| [1] | `social` | `Github` | refreshToken、profileArn、machineId、userId、email |

两条的 `tokenEndpoint`/`issuerUrl`/`scopes`/`password`/`proxyConfig` 均为显式 `null`，
`enabled` 均为 `true`。这印证了 3.1 的判断：KAM 只对 `email` 与 `proxyConfig` 用
`skip_serializing_if`，其余可选字段一律输出 `null`。

### 12.3 Admin 导入验证

**预检（dryRun）**：`container: FlatArray`，两条 `valid: true`，认证类型分别识别为
`idc`（从 `IdC` 归一）与 `social`。

**实际导入**：`summary: {created: 2, updated: 0, duplicate: 0, failed: 0}`，
真实 token 刷新成功，余额查询成功（`0/50` 与 `0.17/50`），凭据 id=2、3 入库。

**重复导入**：`{created: 0, duplicate: 2, failed: 0}`，总数仍为 3，幂等正确。

字段映射逐项核对（读 `credentials.json` 落盘结果）：

| 项 | 期望 | 实测 |
| --- | --- | --- |
| `IdC` → 规范值 | `idc` | ✓ |
| `label` → `nickname` | "Kiro BuilderId 账号" / "Kiro Github 账号" | ✓ |
| `enabled: true` → `disabled` | `false` | ✓ |
| `region` 写入通用字段 | `region: "us-east-1"`，`authRegion: null` | ✓ |
| `machineId`/`userId`/`email` | 保留 | ✓ |
| `password`/`proxyConfig` | 不入库 | ✓（源文件本为 null，另有单测覆盖有值场景） |

**密钥泄露检查**：用源文件的 `refreshToken`/`clientSecret`/`clientId`/`machineId`
逐个在预检响应中检索，**零命中**。预检响应 674 字节 vs 源文件 10679 字节。

### 12.4 容器格式与安全边界验证

以真实记录构造 9 个用例打真实端点：

| 用例 | 结果 |
| --- | --- |
| 平铺数组 | `container: FlatArray`，2 条 valid |
| 平铺单对象 | `container: FlatObject`，1 条 valid |
| `{version, accounts}` | `container: Wrapper`，2 条 valid |
| 旧版 `{credentials:{...}}` | `container: LegacyNested`，1 条 valid |
| 未知包装对象 | **HTTP 400**，报出顶层字段 `[data, meta, version]` |
| `authMethod: "oauth2"` | 逐条 `valid: false`，错误列出全部合法取值 |
| endpoint → `attacker.example` | 拒绝：域名不在允许的 Microsoft 登录域内 |
| endpoint → `...microsoftonline.com@attacker.example` | 拒绝：不得包含 userinfo |
| endpoint → `169.254.169.254` | 拒绝：不得使用 IP 地址 |
| `enabled: false` | 预检 `disabled: true` |

三类 SSRF 攻击均在**入口**被拒，未发起任何出站请求。

### 12.5 启动加载与迁移验证

用独立端口（18991）与独立目录，避免影响运行中的实例：

| 场景 | 日志/结果 |
| --- | --- |
| 平铺数组当 `credentials.json` | `已加载 2 个凭据配置`，**无备份文件**（原生格式不触发迁移，正确） |
| `{version, accounts}` 当 `credentials.json` | `凭据文件已从导入工具格式迁移为原生格式，原文件备份于 ".\credentials-wrapper.json.kam-backup-20260731T013626Z"` → 文件从 dict 变数组，`nickname` 已映射 |
| 备份文件内容 | 确认为原始 wrapper（`keys: [version, accounts]`，accounts 2 条） |
| 未知包装对象 | `ERROR 加载凭证失败: $ 处的 JSON 结构无法识别…该对象的顶层字段为 [data, meta, version]。支持的形态：…`，源文件未被改写 |

最后一项是缺陷二的修复确认：**旧版本此处会打印「已加载 1 个凭据配置」然后一个账号都不能用**
（untagged 把 wrapper 静默匹配成一条零字段凭据）。现在明确报错并指出 JSON 位置与顶层字段名。

### 12.6 端到端对话验证

| 凭据 | 连通性测试 | 说明 |
| --- | --- | --- |
| id=1（导入前已存在，对照组） | 成功 | 证明测试端点本身正常 |
| id=2（导入的 BuilderId IdC） | **失败** | `403 … Your User ID (…) temporarily is suspended` |
| id=3（导入的 Github Social） | **成功** | `claude-sonnet-4.5`、`claude-haiku-4.5` 均通过 |

公开 API 端到端：`POST /v1/messages` with `claude-sonnet-4.5` → 回复 "收到"，
`stop_reason: end_turn`，`usage: {input_tokens: 4111, output_tokens: 3}`。

两个失败/异常都定位到账号侧，非导入缺陷：

- **id=2 账号被上游暂时封禁。** 凭据数据入库正确、token 刷新成功、余额也查到了，
  但拉模型列表返回 403 suspended。属账号状态问题。
- **id=3 首次测试报 `400 INVALID_MODEL_ID`。** 测试端点默认用 `claude-sonnet-4.6`，
  而该账号可用模型只有 9 个、不含 4.6（只有 sonnet-4 与 4.5）。属订阅层级差异，
  换成账号支持的模型即成功。

### 12.7 profileArn 被清除的现象（预期行为）

两条导入都带警告「余额可用，但 profileArn 未解析」。Social 那条源文件里**有**
`profileArn`，但落盘后为空。

查证结论：该值恰好等于 `SOCIAL_SIGN_IN_PROFILE_ARN`
（`src/kiro/profile.rs:21-22`），被 `is_known_placeholder_profile_arn`（`:105-109`）
判为不可信并由 `resolve_profile_arn`（`:206-213`）主动清除。

这是 `profile-arn-resolution` spec 既定的正确行为，不是本次改动引入的问题：
上游对占位 ARN 常返回 403，带着它反而更糟。**KAM 导出的是 IDE 短路用的占位值，
不是真实 ARN。** id=3 在无 profileArn 的情况下对话成功，正好印证该设计。

对操作者的含义：导入 KAM 账号后看到「profileArn 未解析」警告属正常，
不影响对话；只有真实 ARN 才会被保留（external_idp 账号的 ARN 是真实值，
有单测 `test_external_real_profile_arn_is_trusted` 锁定）。

### 12.8 兼容矩阵更新

第 4.2 节表格的实现后状态：

| 登录格式 | 解析 | 字段保留 | 正确刷新 | 状态 | 真实数据验证 |
| --- | --- | --- | --- | --- | --- |
| Google Social | 是 | 是 | 是 | 支持 | 未覆盖（样本无） |
| GitHub Social | 是 | 是 | 是 | **支持** | ✓ 导入 + 对话成功 |
| BuilderId IdC | 是 | 是 | 是 | **支持** | ✓ 导入 + 刷新 + 余额成功（账号被封故未对话） |
| Enterprise IdC | 是 | 是 | 是 | 支持 | 未覆盖（样本无） |
| external_idp（机密客户端） | 是 | 是 | 是 | 支持 | ⚠ 未覆盖，见 12.9 |
| external_idp（公共客户端） | 是 | 是 | 是 | 支持 | ⚠ 未覆盖，见 12.9 |

第 4.1 节容器格式的实现后状态：四种容器在 Admin 导入与启动加载**两条路径**均支持，
「静默误解析」问题已消除（未知结构 fail fast）。

### 12.9 剩余风险

1. **external_idp 未经真实账号验证。** 本次样本无 Entra/Azure 账号。已验证的部分：
   endpoint 白名单的三类拒绝路径（真实端点实测）、form 构造（单测断言公共客户端
   不含 `client_secret` 键）、分派选择（单测断言带 secret 时仍选 external）。
   **未验证**：Microsoft token 端点的真实响应形状是否与 `ExternalRefreshResponse`
   匹配。若不匹配会在反序列化阶段失败——错误可见、不静默、不影响其他凭据。
   建议由具备 Entra 测试租户者做一次真实导入。

2. **Google Social 与 Enterprise IdC 未经真实数据覆盖**，但两者与已验证的
   GitHub Social / BuilderId IdC 共用同一刷新分支，仅 `provider` 字段不同，
   风险低。Enterprise 额外依赖 `startUrl`，有单测 `maps_enterprise_start_url` 覆盖。

3. **external 仍会为取 profileArn 而强刷一次**（已知遗留，非本次修复目标）。
   `refresh_routes_to_idc` 对 external 返回 `false`，故 profile 解析会执行一次
   注定无 ARN 收益的往返。修它需重新定义该谓词语义（从「是否走 OIDC」变为
   「是否返回 ARN」），会牵连 `profile-arn-resolution` 的既有 spec 场景，
   留作独立 change。已用两个测试锁定现状并标注非期望终态。

4. **admin-ui 无组件级渲染测试。** vitest 覆盖的是三个导出的纯函数
   （`parseJsonDocument`、`describePreviewItem`、`describeContainer`）；
   `handleImport` 把服务端结果映射为渲染状态的逻辑只有 `tsc -b` 的类型保证。
   本次真实验证走的是 HTTP API 而非浏览器 UI，UI 交互未验证。

### 12.10 验证过程中未发现的产品缺陷

实现期出现过 1 个真实产品缺陷并已修复：`kam_adapter` 的 `has_value` 原本只排除
`null`，而 `str_field` 会 trim 后过滤，导致 `"  "` 这类空白值在容器识别与字段提取
两处语义不一致。修法是让 `has_value` 也排除空白字符串。

本次真实数据验证**未发现新的产品缺陷**。两个测试失败全部定位到账号侧
（封禁、模型可用性），一个字段现象定位到既有的占位 ARN 设计。
