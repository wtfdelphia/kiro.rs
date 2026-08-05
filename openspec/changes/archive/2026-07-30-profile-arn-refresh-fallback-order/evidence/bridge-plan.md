# bridge-plan: profile-arn-refresh-fallback-order

执行前检查点。分支 `dev`。`openspec status --change profile-arn-refresh-fallback-order --json`
确认 state 非 blocked，四件工件（proposal / design / tasks / specs）均存在。
`openspec validate --all` → 20 passed, 0 failed。

## 范围

两个文件：

- `src/kiro/profile.rs`（主体）
- `src/kiro/token_manager.rs`（**仅**提取 `refresh_routes_to_idc` 纯函数，不改刷新行为）

三项内容：

1. **提取分流谓词**：把 `refresh_token`（`token_manager.rs:128-143`）的
   `auth_method` 分流条件提取为 `pub(crate) fn refresh_routes_to_idc`，纯重构
2. **前移账号类型判定**：把 `profile.rs:207-215` 的软放行判定移到 `profile.rs:186`
   强刷分支之前，谓词换用 `refresh_routes_to_idc`，消除确定无收益的 OIDC 往返
3. **抽出决策边界**：提取纯函数 `decide_profile_action(credentials, ListOutcome) -> ResolveAction`，
   使决策顺序可在离线条件下断言

范围从初版的单文件扩为两文件，原因见补盲 10（tasks 1.2 核对命中停止条件）。

## 非目标

- 不改 Social 凭据强刷行为（其 refresh 端点可能真返回 `profileArn`）
- 不引入负缓存 / 冷却计时器，不给 `CredentialEntry` 增加字段或可变状态
- 不改 `force_refresh_token_for` 语义（Admin 强制刷新入口依赖其无条件行为）
- 不改 `refresh_lock` 粒度、不改 `persist_credentials` 落盘策略
- 不改 `provider.rs` 的 bearer-invalid 错误恢复分支
- 不改固定占位 ARN 表与占位识别逻辑
- 不改任何调用点代码

## 关键设计决策

| 决策 | 依据 |
| --- | --- |
| 前移判定而非加负缓存 | IdC 这条路径收益恒为零，不该被「记住失败」，而该不被尝试；负缓存需回答落盘/冷却时长/失效三问，且要给 `CredentialEntry`（`token_manager.rs:405`）加可变状态 |
| **判定谓词提取为 `refresh_routes_to_idc`，不用 `looks_like_idc \|\| provider==BuilderId`** | 后者与「刷新走 OIDC」不等价，会误软放行 `provider:BuilderId` + `authMethod:social` 的凭据。见补盲 10 |
| 提取纯函数 + 动作枚举，而非注入 async list 依赖 | 仓库**无 `async-trait` 依赖**（已验证 `Cargo.toml`），原生 `async fn in trait` 不满足 dyn 兼容，注入需 `Box<dyn Fn -> Pin<Box<dyn Future>>>` 样板；直接断言 `decide_profile_action(...) == SoftUnavailable` 比 mock list 桩再数强刷次数更难写错 |
| 公开签名保持不变 | 7 处调用点（见补盲 1）零改动；`ensure_profile_arn_for_request` 已把 `ProfileArnUnavailable`/`Unsupported` 一并映射为 `Ok(None)`（`profile.rs:372-373`），新返回路径被现有代码正确吸收 |
| 只前移「刷新走 OIDC」的凭据，不动 Social | 提取谓词后，被软放行的集合与走 OIDC 端点的集合在**定义上恒等**，而非依赖两处巧合一致 |
| 假设不成立也不回归 | 若某 IdC 变体确实返回 ARN，前移后退化为无 ARN 请求，与现状 `profile.rs:196` 结果相同；真出 403 由 `provider.rs:543` 兜底 |

## 高风险项

