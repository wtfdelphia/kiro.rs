## 当前实现

### resolve_profile_arn 决策流

```
resolve_profile_arn(token_manager, credential_id, credentials, token)
  │
  ├─ profile.rs:143  trusted_profile_arn 命中 ────────────────→ return Ok(arn)
  │
  ├─ profile.rs:148  持久化值是已知占位 → clear_profile_arn
  ├─ profile.rs:157  is_api_key_credential ──────────────────→ Err(ProfileArnUnsupported)
  ├─ profile.rs:161  provider = infer_provider(credentials)
  ├─ profile.rs:165  !supports_profiles && provider.is_none() → Err(ProfileArnUnsupported)
  │
  ├─ profile.rs:172  list_available_profiles_with_retry ──[真实 HTTP]
  │     ├─ Ok(非空非占位) → set_profile_arn ──────────────────→ return Ok(arn)
  │     ├─ Ok(占位)       → list_err = "list returned placeholder"
  │     ├─ Ok(空)         → list_err = "empty profile list"
  │     └─ Err(e)         → list_err = e
  │
  ├─ profile.rs:186  refresh_token.is_some() ────────────────┐
  │     │                                                    │  ← 所有带
  │     ├─ force_refresh_token_for Ok                        │    refreshToken
  │     │     ├─ profile_arn_of 命中非占位 → return Ok(arn)   │    的凭据都在
  │     │     ├─ 命中占位 → clear_profile_arn                │    此处 return
  │     │     └─ → Err(ProfileArnUnavailable)                │
  │     └─ force_refresh_token_for Err → bail!(...)          │
  │                                                          ─┘
  ├─ profile.rs:208  looks_like_idc || provider==BuilderId ──→ Err(ProfileArnUnavailable)
  │                  ▲▲▲ 对 IdC 不可达 ▲▲▲
  └─ profile.rs:217  bail!("no available Kiro profile ...")
```

### 死分支的证明

`profile.rs:208` 的判定条件：

| 条件 | 定义位置 | 是否蕴含 refresh_token 存在 |
| --- | --- | --- |
| `looks_like_idc(credentials)` | `profile.rs:54` | 要求 `client_id` + `client_secret` 同时存在；此类凭据依赖 `refresh_token` 刷新（`token_manager.rs:129-133` 据此推断 `auth_method="idc"`） |
| `provider == BuilderId` | `infer_provider`，`profile.rs:76-84` | 同一 `client_id` + `client_secret` 前提 |

两条都要求 IdC 形态的凭据，而 IdC 凭据必须带 `refresh_token` 才能通过
`validate_refresh_token`（`token_manager.rs:72`）完成任何刷新。因此命中 208 行的凭据
在 186 行必然已 `return`。该分支仅在「IdC 形态但完全无 refreshToken」的非工作凭据上可达。

### 强刷的分流去向

```
force_refresh_token_for (token_manager.rs:2210)
  └─ refresh_token (token_manager.rs:112)
       ├─ auth_method ∈ {idc, builder-id, iam}  → refresh_idc_token   (token_manager.rs:230)
       │     └─ POST https://oidc.{region}.amazonaws.com/token
       │          └─ IdcRefreshResponse.profile_arn ← 标准 OAuth2 端点，基本恒为 None
       └─ 否则                                   → refresh_social_token (token_manager.rs:147)
             └─ POST https://prod.{region}.auth.desktop.kiro.dev/refreshToken
                  └─ RefreshResponse.profile_arn ← Kiro 自有端点，可能返回真实 ARN
```

### 判定谓词必须等于分流条件（Bridge Plan 修正）

初版设计打算用 `looks_like_idc || provider == BuilderId` 做前移判定。
核对后发现它与「刷新走 OIDC」不等价，会引入回归：

| 凭据形态 | `looks_like_idc \|\| provider==BuilderId` | `refresh_token` 分流 | 判定是否正确 |
| --- | --- | --- | --- |
| `authMethod: "idc"` + clientId/Secret | true | OIDC | ✓ |
| `authMethod` 缺失 + clientId/Secret | true（`infer_provider`→BuilderId） | OIDC（`token_manager.rs:129-131` 同样推断） | ✓ |
| `provider: "BuilderId"` + `authMethod: "social"` | **true** | **Social** | ✗ 误软放行 |
| `authMethod: "social"`，无 clientId | false | Social | ✓ |

第三行是真回归：`infer_provider`（`profile.rs:67-73`）对显式 `provider` 原样返回，
但该凭据刷新走 Kiro 自有端点，`RefreshResponse.profile_arn` 可能有值。

