# Design: profile-arn-resolution

## 当前实现

- `IdeEndpoint::transform_api_body` 仅调用 `inject_profile_arn`：凭据有 `profile_arn` 才注入，否则原样发送。
- Admin `add_credential` 硬编码 `profile_arn: None`；KAM 导入不解析 `provider`/`profileArn`。
- OIDC/Social refresh 若响应含 `profileArn` 会回写，但 BuilderId 等常见路径不返回该字段。
- `get_usage_limits` 仅在已有 `profile_arn` 时追加 query；对话 403 被识别为 bearer invalid 后 force refresh，仍无 profile 则「所有凭据已用尽」。
- 现场对照：social 有 profileArn 可对话；KAM IdC 无 profileArn 可余额不可对话。Kiro-Go 有完整 `ResolveProfileArn`。

## 目标设计

对齐 Kiro-Go / Kiro IDE 语义，在 kiro-rs 内增加请求前 profile 解析，而不是要求导入方必须手工提供 ARN。

### 数据模型

- `KiroCredentials` 新增可选 `provider: Option<String>`（序列化名 `provider`）。
- 保留既有 `profile_arn`；解析成功后写回并随多凭据格式 persist。

### resolve_profile_arn 算法

1. 缓存：`profile_arn` 非空 → 返回。
2. 固定表（不请求 AWS）：
   - Provider `BuilderId` → `arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX`
   - Provider `Github` / `Google` → `arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK`
3. 不支持类型：按 supports_profiles 规则短路为 `ProfileArnUnsupported`（允许裸发 usage，不打日志风暴）。
4. 支持且需动态解析（Enterprise/Internal/ExternalIdp 等）：`POST https://codewhisperer.us-east-1.amazonaws.com/ListAvailableProfiles`（Bearer + 既有 Kiro headers），取首个非空 `profiles[].arn`。
5. Fallback：token refresh，读取响应 `profileArn`。
6. 成功则 `Update`/persist 到凭据条目；失败返回可区分错误（list 失败 / refresh 空 / 无 refreshToken）。

### 调用点

- `KiroProvider::call_api_with_retry` / `call_mcp_with_retry`：acquire_context 后 resolve，用解析结果注入 body / MCP header。
- `get_usage_limits`：请求前 resolve（失败非 Unsupported 可 warn 后继续，对齐 Go）。
- Admin 导入/强制刷新后：可触发 resolve，更新 hasProfileArn 展示。

### Provider 推断

- 导入显式 `provider` 优先。
- 缺省：有 clientId+clientSecret 且 authMethod=idc → 默认 `BuilderId`（对齐 Kiro-Go KAM 导入）。
- social 无 client → 不强制 provider；若 refresh 带回 arn 则缓存。

### 403 分类修正

| 条件 | 行为 |
| --- | --- |
| body 含 bearer invalid 且当前请求未带 profileArn | 先 resolve；成功则同凭据重试一次 |
| bearer invalid 且已带 profileArn | 既有 force refresh 一次 |
| resolve 明确失败（无 profile） | soft：报告失败/冷却，不标记 InvalidRefreshToken |
| invalid_grant | 保持永久禁用 refreshToken |

## 数据流 / 影响面

```text
acquire_context → resolve_profile_arn → decorate/transform → 上游
                     ↓
               persist profile_arn + provider
```

CodeGraph 关注符号：`inject_profile_arn`、`call_api_with_retry`、`get_usage_limits`、`add_credential`、`MultiTokenManager`、KAM import。

## 异常路径

- ListAvailableProfiles 403（BuilderId）：不应走到 list，应由固定表短路。
- 网络/5xx/429：list 有限重试；仍失败则 refresh fallback。
- 无网络/无 refresh：导入验活标记 profile 未解析；对话返回可诊断错误信息（不泄露 token）。
- 多凭据并发：resolve 写回需在 TokenManager 锁语义内安全。

## 非目标

- 不实现 OpenAI 兼容 API
- 不做完整多 endpoint fallback 产品化（可选后续 change）
- 不改负载均衡算法本身
- 不引入真实凭据入库；测试用 mock HTTP / 固定表单测

## 回滚策略

- 功能开关（可选环境变量或 config，默认开启解析）：关闭后恢复「仅已有 arn 注入」。
- 或回退提交本 change 相关文件；凭据文件中多出的 provider/profileArn 字段向后兼容可忽略。

## 验证策略

- 单测：固定表、缓存命中、list 成功、refresh fallback、unsupported、403 分类
- `cargo test` 覆盖 kiro/token_manager/admin 相关模块
- admin-ui：KAM 导入解析字段（类型/映射单测或 build）
- `openspec validate --all`
- 禁止真实 token；本地手动 curl 仅用用户环境且不入库

## 假设

- 固定 ARN 常量与 Kiro-Go 保持一致，直至上游官方变更。
- ListAvailableProfiles 区域当前按 Go 实现使用 us-east-1 REST base；若后续需按 apiRegion 扩展另开 change。
- 用户确认本 change 范围仅 profile 解析与导入，不含 OpenAI/多端点大功能。