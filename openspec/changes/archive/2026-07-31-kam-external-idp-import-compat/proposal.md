## Why

kiro-account-manager（KAM）导出的 Microsoft Entra ID / Azure AD 账号（`authMethod = external_idp`）
在本项目**完全不可用**，且失败形态按入口分裂成三种互不相同的错误。

分析基线：kiro-rs `79086d4`（HEAD `e775835` 仅改 `.github/workflows`，源码等价）；
KAM `c5c4776`（manifest 1.9.2）。完整核查见 `docs/kiro-account-manager-export-compatibility-optimization.md`。

### 缺陷一：同一账号，三个入口三种错法

| 入口 | 行为 | 证据 |
| --- | --- | --- |
| Admin UI 导入 + 机密客户端（有 `clientSecret`） | 被重算为 `idc` → 刷新打 `oidc.*.amazonaws.com`，并被强填 `provider = BuilderId` | `kam-import-dialog.tsx:240-242,:252-254` |
| Admin UI 导入 + 公共客户端（无 `clientSecret`） | 前端**硬失败**，报「idc 模式需要同时提供 clientId 和 clientSecret」，记录进不了后端 | `kam-import-dialog.tsx:243-251` |
| 直接作为 `credentials.json` | 显式 `authMethod` 保留，但 `refresh_routes_to_idc` 只认 `{idc, builder-id, iam}` → 落 **Social** 端点 | `token_manager.rs:118-130` |

KAM 侧注释已写明误走 AWS OIDC 的后果：向 `oidc.*.amazonaws.com` 发微软 clientId 会得
`400 invalid_request`（`kam/src-tauri/src/commands/common.rs:451-479`）。

三种错法的共同根因相同：`KiroCredentials` 的 25 个字段里没有 `tokenEndpoint` /
`issuerUrl` / `scopes`（`src/kiro/model/credentials.rs:14-129`），刷新分派也只有
Social 与 AWS IdC 两路（`token_manager.rs:133-153`），没有 Microsoft OAuth2 分支。
全仓库 `tokenEndpoint` / `issuerUrl` 仅出现在 `src/kiro/online_auth.rs` 的 AWS OIDC
register-client **出站请求体**中，与凭据模型无关。

### 缺陷二：未知包装对象静默变成一条空凭据

`CredentialsConfig` 是 `#[serde(untagged)]` 的 `Single | Multiple`
（`credentials.rs:151-158`），而 `KiroCredentials` **零必填字段**——25 个字段全部
`Option<_>` 或带 `#[serde(default)]`，且全仓库无 `deny_unknown_fields`。

因此 `{ "version": "...", "accounts": [...] }` 不是被拒绝，而是**必然**匹配
`Single(KiroCredentials::default())`。后果链已确认：

```
main.rs:49-58   load 成功，不报错
main.rs:76      打印「已加载 1 个凭据配置」
token_manager.rs:721-739   自动禁用只对 api_key 生效 → 该空凭据保持启用
token_manager.rs:1194-1197 is_multiple_format == false → 刷新后不回写
```

用户看到的是「加载成功，但一个账号都不能用」，且日志无任何指向 JSON 结构的线索。

### 缺陷三：契约收窄只在前端，Rust 侧无校验

TS 把 `authMethod` 限成 `'social' | 'idc' | 'api_key'`（`admin-ui/src/types/api.ts:75`），
但 Rust `src/admin/types.rs:147-148` 是裸 `String` + `#[serde(default)]` 兜 `"social"`，
**无任何取值校验**。这意味着 `external_idp` 目前已能穿透 Admin API 并持久化，
只是没人给它正确的刷新分支；而未知 `authMethod` 会在刷新时静默 fallback 到 Social
（`refresh_routes_to_idc` 的 `else` 分支），错误延迟到运行期才暴露。

### 缺陷四：四处既存字段映射缺陷

核查过程中确认的独立缺陷，均在本 change 范围内（用户已确认纳入）：

1. **`enabled` 语义相反**：KAM `enabled: bool` 默认 `true`（`kam/core/account.rs:237,:243`），
   本项目是 `disabled: bool` 默认 `false`（`credentials.rs:101-102`）。当前导入丢弃该字段，
   KAM 侧已禁用的账号会静默变为启用。
2. **嵌套分支丢 `label`**：`label → nickname` 只在平铺分支映射（`kam-import-dialog.tsx:66-70`），
   旧版 `{credentials:{...}}` 分支原样返回（`:99`），导入后账号无昵称。
