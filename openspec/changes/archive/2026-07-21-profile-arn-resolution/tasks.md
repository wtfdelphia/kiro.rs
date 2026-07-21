# Tasks: profile-arn-resolution

## 1. 模型与规格落地

- [x] 1.1 为 `KiroCredentials` 增加可选 `provider` 字段（serde camelCase 兼容）及单测/序列化 roundtrip
- [x] 1.2 更新 `credentials.example.*.json` 与 README 凭据字段表（provider / profileArn 说明）

## 2. profile 解析核心

- [x] 2.1 新增 `src/kiro/profile.rs`（或等价模块）：fixed map、supports_profiles、resolve 入口
- [x] 2.2 实现 ListAvailableProfiles 客户端（mockable HTTP）与瞬时错误重试
- [x] 2.3 resolve 成功写回 MultiTokenManager 并 persist 多凭据格式
- [x] 2.4 单测：缓存命中、BuilderId 固定 ARN、Github/Google 固定 ARN、list 成功、refresh fallback、unsupported

## 3. 请求路径接入

- [x] 3.1 `call_api_with_retry` / `call_mcp_with_retry` 在出站前 resolve 并注入
- [x] 3.2 `get_usage_limits` 请求前 resolve（失败策略对齐 design）
- [x] 3.3 修正 bearer invalid 分类：无 profile 先 resolve 再重试；有 profile 再 force refresh
- [x] 3.4 provider 层/token 相关测试更新

## 4. Admin / KAM 导入

- [x] 4.1 `AddCredentialRequest` 接收 `provider`、`profileArn`；add 路径不再强制清空已提供 arn
- [x] 4.2 快照/列表字段暴露 hasProfileArn、provider（如需）
- [x] 4.3 admin-ui KAM 导入：解析 provider/profileArn；idc 默认 provider=BuilderId
- [x] 4.4 导入后触发 resolve + usage（失败可诊断，不写真实密钥到日志）

## 5. 验证与门禁

- [x] 5.1 `cargo test` 相关模块通过
- [x] 5.2 `openspec validate --all` 通过
- [x] 5.3 按需 `pnpm build`（若改 admin-ui）
- [x] 5.4 产出 bridge / compliance / verify / completion evidence（实现阶段 skills）
- [x] 5.5 `git status --short` 确认无真实凭据与 `.codegraph/` 误入
## 6. 合规优化（verify WARN 跟进）

- [x] 6.1 KAM 验活区分 hasProfileArn：verified vs verified_warn
- [x] 6.2 toast/汇总区分完全成功与缺 profile
- [x] 6.3 ListAvailableProfiles 响应解析纯函数 + 单测
- [x] 6.4 相关 cargo test + admin-ui build + 更新证据
