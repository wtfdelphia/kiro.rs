## 当前实现

### 认证类型的三处独立推断

同一个语义（这条凭据是什么登录方式）在三个地方各自实现，规则互不相同：

```
1. admin-ui/src/components/kam-import-dialog.tsx:240-242
   clientId && clientSecret ? 'idc' : 'social'        ← 完全忽略导出的 authMethod

2. src/kiro/model/credentials.rs:136-144
   canonicalize_auth_method_value：builder-id|iam → idc，api_key|apikey → api_key
                                                      ← 未知值原样透传

3. src/kiro/token_manager.rs:118-130
   refresh_routes_to_idc：auth_method 为 None 时才看 client 字段，
                          否则只认 {idc, builder-id, iam}，其余落 Social
```

规则 1 与规则 3 对同一条 external 凭据给出相反答案，这就是 proposal 缺陷一里
「UI 走 IdC、文件走 Social」的直接来源。

### 刷新分派

```rust
// token_manager.rs:133-153
if credentials.is_api_key_credential() { bail!("API Key 凭据不支持刷新 Token"); }
validate_refresh_token(credentials)?;
if refresh_routes_to_idc(credentials) {
    refresh_idc_token(...)     // oidc.{region}.amazonaws.com/token，JSON body
} else {
    refresh_social_token(...)  // prod.{region}.auth.desktop.kiro.dev/refreshToken，JSON body
}
```

两路都是 JSON body。external 需要的是 form-urlencoded，形状不同，不能复用任一分支。

### 文件加载

```rust
// credentials.rs:151-158
#[serde(untagged)]
pub enum CredentialsConfig { Single(KiroCredentials), Multiple(Vec<KiroCredentials>) }
```

`load`（`:166-182`）只做「文件不存在 → 空数组」「内容为空 → 空数组」两个特判，
其余直接 `serde_json::from_str`。因为 `KiroCredentials` 零必填，untagged 的
`Single` 分支会吞下任何 JSON 对象。

### 写入非原子

```rust
// token_manager.rs:1221-1231
let json = serde_json::to_string_pretty(&credentials)?;
if tokio::runtime::Handle::try_current().is_ok() {
    tokio::task::block_in_place(|| std::fs::write(path, &json))?;
} else {
    std::fs::write(path, &json)?;
}
```

直接覆写目标文件。进程在 write 中途死亡会留下截断的 `credentials.json`。

## 目标设计

### 1. 认证类型规范化：单一事实源

在 `src/kiro/model/credentials.rs` 集中实现，UI 与 token_manager 都不再自行推断。

别名表（大小写不敏感）：

| 输入别名 | 规范值 |
| --- | --- |
| `social` | `social` |
| `idc`、`IdC`、`builder-id`、`iam` | `idc` |
| `external_idp`、`external-idp`、`externalidp`、`azure`、`azuread`、`azure_ad` | `external_idp` |
| `api_key`、`apikey` | `api_key` |

`IdC` 是 KAM 实际写入的字面量（`kam/core/account.rs` 系列写入点），
`external-idp` / `azure*` 别名与 KAM 的 dispatch 读取端对齐
（`kam/commands/common.rs:404-416`）。

判别优先级：

```
1. 有合法显式 authMethod        → 用它
2. 无 authMethod，但有白名单内的 tokenEndpoint/issuerUrl → external_idp
3. 无 authMethod，但 clientId + clientSecret 齐全        → idc
4. 无 authMethod，有 refreshToken                        → social
5. 有 authMethod 但不在别名表内                          → 按记录报错，不降级
```

**第 2 步必须排在第 3 步之前**：external 账号也可能同时拥有 clientId 与 clientSecret，
若先判 IdC 则机密客户端永远进不了 external 分支。这与 KAM 的 dispatch 顺序一致
（`kam/commands/common.rs:451-479`，external 先于 `:483` 的 IdC 判定）。

第 5 步是本 change 的行为变更点：当前未知值静默落 Social（`refresh_routes_to_idc`
的 `else`），改为入口拒绝。