3. **region 只写 auth 专用字段**：UI 只写 `authRegion`（`:262`），native `region`
   恒为 `None`。刷新虽可用（`effective_auth_region` 的回退链在 `authRegion` 命中即止），
   但凭据级 `region` 作为通用来源被跳过，字段语义被压缩为「只服务刷新」。
   应写入通用 `region`，让既有回退链自然派生 auth region。

   **注意：这里不包含「`effective_api_region` 应回退到凭据级 `region`」。**
   Bridge Plan 8.1 已证伪该判断：auth region 与 api region 的回退链**故意不同**
   （`README.md:456` 含 `凭据.region`，`:459` 刻意不含；
   `token_manager.rs:3256-3270` 有带解释注释的断言测试；
   `config.rs:267-274` 同为两条互不引用的链）。auth region 是刷新端点的 region，
   api region 是数据面端点的 region，两者可以合法不同。KAM 账号若需区分 api region，
   由用户在 Admin UI 显式设置 `apiRegion`——这是既有设计意图。
4. **样例文件教用户填一个会被删掉的值**：`credentials.example.idc.json` 的 `profileArn`
   恰好等于 `BUILDER_ID_PROFILE_ARN`（`profile.rs:23-24`），而
   `is_known_placeholder_profile_arn`（`:105-109`）判其不可信、`resolve_profile_arn`
   （`:206-213`）会主动 `clear_profile_arn` 清除它。

### 缺陷五：格式矩阵零测试覆盖

`admin-ui/package.json` **无 `test` script、无 vitest/jest**，`parseKamJson` /
`normalizeKamAccount` / `handleImport` 零自动化覆盖。Rust 侧
`test_credentials_config_single` / `_multiple`（`credentials.rs:400-431`）只覆盖原生
扁平对象与数组，**无 wrapper-object 用例**——正是缺陷二所在。`supports_profiles` 已有
`external_idp` 分支（`profile.rs:49`）但无对应测试。也没有任何测试断言未知
`authMethod` 的路由归属。

## What Changes

### 1. 凭据模型与 Admin 契约扩展 external 元数据

`KiroCredentials`、`AddCredentialRequest`（Rust + TS）新增三个 camelCase 可选字段：

```
tokenEndpoint: Option<String>
issuerUrl:     Option<String>
scopes:        Option<String>     // 空格分隔单串，与 KAM 的 Option<String> 对齐，非 Vec
```

`scopes` 用 `String` 而非 `Vec<String>` 是为了与 KAM `Account.scopes`
（`kam/core/account.rs:211-212`）逐位对齐，避免导入时做有损转换。

### 2. 认证类型规范化：显式优先，未知拒绝

新增集中的认证类型规范化，替代当前「UI 猜一次、token_manager 再猜一次」的双重推断。
别名表与判别优先级见 design。核心两条：

- **显式 `authMethod` 优先于任何字段猜测。**
- **显式但未知的值在入口按记录报错**，不再静默 fallback 到 Social。

`refresh_routes_to_idc` 现有的「`auth_method` 为 `None` 时看 client 字段」推断作为
**缺省兜底**保留（旧文件兼容），但不再是 external 的判据。

### 3. Microsoft OAuth2 刷新分支

刷新分派从两路扩为四路：

```
api_key       → 拒绝 refresh（现状不变）
external_idp  → Microsoft OAuth2 refresh_token grant（新增）
idc           → AWS SSO OIDC JSON API（现状不变）
social        → Kiro Desktop Auth JSON API（现状不变）
```

endpoint 校验：HTTPS-only、URL 解析器取 hostname 后小写比较、精确或子域白名单、
拒 userinfo、**显式**拒 IP/loopback/link-local、`issuerUrl` 派生后复检同一校验器。
白名单 4 项与 KAM 一致（`login.microsoftonline.com`、`login.microsoftonline.us`、
`login.partner.microsoftonline.cn`、`login.chinacloudapi.cn`）。

KAM 侧 IP/localhost 只是**隐式**被域名白名单挡住（无显式判断），本项目补显式检查，
不照抄这一点。

### 4. 服务端 KAM adapter 作为单一解析事实源

新增 Rust 侧 KAM adapter，以 `serde_json::Value` 做结构判别后输出规范化
`Vec<KiroCredentials>`，支持四种容器：平铺单对象、平铺数组、`{version, accounts}`、
单个/数组的旧版 `{credentials:{...}}` 嵌套。Admin UI 与启动加载器**共用同一 adapter**，
终结「同一文件两个入口两种结果」。

