# 端到端受控实验（tasks 9.5）— 改动后

> 执行时间：2026-07-31 17:00–17:12（UTC+8）
> 被测二进制：`kiro-rs.exe`（2026-07-31 16:53 构建，含本 change 全部改动）
> 实例：`127.0.0.1:18990`，`.\kiro-rs.exe -c .\config.json`
> 与原实验的差异：**未新建隔离实例**，直接在现有实例上测（见 §1 方法说明）

## 1. 方法：指纹计数器替代日志计数器

原实验用日志字符串 `凭据 #N Token 已强制刷新` 计数。本次该实例日志只走 stdout、
未落盘，且不宜抢占用户控制台，因此改用**等价且更精确**的计数器：

Admin 快照 `GET /api/admin/credentials` 的 `(expiresAt, refreshTokenHash)` 二元组。

依据：`force_refresh_token_for` 无条件调用 `refresh_token` 并整体赋值
`entry.credentials`，Social 刷新必然带回新 `accessToken`（改写 `expiresAt`），
且响应带 `refreshToken` 时会轮换它（`token_manager.rs:332-334`）。
故**指纹变化数 == 为取 profileArn 的强刷次数**。

排除干扰：实验期间（09:00–09:12 UTC）两条启用凭据的 `expiresAt` 均为 09:54 UTC，
距「临期自动刷新」阈值（10 分钟）尚有 44 分钟以上，因此这段时间内任何 token 变化
**只可能**来自 profileArn 解析路径的强刷。

本方法不复制凭据、不打印任何 token / hash 全文（只取 hash 前 12 位）、不落盘机密。

## 2. 凭据形态（补齐原实验缺失信息之一）

| id | authMethod | provider | disabled | profileArn |
| --- | --- | --- | --- | --- |
| 1 | `idc` | `BuilderId` | false | 无 |
| 2 | `idc` | `BuilderId` | **true** | 无 |
| 3 | **`social`** | `Github` | false | **无** ← 本 change 的目标场景 |

`#3` 确实走 Social 分支（`authMethod=social` → `refresh_routes_to_idc` 为 false
→ `decide_profile_action` 落 `ForceRefresh`），与原实验的问题凭据形态一致。

## 3. 主实验：9 个操作的强刷次数

目标凭据 `#3`（Social 无 ARN）。进程启动于 08:54 UTC，启动预热已各刷一次
（两条凭据 `expiresAt` 均为 09:54），即 `#3` 此刻已处于 15 分钟 `NoArn` 冷却中。

| 场景 | 操作次数 | 结果 | 耗时 | 新增强刷 |
| --- | --- | --- | --- | --- |
| 余额查询 `GET /credentials/3/balance?force=true` | 3 | 200 | 774 / 767 / 737 ms | **0** |
| 模型刷新 `POST /credentials/3/models/refresh` | 2 | 200 | 814 / 1486 ms | **0** |
| 真实对话 `POST /v1/messages`（`x-api-key` = `apiKey`） | 3 | 200 | 3556 / 2806 / 2027 ms | **0** |
| **合计（8 个成功操作）** | 8 | — | — | **0** |

对比原基线：同类操作 8 次 → 强刷 8 次（1:1）。

**一处操作失败，与本 change 无关**：`POST /credentials/3/test` 返回 500
（上游 `400 Invalid model. Please select a different model to continue.`）。
该失败发生在 generate 阶段、profileArn 解析之后，且该步同样未产生强刷。
未计入上表的成功操作数。

对话延迟：**2.03–3.56 s**（原基线 4.36–5.02 s）。差值与「省掉一次 list 往返 +
一次 0.87–1.23 s 的 OAuth 强刷」量级相符。**未分别计时 list 与 refresh 两段**
（需要进程内埋点，本次未做），故不宣称精确归因。

## 4. 反证实验（关键）：0 次是冷却生效，而非解析未走到

「强刷 0 次」本身不足以证明冷却起作用——也可能是 balanced 模式下解析压根没落到 `#3`。
用 Admin 手动强刷递增版本号使旧冷却失效，观察强刷是否**重新出现**：

| 步骤 | 操作 | 耗时 | 指纹变化（=强刷） | 预期 | 结果 |
| --- | --- | --- | --- | --- | --- |
| 1 | `POST /credentials/3/refresh`（Admin 手动强刷） | 1273 ms | True | — | 版本号 +1，旧冷却失效 |
| 2 | 余额查询 force=true | **2794 ms** | **True** | True | ✓ 冷却失效后**重新完整解析** |
| 3 | 余额查询 force=true | 1549 ms | False | False | ✓ 新冷却立即恢复抑制 |
| 4 | 余额查询 force=true | **757 ms** | False | False | ✓ 回到无往返的耗时量级 |

