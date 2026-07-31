## 1. 实现前核对

- [x] 1.1 读 `AGENTS.md` 与本 change 的 proposal / design / specs
  → 验证：能陈述本次高风险类型（Token/多凭据、Admin/凭据 CRUD、配置 schema、admin-ui）
  与验证命令（`cargo test`、`pnpm --dir admin-ui build`、`openspec validate --all`）
- [x] 1.2 运行 `openspec-superpowers-bridge`，产出 `evidence/bridge-plan.md`
  → 验证：bridge plan 存在，且逐条核对过 design 中标注为「须停下确认」的三处
  （`effective_api_region` 语义变更、`persist_credentials` 原子化范围、
  Windows `fs::rename` 覆盖行为）
- [x] 1.3 核对 `canonicalize_auth_method_value`（`credentials.rs:136-144`）的既有调用点
  → 验证：能列出全部调用点，并确认新增 `parse_auth_method` 不改变任一既有调用点行为
- [x] 1.4 核对 `ingest_credential` 的字段 overlay 范围（`token_manager.rs:1945-1988`）
  → 验证：能指出三个新字段必须加入 overlay 的具体位置，否则 upsert 会丢字段

## 2. 认证类型规范化（纯函数，先做因为后续都依赖它）

- [x] 2.1 在 `src/kiro/model/credentials.rs` 新增
  `pub(crate) enum AuthMethod { Social, Idc, ExternalIdp, ApiKey }`
  → 验证：`cargo build` 通过
- [x] 2.2 新增 `parse_auth_method(&str) -> Result<AuthMethod, UnknownAuthMethod>`，
  实现 design 的别名表（大小写不敏感）
  → 验证：单测覆盖全部别名；`IdC`、`azure_ad`、`APIKEY` 等变体正确归一
- [x] 2.3 新增 `classify_auth_method(&KiroCredentials) -> Result<AuthMethod, _>`，
  实现 design 的 5 步判别优先级
  → 验证：单测断言 external 判定**先于** idc（一条同时有 clientId+clientSecret+
  合法 tokenEndpoint 但无 authMethod 的凭据 MUST 判为 external_idp）
- [x] 2.4 确认 `canonicalize_auth_method_value` **未被修改**
  → 验证：`git diff` 显示该函数体无变化；既有 `canonicalize_auth_method` 测试全绿

## 3. external endpoint 校验（纯函数，安全关键）

- [x] 3.0 `Cargo.toml` 的 `[dependencies]` 新增 `url = "2"`
  → 验证：`cargo build` 通过；`cargo tree -i url` 显示 kiro-rs 为直接依赖者。
  **背景**：`url` 当前只是 reqwest 的传递依赖（`Cargo.lock:1992`，`url 2.5.7`），
  依赖传递依赖会在 reqwest 升级时断（Bridge Plan 8.2）
- [x] 3.1 新建 `src/kiro/external_idp.rs`，定义 4 项白名单常量与 `EndpointRejected` 错误类型；
  在 `src/kiro/mod.rs` 加 `pub mod external_idp;`
  → 验证：`cargo build` 通过
- [x] 3.2 实现 `validate_token_endpoint(&str) -> Result<Url, EndpointRejected>`，
  按 design 的 6 步顺序（parse → https → userinfo → Host::Domain → localhost → 白名单）
  → 验证：合法用例（4 域 + 各自子域）全部通过
- [x] 3.3 补绕过测试：非 HTTPS、userinfo（`https://login.microsoftonline.com@evil.com`）、
  反斜杠混淆（`https://evil.com\.login.microsoftonline.com`）、
  IPv4（`https://169.254.169.254/token`）、IPv6、`localhost`、`x.localhost`、
  后缀伪装（`https://evil-login.microsoftonline.com.attacker.com`）
  → 验证：全部返回 `Err`；每个用例是独立的 `#[test]` 或表驱动条目，失败可定位