### 5. 启动加载器：wrapper/nested 支持 + 备份后原子回写

- 先识别原生格式，再走 adapter 识别 KAM 容器。
- 未知包装对象 **fail fast** 并指出 JSON path，不再产生空凭据。
- 首次把 KAM 格式规范化回写为原生格式前：备份原文件 → 写临时文件 → 原子替换；
  失败不覆盖原文件。

注意 `persist_credentials`（`token_manager.rs:1221-1231`）当前是直接 `std::fs::write`，
非原子。本 change 引入的原子写工具应同时用于该处，否则迁移路径原子、日常回写不原子，
形成不一致（详见 design 的「原子写入统一」）。

### 6. 导入反馈逐条化

- 预览展示识别类型、provider、字段完整性；不展示 token、client secret、账号 `password`
  或 `proxyConfig.password`（KAM 导出这四类均为明文）。
- 缺字段、未知认证类型、非法 endpoint 均形成逐条失败结果。
- 公共客户端不再因缺 `clientSecret` 被判失败——用户不应被迫伪造 secret。
- 现状在**全部**记录无效时会 throw（`:141-143`），只有部分丢弃才静默
  （`:139,:147,:179`）；目标是逐条可见，而非仅全失败可见。

### 7. 三处既存缺陷修复

1. `enabled → disabled` 语义映射（`disabled = !enabled`，缺省 `enabled = true`）。
2. 嵌套分支补 `label → nickname` 映射。
3. 修正 `credentials.example.idc.json` 的占位 `profileArn`。

导入侧 region 改为写入通用 `region` 字段（而非只写 `authRegion`），
但**不改任何 region 解析函数**——`effective_api_region` 与 `effective_auth_region`
的既有回退链保持原样，理由见 What Changes 第 7 项前的说明与 Bridge Plan 8.1。

### 8. 测试基础设施与矩阵覆盖

- 为 admin-ui 引入测试运行器（vitest，与 Vite 工具链同源）。
- 制作**完全虚构**的脱敏 KAM fixtures，含「可选字段全为显式 `null`」样本——
  KAM 只有 `email` / `proxyConfig` 带 `skip_serializing_if`，其余一律输出 `null`。
- 固化容器格式 × 登录格式矩阵，含 endpoint 绕过样例。

### 9. 文档同步

README 补 KAM 支持范围、`external_idp` 字段说明、推荐导入入口与 endpoint 安全限制；
`authMethod` 说明从「`social` 或 `idc`」扩为四值（README:379）。
`config.example.json` 若引入新配置项则同步（当前评估为不需要，见 Non-Goals）。

## Non-Goals

- **不改 Social 与 IdC 的既有刷新行为。** 两条分支的端点、请求体、错误分类逐位不变。
- **不引入新的运行时配置项。** external endpoint 白名单硬编码在代码中，不做可配置化——
  可配置的白名单等于可绕过的白名单。因此 `config.example.json` 预期无需改动；
  若实现中发现必须新增配置项，须回到本 proposal 补充说明后再改。
- **不迁移 KAM 账号级 `proxyConfig`。** 本项目用另一套 `proxyUrl/proxyUsername/proxyPassword`
  （`credentials.rs:89-98`），字段语义与嵌套结构均不同，属独立 change。
- **不导入 KAM 的账号 `password`、`usageData`、`groupId`、`tagLinks`、
  `availableModelsCache`、`failureCount`、`successCount`。** 非登录格式核心。
- **不改 KAM 侧任何代码。** KAM 是外部只读参考。
- **不实现 external_idp 的登录流程**（授权码换 token）。本 change 只做**导入已有凭据 +
  刷新**，不做 OAuth 授权流。
- **不改 `profileArn` 固定占位表与占位识别逻辑**（`profile.rs:105`）。该语义已正确，
  见 Assumptions。
- **不改 `refresh_lock` 粒度、`persist_credentials` 的触发时机、
  或 `force_refresh_token_for` 语义。**
- **不做 KAM 的 API Key / 网关客户端密钥互操作。** 不属账号导出登录格式。
- **不给 Admin 快照回传任何 secret 明文**，包括 endpoint 之外的字段；
  仅增加非敏感的「是否已配置」状态。

## Assumptions

- **Microsoft OAuth2 `refresh_token` grant 的请求形状与 KAM 已验证实现一致**：
  `application/x-www-form-urlencoded`，`client_id` 与 `grant_type` 与 `refresh_token`
  必填，`scope` 与 `client_secret` 仅非空时追加（`kam/auth/providers/external_idp.rs:139-153`）。
  本 change 在 kiro-rs 中**独立实现并独立测试**，不引用 KAM 代码。
