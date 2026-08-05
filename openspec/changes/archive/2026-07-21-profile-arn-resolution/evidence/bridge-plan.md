# Bridge Plan: profile-arn-resolution

生成时间：2026-07-20（Asia/Shanghai）
技能：openspec-superpowers-bridge
分支：dev
工作区：仅未跟踪 `openspec/changes/profile-arn-resolution/`（无 config.json / credentials.json / .codegraph  staged）

## 1. 范围 / 非目标 / 关键设计决策

### 范围（本 change 可实现）

- 请求前 `resolve_profile_arn`（缓存 → 固定 ARN 表 → ListAvailableProfiles → refresh fallback → 持久化）
- 凭据模型可选 `provider`
- `call_api` / `call_api_stream` / MCP / `get_usage_limits` 接入 resolve
- 403 bearer invalid：无 profile 时先 resolve 再重试；不因该路径标记 InvalidRefreshToken
- Admin API + KAM 导入接收 `provider`/`profileArn`；idc 默认 provider=BuilderId；导入后 resolve + 状态可见

### 非目标（禁止顺手做）

- OpenAI 兼容 API
- 多 endpoint fallback 产品化（q/codewhisperer/amazonq）
- 负载均衡算法改造
- Docker/CI 流程改造（仅文档字段若 README 更新）
- 真实凭据/线上冒烟入库

### 关键设计决策（实现时不得静默改写）

1. 固定 ARN 与 Kiro-Go 常量一致（BuilderId / Github / Google）。
2. ListAvailableProfiles 基址先按 Go：`https://codewhisperer.us-east-1.amazonaws.com`；按 apiRegion 扩展另开 change。
3. Admin `add_credential` 不得再硬编码丢弃调用方提供的 `profileArn`。
4. resolve 写回必须走 MultiTokenManager 锁/持久化语义，禁止无锁直接改共享凭据。
5. 功能默认开启；可选回滚开关（env/config）允许恢复「仅已有 arn 注入」。

### 工件一致性检查

| 工件 | 结论 |
| --- | --- |
| openspec status | planning 完整，`isComplete=true`，非 blocked |
| proposal ↔ design | 一致：解析 + provider + 导入 + 403 分类 |
| design ↔ tasks | tasks 1–4 覆盖模型/核心/请求路径/Admin-UI |
| specs ↔ tasks | profile-arn-resolution 与 credential-import 场景均有对应任务 |
| 与 AGENTS 高风险矩阵 | Token/多凭据 + Admin/凭据 CRUD，已有 OpenSpec |

## 2. 高风险项

| 风险 | 为何高 | 缓解 |
| --- | --- | --- |
| 错误固定 ARN / 错误默认 provider | 可能对错误账号类型注入 ARN，行为难查 | 单测固定表；idc 默认仅 BuilderId；Enterprise 走 list |
| 403 分类改坏 | 可能把真 invalid_grant 或真 token 失效误判为 soft | 分层：无 profile→resolve；有 profile→force refresh；invalid_grant 仍永久禁用 |
| 并发 persist profile | 多请求同时 resolve 写文件 | 仅在 TokenManager 持锁路径更新 + persist |
| ListAvailableProfiles 对 BuilderId 403 | 白刷 token、日志风暴 | fixed map 短路，禁止 BuilderId 走 list |
| 导入日志泄露密钥 | 安全红线 | 日志只记 id/email/hasProfileArn/错误类型 |
| 改 Admin 请求 schema | 旧前端字段兼容 | 新字段全 optional；默认值在服务端/导入层 |

## 3. CodeGraph 证据

### 命令

```text
codegraph status
codegraph query inject_profile_arn
codegraph impact inject_profile_arn --depth 2
codegraph callers/callees call_api_with_retry
codegraph impact add_credential --depth 1
codegraph query/impact get_usage_limits
```

### 结论

| 符号 | 位置 | 结论 |
| --- | --- | --- |
| 索引 | 85 files / 1281 nodes | 最新 |
| `inject_profile_arn` | `src/kiro/endpoint/ide.rs:114` | 仅有则注入；impact→`transform_api_body`→`call_api_with_retry` |
| `call_api_with_retry` | `provider.rs:279` | callees 含 acquire_context、transform_api_body、is_bearer_token_invalid、force_refresh_token_for |
| callers | `call_api` / `call_api_stream` | 流式与非流式同一缺口 |
| `add_credential` | admin service + token_manager | 导入主路径；impact 含 handlers/router/tests |
| `get_usage_limits` | token_manager.rs:323 | 仅影响 usage 查询链；需请求前 resolve |

建议实现后：`codegraph sync`，再 `impact resolve_profile_arn` / `impact call_api_with_retry` 复核。

## 4. rg / 源码补盲

### 关键命中