**兼容性边界**：第 5 步只作用于**导入入口**（Admin API / KAM adapter）。
启动加载既有文件时，未知 `authMethod` 应产生**明确的加载错误**而非静默 Social ——
但这会让某些当前能启动的畸形文件启动失败。取舍：宁可 fail fast（与缺陷二的修复方向
一致），并在错误信息里给出该凭据的 index 与合法取值列表。

#### 为何不复用 `canonicalize_auth_method_value`

现有函数（`credentials.rs:136-144`）的契约是「归一已知别名，未知原样透传」，
被 `canonicalize_auth_method`（落盘前调用，`token_manager.rs:1211`）依赖。
若改成「未知即报错」，会让既有落盘路径在遇到历史脏数据时 panic/报错。
故新增独立的 `parse_auth_method(&str) -> Result<AuthMethod, UnknownAuthMethod>`，
现有函数保持不变。

#### `AuthMethod` 用枚举而非字符串

新增内部枚举：

```rust
pub(crate) enum AuthMethod { Social, Idc, ExternalIdp, ApiKey }
```

持久化字段 `KiroCredentials.auth_method` **保持 `Option<String>`**，不改为枚举——
改成枚举会让任何历史脏值直接导致整个 `credentials.json` 反序列化失败，
影响面远超本 change。枚举只用于内部决策，在决策入口处解析一次。

### 2. external endpoint 校验

新增 `src/kiro/external_idp.rs`：

```rust
const ALLOWED_HOSTS: &[&str] = &[
    "login.microsoftonline.com",
    "login.microsoftonline.us",
    "login.partner.microsoftonline.cn",
    "login.chinacloudapi.cn",
];

pub(crate) fn validate_token_endpoint(raw: &str) -> Result<Url, EndpointRejected>
```

校验顺序（每步失败即拒，不继续）：

1. `Url::parse` 成功 —— 用解析器，不做字符串处理。
2. scheme 必须为 `https`。
3. `username()` 为空且 `password()` 为 `None` —— 拒 userinfo。
4. `host()` 必须是 `Host::Domain`（**显式**排除 `Host::Ipv4` / `Host::Ipv6`）。
5. domain 小写后不得为 `localhost`，且不得以 `.localhost` 结尾。
6. domain 小写后精确等于白名单某项，或以 `.<白名单项>` 结尾（子域）。

第 4 步是与 KAM 的差异点：KAM 只靠白名单隐式挡住 IP
（`kam/auth/providers/external_idp.rs:46-87` 无显式 IP 判断），本项目显式拒绝，
使意图在代码中可读、在测试中可断言。

`issuerUrl` 派生规则：`{issuer}/oauth2/v2.0/token`（Microsoft v2.0 端点惯例），
拼接后**再次**走同一个 `validate_token_endpoint`。派生不绕过校验。

#### 为何白名单不可配置

可配置的白名单等于可绕过的白名单：攻击面从「导入文件」扩大到「导入文件 + 配置文件」，
而配置文件同样可能来自不可信来源（Docker 挂载、CI 注入）。
Microsoft 云的登录域是稳定的公开事实，硬编码的维护成本低于该风险。

### 3. external refresh 请求

```
POST <validated token_endpoint>
Content-Type: application/x-www-form-urlencoded
Accept: application/json

grant_type=refresh_token
client_id=<clientId>                  必填
refresh_token=<refreshToken>          必填
scope=<scopes>                        仅当非空
client_secret=<clientSecret>          仅当非空
```

`client_secret` 与 `scope` 的「仅非空追加」语义是公共客户端可用的关键：
公共客户端没有 secret，发空串会被 Microsoft 拒绝。

响应解析：`access_token`（必需）、`expires_in` 或 `expires_at`、
可选轮换的 `refresh_token`。若响应含新 `refresh_token` 则轮换，与 IdC/Social
分支的既有轮换语义一致（`token_manager.rs:310-312` 形态）。

**external 不产生 `profileArn`**——Microsoft token 端点不返回它。因此 external 凭据
的 ARN 必须来自导入时保留的真实值或 `ListAvailableProfiles`。这一点直接决定：
`refresh_routes_to_idc` 那条「刷新端点是否返回 profileArn」的判定，
对 external 应给出「不返回」。见下节。