- **固定占位 ARN 问题已修复，本 change 无需新功能。** `get_fixed_profile_arn`
  已是 dead code（`profile.rs:93` 标 `#[allow(dead_code)]`，调用点全在 `#[cfg(test)]`），
  `profile.rs:221` 注释明确 "Fixed ARN table is documentation/history only —
  never short-circuit or persist"。本 change 只补 external 的回归测试锁定该行为。
- **KAM 导出的 `authMethod` 可能是 KAM 自己猜的值。** `export_accounts` 的
  `fix_account`（`kam/commands/account_cmd.rs:911-956`）在字段为 null 时按
  `startUrl`/`clientSecret`/邮箱域猜测并回填。这**不改变**「显式值优先于字段猜测」
  的设计方向（仍比我们自己猜更可靠），但意味着 external 分支不能只凭 `authMethod`
  就发请求，必须同时校验 endpoint 存在——已反映在 design 的校验规则中。
- **KAM 对 external 账号刻意置 `provider = null`**（`kam/commands/account_cmd.rs:863-864`），
  因此 external 分支**不得**因 provider 缺省而回填 `BuilderId`。
- **上游 Kiro 接受 external_idp 账号的真实 `profileArn`。** external 账号的 `profileArn`
  是真实值而非占位，须原样保留。此前提由既有 `credential-import` spec 的
  「MUST NOT discard a provided profileArn」确立，非本 change 新增。
- `KiroCredentials` 新增可选字段不破坏旧 `credentials.json` 加载——依据既有
  `credential-ingest` spec 的「旧文件缺字段可加载」场景与 `#[serde(default)]` 惯例。

## Impact

### 代码

| 文件 | 改动性质 |
| --- | --- |
| `src/kiro/model/credentials.rs` | 新增 3 字段；认证类型规范化；wrapper 判别。**region 解析函数不改** |
| `Cargo.toml` | 新增 `url` 直接依赖（当前仅为 reqwest 传递依赖，见 Bridge Plan 8.2） |
| `.gitignore` | 加 `!/credentials.example.*.json` 例外，否则新增 example 被 `/credentials.*` 吞掉（Bridge Plan 8.4） |
| `admin-ui/pnpm-lock.yaml` | 同步 vitest；CI 用 `--frozen-lockfile`，不同步则 install 失败（Bridge Plan 8.3） |
| `src/kiro/kam_adapter.rs`（新增） | KAM 四容器结构判别 → `Vec<KiroCredentials>` |
| `src/kiro/token_manager.rs` | 新增 external refresh 分支；分派改四路；原子写入 |
| `src/kiro/external_idp.rs`（新增） | endpoint 校验 + Microsoft OAuth2 refresh |
| `src/admin/types.rs` | `AddCredentialRequest` 加 3 字段 + `auth_method` 取值校验 |
| `src/admin/service.rs` | ingest 透传 external 元数据 |
| `src/admin/handlers.rs`、`src/admin/router.rs` | 新增 KAM 专用导入 endpoint |
| `src/main.rs` | 启动加载走 adapter；未知包装 fail fast |
| `src/kiro/profile.rs` | **仅新增 external 回归测试**，不改逻辑 |
| `admin-ui/src/components/kam-import-dialog.tsx` | 尊重显式 authMethod；透传 external；补 label/enabled 映射；逐条反馈 |
| `admin-ui/src/types/api.ts` | `authMethod` 联合类型加 `external_idp` |
| `admin-ui/package.json` | 新增 vitest 与 `test` script |
| `credentials.example.idc.json` | 修正占位 profileArn |
| `credentials.example.external.json`（新增） | 仅占位值的 external 样例 |
| `README.md` | KAM 支持范围、authMethod 四值、endpoint 安全限制 |

`KiroCredentials` 在 `src/` 下有 10 个文件、184 处引用，集中在 `token_manager.rs`（81）、
`model/credentials.rs`（49）、`profile.rs`（21）、`machine_id.rs`（15）。新增字段为可选，
不改既有字段语义，但需全量回归。

### spec

- MODIFIED `credential-import`：KAM 容器格式、显式 authMethod 优先、external 字段透传、
  enabled/label 映射、逐条反馈。
- MODIFIED `credential-ingest`：external_idp 认证族的校验规则与 refresh gate。
- ADDED `external-idp-credentials`（新能力）：认证类型规范化、endpoint 安全校验、
  Microsoft OAuth2 刷新、启动加载兼容与原子迁移。

