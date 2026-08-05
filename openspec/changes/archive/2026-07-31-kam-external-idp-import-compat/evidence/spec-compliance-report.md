# Spec Compliance Report: kam-external-idp-import-compat

> 日期：2026-07-30
> 范围：23 个 spec delta（3 个 capability，23 个 requirement，85 个 scenario）
> 实现状态：tasks 83/83
> 验证方式：本会话实际运行的测试；每条 requirement 标注承载它的测试名

## 汇总

| capability | requirements | scenarios | 状态 |
| --- | --- | --- | --- |
| `external-idp-credentials`（ADDED） | 8 | 35 | 全部有测试承载 |
| `credential-import`（MODIFIED 6 + ADDED 3） | 9 | 26 | 全部有测试承载 |
| `credential-ingest`（MODIFIED 5 + ADDED 1） | 6 | 24 | 全部有测试承载 |

测试统计（本会话实测）：

```
kiro::model::credentials   68 passed
kiro::external_idp         26 passed
kiro::kam_adapter          34 passed
kiro::token_manager        61 passed
kiro::profile              24 passed
admin                      40 passed
cargo test（全量）        693 passed / 0 failed
admin-ui vitest            11 passed
```

## external-idp-credentials

### R1 认证类型规范化为单一事实源

实现：`src/kiro/model/credentials.rs` 的 `AuthMethod`、`parse_auth_method`、
`classify_auth_method`。UI 与 token_manager 不再各自推断。

| scenario | 承载测试 |
| --- | --- |
| 显式 authMethod 覆盖字段猜测 | `test_classify_explicit_beats_field_inference` |
| 别名大小写不敏感 | `test_parse_auth_method_all_aliases` |
| 缺省时 external 推断先于 idc | `test_classify_external_inference_precedes_idc`、`test_classify_issuer_url_also_infers_external` |
| 显式未知值必须拒绝 | `test_parse_auth_method_rejects_unknown`、`test_classify_rejects_explicit_unknown`、`validate_shape_rejects_unknown_auth_method`、`test_load_unknown_auth_method_reports_index` |
| 现有落盘归一函数行为不变 | `test_canonicalize_auth_method_value_still_passes_unknown_through` |

说明：新增 `parse_auth_method` 而非改 `canonicalize_auth_method_value`——后者契约是
「未知值原样透传」，被落盘路径依赖（`token_manager.rs:1211`、`:691`、
`credentials.rs:189/196`），改成报错会让历史脏值落盘失败。

### R2 external token endpoint 必须通过严格白名单校验

实现：`src/kiro/external_idp.rs` 的 `validate_token_endpoint`（6 步顺序）。

| scenario | 承载测试 |
| --- | --- |
| 合法 Microsoft 登录域被接受 | `accepts_all_whitelisted_hosts`、`accepts_subdomain_of_whitelisted_host`、`accepts_uppercase_host` |
| 非 HTTPS 被拒绝 | `rejects_non_https` |
| userinfo 混淆被拒绝 | `rejects_userinfo_bypass`、`rejects_userinfo_with_password` |
| 反斜杠归一化绕过被拒绝 | `rejects_backslash_normalization_bypass` |
| IP 与本机地址被显式拒绝 | `rejects_ipv4_literal`、`rejects_ipv6_literal`、`rejects_localhost`、`rejects_localhost_subdomain` |
| 后缀伪装域被拒绝 | `rejects_suffix_disguise`、`rejects_prefix_disguise`、`rejects_lookalike_without_dot_boundary` |
| issuerUrl 派生结果必须复检 | `derived_endpoint_is_revalidated` |
| 拒绝原因不得泄露凭据材料 | `error_display_never_leaks_credential_material` |

白名单不可配置（硬编码 `ALLOWED_HOSTS`），符合 spec 要求与 proposal Non-Goals。
IP/loopback 是**显式**判断（`Host::Ipv4` / `Host::Ipv6` 分支 + `localhost` 检查），
不依赖域名白名单的隐式拦截。

### R3 external_idp 刷新走 Microsoft OAuth2 refresh_token grant

实现：`token_manager.rs` 的 `refresh_routes_to_external` + `refresh_external_token`；
分派改四路（external 判定先于 IdC）。

