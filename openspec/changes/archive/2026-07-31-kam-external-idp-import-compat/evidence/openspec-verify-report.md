# OpenSpec Verify Report: kam-external-idp-import-compat

> 日期：2026-07-30
> 结论：**通过。** 工件完整、spec 校验通过、tasks 全部完成、Non-Goals 边界守住、
> 无密钥入库风险。可以归档。

## 1. 工件完整性

```
openspec/changes/kam-external-idp-import-compat/
├── .openspec.yaml
├── proposal.md                                   320 行
├── design.md                                     478 行
├── tasks.md                                      262 行（83 项，全部完成）
├── specs/
│   ├── credential-import/spec.md                 MODIFIED 6 + ADDED 3
│   ├── credential-ingest/spec.md                 MODIFIED 5 + ADDED 1
│   └── external-idp-credentials/spec.md          ADDED 8
└── evidence/
    ├── bridge-plan.md                            实现前检查点
    ├── spec-compliance-report.md                 requirement → 测试映射
    ├── openspec-verify-report.md                 本文件
    └── verification-before-completion.md         最终验证记录
```

`openspec status --change kam-external-idp-import-compat --json`：
四工件（proposal / design / specs / tasks）均 `status: done`，`isComplete: true`。

`openspec validate --all`：**19 passed, 0 failed**。
`openspec validate kam-external-idp-import-compat --strict`：valid。
`openspec show --json`：`deltaCount: 23`，全部正确解析。

## 2. MODIFIED requirement 标题匹配

MODIFIED 的 requirement 标题必须与目标 spec 逐字一致，否则归档时会新建而非修改。

| capability | change 中的 MODIFIED 标题 | 目标 spec 中存在 |
| --- | --- | --- |
| credential-import | KAM/Admin 导入接收 provider 与 profileArn | ✓ |
| credential-import | 导入后触发 profile 解析与可观测状态 | ✓ |
| credential-import | 导入验活区分余额与对话前置条件 | ✓ |
| credential-import | KAM/Admin 导入接收身份字段 | ✓ |
| credential-import | 导入默认冲突策略利于重导 | ✓ |
| credential-import | 批量导入主路径服务端化 | ✓ |
| credential-ingest | 统一 ingest 管道为唯一入库路径 | ✓ |
| credential-ingest | 凭据身份元数据字段 | ✓ |
| credential-ingest | 冲突与 upsert 策略 | ✓ |
| credential-ingest | POST /credentials 兼容扩展 | ✓ |
| credential-ingest | 密钥与日志安全 | ✓ |

已用 `diff` 逐条比对确认（本会话执行）。`credential-ingest` 的 MODIFIED 是既有 8 条
requirement 的子集（未覆盖「OAuth 用户信息 enrich」「单条 import API」
「批量 import API」），属正常——这三条本 change 不改。

## 3. Tasks 完成度

83/83。分组完成情况：

| 组 | 内容 | 项数 |
| --- | --- | --- |
| 1 | 实现前核对 | 4 |
| 2 | 认证类型规范化 | 4 |
| 3 | endpoint 校验（含 3.0 加 url 依赖） | 6 |
| 4 | 凭据模型扩展 | 5 |
| 5 | 刷新分派扩四路 | 6 |
| 6 | KAM adapter | 8 |
| 7 | 脱敏 fixtures | 3 |
| 8 | Admin 契约与端点 | 8 |
| 9 | 原子写入与启动加载 | 9 |
| 10 | 前端 | 6 |
| 11 | profile 回归测试 | 3 |
| 12 | 样例与文档（含 12.1a gitignore） | 7 |
| 13 | 全量验证 | 10 |
| 14 | 交付门禁 | 4 |

## 4. Non-Goals 边界核查

proposal 列了 14 条 Non-Goals。逐条以 `git diff` 验证：

| Non-Goal | 验证方式 | 结果 |
| --- | --- | --- |
| 不改 Social 与 IdC 既有刷新行为 | `git diff` 查 `refresh_social_token` / `refresh_idc_token` / 端点常量的删改行 | 无删改 ✓ |
| 不引入新运行时配置项 | `git diff src/model/config.rs`、`config.example.json` | 均无改动 ✓ |
| 不迁移账号级 proxyConfig | `drops_sensitive_and_non_login_fields` 断言 proxy 三字段为 None | ✓ |
| 不导入 password / usageData / groupId / tagLinks / 计数字段 | 同上测试断言序列化输出不含这些值 | ✓ |
| 不改 KAM 侧任何代码 | KAM 仓库 16 个改动文件的 mtime 为 11:33，早于本会话实现阶段 | ✓ |
| 不实现 external_idp 登录流程 | 未新增授权码换 token 代码；`online_auth.rs` 未改 | ✓ |
| 不改 profileArn 固定占位表与识别逻辑 | `git diff src/kiro/profile.rs` 无删改行，新增全在 `#[cfg(test)]` | ✓ |
| 不改 refresh_lock 粒度 / persist 触发时机 / force_refresh 语义 | `persist_credentials` 只改写入方式（`fs::write` → `write_atomic`），触发点与签名不变 | ✓ |
| 不做 KAM API Key 互操作 | adapter 只在 `kiroApiKey` 已存在时透传，未新增网关密钥逻辑 | ✓ |
| 不给 Admin 快照回传 secret 明文 | `kam_preview_item_never_serializes_secrets`、`kam_import_preview_never_leaks_secrets` | ✓ |

额外确认：`AGENTS.md` 无改动（不涉及 AI 纪律或高风险矩阵变化）。

## 5. bridge-plan 停止条件复核

bridge-plan 第 11 节列了 10 条实现期停止条件。实际触发情况：