- [x] 3.4 实现 `derive_token_endpoint_from_issuer(&str) -> Result<Url, EndpointRejected>`，
  按 `{issuer}/oauth2/v2.0/token` 派生后**再次**调用 `validate_token_endpoint`
  → 验证：单测断言非白名单 issuer 派生出的 endpoint 同样被拒
- [x] 3.5 错误类型不得携带完整原始 URL 的 userinfo 部分
  → 验证：单测断言含 userinfo 的输入，其错误 `Display` 输出不含密码片段

## 4. 凭据模型扩展

- [x] 4.1 `KiroCredentials` 新增 `token_endpoint` / `issuer_url` / `scopes`
  （均 `Option<String>` + `skip_serializing_if` + camelCase）
  → 验证：`cargo build` 通过；`scopes` 为 `Option<String>` 而非 `Vec<String>`
- [x] 4.2 补 round-trip 测试：三字段序列化/反序列化不丢
  → 验证：新测试通过
- [x] 4.3 补旧文件兼容测试：缺三字段的 `credentials.json` 仍可加载
  → 验证：新测试通过；既有 `test_from_json*` 全绿
- [x] 4.4 确认 `effective_api_region` 与 `effective_auth_region` **均未被修改**
  → 验证：`git diff` 显示两函数体无变化；
  `token_manager.rs:3256-3273` 的 `test_api_call_uses_effective_api_region` 与
  `test_api_call_uses_credential_api_region` 继续通过。
  **背景**：早期草案计划改 api region 回退链，Bridge Plan 8.1 证伪后已撤销
  （既有测试注释、README:456-459、`config.rs:267-274` 三重反证两条链是有意设计）。
  若这两个测试失败，说明动了不该动的地方
- [x] 4.5 全量回归既有 credentials 测试
  → 验证：`cargo test kiro::model::credentials` 全绿

## 5. 刷新分派扩为四路

- [x] 5.1 在 `src/kiro/external_idp.rs` 实现 refresh 请求构造（纯函数，返回 form 键值对）
  → 验证：单测断言公共客户端（无 clientSecret）的 form **不含** `client_secret` 键；
  `scopes` 为空时 **不含** `scope` 键；`grant_type`/`client_id`/`refresh_token` 恒存在
- [x] 5.2 实现 `refresh_external_token`，form-urlencoded + `Accept: application/json`
  → 验证：`cargo build` 通过；响应解析覆盖 `expires_in` 与 `expires_at` 两种形态
- [x] 5.3 `refresh_token`（`token_manager.rs:133-153`）分派改四路
  → 验证：单测断言 external 凭据**即使同时带 clientId+clientSecret** 也选 external，
  不落 IdC；Social 与 IdC 的分派结果逐位不变
- [x] 5.4 external 响应含新 `refresh_token` 时轮换
  → 验证：单测断言轮换行为，与 IdC/Social 既有语义一致
- [x] 5.5 确认 Social 与 IdC 两条分支的端点、请求体、错误分类**未被修改**
  → 验证：`git diff` 显示 `refresh_social_token` / `refresh_idc_token` 函数体无变化
- [x] 5.6 记录已知遗留：external 仍会为取 profileArn 而强刷一次
  → 验证：新增测试断言当前行为（`refresh_routes_to_idc(external) == false`），
  测试名或注释标明这是记录遗留而非期望终态，并引用 design「未决与已知遗留」第 1 条

## 6. KAM adapter

- [x] 6.1 新建 `src/kiro/kam_adapter.rs`，实现容器判别（`Value` 驱动，不用 untagged）；
  在 `src/kiro/mod.rs` 加 `pub mod kam_adapter;`
  → 验证：单测覆盖 4 容器；wrapper 判定先于平铺单条；未知对象返回 `Err`
- [x] 6.2 `Err` 携带 JSON path 与顶层 key 列表
  → 验证：单测断言错误信息含可定位信息，且不含任何字段值（只含 key 名）
- [x] 6.3 实现单条 normalize，按 design 字段映射表
  → 验证：单测逐字段断言映射结果