| 风险 | 说明 | 处置 |
| --- | --- | --- |
| 触及 7 处调用点的热路径 | `resolve_profile_arn` 经 `ensure_profile_arn_for_request` 服务于对话、MCP、余额、模型缓存、Admin test | 公开签名不变，依赖注入只作用于内部；`cargo test` 全量回归；tasks 5.5 用 `git diff --name-only` 守边界 |
| 「OIDC 不返回 profileArn」是假设非已验证事实 | 未联网复现 AWS SSO OIDC 响应体 | 风险已被吸收：假设不成立时行为退化为与现状一致的无 ARN 请求，不构成回归；`IdcRefreshResponse.profile_arn` 标 `#[serde(default)]` 属防御性可选 |
| Social 路径回归 | 前移判定若误将 Social 纳入软放行，会丢失其唯一取 ARN 途径 | tasks 4.4 单列 Social × 三种 list 结果仍为 `ForceRefresh` 的断言；tasks 3.3 逐条对照改前代码核验副作用 |
| 测试替身与真实上游语义漂移 | 断言的是决策顺序而非真实 HTTP 语义 | `list_available_profiles` 的重试与错误分类仍由既有纯函数测试覆盖（`profile.rs:431` `test_transient_classification`），不因重构失去覆盖 |
| 无 ARN 请求被上游拒绝 | 若上游对 IdC 无 ARN generate 返回 403 | `provider.rs:543` bearer-invalid 分支兜底：先重新 resolve 再强刷一次（`force_refreshed` HashSet 每凭据每请求限一次）。本 change 不改该分支 |
| CI 不跑测试 | `.github/workflows/` 三个 workflow 均**无 `cargo test`** 步骤（已验证） | 回归完全依赖本地 `cargo test`，须在最终报告粘贴真实输出 |

## CodeGraph 证据

索引状态：`codegraph status` → 128 文件 / 2,465 节点 / 6,557 边，backend `node:sqlite`。

| 命令 | 结论 |
| --- | --- |
| `codegraph callers resolve_profile_arn` | 唯一调用方 `ensure_profile_arn_for_request`（`profile.rs:339`）。确认它是模块内私有入口，外部一律经 `ensure_*` |
| `codegraph impact resolve_profile_arn` | 3 个受影响符号，全部在 `src/kiro/profile.rs` 内。影响面闭合在单文件 |
| `codegraph callers looks_like_idc` | 2 个：`supports_profiles`（`profile.rs:40`）、`resolve_profile_arn`（`profile.rs:137`）。前移判定复用它不会外溢 |
| `codegraph callers trusted_profile_arn` | 2 个，均在 `profile.rs` 内 |
| `codegraph callers infer_provider` | CodeGraph 报 2 个（均在 `profile.rs`），**漏报** `token_manager.rs:1955`。见补盲 2 |
| `codegraph callers ensure_profile_arn_for_request` | **报「No callers found」，与事实矛盾**（rg 实测 7 处）。CodeGraph 不解析 `crate::kiro::profile::` 全路径限定调用与 `profile::` 模块前缀调用，此为覆盖限制 |
| `codegraph callers force_refresh_token_for` | 仅报 `admin/service.rs:574`，**漏报** `profile.rs:187`、`provider.rs:313`、`provider.rs:571` 三处方法调用 |

**结论：本 change 的调用面判定不能依赖 CodeGraph**，方法调用（`self.token_manager.x()`）
与全路径限定调用均被漏报。以下 rg 补盲为准。

## rg / 源码补盲

**1. `ensure_profile_arn_for_request` 实为 7 处调用点（更正 proposal 的「5 个」）**

```
rg -n "ensure_profile_arn_for_request" -g '*.rs' src/
→ src/kiro/profile.rs:339         (定义)
  src/kiro/provider.rs:178        主链路：MCP / WebSearch
  src/kiro/provider.rs:298        bearer-invalid 恢复分支（MCP）
  src/kiro/provider.rs:393        主链路：generateAssistantResponse
  src/kiro/provider.rs:556        bearer-invalid 恢复分支（generate）
  src/kiro/token_manager.rs:1808  主链路：查余额前
  src/kiro/token_manager.rs:2469  主链路：刷新模型缓存前
  src/admin/service.rs:734        主链路：Admin 凭据连通性测试
```

proposal「Impact」列的 5 个是**主链路**调用点；另有 2 处在 bearer-invalid
恢复分支内（`provider.rs:298`、`:556`）。实现时以 7 处为准。

对本 change 的影响：`provider.rs:298`/`:556` 调用 `ensure_*` 后取
`.ok().flatten().is_some()` 判断，前移后 IdC 在此返回 `Ok(None)` → `is_some()` 为 false
→ 落到紧随其后的强刷（`provider.rs:313`/`:571`）。**该路径行为不变**，
因为原本 IdC 在此也拿不到 ARN。且那处强刷由真实 403 触发、每凭据每请求限一次，属正当恢复。