| 停止条件 | 是否触发 |
| --- | --- |
| 任一 endpoint 绕过测试通过 | 未触发（14 个拒绝用例全部 `Err`） |
| `refresh_social_token` / `refresh_idc_token` 函数体出现 diff | 未触发 |
| `profile.rs` 出现 `#[cfg(test)]` 之外的 diff | 未触发 |
| Windows `fs::rename` 覆盖行为与假设不符 | 未触发（`write_atomic_replaces_existing` 实测通过） |
| `persist_credentials` 原子化牵连超 3 个既有测试 | 未触发（0 个，全量 693 通过） |
| 需要新增运行时配置项 | 未触发 |
| 工作区出现真实 config / credentials / token / Cookie | 未触发 |
| 任一既有测试被修改以迁就新实现 | 未触发（见第 6 节） |
| `test_api_call_uses_effective_api_region` 等哨兵测试失败 | 未触发（两个都通过） |
| `git check-ignore` 显示 `credentials.json` 变为可跟踪 | 未触发（双向验证通过） |

## 6. 既有测试的修改核查

纪律要求：不得为让新实现通过而修改既有测试。本会话对既有测试文件的改动仅一处：

**`credentials.rs` 中 4 处 `KiroCredentials` 全字段初始化尾部追加
`..Default::default()`。**

性质：`KiroCredentials` 新增 3 个字段后，全字段初始化语法必须补齐字段（`E0063`
编译错误），否则测试文件无法编译。追加 `..Default::default()` 是最小改动，
且不改变任何断言语义——被追加的 4 处原本就把所有 `Option` 字段显式写为 `None`。

**未修改任何断言。** 特别是 `token_manager.rs:3256-3273` 的两个 region 哨兵测试
保持原样并通过——这是 bridge-plan 8.1 的核心结论（api region 回退链不改）。

## 7. 高风险项落实核查

| 风险 | 缓解措施落实情况 |
| --- | --- |
| R1 endpoint SSRF（最高） | HTTPS-only + `url::Url` 解析 hostname + 4 域精确/子域白名单 + 显式 IP/IPv6/localhost 拒绝 + 派生后复检 + 白名单硬编码不可配置。14 个拒绝用例（含反斜杠归一化、userinfo、后缀伪装、前缀伪装、无点边界仿冒）全部通过 |
| R2 `KiroCredentials` 影响面（CodeGraph 实测 151 符号） | 3 个新字段全部可选 + `skip_serializing_if`；round-trip、旧文件兼容、upsert overlay 三类测试；全量 693 通过 |
| R3 迁移损坏凭据文件 | 序列化 → 备份 → 临时文件 → 原子替换；三条失败路径各有测试；失败不阻止启动 |
| R5 `persist_credentials` 原子化牵连 13 个调用点 | 函数体内部改动，签名不变，0 个既有测试受影响 |
| R6 导入强制刷新触发限流 | 复用既有 batch 管道（默认 concurrency 1，`stopOnError: false`） |
| R7 前端新增 vitest | 版本固定 `2.1.9`（无 caret）；`pnpm-lock.yaml` 已同步；`install --frozen-lockfile` 实测通过 |
| R8 错误体泄露 token/租户 | external 刷新错误只回显 HTTP status，不含 body；预检只回传布尔状态；4 个泄露检查测试 |
| `.gitignore` 例外扩大可跟踪范围 | 例外模式收窄到 `credentials.example.*.json`；`git check-ignore` 双向验证 |

## 8. 密钥与文件卫生

`git status --short`（本会话末）：

```
 M .gitignore                    Cargo.lock  Cargo.toml  README.md
 M admin-ui/{package.json,pnpm-lock.yaml,src/api/credentials.ts,
             src/components/kam-import-dialog.tsx,src/types/api.ts}
 M credentials.example.idc.json
 M src/admin/{handlers.rs,router.rs,service.rs,types.rs}
 M src/common/mod.rs  src/kiro/mod.rs
 M src/kiro/model/{credentials.rs,token_refresh.rs}
 M src/kiro/{profile.rs,token_manager.rs}  src/main.rs
?? admin-ui/src/components/kam-import-dialog.test.ts
?? credentials.example.external.json
?? docs/kiro-account-manager-export-compatibility-optimization.md
?? openspec/changes/kam-external-idp-import-compat/
?? src/common/atomic_file.rs
?? src/kiro/{external_idp.rs,kam_adapter.rs}
```

核查结论：

- 无 `config.json`、`credentials.json`、`credentials.*`（除 `*.example.*`）
- 无 `.codegraph/`、`*.log`、token、Cookie
- `credentials.example.external.json` 全部为占位值（已用脚本验证）
- `credentials.example.idc.json` 的 `profileArn` 已改为
  `...:000000000000:profile/XXXXXXXXXXXX`，与 `BUILDER_ID_PROFILE_ARN`
  和 `SOCIAL_SIGN_IN_PROFILE_ARN` 均不同
- 新增源文件中无疑似真实凭据（`ksk_` / JWT 模式检索为空）
- 实现过程中创建的临时脚本 `fix_placeholder.py` 已删除

## 9. 归档前需注意

1. **归档会把 3 个 delta 合并进 `openspec/specs/`。** 新增 capability
   `external-idp-credentials` 将成为第 19 个 spec。
2. **`docs/kiro-account-manager-export-compatibility-optimization.md` 是未跟踪的
   分析文档。** 已在本会话修正其 region 相关的错误判断（§4.3 加勘误注）。
   是否纳入版本控制由操作者决定。
3. **未做真实账号在线验活。** 详见 spec-compliance-report 的「未验证项」与
   verification-before-completion.md。