- [x] 6.4 `label → nickname` 在**平铺与嵌套两条路径**都生效
  → 验证：单测分别用平铺与 `{credentials:{...}}` 嵌套输入，断言 nickname 均被设置
- [x] 6.5 `enabled → disabled` 取反映射，`enabled` 缺省视为 `true`
  → 验证：单测断言 `enabled: false → disabled: true`、
  `enabled: true → disabled: false`、字段缺失 → `disabled: false`
- [x] 6.6 `region` 只写 `region`（不写 `authRegion`），靠既有回退链派生
  → 验证：单测断言 `authRegion` 为 `None` 且 `effective_auth_region` 能取到 region；
  **同时断言 `effective_api_region` 仍取全局配置**（这是既有设计，见 4.4）
- [x] 6.7 显式丢弃 `password`、`usageData`、`groupId`、`tagLinks`、
  `availableModelsCache`、`failureCount`、`successCount`、`proxyConfig`
  → 验证：单测断言输出凭据不含这些数据；`password` 不出现在任何字段
- [x] 6.8 容忍全字段显式 `null`
  → 验证：用「除 refreshToken 外全为 `null`」的 fixture 测试；
  判别代码不得用裸 `contains_key` 判有值

## 7. 脱敏 fixtures

- [x] 7.1 在 `src/kiro/kam_adapter.rs` 的 `#[cfg(test)]` 或
  `openspec/changes/.../fixtures/` 下建完全虚构的 KAM fixtures
  → 验证：4 容器 × 4 登录格式（Social/BuilderId/Enterprise/external）覆盖齐；
  **无任何真实 token、真实租户 ID、真实邮箱**
- [x] 7.2 补「可选字段全为显式 null」样本与「external 公共客户端（无 clientSecret）」样本
  → 验证：两个样本均被 adapter 正确处理
- [x] 7.3 确认 fixtures 不含可被误认为真实凭据的值
  → 验证：所有 token 类字段为明显占位（如 `xxxx-fake-refresh-token`）；
  `git status --short` 不出现 `credentials.json` / `config.json`

## 8. Admin 契约与导入端点

- [x] 8.1 `AddCredentialRequest`（`src/admin/types.rs:142-216`）新增三字段
  → 验证：既有 `add_credential_request_accepts_identity_fields` 测试仍绿；
  新测试断言三字段可反序列化
- [x] 8.2 新增 `auth_method` 取值校验（当前是无校验裸 `String`）
  → 验证：单测断言未知值被拒并返回合法取值列表；`external_idp` 被接受；
  缺省仍为 `social`（`default_auth_method`，`:214-216`）
- [x] 8.3 按族实现必需字段校验（design 表格）
  → 验证：单测覆盖四族的必需/可选组合；external 公共客户端（无 clientSecret）**通过**
- [x] 8.4 `ingest_from_request`（`service.rs:354-380`）组装三个新字段
  → 验证：单测断言字段进入 `KiroCredentials`
- [x] 8.5 `ingest_credential` 的字段 overlay（`token_manager.rs:1945-1988`）加三字段
  → 验证：单测断言 upsert 同一账号后三字段不丢
- [x] 8.6 新增 `POST /api/admin/credentials/import/kam`，接收原始 `Value`，返回逐条结果
  → 验证：单测/集成测断言混合批次的逐条 created/updated/duplicate/failed；
  单条失败不影响其余
- [x] 8.7 确认 `/credentials/import/batch` 契约**未变**
  → 验证：既有 `batch_import_request_deserializes_items` 测试绿；
  `git diff` 显示该 handler 行为无变化
- [x] 8.8 Admin 快照只增加非敏感的「是否已配置」状态
  → 验证：单测断言响应不含 `tokenEndpoint` 之外的 secret；
  不含 `clientSecret`、`refreshToken`、`scopes` 明文（若判定 scopes 敏感）

## 9. 原子写入与启动加载