#### 与 profile ARN 解析的交互

既有 spec（`profile-arn-resolution`）的「IdC 账号不得为取 ARN 而强制刷新」
判定谓词是 `refresh_routes_to_idc`。external 同样属于「刷新端点不返回 profileArn」，
逻辑上应享受同一软放行。

但**本 change 不改 `profile.rs` 的逻辑**（见 proposal Non-Goals）。原因：
`refresh_routes_to_idc` 对 external 返回 `false`（它只认 `{idc, builder-id, iam}`），
所以 external 会落到 Social 的强刷分支——这是一次注定无 ARN 收益的往返。

取舍：本 change 只**加测试记录该行为**，不修。理由是修它需要重新定义
`refresh_routes_to_idc` 的语义（从「是否走 OIDC」变为「是否返回 ARN」），
而该函数是 `2026-07-30-profile-arn-refresh-fallback-order` 刚确立的单一事实源，
改动会牵连该 change 的全部 spec 场景。留作独立 change 更安全。
**已在 tasks 中标记为已知遗留项**，不是遗漏。

### 4. KAM adapter

新增 `src/kiro/kam_adapter.rs`，以 `serde_json::Value` 判别，不用 untagged：

```
Value::Array                                   → 平铺数组，逐项 normalize
Value::Object 含 "accounts": Array              → wrapper，取 accounts 逐项 normalize
Value::Object 含 "credentials": Object          → 旧版嵌套单条
Value::Object 含 "refreshToken": String         → 平铺单条
其他 Value::Object                              → Err，附 JSON path 与已识别的顶层 key 列表
```

判别顺序有意义：wrapper 判定必须先于平铺单条，否则一个同时含 `accounts` 与
`refreshToken` 的畸形对象会被误判。

单条 normalize 负责：

| KAM 字段 | 目标 | 备注 |
| --- | --- | --- |
| `refreshToken` / `accessToken` / `expiresAt` | 同名 | |
| `authMethod` | 走 `parse_auth_method` | 显式优先 |
| `provider` | 同名 | external 不回填 BuilderId |
| `clientId` / `clientSecret` | 同名 | external 的 secret 可缺 |
| `tokenEndpoint` / `issuerUrl` / `scopes` | 新增字段 | |
| `region` | 只写通用 `region`（不写 `authRegion`） | 见下 |
| `startUrl` / `profileArn` / `machineId` | 同名 | |
| `userId` / `email` | 同名 | |
| `label` | `nickname` | **平铺与嵌套两条路径都要映射** |
| `enabled`（缺省 `true`） | `disabled = !enabled` | 语义取反 |

其余 KAM 字段（`password`、`usageData`、`groupId`、`tagLinks`、
`availableModelsCache`、`failureCount`、`successCount`、`proxyConfig`）显式丢弃。

**必须容忍全字段 `null`**：KAM 只有 `email` 与 `proxyConfig` 带
`skip_serializing_if`（`kam/core/account.rs:175,:239`），其余可选字段一律输出显式
`null`。`Option<T>` 对 `null` 天然处理，但 `Value` 判别代码里不能用
`obj.contains_key("x")` 判断有值——必须同时排除 `Value::Null`。

#### `region` 写哪个字段

当前 UI 只写 `authRegion`（`kam-import-dialog.tsx:262`），native `region` 恒为 `None`。
两种修法：

- **A：同时写 `region` 与 `authRegion`。** 语义最贴近 KAM——KAM 的 `region`
  同时用于 OIDC 端点与其他区域决策，没有 auth/api 之分。
- **B：只写 `region`，靠 `effective_auth_region` 的既有回退链取到它。**
  `effective_auth_region`（`credentials.rs:220-227`）已经是
  `auth_region > region > config.auth_region > config.region`，写 `region` 就够。

**选 B。** 理由：写两个字段是数据冗余，将来若用户在 Admin UI 单独改了 `authRegion`，
`region` 会变成一个陈旧的影子值，语义歧义。只写单一来源、靠回退链派生更干净。

#### 不改 `effective_api_region`（撤销的早期决策）