这条链完整闭合了因果：

- 步骤 2 证明**解析确实在走 `#3`**，且此前的 0 次强刷**唯一**归因于冷却抑制。
- 步骤 2 的 2794 ms vs 步骤 4 的 757 ms：一次完整解析（list + 强刷）约 2 s，
  与 proposal 对代价的估计一致。
- 步骤 3–4 验证 spec `Scenario: 凭据变更使冷却失效` 与
  `Scenario: Admin 强制刷新语义不变`——冷却失效由版本号变化**自动达成**，
  `force_refresh_token_for` 内无任何显式清除逻辑。
- 步骤 2 保留的那一次强刷，正是设计意图中的「每凭据每窗口一次尝试」
  （spec `Scenario: Social 首次强刷行为不得回归`）。

## 5. IdC 回归检查（`#1`）

| 操作 | 次数 | 耗时 | 强刷 |
| --- | --- | --- | --- |
| 余额查询 `GET /credentials/1/balance?force=true` | 3 | 1768 / 1808 / 1665 ms | **0** |

上个 change（`2026-07-30-profile-arn-refresh-fallback-order`）的 IdC 软放行行为
未被本 change 破坏，逐位不变。

## 6. Success Criteria 对照

| 指标 | 原基线 | 目标 | 实测 | 达成 |
| --- | --- | --- | --- | --- |
| 8 个操作（3 余额 + 2 模型 + 3 对话）的强刷次数 | 8（1:1） | 1 次尝试 + 其余命中冷却 | **0**（窗口内已有启动预热那次尝试）；反证实验中冷却失效后恰好 **1** 次 | ✓ |
| `ListAvailableProfiles` 调用次数 | 8（按链路推算） | 1 | 与强刷同步为 0 / 1（冷却同时覆盖 list 与 refresh，代码层保证；未独立计数） | ✓（间接） |
| `credentials.json` 写入次数 | 8 | 1 | 与强刷同步（写盘紧随强刷）：0 / 1 | ✓（间接） |
| 对话请求延迟 | 4.36–5.02 s | 首次之后减少「强刷 + 一次 list」 | **2.03–3.56 s** | ✓ |

`hasProfileArn` 终态：三条凭据均为 false。即**本次实验中 Social 强刷仍 0 次拿到 ARN**,
与原实验的 10 次 0 命中一致。proposal Assumptions 里「Social refresh 可能返回 ARN」
仍是基于字段存在的推断，负向样本继续累积（现为 11 次 0 命中）。

## 7. 剩余未验证项

1. **list 与 refresh 未分别计时**。需进程内埋点，本次未做。因此
   「延迟下降来自省掉两段往返」是量级相符的推断，不是分段实测。
2. **`ListAvailableProfiles` 的上游状态码与响应体未捕获**（原实验缺失信息之一，
   仍未补齐）。它决定 `ListOutcome` 落 `Failed` 还是 `Empty`。
   对本 change 的结论无影响（`decide_profile_action` 对三种 miss 处理完全相同，
   且冷却在 list 之前拦截），但仍影响问题普适性判断。需抓包或 debug 日志。
3. **`NoArn` 15 分钟窗口的到期行为未实测**（需等待 15 分钟）。
   单测已覆盖（`test_cooldown_expiry_allows_new_attempt`，回拨 16 分钟）。
4. **并发去重未在真实实例上验证**。单测已覆盖
   （`test_concurrent_resolve_deduplicates`）。
5. **`POST /credentials/3/test` 的 500**（上游 400 Invalid model）属既有问题，
   与本 change 无关，未深究。

## 8. 结论

改动生效，且因果关系由反证实验闭合确认：

- Social 无 ARN 凭据的 profileArn 解析从**每操作一次强刷**降到**每凭据每 15 分钟一次**。
- 冷却同时抑制 list 与强刷，命中时以无 ARN 软放行，业务请求全部正常返回 200。
- 凭据变更（含 Admin 手动强刷）使冷却自动失效，无需显式清除逻辑。
- IdC 行为无回归。
- 对话延迟从 4.36–5.02 s 降至 2.03–3.56 s。