- [x] 9.1 实现原子写工具（临时文件 + `fs::rename`）
  → 验证：**在 Windows 上实际运行测试**断言覆盖已存在文件成功；
  不得假设 `fs::rename` 的覆盖行为
- [x] 9.2 `persist_credentials`（`token_manager.rs:1221-1231`）改用原子写
  → 验证：`cargo test kiro::token_manager` 全绿；
  **若牵连过多既有测试则停下确认**，不得单方面缩减范围
- [x] 9.3 `CredentialsConfig::load` 改为 `Value` 驱动：原生判别先行，再走 adapter
  → 验证：既有 `test_credentials_config_single` / `_multiple` /
  `_priority_sorting` 全绿
- [x] 9.4 补 wrapper-object 测试（当前缺口，缺陷二所在）
  → 验证：`{version, accounts:[...]}` 被识别为 KAM wrapper 并正确规范化，
  **不再**产生 `Single(default)`
- [x] 9.5 未知包装对象 fail fast
  → 验证：单测断言返回 `Err` 含 JSON path；**不再**打印「已加载 1 个凭据配置」
- [x] 9.6 显式未知 `authMethod` 在加载时报错并指出凭据 index
  → 验证：单测断言错误信息含 index 与合法取值列表
- [x] 9.7 实现迁移写回：备份 → 临时文件 → 原子替换
  → 验证：临时目录集成测断言备份文件存在、目标文件为原生数组格式、内容等价
- [x] 9.8 迁移失败不覆盖原文件、不阻止启动
  → 验证：注入写失败后断言原文件内容不变、进程以内存结果继续
- [x] 9.9 `main.rs` 加载路径适配
  → 验证：`cargo build` 通过；正常原生格式启动路径行为不变

## 10. 前端

- [x] 10.1 `admin-ui` 引入 vitest 与 `test` script（固定版本，不用 range），
  **并同步 `admin-ui/pnpm-lock.yaml`**
  → 验证：`pnpm --dir admin-ui install --frozen-lockfile` 通过（CI 用该 flag，
  见 `.github/workflows/build.yaml:107` 与 `build-dev-release.yaml:99`，
  lockfile 不同步则两个 workflow 都在 install 阶段失败，Bridge Plan 8.3）；
  `pnpm --dir admin-ui test` 可运行；`pnpm --dir admin-ui build` 不受影响
- [x] 10.2 `types/api.ts:75` 的 `authMethod` 联合类型加 `'external_idp'`
  → 验证：`tsc -b` 通过
- [x] 10.3 移除 `kam-import-dialog.tsx:240-242` 的类型重算与 `:243-251` 的硬失败
  → 验证：公共客户端 fixture 不再产生「idc 模式需要同时提供 clientId 和 clientSecret」
- [x] 10.4 改为把原始文档 POST 到 `/credentials/import/kam`，渲染服务端逐条结果
  → 验证：前端测试断言渲染逻辑消费服务端结果，不再本地判别类型
- [x] 10.5 预览由服务端预检结果驱动，不展示 token / clientSecret / password
  → 验证：前端测试断言预览渲染不含这些字段
- [x] 10.6 移除 `:139,:147,:179` 的静默 filter，改为逐条可见
  → 验证：部分记录无 refreshToken 时，UI 显示逐条失败而非仅 console.warn

## 11. profile 回归测试（不改逻辑）

- [x] 11.1 补 `supports_profiles` 的 external_idp 测试（`profile.rs:49` 分支当前无测试）
  → 验证：新测试通过
- [x] 11.2 补测试锁定「external 的真实 profileArn 不被占位逻辑清除」
  → 验证：新测试断言非占位 ARN 保留；占位 ARN 仍被清除
- [x] 11.3 确认 `profile.rs` 逻辑**未被修改**
  → 验证：`git diff src/kiro/profile.rs` 只显示 `#[cfg(test)]` 区域变化

## 12. 样例与文档