**2. `infer_provider` 是跨模块公开 API（CodeGraph 漏报）**

```
rg -n "profile::" -g '*.rs' src/ | grep infer_provider
→ src/kiro/token_manager.rs:1955
     validated_cred.provider = new_cred.provider.clone()
         .or_else(|| crate::kiro::profile::infer_provider(&validated_cred));
```

`infer_provider` 被 `token_manager` 在凭据更新路径调用。**本 change 不改其行为**，
仅在 `decide_profile_action` 内复用其返回值。tasks 2.3 需确认未修改该函数。

**3. 错误类型消费点确认（前移安全性的关键）**

```
rg -n "ProfileArnUnavailable|ProfileArnUnsupported" -g '*.rs' src/
→ profile.rs:29/31/37    ProfileArnUnsupported 定义
  profile.rs:123/125/131 ProfileArnUnavailable 定义
  profile.rs:158,166     返回 Unsupported
  profile.rs:196,214     返回 Unavailable  ← 196 是强刷后，214 是死分支
  profile.rs:372,373     ensure_* 内两者均 → Ok(None)
```

**两个错误类型没有任何模块外消费者**。前移后 IdC 从 `:196` 路径改走 `:214` 语义，
对外表现完全一致（都是 `Ok(None)`）。这是本 change 能不改调用点的根本原因。

**4. example 凭据中 IdC 样本恰好携带占位 ARN（典型触发样本）**

```
credentials.example.idc.json:
  "authMethod": "idc", "clientId": ..., "clientSecret": ...,
  "provider": "BuilderId",
  "profileArn": "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX"
```

该 ARN 正是 `profile.rs:25` 的 `BUILDER_ID_PROFILE_ARN` 占位值。按
`trusted_profile_arn`（`profile.rs:112`）它不可信 → `resolve_profile_arn` 会
`clear_profile_arn` 后走 list → 失败后（现状）强刷。**这个 example 就是本缺陷的
标准复现配置**，可作为测试用例的凭据形态蓝本。

`credentials.example.social.json` 无 `clientId`/`clientSecret`，`authMethod: social`
→ `looks_like_idc` 为 false → 前移后仍走强刷，符合预期。

本 change 不需要修改任何 example 文件（字段无增减）。

**5. 测试基线：12 个，非 13 个（更正 proposal/design 的计数）**

```
cargo test --bin kiro-rs kiro::profile
→ running 12 tests ... test result: ok. 12 passed; 0 failed; 551 filtered out
```

proposal 与 design 写「13 个测试」有误，实际 12 个（`rg -c` 把 `#[test]` 与
`fn test_` 重复计数）。实现时以 12 为基线。

**6. 本 crate 无 lib target，测试命令须用 `--bin`**

```
cargo test --lib kiro::profile
→ error: no library targets found in package `kiro-rs`
```

正确命令：`cargo test --bin kiro-rs kiro::profile`。
tasks 5.1 写的 `cargo test --lib kiro::profile` **不可用**，以本条为准。

**7. 无 `async-trait` 依赖，dev-dependencies 仅 `tower`**

```
grep -n "async-trait\|async_trait" Cargo.toml src/kiro/*.rs  → 无命中
[dev-dependencies] tower = { version = "0.5.2", features = ["util"] }
```

`edition = "2024"`。这印证了 design 选择「纯函数 + 枚举」而非 trait 注入的判断。
`profile.rs` 现有测试**全部为同步 `#[test]`，无 `#[tokio::test]`**，
新增测试沿用同步写法即可，不需 async runtime。

**8. CI 不跑测试（验证纪律相关）**

```
grep -rn "cargo test" .github/workflows/  → 无命中
```

`build.yaml`、`build-dev-release.yaml`、`docker-build.yaml` 均只构建不测试。
回归完全依赖本地执行，最终报告须粘贴真实 `cargo test` 输出。

**10. `looks_like_idc || provider==BuilderId` 与「刷新走 OIDC」不等价（本次核对的关键发现）**

执行 tasks 1.2 时命中停止条件。`refresh_token` 的实际分流（`token_manager.rs:128-143`）：