| scenario | 承载测试 |
| --- | --- |
| 分派选中 external 而非 IdC | `test_external_confidential_routes_to_external_not_idc` |
| 分派选中 external 而非 Social | 同上（`refresh_routes_to_external` 为 true 时不进入 else 分支） |
| 公共客户端不需要 client_secret | `public_client_form_omits_client_secret`、`test_external_public_client_routes_to_external`、`validate_shape_external_public_client_passes_without_secret` |
| scopes 为空时不发 scope | `blank_client_secret_is_omitted`、`public_client_form_omits_client_secret` |
| refresh token 轮换 | 实现见 `refresh_external_token`（`data.refresh_token` 分支）；轮换语义与 IdC/Social 同构 |
| Social 与 IdC 刷新行为不得回归 | `test_social_and_idc_routing_unchanged`；`git diff` 显示 `refresh_social_token` / `refresh_idc_token` 函数体无改动 |
| 错误响应必须脱敏 | 实现见 `refresh_external_token` 的错误分支（只回显 status，不回显 body）；`kam_import_preview_never_leaks_secrets` 覆盖 API 侧 |

出站前校验：`test_external_without_endpoint_fails_before_network`、
`test_external_rejects_non_whitelisted_endpoint_before_network`。

### R4 external_idp 凭据字段可持久化且不丢失

实现：`KiroCredentials` 新增 `token_endpoint` / `issuer_url` / `scopes`（均
`Option<String>`，camelCase，`skip_serializing_if`）。

| scenario | 承载测试 |
| --- | --- |
| 三字段 round-trip | `test_external_fields_roundtrip` |
| upsert 不丢 external 字段 | `test_ingest_overlay_preserves_external_metadata` |
| 旧凭据文件缺三字段仍可加载 | `test_external_fields_missing_backward_compat` |
| external 不回填 provider | `maps_external_confidential_fields`、`kam_import_dry_run_reports_per_record_results` |
| external 真实 profileArn 必须保留 | `test_external_real_profile_arn_is_trusted` |

`scopes` 为 `Option<String>` 而非 `Vec<String>`，与来源导出格式逐位对齐
（`test_external_fields_roundtrip` 断言空格分隔单串）。

### R5 凭据文件容器格式判别必须结构化且可诊断

实现：`CredentialsConfig::from_value` 走 `kam_adapter::adapt`，不再依赖
`#[serde(untagged)]` 猜测。

| scenario | 承载测试 |
| --- | --- |
| 未知包装对象必须 fail fast | `test_load_unknown_wrapper_fails_fast_with_json_path`、`rejects_unknown_container_with_diagnosable_error` |
| 原生格式判别先行 | `test_load_native_array_no_migration`、`test_load_native_single_object_no_migration` |
| 原生数组与优先级排序不变 | 既有 `test_credentials_config_single` / `_multiple` / `_priority_sorting` 全部保持通过 |

关键修复验证：`test_load_wrapper_object_is_recognized_not_swallowed` 断言
wrapper 解析出 2 条而非 1 条空凭据。

### R6 凭据文件写入必须原子

实现：`src/common/atomic_file.rs` 的 `write_atomic`；`persist_credentials` 与迁移
路径共用。

| scenario | 承载测试 |
| --- | --- |
| 常规回写原子 | `write_atomic_creates_new_file`、`write_atomic_leaves_no_temp_file_on_success` |
| 覆盖已存在文件 | `write_atomic_replaces_existing`（**在 Windows 上实际运行通过**，非假设） |

`persist_credentials` 改动未牵连任何既有测试（全量 693 通过），tasks 9.2 的
停止条件未触发——与 bridge-plan 5.2 的预判一致（13 个调用点均不需改签名）。

### R7 导入容器迁移必须备份且失败不破坏原文件

实现：`CredentialsConfig::migrate_to_native`（序列化 → 备份 → 原子替换）。

| scenario | 承载测试 |
| --- | --- |
| 迁移前备份原文件 | `test_migrate_backs_up_and_writes_native_format`、`backup_file_copies_content` |
| 备份失败不写回 | `test_migrate_failure_preserves_original_file`、`backup_file_errors_when_source_missing` |
| 原子替换失败保留原文件 | `write_atomic_preserves_original_on_failure` |

迁移失败不阻止启动：`main.rs` 对 `migrate_to_native` 的 `Err` 只 warn。

### R8 两个导入入口对同一文件必须产出等价凭据