- `src/admin/service.rs`：`profile_arn: None` 硬编码（导入丢 arn）
- `admin-ui/.../kam-import-dialog.tsx`：只传 authMethod/clientId/clientSecret/authRegion/machineId，**无 provider/profileArn**
- `admin-ui/src/types/api.ts`：`AddCredentialRequest` 无 provider/profileArn
- `src/admin/types.rs`：`AddCredentialRequest` 需扩展
- `src/kiro/model/credentials.rs`：有 `profile_arn`，**无 provider**
- `src/kiro/provider.rs`：401/403 走 `is_bearer_token_invalid` + force refresh
- README 已有 `profileArn` 字段表，但 **无 provider**；IdC 说明称 builder-id/iam 归一为 idc
- `credentials.example.idc.json`：无 profileArn/provider 示例字段
- Docker/CI：挂载 credentials 路径；**无** profile 逻辑，无需改 Dockerfile/workflows（除非文档）

### 盲区结论

配置/示例/前端导入是实现必改面；Docker/CI 非本 change 行为面。

## 5. 任务 → 执行步骤映射

| Task | 执行步骤 | 验证 | 停止条件 |
| --- | --- | --- | --- |
| 1.1 provider 字段 | 改 credentials 模型 + roundtrip 测试 | `cargo test credentials` 或模块测试 | serde 名冲突/破坏旧 JSON |
| 1.2 示例与 README | 更新 example + 字段表加 provider | 人工 diff；无真实密钥 | 写进真实 arn 密钥 |
| 2.1–2.4 profile 模块 | 新 `src/kiro/profile.rs` + mod 导出 + list HTTP mock 测试 | 固定表/list/fallback/unsupported 单测 | 无法 mock 出站仍硬编码网络 |
| 2.3 写回 persist | TokenManager API：update_profile_arn | 多凭据写回测试 | 无锁写共享状态 |
| 3.1–3.2 请求路径 | provider + get_usage_limits 接入 | cargo test provider/token | 改变负载均衡语义 |
| 3.3 403 分类 | 改 force_refresh 分支 | 单测模拟 body | 误伤 invalid_grant |
| 3.4 回归测试 | 更新既有 token_manager 测试 | cargo test | 大范围无关重构 |
| 4.1–4.2 Admin API | types/service/handlers 快照字段 | admin 相关 test | 破坏现有 Admin 客户端必填 |
| 4.3–4.4 KAM UI | types + kam-import-dialog | `pnpm build`（改 UI 时） | 导入时打印 refreshToken |
| 5.x 门禁 | validate + tests + evidence + git status | 见下节必跑验证 | validate/测试失败未修 |

推荐实现顺序：**1.1 → 2.1–2.4 → 3.x → 4.x → 1.2/文档 → 5.x**（先核心可测，再导入与文档）。

## 6. 必跑验证

| 阶段 | 命令 | 通过标准 |
| --- | --- | --- |
| 规划 | `openspec validate --all` | 已通过（bridge 前） |
| 实现中 | `cargo test`（至少 kiro/admin 相关） | 全绿 |
| 改 UI 后 | `cd admin-ui && pnpm build` | 构建成功 |
| 图谱 | `codegraph sync` + 关键 impact | 新符号可查询 |
| 收尾 | `openspec validate --all` | 通过 |
| 收尾 | `git status --short` | 无 credentials.json/config.json/.codegraph 误加 |
| 证据 | compliance / verify / completion | 实现后 skills |

**不在本仓库 CI 内默认跑的：** 对真实 Kiro 账号 curl（用户环境可选；禁止把输出密钥写入仓库）。

## 7. README / AGENTS / spec 同步判断

| 文件 | 是否需同步 | 说明 |
| --- | --- | --- |
| README.md | **是**（task 1.2） | credentials 字段表补 `provider`；IdC/KAM 导入注意点 |
| AGENTS.md | **否** | 纪律/矩阵不变 |
| spec/requirements.md | **建议实现后轻量补**「profile 解析」到核心能力列表 | 或归档 sync-specs 时再合入 |
| spec/design.md | **建议实现后** 在数据流第 3–4 步注明 resolve_profile_arn | 非阻塞实现 |
| openspec/specs 主规格 | 本 change 用 delta；归档时再 sync | 实现期不强制 |
| Docker/workflows | **否** | 无行为入口变化 |

## 8. 停止条件

出现以下任一情况，**停止编码并向用户升级**：

1. 发现需要改负载均衡 / 多 endpoint fallback 才能过验收（超出非目标）。
2. 固定 ARN 与上游真实行为冲突且无法用 list/refresh 补救（需产品决策）。
3. 工作区出现将被提交的真实 `config.json` / `credentials.json` / token。
4. OpenSpec 工件与实现过程严重偏离且无法在 tasks 内修正。
5. `cargo test` 或 `openspec validate` 持续失败且根因不在本 change 范围。

## 9. Bridge 结论

- **状态：GO（可以开始实现）**
- 无 blocked；范围/非目标/验收与源码证据一致。
- 下一步：按 tasks 顺序实现，或使用 `openspec-apply-change` 驱动任务执行；实现后跑 `spec-compliance-check`。