```rust
let auth_method = credentials.auth_method.as_deref().unwrap_or_else(|| {
    if client_id.is_some() && client_secret.is_some() { "idc" } else { "social" }
});
if auth ∈ {idc, builder-id, iam} { refresh_idc_token } else { refresh_social_token }
```

对比 `profile.rs:208` 原谓词：

| 凭据形态 | `looks_like_idc \|\| provider==BuilderId` | 实际分流 | 判定 |
| --- | --- | --- | --- |
| `authMethod: idc` + clientId/Secret | true | OIDC | ✓ |
| `authMethod` 缺失 + clientId/Secret | true | OIDC | ✓ |
| `provider: BuilderId` + `authMethod: social` | **true** | **Social** | **✗ 会误软放行** |
| `authMethod: social`，无 clientId | false | Social | ✓ |

第三行是真回归：`infer_provider`（`profile.rs:67-73`）对显式 `provider` 原样返回，
而该凭据刷新走 Kiro 自有端点，`RefreshResponse.profile_arn`（`token_refresh.rs:18`）
可能真有值。该组合经 Admin/KAM 导入原样接收 `provider`（`token_manager.rs:1952-1955`）
是可表达的配置。

**处置**：把分流条件提取为 `pub(crate) fn refresh_routes_to_idc`（token_manager.rs），
`refresh_token` 与 `decide_profile_action` 共用。提取为纯重构，分流结果逐位不变。
范围因此从单文件扩为两文件，proposal Impact / design / tasks 已同步更新。

**注意 spec delta 无需修改**：其条件表述本就是「该账号类型的 token 刷新走 AWS SSO
OIDC token 端点」，规格层面正确，是初版实现谓词偏窄。

**9. 工作区敏感文件状态**

```
config.json      → 不存在
credentials.json → 不存在
.gitignore: /config.json(:2) /credentials.json(:3) /credentials.*(:9) .codegraph/(:14)
```

`git status --short` 现有改动为上一个 change（`src/openai/*`、`admin-ui/*`）的未提交内容，
与本 change 无交集。

## 任务到执行步骤映射

