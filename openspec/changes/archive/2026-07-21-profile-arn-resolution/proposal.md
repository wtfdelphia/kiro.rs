# Proposal: profile-arn-resolution

## Why

KAM 导入的 IdC/Builder 凭据在 kiro-rs 上常出现「余额查询成功、对话 403 invalid bearer」。
对比 Kiro-Go 与现场实测，根因是缺少与 Kiro IDE 对齐的 profileArn 解析前置路径：
当前仅「凭据里已有则注入」，而 OIDC refresh 往往不返回 profileArn，导致
generateAssistantResponse 裸发失败。

## What Changes

- 新增 profileArn 解析：缓存优先、固定 ARN 表（BuilderId/Github/Google）、
  ListAvailableProfiles、refresh fallback、持久化缓存
- 凭据模型增加可选 provider 字段（BuilderId / Enterprise / Github / Google 等）
- 上游 API / MCP / usage limits 请求前强制 resolve（对齐 Kiro-Go）
- 修正 403 分类：缺 profile 时先 resolve 再重试，避免误判 token 永久失效
- Admin / KAM 导入接收 provider、profileArn，导入后异步 resolve
- Admin 展示 hasProfileArn / 解析失败信息（最小可用）

## Capabilities

### New Capabilities

- `profile-arn-resolution`：上游请求前的 profileArn 解析、缓存与固定表策略
- `credential-import`：KAM/Admin 导入对 provider/profileArn 的接收与验活语义

### Modified Capabilities

- （无既有 openspec/specs/ 主规格；本 change 以 delta 新增能力规格为准）

## Impact

- 源码：`src/kiro/`（credentials、token_manager、provider、endpoint、新建 profile 模块）、
  `src/admin/`、`admin-ui` KAM 导入
- 示例配置：`credentials.example.*.json` 可补充 provider/profileArn 字段说明
- 行为：KAM IdC 对话可用性；usage limits 可附带 profileArn
- 风险类型：Token/多凭据、Admin/凭据管理、上游请求体字段（高风险）
- 非目标见 design；本提案不包含 OpenAI 兼容层或完整多端点产品化