`profile-arn-resolution` **不修改**——external 的 ARN 语义已被既有 requirement 覆盖，
本 change 只补测试。

### 风险类型（AGENTS.md 高风险矩阵）

Token / 多凭据、Admin / 凭据 CRUD、配置 schema 行为变化、admin-ui。
对应验证：`cargo test`、admin 测试、example 配置完整性、`pnpm build`。

### README / AGENTS / spec 同步

README 必须同步（`authMethod` 取值、KAM 支持范围、示例文件清单）。
AGENTS.md 无需改动——不涉及 AI 纪律或验证命令变化。

## Success Criteria

- 六种登录格式（Google/GitHub Social、BuilderId/Enterprise IdC、
  external_idp 机密/公共客户端）全部达到「正确刷新」。
- **同一脱敏 fixture 从 Admin 导入与启动加载得到等价的规范化凭据。**
  当前 external 在两个入口分别走 IdC 与 Social，这是最直接的验收信号。
- external 公共客户端无需伪造 `clientSecret` 即可导入。
- 显式未知 `authMethod` 在入口按记录报错，不再静默走 Social。
- 未知包装对象不再产生「已加载 1 个凭据配置」的空凭据，错误信息含 JSON path。
- endpoint 绕过测试全部通过，含显式 IP/loopback 与反斜杠混淆用例。
- KAM 格式回写迁移：备份存在、原子替换、失败不覆盖原文件。
- Social / BuilderId / Enterprise 三类账号的刷新路由、region 与 profile 行为无回归。
- 日志与 API 错误体不含 token、client secret、password 明文。
- `cargo test` 通过；`pnpm --dir admin-ui build` 通过；新增前端测试通过；
  `openspec validate --all` 通过。

## Risks

- **信任导出文件中的 token endpoint → SSRF / 凭据外传。** 这是本 change 最高风险项：
  导入文件是外部输入，若 endpoint 未严格校验，等于把导入功能变成任意 SSRF 入口，
  且泄露的是 refresh token。缓解：HTTPS + 精确 hostname 白名单 + 显式 IP/loopback 拒绝 +
  派生后复检 + 专项绕过测试。**白名单不可配置化**（见 Non-Goals）。
- **扩展 `KiroCredentials` 影响面大。** CodeGraph 实测 `KiroCredentials` 影响
  **151 个符号**（`codegraph impact`，见 Bridge Plan 5.1）。缓解：新增字段全部可选、
  不改既有字段语义、round-trip 测试 + Token/Profile/Admin 全量回归。
- **启动加载器回写迁移可能损坏用户凭据文件。** 缓解：备份 → 临时文件 → 原子替换，
  失败不覆盖；README 明确推荐路径仍是 Admin 导入，直接换文件属离线迁移能力。
- **本 change 不含任何影响非 KAM 账号的语义变更。** 早期草案曾计划改
  `effective_api_region` 的回退链，Bridge Plan 8.1 证伪该判断后已撤销。
  所有 region 解析函数保持原样，`token_manager.rs:3256-3273` 的两个既有断言测试
  必须继续通过——若实现中它们失败，说明动了不该动的地方。
- **导入时强制刷新在大批量下触发上游限流。** 缓解：沿用既有 batch 默认并发 1
  （`credential-ingest` spec 已确立），逐条结果，`invalid_grant` 不重试。
- **错误体回显上游响应泄露 token/租户信息。** 缓解：结构化脱敏错误，
  禁止记录请求 form 与完整响应敏感字段。
- **前端测试基础设施是新增依赖面。** vitest 与既有 Vite 工具链同源，但仍是新依赖；
  须固定版本，且 `pnpm build` 不受影响。
- **`.gitignore` 例外规则的副作用。** 加 `!/credentials.example.*.json` 会让所有
  匹配该模式的文件变为可跟踪。风险是将来有人把真实凭据命名为
  `credentials.example.mine.json` 而被意外提交。缓解：例外规则收窄到
  `credentials.example.*.json`（不是 `credentials.*`），且 AGENTS.md 的
  「忽略并永不提交」纪律与提交前 `git status --short` 检查继续适用。
- **回滚**：无数据迁移不可逆步骤——新增字段可选，旧文件仍可加载；唯一有状态副作用是
  KAM 格式回写迁移，但有备份文件。`git revert` 后已迁移的 `credentials.json`
  仍是合法原生格式，可正常加载。`.gitignore` 与 `Cargo.toml` 改动随 revert 一并回退。