| 任务 | 执行步骤 | 如何验证 | 何时停止 |
| --- | --- | --- | --- |
| 1.1 读工件 | 读 AGENTS.md + 本 change 四件工件 | 能陈述风险类型（Token/多凭据）与验证命令 | — |
| 1.2 口径一致性 | **已执行**：对照 `profile.rs:54-59` 与 `token_manager.rs:128-143` | **结论：不等价**，已命中停止条件并按补盲 10 处置（提取谓词、扩范围、更新工件） | 已处置 |
| 1.3 死分支确认 | **已执行**：命中 `:208` 需 clientId+clientSecret → 必持 refreshToken（`token_manager.rs:72`）→ 必先在 `:186` return | 蕴含链成立，缺陷定性正确 | 已处置 |
| 2.0 提取谓词 | `token_manager.rs` 新增 `refresh_routes_to_idc`，`refresh_token` 改为调用 | `cargo test` 通过；`refresh_token` 内不再内联推断 | 若分流结果有任何变化 → 停，提取应为纯重构 |
| 2.1 `ListOutcome` | 新增私有枚举，4 个变体 | `cargo build` 通过 | — |
| 2.2 `ResolveAction` | 新增私有枚举，5 个变体 | `cargo build` 通过 | — |
| 2.3 决策函数 | 按 design 真值表 8 行实现 `decide_profile_action` | 函数体无 `.await`、不接 `&MultiTokenManager`；**未修改 `infer_provider`**（补盲 2） | 若必须传入 token_manager → 停，纯函数目标失败 |
| 2.4 签名不变 | 只读核对 | `rg -n "fn resolve_profile_arn\|fn ensure_profile_arn_for_request" src/` 与改前一致 | 签名变化 → 停，7 处调用点将受影响 |
| 3.1 重排主流程 | 缓存/Unsupported 前置判定留在 `:143-167`；list → `ListOutcome` → 决策 → 副作用 | `cargo build` 通过；无「先强刷后判类型」顺序 | — |
| 3.2 删死分支 | 删原 `:207-215` | 文件内无重复 `looks_like_idc \|\| provider==BuilderId` 判定 | — |
| 3.3 Social 副作用保真 | 逐条对照改前 `:186-204` | `force_refresh_token_for` → `profile_arn_of` → 占位则 `clear` → 失败则 bail 且文案含 list+refresh 双原因 | 任一条行为不一致 → 停，Social 回归 |
| 3.4 `set_profile_arn` 位置 | 仅 `Use(arn)` 分支调用，保留 `provider.clone()` | 其他分支无持久化调用 | — |
| 3.5 清理孤儿 | 仅清理本次改动造成的 | `cargo build` 无 unused 警告增量 | 若需删既有 dead code → 停，属范围外 |
| 4.1-4.3 IdC 三态 | IdC × {Failed, Empty, Placeholder} → `SoftUnavailable` | `cargo test --bin kiro-rs decide_profile_action` 通过且断言非 `ForceRefresh` | — |
| 4.4 Social 无回归 | Social × 同三态 → `ForceRefresh` | 三条断言均通过 | 任一为 `SoftUnavailable` → 停，前移误伤 Social |
| 4.4b 混合形态不回归 | `provider:BuilderId` + `authMethod:social` → `ForceRefresh` | 断言通过（补盲 10 的回归已规避） | 若为 `SoftUnavailable` → 停，谓词提取未生效 |
| 4.4c 缺 authMethod 被覆盖 | `authMethod` 缺失 + clientId/Secret → `SoftUnavailable` | 断言通过（缺陷修得干净） | 若为 `ForceRefresh` → 停，谓词漏覆盖 |
| 4.5 Resolved 路径 | 支持型 × `Resolved(arn)` → `Use(arn)` | 返回 ARN 与输入一致 | — |
| 4.6 边界态 | api_key → `Unsupported`；非 IdC 无 refreshToken → `Fail` | 两条断言通过 | — |
| 4.7 离线性 | 测试体检查 | 无 HTTP client 构造、无 `MultiTokenManager`；凭据用 `KiroCredentials::default()`；沿用同步 `#[test]`（补盲 7） | 若测试需 async runtime → 停，说明未真正解耦 |
| 5.1 原测试回归 | **`cargo test --bin kiro-rs kiro::profile`**（非 `--lib`，见补盲 6） | 原 **12** 个（非 13，见补盲 5）全绿；`git diff` 中原测试块无改动 | 原测试需改断言 → 停，属行为回归 |
| 5.2 全量测试 | `cargo test` | 粘贴真实输出；CI 不跑测试（补盲 8），本地是唯一门禁 | 失败即停，不得声称通过 |
| 5.3 构建 | `cargo build` | 无新增警告 | — |
| 5.4 规格校验 | `openspec validate --all` | 全通过 | — |
| 5.5 改动面 | `git diff --name-only` | 仅 `src/kiro/profile.rs`、`src/kiro/token_manager.rs` + 本 change 目录；后者 diff 仅为谓词提取 | 出现 `provider.rs`/`service.rs`，或 `token_manager.rs` 含刷新行为改动 → 停 |
| 5.6 敏感文件 | `git status --short` | 无 `config.json`/`credentials.json`/`.codegraph/` | 出现即停 |
| 5.7 同步判断 | 见下节 | 报告说明无需同步的理由 | — |
| 6.1-6.2 遗留记录 | 写入最终报告 | 含未联网复现项与 Social 独立 change 项 | — |

## 必跑验证

| 命令 | 归属 | 必须 |
| --- | --- | --- |
| `cargo test --bin kiro-rs kiro::profile` | 本 change 直接影响面；确认 12 个基线 + 新增测试 | 是 |
| `cargo test` | AGENTS.md 高风险矩阵 Token/多凭据项 | 是 |
| `cargo build` | 确认无新增警告 | 是 |
| `openspec validate --all` | AGENTS.md 高风险矩阵 OpenSpec 项 | 是 |
| `git status --short` / `git diff --name-only` | AGENTS.md 验证纪律 | 是 |
| `cd admin-ui && pnpm build` | 前端未改动 | 否（需在报告说明原因） |

**无法自动化的项**（须在最终报告标注未执行）：

- 真实上游对无 `profileArn` 的 IdC generate 是否返回 200
- 真实 AWS SSO OIDC 响应体是否含 `profileArn`
- 真实环境下「连续请求不再出现 `Token 已强制刷新` 日志」