早期草案计划让 `effective_api_region` 也回退到凭据级 `region`，理由是「两个函数对
同一字段给出不同回退语义本身就是缺陷」。**Bridge Plan 8.1 证伪了该判断，已撤销。**

三重反证说明两条链的差异是有意设计：

1. `src/kiro/token_manager.rs:3256-3270` 的既有测试带解释性注释明确断言该行为：
   ```rust
   config.region = "us-west-2";
   credentials.region = Some("eu-west-1");
   // 凭据.region 不参与 api_region 回退链
   assert_eq!(api_host, "q.us-west-2.amazonaws.com");
   ```
2. `README.md:456` 与 `:459` 分两行文档化了两条链，前者含 `凭据.region`，
   后者刻意不含。
3. `src/model/config.rs:267-274` 的 Config 层同为两条互不引用的链。

语义解释：auth region 是**刷新端点**的 region，api region 是**数据面端点**的 region，
两者可以合法不同（例如在 A 区认证、在 B 区调用）。把它们统一会破坏这种配置能力。

因此本 change **不改任何 region 解析函数**。KAM 账号导入后：
`effective_auth_region` 通过 `region` 回退取到导入值（刷新正确）；
`effective_api_region` 仍取全局配置。若用户需要凭据级 api region，
在 Admin UI 显式设置 `apiRegion`——这是既有设计意图，不是缺陷。

`token_manager.rs:3256-3273` 的两个断言测试必须继续通过；若实现中它们失败，
说明动了不该动的地方。

### 5. 启动加载与迁移

```
CredentialsConfig::load(path)
  ├─ 文件不存在 / 空 → Multiple(vec![])                        （现状不变）
  ├─ 解析为 Value
  ├─ 原生格式判别（Array，或含 refreshToken/kiroApiKey 的 Object）
  │    → 按现有路径反序列化，不触发迁移
  ├─ KAM 容器判别（wrapper / nested）
  │    → adapter 规范化 → 标记需迁移
  └─ 都不匹配 → Err（含 JSON path 与顶层 key 列表）
```

「原生格式判别」必须先行，且判据要比 untagged 严：含 `refreshToken` 或
`kiroApiKey` 的对象才算原生单条。这样 `{version, accounts}` 不再落入 Single。

迁移写回（仅在识别到 KAM 容器时触发一次）：

```
1. 备份：copy path → path.kam-backup-<UTC timestamp>
2. 序列化规范化后的 Vec<KiroCredentials> 为 pretty JSON
3. 写临时文件：path.tmp-<pid>
4. 原子替换：fs::rename(tmp, path)
5. 任一步失败 → 保留原文件，warn 后以内存中的规范化结果继续运行
```

第 5 步的取舍：迁移失败不应阻止启动。凭据已在内存中正确解析，
只是没能持久化为原生格式；下次启动会再试一次。

#### 原子写入统一

`persist_credentials`（`token_manager.rs:1221-1231`）当前是直接 `std::fs::write`。
本 change 引入的原子写工具应同时用于该处，否则迁移路径原子、日常回写不原子。

这超出了「KAM 兼容」的字面范围，但属于 proposal 明确列出的改动（What Changes 第 5 项）。
理由：在同一个文件上同时存在原子与非原子两条写路径，是比两者都不原子更糟的状态——
读者无法判断哪条路径可信。若实现中发现改动 `persist_credentials` 会牵连过多测试，
须停下确认，不得单方面缩减为「只让迁移原子」。

Windows 注意：`fs::rename` 在目标已存在时于 Windows 上会失败。需用
`std::fs::rename` 前先确认平台行为，或直接使用同目录内的替换语义
（Windows 下 `rename` 覆盖同目录已存在文件在现代 Rust std 中可行，
但须以测试验证而非假设）。

### 6. Admin 契约与导入端点

`AddCredentialRequest`（`src/admin/types.rs:142-216`）新增三字段，
并新增 `auth_method` 取值校验——当前是裸 `String` 无校验。

校验规则按认证族：