**解法**：把 `refresh_token`（`token_manager.rs:128-143`）的分流条件提取为纯函数

```rust
// token_manager.rs
pub(crate) fn refresh_routes_to_idc(credentials: &KiroCredentials) -> bool {
    let auth = credentials.auth_method.as_deref().unwrap_or_else(|| {
        if credentials.client_id.is_some() && credentials.client_secret.is_some() {
            "idc"
        } else {
            "social"
        }
    });
    auth.eq_ignore_ascii_case("idc")
        || auth.eq_ignore_ascii_case("builder-id")
        || auth.eq_ignore_ascii_case("iam")
}
```

`refresh_token` 改为调用它做分流（结果逐位不变），`decide_profile_action` 复用同一谓词。
这样「被软放行的集合」与「强刷会走向 OIDC 的集合」在定义上恒等，而非依赖两处巧合一致。

### 测试覆盖现状

`profile.rs:378-499` 共 12 个测试，全部为同步 `#[test]` 纯函数（无 `#[tokio::test]`）：

| 测试 | 覆盖对象 |
| --- | --- |
| `test_fixed_builder_id` / `test_fixed_github_google` | `get_fixed_profile_arn` |
| `test_infer_provider_idc_defaults_builder` / `test_idc_defaults_to_builder_fixed_arn` | `infer_provider` |
| `test_supports_profiles_social` / `test_api_key_unsupported` | `supports_profiles` |
| `test_transient_classification` | `is_transient_profile_fetch_error` |
| `test_parse_list_available_profiles_body_*`（3 个） | `parse_list_available_profiles_body` |
| `test_cache_contract_nonempty_profile_arn` / `test_placeholder_arn_not_trusted` | `trusted_profile_arn` |

**`resolve_profile_arn` 的决策顺序零覆盖**。它内联了真实 HTTP 调用，
且需要 `&MultiTokenManager`，无法在不联网的条件下断言分支走向。

## 目标设计

### 决策与副作用分离

不注入「异步 list 依赖」，而是把决策提取为**纯函数 + 显式动作枚举**：

```rust
/// list 阶段的结果（不含 HTTP 细节）
enum ListOutcome {
    Resolved(String),   // 拿到可信 ARN
    Placeholder,        // 上游返回已知占位值
    Empty,              // 空列表
    Failed,             // 请求失败
}

/// 决策结果：调用方据此执行副作用
enum ResolveAction {
    Use(String),        // 使用该 ARN 并持久化
    Unsupported,        // 该凭据类型不支持 profile
    SoftUnavailable,    // 无 ARN 继续（IdC/BuilderId）
    ForceRefresh,       // 尝试强刷（Social 等）
    Fail,               // 按既有规则 bail
}

fn decide_profile_action(credentials: &KiroCredentials, list: ListOutcome) -> ResolveAction
```

`resolve_profile_arn` 保持现有签名，改为：读缓存 → 发 list → 归一为 `ListOutcome`
→ 调 `decide_profile_action` → 执行副作用。

**为何选此方案而非注入 async 依赖**：仓库无 `async-trait` 依赖，而原生
`async fn in trait` 不满足 dyn 兼容，注入需引入 `Box<dyn Fn(...) -> Pin<Box<dyn Future>>>`
一类样板。更关键的是，测试要断言的是「IdC 不得走到强刷」——直接断言
`decide_profile_action(...) == SoftUnavailable` 比「mock 一个 list 桩、再数
`force_refresh_token_for` 的调用次数」更直接，也更难写错。
这满足「抽出可注入边界」的目标：被测边界是决策函数，注入物是 `ListOutcome`。

### 调整后的决策表

`decide_profile_action` 的完整真值表（列为测试基准）：

| 凭据形态 | ListOutcome | 动作 | 与现状对比 |
| --- | --- | --- | --- |
| api_key | 任意 | `Unsupported` | 不变（提前于 list 判定，见下） |
| 不支持 profile 且 provider 未知 | 任意 | `Unsupported` | 不变 |
| 任意支持型 | `Resolved(arn)` | `Use(arn)` | 不变 |
| `refresh_routes_to_idc` == true | `Failed` | `SoftUnavailable` | **变更**：原为 `ForceRefresh` |
| `refresh_routes_to_idc` == true | `Empty` | `SoftUnavailable` | **变更**：原为 `ForceRefresh` |
| `refresh_routes_to_idc` == true | `Placeholder` | `SoftUnavailable` | **变更**：原为 `ForceRefresh` |
| 刷新走 Social 且有 refreshToken | `Failed` / `Empty` / `Placeholder` | `ForceRefresh` | 不变 |
| 刷新走 Social 且无 refreshToken | `Failed` / `Empty` / `Placeholder` | `Fail` | 不变 |