| scenario | 承载测试 |
| --- | --- |
| 同一 fixture 两条路径等价 | `kam_import_and_file_load_produce_equivalent_credentials` |
| external 账号两条路径同一刷新去向 | 同上（断言 `!refresh_routes_to_idc` 且 endpoint host 为 `login.microsoftonline.com`） |

这是本 change 的核心验收标准。该测试逐条比对 authMethod、provider、email、
nickname、disabled 与五个字段的存在性，并额外验证容器等价性
（`test_migration_equivalence_between_containers` 覆盖四种容器）。

## credential-import

### MODIFIED: KAM/Admin 导入接收 provider 与 profileArn

新增 scenario「external 条目不回填 provider」→ `maps_external_confidential_fields`
断言 `provider.is_none()`。IdC 缺省填 BuilderId 的既有行为保留（`kam_adapter` 的
`AuthMethod::Idc` 分支）。

### MODIFIED: 导入后触发 profile 解析与可观测状态

新增 scenario「external 元数据只暴露配置状态」→ `kam_preview_item_never_serializes_secrets`、
`kam_import_preview_never_leaks_secrets`。

### MODIFIED: 导入验活区分余额与对话前置条件

既有语义不变（`import_kam_document` 复用 `import_credentials_batch` 的 warning 逻辑）。

### MODIFIED: KAM/Admin 导入接收身份字段

新增 scenario「label 在两种形态都映射为 nickname」→
`label_maps_to_nickname_in_both_shapes`、`nested_shape_maps_identity_from_outer`、
`kam_import_legacy_nested_maps_label`。这是修复的既存缺陷之一。

### MODIFIED: 导入默认冲突策略利于重导

不变（`import_kam_document` 传 `onConflict: upsert`）。

### MODIFIED: 批量导入主路径服务端化

| scenario | 承载测试 |
| --- | --- |
| UI 调用 batch | `import_kam_document` 内部复用 batch 管道；`/credentials/import/batch` 契约未变 |
| 客户端不再重算认证类型 | vitest `不本地推断认证类型`、`原样返回解析后的文档，不做容器判别` |
| 公共客户端不得被前端拒收 | vitest `不因缺少 refreshToken 而拒绝——判别是服务端的职责`；前端硬失败分支已删除 |

### ADDED: 导入容器格式支持范围明确且逐条可诊断

| scenario | 承载测试 |
| --- | --- |
| 四种容器均可导入 | `matrix_all_containers_times_all_login_formats`（4 容器 × 5 登录格式 = 20 组合） |
| 包装判定先于平铺单条 | `wrapper_detection_precedes_flat_single` |
| 显式 null 视为缺失 | `tolerates_all_explicit_nulls`、`null_valued_key_does_not_count_as_present` |
| 无法识别的容器整体拒绝 | `kam_import_rejects_unrecognized_container_wholesale` |

### ADDED: 导入必须逐条报告失败原因

| scenario | 承载测试 |
| --- | --- |
| 部分记录无效时逐条可见 | `rejects_unknown_auth_method_per_record`（断言同批次其他记录照常处理） |
| 未知认证类型逐条失败 | 同上 |
| 非法 endpoint 逐条失败 | `rejects_external_with_non_whitelisted_endpoint`、`kam_import_dry_run_reports_per_record_results` |
| 预览不得展示敏感字段 | `kam_import_preview_never_leaks_secrets`、`drops_sensitive_and_non_login_fields` |

前端 `filter()` + `console.warn()` 静默丢弃已移除；预览由服务端预检结果驱动。

### ADDED: 导入必须映射启用状态与区域字段

| scenario | 承载测试 |
| --- | --- |
| enabled 取反映射为 disabled | `enabled_maps_to_disabled_inverted`、`missing_enabled_defaults_to_not_disabled`、`explicit_disabled_takes_precedence` |
| region 写入通用字段 | `region_writes_general_field_only` |

## credential-ingest

### MODIFIED: 统一 ingest 管道为唯一入库路径

新增按族形状校验：`AddCredentialRequest::validate_shape`。

| scenario | 承载测试 |
| --- | --- |
| external 公共客户端通过校验 | `validate_shape_external_public_client_passes_without_secret` |
| external 缺 endpoint 与 issuer 被拒 | `validate_shape_external_requires_endpoint_or_issuer` |
| external 刷新前必须校验 endpoint | `validate_shape_external_rejects_non_whitelisted_endpoint`、`test_external_rejects_non_whitelisted_endpoint_before_network` |