- [x] 12.1 修正 `credentials.example.idc.json` 的占位 `profileArn`
  → 验证：新值不等于 `BUILDER_ID_PROFILE_ARN`（`profile.rs:23-24`）
  且不等于 `SOCIAL_SIGN_IN_PROFILE_ARN`（`:20-21`）
- [x] 12.1a `.gitignore` 在 `/credentials.*` 之后加 `!/credentials.example.*.json` 例外
  → 验证：`git check-ignore -v credentials.example.external.json` 显示未被忽略；
  `git check-ignore -v credentials.json` 仍显示被忽略。
  **背景**：`/credentials.*` 会吞掉新增 example 文件（既有 4 个已跟踪故不受影响），
  不加例外则新文件静默不入库，文档会引用一个仓库里不存在的文件（Bridge Plan 8.4）
- [x] 12.2 新增 `credentials.example.external.json`（仅占位值）
  → 验证：含 `authMethod: "external_idp"`、`clientId`、`tokenEndpoint`、
  可选 `scopes`；无真实值；`git status --short` 显示该文件为未跟踪/已暂存
  而非被忽略
- [x] 12.3 README 的 `authMethod` 说明（`README:379`）从「`social` 或 `idc`」扩为四值
  → 验证：README 含 external_idp 说明与三个新字段
- [x] 12.4 README 补 KAM 支持范围、推荐导入入口、endpoint 安全限制
  → 验证：README 明确「直接换 credentials.json 属离线迁移，推荐 Admin 导入」；
  **`README.md:456-459` 的两条 region 优先级链保持原样不改**（见 4.4）
- [x] 12.5 确认 `config.example.json` 无需改动
  → 验证：本 change 未引入运行时配置项（proposal Non-Goals）；
  **若实现中新增了配置项则回到 proposal 补充说明后再改**
- [x] 12.6 确认 `AGENTS.md` 无需改动
  → 验证：不涉及 AI 纪律或验证命令变化

## 13. 全量验证

- [x] 13.1 `cargo test kiro::model::credentials`
  → 验证：全绿，含新增 round-trip / region / wrapper 测试
- [x] 13.2 `cargo test kiro::external_idp` 与 `cargo test kiro::kam_adapter`
  → 验证：全绿，含全部绕过用例
- [x] 13.3 `cargo test kiro::token_manager`
  → 验证：全绿，含四路分派与原子写
- [x] 13.4 `cargo test kiro::profile`
  → 验证：既有 12+ 测试全绿，新增 external 测试通过
- [x] 13.5 `cargo test admin`
  → 验证：全绿，含新 KAM 端点与 authMethod 校验
- [x] 13.6 `cargo test`（全量）
  → 验证：无失败；无 warning 回归
- [x] 13.7 `pnpm --dir admin-ui test` 与 `pnpm --dir admin-ui build`
  → 验证：两者均通过
- [x] 13.8 端到端等价性验证：同一脱敏 fixture 走 Admin 导入与启动加载
  → 验证：两条路径产出的规范化凭据字段逐一相等（本 change 的核心验收标准）
- [x] 13.9 `openspec validate --all`
  → 验证：通过
- [x] 13.10 `git status --short`
  → 验证：无 `config.json`、`credentials.json`、`credentials.*`（除 `*.example.*`）、
  `.codegraph/`、token、Cookie 进入候选

## 14. 交付门禁

- [x] 14.1 运行 `spec-compliance-check`，产出 `evidence/spec-compliance-report.md`
  → 验证：报告存在且逐条对应本 change 的 spec requirement
- [x] 14.2 运行 `openspec-verify-change`，产出 `evidence/openspec-verify-report.md`
  → 验证：报告存在，无未解决项
- [x] 14.3 运行 `verification-before-completion`，产出
  `evidence/verification-before-completion.md`
  → 验证：只记录本会话真实运行过的命令与结果；未运行项写明原因与剩余风险
- [x] 14.4 在最终报告中声明未做真实账号在线验活及其剩余风险
  → 验证：报告含该声明