注：`refresh_routes_to_idc` == true 的凭据必然持有可用 `refreshToken`（否则无法工作），
故该维度不再需要单独的 refreshToken 分支；`provider: "BuilderId"` + `authMethod: "social"`
这类组合按分流实际去向落入「刷新走 Social」行，不被误软放行。

注：`Unsupported` 与缓存命中的判定仍在 list **之前**执行（沿用
`profile.rs:143-167` 的位置），不进入决策函数的 `ListOutcome` 维度。

### 数据流与影响面

```
provider.rs:393  (generate)             ─┐
provider.rs:178  (MCP)                  ─┤
token_manager.rs:1808 (余额)             ─┤
token_manager.rs:2469 (模型缓存)         ─┼─→ ensure_profile_arn_for_request ← 签名不变
admin/service.rs:734 (test)             ─┤        └─→ resolve_profile_arn    ← 签名不变
provider.rs:298  (MCP bearer-invalid)   ─┤              └─→ decide_profile_action ← 新增纯函数
provider.rs:556  (gen bearer-invalid)   ─┘
```

7 个调用点**零改动**。`ensure_profile_arn_for_request`（`profile.rs:360-375`）
已对 `ProfileArnUnavailable` 与 `ProfileArnUnsupported` 一并映射为 `Ok(None)`，
前移后 IdC 返回 `ProfileArnUnavailable` 的分支已被现有代码正确吸收，无需改动。

### 异常路径

| 情形 | 行为 |
| --- | --- |
| list 失败 + 刷新走 OIDC | `SoftUnavailable` → `Ok(None)` → 请求不带 ARN 继续（原路径也是此结果，只是多一次 OIDC 往返） |
| `provider: BuilderId` + `authMethod: social` | 按分流落入 Social 行 → `ForceRefresh`，与现状一致（不回归） |
| list 失败 + Social 强刷成功且返回 ARN | `Use(arn)`，与现状一致 |
| list 失败 + Social 强刷成功但无 ARN | `ProfileArnUnavailable`，与现状一致 |
| list 失败 + Social 强刷失败 | `bail!("no available Kiro profile (list: ...; refresh: ...)")`，错误文案保持 |
| 无 ARN 请求被上游 403 拒绝 | 由 `provider.rs:543` bearer-invalid 分支兜底：先重新 resolve，再强刷一次（每凭据每请求限一次，`force_refreshed` HashSet 去重）。本 change 不改该分支 |
| 某 IdC 变体的 refresh 确实返回 ARN | 前移后不再获取，退化为无 ARN 请求；若上游因此 403，由上一行兜底 |

### 回滚

改动限于两个文件：

- `src/kiro/profile.rs`：新增两个私有 enum、一个纯决策函数，重排
  `resolve_profile_arn` 内部分支顺序，新增测试
- `src/kiro/token_manager.rs`：提取 `refresh_routes_to_idc` 纯函数，
  `refresh_token` 改为调用它（分流结果不变）

无持久化格式变更、无 `CredentialEntry` 字段变更、无新增可变状态、无 schema 迁移。
`git revert` 单个 commit 即可完全恢复。

### 验证策略

| 层级 | 手段 |
| --- | --- |
| 决策顺序 | `decide_profile_action` 的真值表单元测试，覆盖上表 8 行，**不联网** |
| IdC 不强刷 | 断言 IdC × {Failed, Empty, Placeholder} 三种输入均得 `SoftUnavailable`（非 `ForceRefresh`） |
| Social 无回归 | 断言 Social × 同三种输入仍得 `ForceRefresh` |
| 既有语义 | `profile.rs` 原 12 个测试全部保持通过，不修改断言（`cargo test --bin kiro-rs kiro::profile`；本 crate 无 lib target） |
| 全量回归 | `cargo test`（AGENTS.md：Token/多凭据 → token_manager 相关测试） |
| 规格一致 | `openspec validate --all` |

**未覆盖项（剩余风险）**：真实上游对无 `profileArn` 的 IdC generate 请求是否 200,
本 change 不做联网复现——该前提由既有 `profile-arn-resolution` spec 的
「BuilderId 无可信缓存」场景确立，非本次引入。