| 族 | 必需 | 可选 |
| --- | --- | --- |
| social | `refreshToken` | `profileArn`、`machineId` |
| idc | `refreshToken` + `clientId` + `clientSecret` | `startUrl`（Enterprise 应有） |
| external_idp | `refreshToken` + `clientId` + (`tokenEndpoint` 或 `issuerUrl`) | `clientSecret`、`scopes` |
| api_key | 沿用现有 | |

新增 `POST /api/admin/credentials/import/kam`：接收原始 KAM 文档（`Value`），
服务端跑 adapter + 逐条预检，返回逐条结果。

现有 `/credentials/import/batch` **保持不变**作为原生凭据 API
（既有 `credential-ingest` spec 已定义其契约）。

### 7. 前端改动

`kam-import-dialog.tsx`：

- 移除 `:240-242` 的类型重算与 `:243-251` 的硬失败。
- 改为把原始文档 POST 到新的 KAM 导入端点，渲染服务端逐条结果。
- 保留客户端预览（识别类型、provider、字段完整性），但预览也由服务端预检结果驱动，
  避免前后端两套判别规则再次分叉。

`admin-ui/src/types/api.ts:75` 的 `authMethod` 联合类型加 `'external_idp'`。

前端测试用 vitest（与 Vite 同源工具链，配置成本最低）。

## 数据流

```
KAM 导出文件（平铺数组，含明文 secret/password）
        │
        ├─── 路径 A：Admin UI 导入（推荐）
        │      POST /api/admin/credentials/import/kam  { 原始 Value }
        │        → kam_adapter：容器判别 → 逐条 normalize
        │        → parse_auth_method（显式优先，未知拒绝）
        │        → 按族校验必需字段
        │        → external：validate_token_endpoint
        │        → ingest_credential → refresh_token（四路分派）
        │        → 逐条结果回前端
        │
        └─── 路径 B：直接作为 credentials.json（离线迁移）
               CredentialsConfig::load
                 → 原生判别 → 若否则 kam_adapter
                 → 同一个 normalize + parse_auth_method
                 → 备份 + 原子回写为原生格式
                 → 运行期 refresh_token（同一个四路分派）
```

两条路径共用 `kam_adapter` 与 `parse_auth_method`，这是「同一 fixture 两个入口
等价结果」这条验收标准的实现基础。

## 影响面

`KiroCredentials` 新增字段的传播路径（依据 184 处引用的分布）：

- `token_manager.rs`（81 处）：refresh 分派、ingest 字段 overlay（`:1945-1988`）、
  persist（`:1205-1216`）。新增字段必须进 overlay，否则 upsert 会丢。
- `profile.rs`（21 处）：`supports_profiles` 已有 external 分支（`:49`），
  逻辑不改，只加测试。
- `machine_id.rs`（15 处）：与新增字段无关。
- `admin/service.rs`（4 处）：`ingest_from_request` 的字段组装（`:354-380`）需加三字段。
- `main.rs`（3 处）：加载路径。
- `models_api.rs` / `provider.rs` / `endpoint/mod.rs`：与新增字段无关。

## 异常路径

| 场景 | 行为 |
| --- | --- |
| 显式 `authMethod` 不在别名表 | 导入：该条 failed，附合法取值列表。加载：Err 并指出 index |
| external 缺 `tokenEndpoint` 与 `issuerUrl` | 该条 failed，明确说明 external 需要其一 |
| endpoint 非 HTTPS / 非白名单 / 含 userinfo / 是 IP / 是 localhost | 该条 failed，错误**不回显完整 URL 的 userinfo 部分** |
| `issuerUrl` 派生后仍不合法 | 该条 failed，说明派生结果被拒 |
| external refresh 返回 `invalid_grant` | 标记凭据失效，**不重试**（沿用既有 invalid_grant 语义） |
| external refresh 返回其他 4xx/5xx | 按既有瞬态/永久分类处理；错误体脱敏后记录 |
| KAM 容器判别失败 | 加载：Err 含 JSON path + 顶层 key 列表。导入：整体 400，不产生部分导入 |
| 迁移备份失败 | 不写回，warn，以内存结果继续运行 |
| 迁移原子替换失败 | 保留原文件与备份，warn，以内存结果继续运行 |
| 混合批次单条失败 | 其余条目照常处理（沿用既有 batch 语义，不因单条失败回滚） |