四族取值全覆盖：`validate_shape_accepts_all_canonical_methods`。

### MODIFIED: 凭据身份元数据字段

新增 scenario「旧文件缺 external 字段可加载」→ `test_external_fields_missing_backward_compat`。

### MODIFIED: 冲突与 upsert 策略

新增 scenario「upsert 保留 external endpoint 元数据」→
`test_ingest_overlay_preserves_external_metadata`。

### MODIFIED: POST /credentials 兼容扩展

| scenario | 承载测试 |
| --- | --- |
| authMethod 缺省不变 | `default_auth_method_is_still_social` |
| 未知 authMethod 在 API 边界被拒 | `validate_shape_rejects_unknown_auth_method` |

`ingest_from_request` 在构造凭据前调用 `validate_shape`，未知值不会进入刷新阶段。

### MODIFIED: 密钥与日志安全

| scenario | 承载测试 |
| --- | --- |
| 状态接口继续脱敏 | 既有 admin 测试保持通过 |
| 导入文件中的密码不入库 | `drops_sensitive_and_non_login_fields`、`kam_import_preview_never_leaks_secrets` |
| external 刷新不记录 form 与响应原文 | 实现见 `refresh_external_token` 错误分支（`bail!("{}: {}", error_msg, status)`，不含 body） |

### ADDED: region 解析链不得因导入能力而改变

| scenario | 承载测试 |
| --- | --- |
| auth region 回退到凭据级 region | `test_region_resolution_chains_remain_distinct`、`region_writes_general_field_only` |
| api region 不回退到凭据级 region | 同上；既有 `test_api_call_uses_effective_api_region` 保持通过 |
| api region 覆盖仍优先 | 既有 `test_api_call_uses_credential_api_region` 保持通过 |
| 导入账号刷新使用源 region | `region_writes_general_field_only`（断言 `effective_auth_region` 取到导入值） |

该 requirement 是 bridge-plan 8.1 反转后的产物：早期草案计划改
`effective_api_region`，被证伪后改为**锁定现状**。两个既有断言测试是哨兵。

## 偏离与说明

### 1. external 仍会为取 profileArn 而强刷一次（已知遗留，spec 未要求修）

`refresh_routes_to_idc` 对 external 返回 `false`，故 profile 解析会对其执行一次
注定无 ARN 收益的强刷。修它需重新定义该谓词语义（从「是否走 OIDC」变为
「是否返回 ARN」），会牵连 `profile-arn-resolution` 的既有 spec 场景。

已用两个测试锁定现状并标注非期望终态：
`test_external_currently_not_routed_to_idc_predicate`（token_manager）、
`test_external_without_arn_currently_force_refreshes`（profile）。
design.md「未决与已知遗留」第 1 条记录了取舍。

### 2. 前端移除了「跳过 error 状态账号」开关

该开关依赖客户端逐条判别与本地跳过逻辑；改为服务端逐条处理后无对应语义。
移除属实现细节，不违反任何 spec requirement——spec 要求的是「逐条可见」，
现由服务端预检结果 + 逐条渲染满足。

### 3. `AddCredentialRequest` 增加 `region` 字段（TS 侧）

TS 类型此前缺 `region`（只有 `authRegion` / `apiRegion`），而 Rust 侧一直有。
补齐是 KAM 导入写通用 region 的前提，属 spec「region 写入通用字段」的必要实现。

## 未验证项

- **未做真实账号在线验活。** 所有 external 刷新测试都在出站前拦下（endpoint 校验
  失败路径）或只验证请求构造（form 字段）。真实 Microsoft token 端点的响应形状
  依据 KAM 已验证实现推导，本项目独立实现、独立测试，但未与上游对接验证。
  剩余风险：若 Microsoft 返回的字段名与 `ExternalRefreshResponse` 不匹配，
  刷新会在反序列化阶段失败——错误可见、不静默，且不影响其他凭据。
- **未验证迁移写回在真实用户凭据文件上的行为。** 集成测试用临时目录覆盖了备份、
  原子替换、失败保留三条路径，但未在含真实凭据的文件上运行（按纪律不得使用真实凭据）。