前两项属本 change 的假设前提（proposal Assumptions 已声明风险被吸收）；
第三项需真实凭据，按 AGENTS.md「禁用真实密钥入库」不在本地复现。

## README / AGENTS / spec 同步判断

| 目标 | 是否需要同步 | 理由 |
| --- | --- | --- |
| `README.md` | 否 | 不影响启动、构建、部署、测试或 API 入口；无新增命令、配置项、端点 |
| `AGENTS.md` | 否 | 不改 AI 纪律与验证命令；Token/多凭据已在高风险矩阵中 |
| `spec/design.md` | 否 | 模块边界与关键数据流不变；`src/kiro/` 职责描述仍准确 |
| `spec/requirements.md` | 否 | 无新增长期需求；profileArn 解析语义由 openspec spec 承载 |
| `spec/structure.md` | 否 | 无新增文件或模块 |
| `openspec/specs/profile-arn-resolution/` | 归档时同步 | 本 change 含 MODIFIED delta（强刷顺序约束）与 ADDED delta（决策可离线验证） |
| `credentials.example.*.json` | 否 | 字段无增减（补盲 4） |
| `docs/tooling-sources.md` | 否 | 无新增工具来源 |

按 AGENTS.md「单次变更过程只写在 `openspec/changes/<name>/`」，实现期间不动
`openspec/specs/`，由 openspec-archive-change 在归档时落地。

## 停止条件

- 工件缺失、互相矛盾或状态变为 blocked。
- ~~`looks_like_idc` 与 `refresh_token` 分流口径不一致~~ —— **已在 tasks 1.2 命中并处置**：
  提取 `refresh_routes_to_idc` 使两者定义上恒等，范围扩为两文件。
- `refresh_routes_to_idc` 提取后 `cargo test` 出现任何失败 —— 提取应为纯重构，
  有失败说明分流结果被改变。
- **`profile.rs:207-215` 对 IdC 实际可达** —— 缺陷定性错误，回 proposal 重写 Why。
- **Social 路径出现任何行为变化** —— 越过 Non-Goals 边界。
- 决策函数无法做成纯函数（必须持有 `&MultiTokenManager` 或 `.await`）
  —— 可测试性目标失败，需重新设计边界。
- 需要修改 7 处调用点中任何一处 —— 说明公开签名未能保持，范围超出预期。
- `token_manager.rs` 的改动超出「提取 `refresh_routes_to_idc`」—— 越过新范围边界。
- 需要给 `CredentialEntry` 增加字段或引入负缓存 —— 越过 Non-Goals。
- 原有 12 个测试需要修改断言才能通过 —— 属行为回归，非重构。
- `cargo test` 失败且原因无法从本 change 改动中定位。
- 工作区出现真实 `config.json`、`credentials.*`、token、Cookie 或 `.codegraph/` 待提交。
  当前状态：两者均不存在；`.gitignore` 已覆盖（`:2`、`:3`、`:9`、`:14`）。
- 无法确定某项验证命令或剩余风险。

## 待更正项（实现时以本文件为准）

| 工件 | 原文 | 更正 |
| --- | --- | --- |
| proposal.md「触发面」/「Impact」 | `ensure_profile_arn_for_request` 有 5 个调用点 | 实为 **7 处**：5 个主链路 + `provider.rs:298`/`:556` 两处 bearer-invalid 恢复分支。后两者行为不变（补盲 1） |
| proposal.md / design.md | `profile.rs` 原有 **13** 个测试 | 实为 **12** 个（补盲 5） |
| tasks.md 5.1 | `cargo test --lib kiro::profile` | 本 crate 无 lib target，须用 `cargo test --bin kiro-rs kiro::profile`（补盲 6） |
| design.md「验证策略」 | 「原 13 个测试」 | 同上，12 个 |
| proposal.md「范围」/ design.md「回滚」 | 改动限于 `profile.rs` 单文件 | 扩为两文件，增 `token_manager.rs` 谓词提取（补盲 10） |
| design.md 决策表 | 账号维度写 `IdC / BuilderId` | 改为 `refresh_routes_to_idc == true`（补盲 10） |

上述 1、5、6 三项已回写至 proposal / design / tasks；补盲 10 的范围变更亦已同步。
本文件与工件现已一致。
