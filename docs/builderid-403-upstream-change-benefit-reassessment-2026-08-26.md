# BuilderId 403 问题后续：上游行为变化与继续实施的收益重估

> 状态：实测复核 + 收益重估（设计输入；任何落地仍需 OpenSpec change）
> 日期：2026-08-26
> 复核对象：[`builderid-403-profile-arn-analysis-and-optimization-design.md`](builderid-403-profile-arn-analysis-and-optimization-design.md)（下称「原设计」）的 6.1–6.7 优化方案
> 复核环境：`172.20.66.24` pm2 `kiro-rs`（v2026.8.13，online），凭据文件 `/home/openclaw/kiro.rs/credentials.json`
> 方法：生产实例只读检查（凭据形态/订阅落盘/当日日志）+ 一次受控上游探测（5 项，不刷新不轮转任何 token，仅一次最小对话消耗）+ 本地源码精读

## 1. 结论速览

1. **原设计的核心痛点已消失**：当日新注册的 BuilderId 凭据（#5–#12，accessToken 均为 `aoa…` 形态）经实测**无需任何 `profileArn` 即可直通过话面**（`generateAssistantResponse` 200 出 SSE 流）、计费面、模型目录与 `mcp`。原设计实测时（2026-08-25）「BuilderId 对话面恒 403」的结论对**新形态凭据**不再成立。
2. 上游变化的可观测特征：设备码流程现在签发 `aoa` 前缀 token（与门户面同族），且 `q.*` 数据面不再按此路径拒发。旧形态凭据（原设计中的 #1/#2）已被用户删除，无法复测；行为变化的触发条件（上游放权 vs 新注册流程差异）尚不能归因，但**对新凭据的行为是当前事实**。
3. 原设计 6 项方案中：**2 项收益蒸发（6.2 占位 ARN、6.4 UI 引导）、1 项降为可选增强（6.1 一键升级）、1 项收益蒸发（6.7 门户直连）、1 项残余收益很小（6.3 跳过必败探测）**。
4. **新的真实痛点是「账号冻结」**：当日凭据 #12 入库即收到 `403 Your User ID … temporarily is suspended`（账号被锁），代码对该错误无任何专门识别（`rg suspended` 零命中），只能靠「连续失败 3 次自动禁用」兜底，且禁用原因记为 `TooManyFailures` 而非可操作的信息。这是本次复核发现的**唯一值得立项的新收益点**。
5. 实施建议：搁置原设计 6.1–6.7 的大部分；把「冻结账号识别与调度隔离」作为新的 P0 候选（小改、纯收益）；6.3 降级为顺手项。

## 2. 复核环境事实（2026-08-26 实测）

### 2.1 凭据盘面（`credentials.json`，10 条）

| 凭据 | provider | authMethod | 订阅（已落盘） | profileArn | 备注 |
| --- | --- | --- | --- | --- | --- |
| #3 | Github | social | KIRO FREE | 无 | 旧凭据 |
| #4 | — | api_key | KIRO PRO+ | 无 | API Key 路径 |
| #5/#6/#8 | BuilderId | idc | KIRO PRO+ | 无 | 当日新增 |
| #9/#10/#12 | BuilderId | idc | KIRO PRO | 无 | 当日新增 |
| #7/#11 | BuilderId | idc | KIRO FREE | 无 | 当日新增 |

- 全部 BuilderId 凭据的 `accessToken` 前缀为 `aoa…`（原设计时代为设备码旧形态），`expiresAt` 由服务正常刷新维持。
- **订阅信息全部成功落盘**（`subscriptionTitle` 字段），日志逐条对应「凭据 #N 订阅等级已更新: None -> KIRO PRO+/PRO/FREE」——入库时 `getUsageLimits` 直接 200，无 403。
- 模型刷新：8 个新凭据全部成功（「模型缓存已刷新: 19 个」），无 403 告警；唯一失败是 #12 的账号冻结错误（见 2.3）。
- 全日志 `HTTP 403` 仅 10 条：其中 9 条属于 2026-08-25 旧凭据时代，2026-08-26 仅剩 #12 冻结一例。