## 回滚

- 新增字段全部可选，`git revert` 后旧版本仍能加载含 external 字段的
  `credentials.json`（未知字段被 Serde 忽略）。
- 唯一有状态副作用是 KAM 格式回写迁移。revert 后已迁移的文件是合法原生数组格式，
  旧版本可正常加载。备份文件 `*.kam-backup-*` 保留在原目录，可手工恢复。
- 无数据库、无 schema migration、无不可逆步骤。
- 本 change 不含影响非 KAM 账号的语义变更，故 revert 不会改变既有账号的行为。
- `Cargo.toml` 的 `url` 依赖与 `.gitignore` 的例外规则随 revert 一并回退；
  `admin-ui/pnpm-lock.yaml` 同理。

## 验证策略

分层，每层可独立断言：

| 层 | 手段 | 关键点 |
| --- | --- | --- |
| endpoint 校验 | 纯函数单测 | 合法 4 域 + 子域；HTTPS/userinfo/IP/localhost/反斜杠混淆全部拒绝 |
| 认证类型规范化 | 纯函数单测 | 全别名 + 未知值拒绝 + 5 步 fallback 顺序（尤其 external 先于 idc） |
| KAM adapter | 表驱动单测 + 脱敏 fixture | 4 容器 × 4 登录格式；全字段 `null` 样本；label/enabled 映射 |
| 模型 round-trip | serde 单测 | 三新字段序列化/反序列化不丢；旧文件缺字段可加载 |
| 刷新路由 | 纯函数单测 | external 带 clientSecret 时仍选 external，不落 IdC |
| external request 构造 | 纯函数单测 | form 字段；公共客户端无 `client_secret` 键；scope 空时无 `scope` 键 |
| 加载与迁移 | 临时目录集成测 | 原生/wrapper/nested/未知包装；备份存在；原子替换；失败不覆盖 |
| Admin 契约 | 既有 types/service 测试扩展 | 三字段反序列化；authMethod 校验；混合批次逐条结果 |
| 前端 | vitest | 服务端结果渲染；不再本地重算类型 |
| 安全 | 断言错误体与日志 | 无 token / clientSecret / password 明文 |
| 回归 | `cargo test` 全量 | Social/IdC 路由、region 优先级、profile 行为不变 |

**不做真实账号在线验活。** 若需本地验证，只用临时凭据，且确认
`config.json`、`credentials.json`、`credentials.*`、`.codegraph/` 不进 Git 候选。

验证命令：

```powershell
cargo test kiro::model::credentials
cargo test kiro::kam_adapter
cargo test kiro::external_idp
cargo test kiro::token_manager
cargo test kiro::profile
cargo test admin
cargo test
pnpm --dir admin-ui test
pnpm --dir admin-ui build
openspec validate --all
git status --short
```

## 未决与已知遗留

1. **external 凭据仍会为取 profileArn 而强刷一次。** `refresh_routes_to_idc` 对
   external 返回 `false`，故落 Social 强刷分支，而 Microsoft token 端点不返回
   `profileArn`。修它需重新定义该函数语义，牵连 `profile-arn-resolution` 的既有
   spec 场景，留作独立 change。本 change 加测试记录现状。
2. **KAM 导入账号的 api region 仍取全局配置。** 按上文「不改 `effective_api_region`」
   的结论，这是既有设计而非遗留缺陷。若将来出现真实的跨区需求（认证区与数据面区
   必须都跟随凭据），应作为独立 change 讨论是否引入凭据级 `apiRegion` 的导入映射，
   而不是改回退链。
3. **Windows `fs::rename` 覆盖行为需以测试验证**，不得假设。
4. **KAM 的 `fix_account` 会回填猜测值**，故导出文件的 `authMethod` 未必是登录时的真值。
   本 change 仍以显式值优先（比自己猜更可靠），但 external 分支额外要求 endpoint 存在。
5. **`.gitignore` 例外规则扩大了可跟踪范围。** 加 `!/credentials.example.*.json` 后，
   任何该命名模式的文件都不再被忽略。依赖 AGENTS.md 纪律与提交前
   `git status --short` 检查兜底。