### 2.2 受控上游探测（凭据 #5，KIRO PRO+，05:48 UTC）

与原设计 4.1–4.5 矩阵同构的最小复测，全部**不带** `profileArn`：

| 项 | 端点 | 原设计结果（旧凭据） | 本次结果（新凭据） |
| --- | --- | --- | --- |
| P1 | `getUsageLimits` | 403 | **200**，KIRO PRO+ |
| P2 | `ListAvailableModels` | 403 | **200**，19 个模型 |
| P3 | `ListAvailableProfiles` | 403 unsupported | **403 unsupported**（未变） |
| P4 | `generateAssistantResponse` | 403 | **200**，SSE eventstream 正常出字 |
| P5 | `mcp` tools/list | 403 | **200**，工具列表正常 |

即：除 `ListAvailableProfiles`（BuilderId 账号类型的固有不支持，原本就不影响无 ARN 路径）外，**全部数据面对新形态 BuilderId 凭据放开**。

### 2.3 新增风险形态：账号冻结

凭据 #12 入库后模型刷新报：

```
HTTP 403: {"message":"Your User ID (…) temporarily is suspended.
We've locked your account as a security precaution. …"}
```

- 这是**账号级风控**，与 token 形态、ARN、权限配置均无关；批量注册同类账号时概率性出现（8 个新凭据中 1 个）。
- 该凭据当前 `disabled: true`，由既有「连续失败 3 次自动禁用」（`MAX_FAILURES_PER_CREDENTIAL = 3`，`token_manager.rs:892`）兜底，禁用原因 `TooManyFailures`。
- 代码中 `rg 'suspended'` 零命中：无识别、无专门禁用原因、无面向用户的支持链接透出。

## 3. 原设计方案收益重估

| 方案 | 原优先级 | 收益重估 | 结论 |
| --- | --- | --- | --- |
| 6.1 一键升级 API Key | P0 | 对话面已直通，「为解锁对话而升级」的动机消失。残余价值：API Key 免刷新、长期有效，可降低刷新链路的运维噪音 | **降为可选增强（P3）**；仅当刷新链路出现稳定性问题时再议 |
| 6.2 占位 ARN 注入 | P1 | 计费面无 ARN 即 200（P1），注入前提不存在 | **取消** |
| 6.3 跳过必败探测 | P2 | `ListAvailableProfiles` 对 BuilderId 仍恒 403；当前代码 `supports_profiles()` 对 BuilderId 返回 true（`profile.rs:44-56`），解析路径仍会发一次必败请求（15 分钟冷却，403 非瞬时不重试，单次一个 TLS 往返）。8 个凭据 × 每 15 分钟一次的量级，浪费有限但确实存在 | **降级为顺手项（P2 尾部）**：短路返回 `SoftUnavailable`，一行级改动，可与下一个凭据相关 change 合并 |
| 6.4 Admin UI 标注与引导 | P3 | 「对话 403 误判」场景不复存在 | **取消**（冻结账号的 UI 呈现并入新方案，见 4） |
| 6.7 门户直连对话 | P0 并列 | BuilderId 直连 `q.*` 对话面已通，门户绕行路径失去存在理由；且其依赖本机浏览器 + 7 天人工引导，成本/收益比进一步恶化 | **取消** |
| 6.5 「不做的事」清单 | — | 依然成立（不注入占位 ARN 碰运气、不伪装 token 类型等） | 保留 |
| 6.6 遗留疑问 | — | scope 差异假设已无关紧要（无 ARN 即全通）；门户协议耦合问题随 6.7 取消而失效 | 关闭 |

一句话：原设计是在「对话面被锁」前提下设计的绕行方案；锁开了，绕行方案自然退役。原设计的证据与方法（对照组、变量隔离、可复现脚本）仍然有效，作为历史档案保留即可。

## 4. 新的收益点：冻结账号的识别与隔离

当前新环境下唯一值得立项的行为变化（按项目纪律需 OpenSpec change）：

### 4.1 问题

1. 冻结错误与普通 403 同等待遇：占用 3 次失败配额后才禁用，期间若被调度命中会向用户透出裸 403 文案。
2. 禁用原因 `TooManyFailures` 丢失了关键信息（账号被风控锁定、需走 support 表单申诉），运维者无法从 Admin 状态区分「该删掉」与「申诉后可恢复」。
3. 冻结是账号级长期状态，`temporarily` 字样暗示可恢复，但恢复时间不可控；混在正常凭据池里会在每次冷却后重新产生失败。

### 4.2 方案要点（设计输入）

- 在错误分类层识别上游消息特征（`temporarily is suspended` / `locked your account`），命中即：
  - 立即禁用，新增 `DisabledReason::AccountSuspended`（区别于 `TooManyFailures`）；
  - 不消耗 3 次失败配额、不触发重试；
  - 凭据详情透出上游给出的恢复链接（`https://app.kiro.dev/account/usage?support_form`）。
- 调度层：被冻结凭据不参与 `select_next_credential`（现有 `disabled` 语义已覆盖，只需新原因）。
- Admin UI：列表/详情显示冻结态徽标（如后续做 6.4 的残余部分，合并到这里）。
- 验证：错误分类单测（消息特征 → 原因映射）；禁用路径单测（一次命中即禁用，配额不消耗）。

### 4.3 收益

- 每个冻结账号省 3 次失败往返与对应的用户可见错误；
- 凭据池健康状态可读，批量注册场景（本次 8 凭据出现 1 例冻结，发生率不可忽略）下运维成本显著下降；
- 为「冻结自愈探活」留接口（可选，不建议首版做：周期性探活冻结账号恢复状态，恢复后自动解禁——上游无恢复信号接口，探活只能靠真实请求，成本高于收益）。

## 5. 实施优先级总表

| 优先级 | 事项 | 形态 |
| --- | --- | --- |
| P0 | 冻结账号识别与隔离（4.2） | 新 OpenSpec change |
| P2 尾部 | BuilderId 短路 `ListAvailableProfiles`（原 6.3） | 并入凭据相关 change，一行级 |
| P3 | 一键升级 API Key（原 6.1） | 搁置，仅在刷新链路出现稳定性问题时重议 |
| — | 原 6.2/6.4/6.6/6.7 | 取消，原设计文档存档 |

## 6. 持续观察项

- **行为归因未定**：对话面放开是「上游全局放权」还是「新注册流程（`aoa` token）专属」尚无法归因（旧凭据已删除不可复测）。若未来新建凭据再次出现对话面 403，应先用本文 2.2 的探测矩阵定位，再决定原设计方案的复活范围。
- **冻结发生率**：8 个新凭据 1 例冻结是单批小样本；建议后续在凭据导入日志中留意该错误频率，作为 4.2 方案优先级的输入。
- 本文不修改原设计文档；两文档并存，本文为 2026-08-26 环境的最新事实基线。

## 7. 证据留档说明

- 探测于 2026-08-26 05:48 UTC 在 `172.20.66.24` 执行，脚本一次性、未入仓（避免与 `scripts/repro_builderid_upstream_matrix.py` 的全量矩阵重复维护）；仅 P4 消耗一次对话请求。
- 凭据检查只读取非敏感字段；本文不含任何 token、邮箱、userId 完整值（#12 的 userId 为上游错误消息自带，已截断）。
- 代码引用基于当前工作区 `dev` 分支（`c7daa49` 之后的工作区状态）：`src/kiro/token_manager.rs:892`（失败阈值）、`src/kiro/profile.rs:44-56`（`supports_profiles`）、`src/kiro/profile.rs:162`（`decide_profile_action`）。
