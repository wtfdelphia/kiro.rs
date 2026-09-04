# Builder ID 登录 403 分析与上游接口全量实测及优化方案

> 状态：实测分析 + 优化方案（未实现；落地需按项目纪律建立 OpenSpec change）
> 日期：2026-08-25 首次～四次修订；2026-08-26 五次修订：基于用户导出的门户 HAR（含完整对话链路）补全门户面协议，`csrfToken` 来源定位，`SessionToken` 依赖确认为最后一道门槛；同日六次修订：经本机 Chrome（CDP）取得 `SessionToken`，**门户对话面全链路实测打通**（`StreamSendMessage` 200 出字、`RefreshToken` 可续会话），门户直连从「不可用」转为「可用，会话需一次人工引导」
> 分析基线：kiro-rs 当前工作区（`src/kiro/profile.rs`、`models_api.rs`、`user_info.rs`、`token_manager.rs`、`endpoint/ide.rs`）
> 分析方法：源码精读 + CodeGraph 调用链 + **生产实例受控实验**（`172.20.66.24` 上运行的实例，凭据 #1/#2 为 BuilderId、#3 为 Github 社交登录；使用各凭据实时 accessToken 直接请求上游，token 与邮箱等敏感信息不入档）
> 现象来源：BuilderId 方式登录的账号（凭据 #1/#2）直接返回
> `403 {"message":"User is not authorized to make this call.","reason":null}`；
> 而**同一账号通过 Kiro 页面创建的 API Key（凭据 #4）访问全部正常**

## 1. 结论速览

1. **上游对 `profileArn` 做「ARN ↔ token 身份」绑定校验**。BuilderId token 携带 social 占位 ARN 时返回 `403 bearer token invalid`，而非静默接受；携带 Builder ID 占位 ARN（`arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX`）才被接受。
2. **占位 `profileArn` 只能解锁计费面**：`getUsageLimits` 从不带参数的 403 变为 200，返回真实订阅信息（`KIRO PRO+`、2000 credits/月）、邮箱与 userId。两个 BuilderId 凭据（#1/#2）行为一致，账号订阅真实有效。
3. **对话面接口带同样占位 ARN 依然 403**（凭据 #1/#2 的 OIDC token）：`generateAssistantResponse`、`ListAvailableModels`、`mcp` 全部 403。且 `ListAvailableProfiles` 对 BuilderId 明文返回 `AWS Builder ID is not supported for this operation.`，这类凭据**拿不到真实的 Kiro profile ARN**。
4. **（修订）这不是账号类型级硬限制，而是 token 类型限制**：同一账号的 API Key（凭据 #4）不带任何 `profileArn` 即全通——`getUsageLimits`、`ListAvailableModels`、`generateAssistantResponse`（SSE 出字）、`mcp` 全部 200。上游按 `tokentype: API_KEY` 请求头 + key 本身识别该路径；key 不带此头、或 accessToken 冒充此头，均报 `bearer token invalid`（见 4.9/4.10）。
5. **上游先鉴权、后校验请求体**：同一份请求体在 #3 上报 `400 INVALID_MODEL_ID` / `REQUEST_BODY_INVALID`，在 #1 上直接 `403`。请求体、模型 ID、请求头均复刻到与代理一致并经 #3/#4 对照组验证可出 SSE 流，排除代理参数问题。
6. 初期「账号侧无权限」的判断**已被两次修正**：账号订阅激活（计费面带占位 ARN 可见）；补 `profileArn` 不能解锁 OIDC token 的对话面；但同账号 API Key 对话面完全可用。
7. **（三次修订）一键升级链路已实测全通**：设备码 accessToken 可直接调用 `management.{region}.kiro.dev` 的 `KiroControlPlaneBearerService.CreateApiKey`（无需浏览器抓包中的 `x-csrf-token`/`x-kiro-userid`），返回的 `rawKey` 立即通过对话面接口的完整闭环验证（见 4.7/4.8）。这使「BuilderId 凭据一键自动升级为 API Key」成为可落地方案（见 6.1，已升级为首选）。
8. **（四次修订）Kiro Web 门户接口可用且不需要 `profileArn`**：`app.kiro.dev` 的 `KiroWebPortalService`（Smithy RPC v2 CBOR 协议）只读操作 `GetUserInfo`/`GetUserUsageAndLimits`/`ListAvailableModels` 用设备码裸 accessToken 即全部 200——鉴权走 `AccessToken`/`Idp` cookie，**完全不需要占位 `profileArn`**，比 `q.us-east-1` 计费面（必须带占位 ARN）更干净（见 4.11）。门户的对话操作（`SendMessage`）鉴权同样通过，但需要真实 `csrfToken`，伪造直接 `Invalid CSRF token`，暂不可用。
9. **（五次修订）HAR 补全门户对话全链路；写操作卡在 `SessionToken`**：用户导出的门户 HAR 包含完整链路 `GetToken` → `CreateSpace` → `GetMainChatSession` → `StreamSendMessage`（200 出 eventstream）。`csrfToken` 由服务端渲染在首页 `<meta name="csrf-token">`（`GET /` 与 `GetRootPage` 均可拿到，设备码 cookie 即可）；但实测携带该真值调写操作仍报 `Invalid CSRF token`，且 `Logout` 明确报 `Session token is required`——**csrf 与会话级 `SessionToken` cookie（HttpOnly，HAR 不可见）绑定**，这是最后一道门槛（见 4.12）。对话响应 `application/vnd.amazon.eventstream` + CBOR，事件类型（`agent_message_chunk`/`tool_call`/`session_info_update`/`stopReason`）与现有 IDE 数据面同构，解析器可复用（见 4.13）。
10. **（六次修订）门户对话面全链路打通**：经本机 Chrome 调试实例（复用用户已登录 profile 的 cookie 库）取得 `SessionToken`（HttpOnly、7 天有效期、与 `AccessToken` cookie 同寿）后，`ListSpaces`→`GetMainChatSession`→`StreamSendMessage` 全部 200，助手回复正常出字；**`RefreshToken` 命令 200，响应体返回新 `accessToken`+`csrfToken` 并 `Set-Cookie` 续发 `AccessToken`，会话可程序化续期**。至此门户直连对话**技术上完全可行**，唯一前置是一次人工的浏览器登录（每 7 天一次），或将门户设备登录流程（`AuthorizeDevice`）走通实现全自动（见 4.14/6.6）。方案排序相应调整：门户直连对话升级为与 6.1 一键升级并列的 P0 候选（见 6.1/6.7）。

## 2. 测试环境

- 实例：`172.20.66.24`，pm2 `kiro-rs`（id 4），凭据文件 `/home/openclaw/kiro.rs/credentials.json`
- 凭据（字段结构，敏感值略）：

| 凭据 | provider | authMethod | 特征字段 | 测试时状态 |
| --- | --- | --- | --- | --- |
| #1 | BuilderId | idc | clientId + clientSecret（设备码注册产物）、region=us-east-1 | accessToken 实时有效（强制刷新后） |
| #2 | BuilderId | idc | 同 #1 | 同 #1 |
| #3 | Github | social | email/userId/nickname、subscriptionTitle=KIRO FREE | accessToken 实时有效 |
| #4 | —（同 #1 账号） | api_key | `kiroApiKey`（36 字符，Kiro 页面创建）、subscriptionTitle=KIRO PRO+ | 长期有效，无需刷新 |

- 实例日志特征（`pm2 logs kiro-rs`）：
  - #1/#2：入库即 `GetUserInfo failed: 403`，启动预热 `模型刷新失败: HTTP 403`，反复强刷 Token
  - #3：`无可信 profileArn，尝试刷新 Token 以获取` → 强刷成功 → `模型缓存已刷新: 9 个`

## 3. 上游接口全量清单（含参数与请求头）

以下为项目代码实际会调用的全部上游端点。数据面 5 个已逐一实测；认证/刷新面 5 个因会**轮转 refreshToken**（单独调用会破坏运行中服务的凭据），不单独触发，用实例日志中的成功记录作为证据。

### 3.1 数据面（Kiro API，Bearer accessToken）

#### (1) getUsageLimits —— 订阅/额度/用户信息

- 方法与 URL：`GET https://q.{region}.amazonaws.com/getUsageLimits`
- Query 参数：
  - `origin=AI_EDITOR`（固定）
  - `resourceType=AGENTIC_REQUEST`（固定）
  - `isEmailRequired=true`（仅 `user_info.rs` 路径携带；`token_manager::get_usage_limits` 路径不带）
  - `profileArn={urlencoded}`（可选；当前代码中仅凭据已持久化 `profile_arn` 时携带）
- 请求头：`Authorization: Bearer {accessToken}`、`Accept: application/json`；`user_info.rs` 路径用 `User-Agent: aws-sdk-js/1.0.18 KiroAPIProxy`，`token_manager` 路径用带 machineId 的 `aws-sdk-js/1.0.0 ... KiroIDE-{version}-{machineId}`
- 代码位置：`src/kiro/user_info.rs:17`、`src/kiro/token_manager.rs:457`

#### (2) ListAvailableModels —— 模型目录

- 方法与 URL：`GET https://codewhisperer.us-east-1.amazonaws.com/ListAvailableModels`
- Query 参数：`origin=AI_EDITOR`、`maxResults=50`、`profileArn={urlencoded}`（可选）
- 请求头：`accept`、`x-amz-user-agent`、`user-agent`（含 machineId）、`host`、`amz-sdk-invocation-id`、`amz-sdk-request: attempt=1; max=1`、`Authorization`、`x-amzn-codewhisperer-optout: true`、`Connection: close`；API Key 凭据额外带 `tokentype: API_KEY`
- 代码位置：`src/kiro/models_api.rs:11-22`

#### (3) ListAvailableProfiles —— 解析真实 profile ARN

- 方法与 URL：`POST https://codewhisperer.us-east-1.amazonaws.com/ListAvailableProfiles`
- 请求体：`{"maxResults":10}`
- 请求头：同 (2)（`content-type: application/json`）
- 代码位置：`src/kiro/profile.rs:23`

#### (4) generateAssistantResponse —— 对话主接口（SSE）

- 方法与 URL：`POST https://q.{api_region}.amazonaws.com/generateAssistantResponse`
- 请求体（实测所用最小合法形态，与 `src/anthropic/converter.rs` 产出一致）：

```json
{
  "conversationState": {
    "agentTaskType": "vibe",
    "chatTriggerType": "MANUAL",
    "currentMessage": {
      "userInputMessage": {
        "userInputMessageContext": {},
        "content": "Say hi",
        "modelId": "auto",
        "origin": "AI_EDITOR"
      }
    },
    "conversationId": "conv-mx"
  },
  "profileArn": "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX"
}
```

- `profileArn` 由端点层注入请求体根对象（`src/kiro/endpoint/ide.rs:113-122`），凭据无 `profile_arn` 时不注入
- 请求头：`content-type`、`accept`、`x-amzn-codewhisperer-optout: true`、`x-amzn-kiro-agent-mode: vibe`、`x-amz-user-agent`、`user-agent`（`api/codewhispererstreaming#1.0.34`）、`host`、`amz-sdk-invocation-id`、`amz-sdk-request: attempt=1; max=3`、`Authorization`
- 代码位置：`src/kiro/endpoint/ide.rs:62-88`

#### (5) mcp —— 工具调用（JSON-RPC 2.0）

- 方法与 URL：`POST https://q.{api_region}.amazonaws.com/mcp`
- 请求体：`{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}`
- profileArn 传递方式（两种都测过）：请求体注入 `profileArn`，或请求头 `x-amzn-kiro-profile-arn: {arn}`（`src/kiro/endpoint/ide.rs:99-101`）
- 代码位置：`src/kiro/endpoint/ide.rs:69-106`

### 3.2 认证/刷新面（未单独实测，日志证据）

| 端点 | 方法/参数 | 用途 | 日志证据 |
| --- | --- | --- | --- |
| `oidc.{region}.amazonaws.com/client/register` | POST `clientName=Kiro, clientType=public, scopes=[codewhisperer:* 5 项], grantTypes=[device_code, refresh_token], issuerUrl=https://view.awsapps.com/start` | BuilderId 设备码登录注册客户端（`online_auth.rs:491`） | 登录流程已成功产出凭据 #1/#2 |
| `oidc.{region}.amazonaws.com/device_authorization` | POST `clientId, clientSecret, startUrl` | 设备授权（`online_auth.rs:515`） | 同上 |
| `oidc.{region}.amazonaws.com/token` | POST JSON `{clientId, clientSecret, refreshToken, grantType:"refresh_token"}`（`token_manager.rs:368`） | BuilderId/IdC 刷新 | `凭据 #1/#2 Token 已强制刷新` 多次成功 |
| `prod.{region}.auth.desktop.kiro.dev/refreshToken` | POST JSON `{refreshToken}`（`token_manager.rs:277`） | Social 刷新，响应可携带 `profileArn` | `凭据 #3 Token 已强制刷新` 成功 |
| Microsoft OAuth2 token 端点（`external_idp`，按凭据配置） | form `grant_type=refresh_token` | external_idp 刷新 | 本实例无此类凭据，未涉及 |

### 3.3 Kiro Web 门户面（KiroWebPortalService，CBOR；四次修订新增，五次修订按 HAR 补全）

来源为用户抓包与门户 HAR（`app.kiro.dev`，2026-08-26 导出，含完整对话链路），代理代码当前不调用，作为候选路径实测。

- 端点：`POST https://app.kiro.dev/service/KiroWebPortalService/operation/{Operation}`
- 协议：`content-type: application/cbor` + `smithy-protocol: rpc-v2-cbor`（Smithy RPC v2 CBOR，非 JSON；请求体为 CBOR map，例如 `GetUserInfo` 的 `{"origin":"KIRO_IDE","profileArn":…}`）
- 鉴权（只读操作实测最小集）：**cookie** `AccessToken={设备码 accessToken}` + `Idp=BuilderId`；`Authorization` 头与 `UserId` cookie 均非必需（对照 11f/11h/11l/11m）。缺 `AccessToken` 报 `Access token is required`，缺 `Idp` 报 `Identity provider is required`
- **写操作额外要求（五次修订）**：`x-csrf-token` 请求头 + 请求体 `csrfToken` 字段（部分操作仅其一），且二者必须与**会话级 `SessionToken` cookie**（HttpOnly，HAR 不可见）配对；`Logout` 无会话时报 `Session token is required` 为直接证据
- `csrfToken` 来源：服务端渲染在首页 `<meta name="csrf-token" content="…">`（`GET /` 与 `GetRootPage` 均可拿到），每次页面渲染重新生成
- `GetToken`（HAR 确认）：门户侧令牌签发端点，请求体仅 `{"profileArn": 占位 ARN}`，响应 `{"accessToken": "aoa…"}`——门户自己的 accessToken 由此换发
- 已确认存在的操作：只读类 `GetUserInfo`/`GetUserUsageAndLimits`/`ListAvailableModels`/`GetRootPage`/`GetLoginMetadata`（无 CSRF 要求）；写/会话类 `CreateSpace`/`GetSpace`/`GetMainChatSession`/`StreamSendMessage`/`ListSpaces`/`GetUserSettings`（需 CSRF + 会话）；登录类 `AuthorizeDevice`（需 `userCode`）/`InitiateLogin`（需 `redirectUri`/`codeChallenge` 等）/`ExchangeToken`（需 `code`/`idp`）；前端打包代码（`main.js`）中可枚举全部 **144 个命令**（`*Command`），含 `SendMessageCommand`/`StreamSendMessageCommand`/`CancelSessionCommand`/`RefreshTokenCommand`/`DeleteAccountCommand` 等
- `GenerateAssistantResponse`/`CreateConversation`/`Chat` 等猜测名返回 `UnknownOperationException`（404），该服务面不承载 IDE 协议对话主接口；其对话接口是 `StreamSendMessage`

## 4. 实测结果矩阵

**可查性**：本节全部实验项已固化为可复现脚本 [`scripts/repro_builderid_upstream_matrix.py`](../scripts/repro_builderid_upstream_matrix.py)（与部署实例同机运行：`python3 scripts/repro_builderid_upstream_matrix.py`；脚本会先经本地 Admin 接口强刷全部可刷新凭据，再直发上游，并在 `finally` 中删除自建验证 key）。2026-08-25 15:43 UTC 的一次完整实跑输出（61 项）存档于 [`docs/builderid-403-repro-evidence-2026-08-25.txt`](builderid-403-repro-evidence-2026-08-25.txt)。下文矩阵中的实验编号（1a…12g）与脚本输出编号一一对应；文档表格与脚本输出的少量编号差异以本节注记为准（11d 并入 11c、11e 并入 11h、11k 并入 11j、11g/11r 为空 body 的两半对照、5c2 为 5b 的请求头变体）。

占位 ARN 两个：

- Builder：`arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX`
- Social：`arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK`

### 4.1 getUsageLimits

| 凭据 | profileArn | 结果 |
| --- | --- | --- |
| #1 BuilderId | 无 | **403** `User is not authorized` |
| #1 BuilderId | Builder 占位 | **200**：`KIRO PRO+`（`Q_DEVELOPER_STANDALONE_PRO_PLUS`）、Credit 限额 2000/月、邮箱与 userId 正常返回 |
| #1 BuilderId | Social 占位 | **403** `The bearer token included in the request is invalid.` |
| #2 BuilderId | 无 | **403** `User is not authorized` |
| #2 BuilderId | Builder 占位 | **200**：同为 `KIRO PRO+` |
| #3 Github | 无 | **200**：`KIRO FREE`（`Q_DEVELOPER_STANDALONE_FREE`） |

### 4.2 ListAvailableModels

| 凭据 | profileArn | 结果 |
| --- | --- | --- |
| #1 | 无 | **403** |
| #1 | Builder 占位 | **403** |
| #3 | 无 | **200**，9 个模型：`auto, claude-sonnet-4.5, claude-sonnet-4, claude-haiku-4.5, deepseek-3.2, minimax-m2.5, minimax-m2.1, glm-5, qwen3-coder-next` |

### 4.3 ListAvailableProfiles

| 凭据 | 结果 |
| --- | --- |
| #1 BuilderId | **403** `AWS Builder ID is not supported for this operation.` |
| #3 Github | **200** `{"nextToken":null,"profiles":[]}`（空列表 → 走强刷兜底拿到 ARN） |

### 4.4 generateAssistantResponse（合法请求体，见 3.1(4)）

| 凭据 | profileArn | modelId | 结果 |
| --- | --- | --- | --- |
| #1 | 无 | auto | **403** `User is not authorized` |
| #1 | Builder 占位 | auto | **403** `User is not authorized` |
| #1 | Social 占位 | auto | **403** `bearer token invalid` |
| #3 | 无 | auto | **200** SSE eventstream 正常出字 |
| #3 | 无 | 不存在的模型 | **400** `INVALID_MODEL_ID`（证明 #1 的 403 早于模型校验） |

早期调试记录（用于排除请求体因素）：缺字段的请求体在 #3 上报 `400 REQUEST_BODY_INVALID`、在 #1 上报 `403`——同一份非法请求体，两类账号得到不同阶段的错误，证明**鉴权在请求体校验之前执行**。

### 4.5 mcp

| 凭据 | profileArn 方式 | 结果 |
| --- | --- | --- |
| #1 | 无 | **403** |
| #1 | 请求体 `profileArn`（Builder 占位） | **403** |
| #1 | 请求头 `x-amzn-kiro-profile-arn`（Builder 占位） | **403** |
| #3 | 无 | **200** 返回 `tools/list`（`web_search` 等） |

### 4.6 认证/刷新面

见 3.2 表格的日志证据列；两类刷新在实例运行期间均多次成功。

### 4.7 Kiro Control Plane API Key 管理面（新增实测）

- 端点：`POST https://management.{region}.kiro.dev/`（本实验 `us-east-1`）
- 协议：`content-type: application/x-amz-json-1.0` + `x-amz-target: KiroControlPlaneBearerService.{操作}`；鉴权 `Authorization: Bearer {设备码 accessToken}`
- 用户抓包中出现的 `x-csrf-token`、`x-kiro-userid`、`x-kiro-visitorid` 经剥离测试确认**非必需**（9d/10a 均未携带，上游 200）；`origin`/`referer` 保留为浏览器形态但同样非决定性因素

| 实验 | 操作 | Bearer | 请求体 | 结果 |
| --- | --- | --- | --- | --- |
| 9a | ListApiKeys | #1 accessToken（已过期） | `{"profileArn": 占位}` | **400** `AccessDeniedException: Invalid token`（token 过期所致） |
| 9b | ListApiKeys | 同上 + `x-kiro-userid` 头 | 同上 | 同上（排除了缺头因素，确认是过期） |
| 9c | ListApiKeys | 同上 + `origin`/`referer` | 同上 | 同上 |
| 9d | ListApiKeys | #1 accessToken（经服务强刷后的新 token） | 同上 | **200**，返回该账号既有两把 key |
| 10a | CreateApiKey | 同 9d | `{"profileArn": 占位, "label": "kiro-rs-repro"}` | **200**：返回 `createdAt`/`keyId`/`keyPrefix`/`rawKey`（36 字符，仅出现一次） |
| 10b | generateAssistantResponse | 10a 返回的 `rawKey`（+`tokentype: API_KEY`，无 ARN） | 合法对话请求体 | **200**，SSE 出字 |
| 10c | ListAvailableModels | 同 10b | — | **200** |
| 10d | DeleteApiKey | 同 9d | `{"profileArn": 占位, "keyId": …}` | **200** `{}`；ListApiKeys 复核确认已删除，账号恢复实验前状态 |

### 4.8 一键升级闭环结论

完整链路「设备码凭据 → 刷新 accessToken → CreateApiKey → rawKey → 对话面 200」每一跳均实测通过，且验证用 key 已删除、未留残留。关键事实：

- `CreateApiKey` 的 `profileArn` 用 Builder ID 占位 ARN 即可（与 4.1 计费面同一枚占位）
- `rawKey` 仅在创建响应中出现一次，必须当场持久化
- 管理面对过期 accessToken 报 `400 AccessDeniedException: Invalid token` 而非 401，实现时刷新后重试即可

### 4.9 API Key 路径（凭据 #4，与 #1 同账号）——推翻「账号级限制」结论

| 端点 | 请求形态 | 结果 |
| --- | --- | --- |
| `getUsageLimits` | Bearer=kiroApiKey + `tokentype: API_KEY`，无 ARN | **200** PRO+ |
| `ListAvailableModels` | 同上，无 ARN | **200**（完整模型目录） |
| `generateAssistantResponse` | 同上，无 ARN，合法请求体 | **200**，SSE eventstream 正常出字 |
| `mcp` tools/list | 同上 | **200**，返回工具列表 |
| `ListAvailableProfiles` | 同上 | **403** `API key authentication is not supported for this operation.`（符合预期，该路径本就不解析 ARN） |

### 4.10 token 类型识别验证（隔离变量）

| 实验 | Bearer | `tokentype: API_KEY` 头 | 结果 |
| --- | --- | --- | --- |
| 8a | #4 kiroApiKey | 不带 | **403** `bearer token invalid` |
| 8b | #1 accessToken | 带 | **403** `bearer token invalid` |
| 8c | #4 kiroApiKey | 带 | + Builder 占位 ARN → **200**（ARN 身份绑定校验对 key 同样生效） |
| 8d | #4 kiroApiKey | 带 | + Social 占位 ARN → **403** `bearer token invalid` |

结论：`tokentype: API_KEY` 请求头与 key 本体共同构成一条独立鉴权路径；上游对 ARN ↔ 身份的绑定校验在该路径上同样执行。

### 4.11 Kiro Web 门户面实测（四次修订新增）

凭据 #1（#2 同型复测一致；#2 首轮异常系 accessToken 过期，强刷后复测全绿，见 4.11 注）：

| 实验 | 操作 | 鉴权形态 | profileArn | 结果 |
| --- | --- | --- | --- | --- |
| 11a | GetUserInfo | 仅 Authorization 头 | 带 | **401** `Identity provider is required` |
| 11b | GetUserUsageAndLimits | 仅 Authorization 头 | 带 | **401** `Authentication required or profile is invalid.` |
| 11c/11d | GetUserInfo | Authorization + `Idp`/`UserId` cookie（±`x-kiro-userid` 头） | 带 | **401**（同上，排除头因素） |
| 11e | GetUserInfo | Authorization + `AccessToken`/`Idp`/`UserId` cookie | 带 | **200**：`idp=BuilderId`、`status=Active`、email/userId、16 个 featureFlags（含 `enableApiKeys`） |
| 11f | GetUserInfo | 仅 `Idp`/`UserId` cookie（无 Authorization） | 带 | **401** `Access token is required` |
| 11g | GetUserInfo | 空 body `{}`，**无任何 cookie** | — | **401** `Identity provider is required`（缺失来源是缺 `Idp` cookie，不是空 body；见 11r 对照） |
| 11r | GetUserInfo | 空 body `{}` + 完整 cookie | — | **200**（复现脚本中修正的空 body 对照） |
| 11h | GetUserInfo | 仅 cookie（`AccessToken`+`Idp`+`UserId`），**无 Authorization 头** | 带 | **200**（解码出完整字段，证明 cookie 自足） |
| 11i | GetUserUsageAndLimits | 同 11h | 带 | **200**：`KIRO PRO+`、2000 credits、`nextDateReset=2026-09-01` |
| 11j/11k | ListAvailableModels | 同 11h | 带（±空 `csrfToken` 字段） | **200**（完整模型目录） |
| 11l | GetUserInfo | 仅 `AccessToken` cookie（无 `Idp`） | 带 | **401** `Identity provider is required`（`Idp` cookie 必需） |
| 11m | GetUserInfo | Authorization + `AccessToken`+`Idp` cookie（无 `UserId`） | 带 | **200**（`UserId` cookie 非必需） |
| 11n | GetUserInfo | 同 11h | **不带** | **200** |
| 11o | GetUserUsageAndLimits | 同 11h | **不带** | **200**（`KIRO PRO+`） |
| 11p | ListAvailableModels | 同 11h | **不带** | **200** |
| 11q | GetUserInfo（#3 Github） | `Idp=Social` | — | **200**（响应 `status=Stale`，token 语义不同） |
| 12a | ListSpaces | body 内伪造 `csrfToken` | — | **400** `CSRF token is required`（body 内不认） |
| 12b–12d | ListSpaces | `x-csrf-token` 头伪造（±`x-kiro-userid`） | — | **401** `Invalid CSRF token`（密码学校验，非存在性检查） |

操作面探测：`SendMessage` 鉴权通过后报参数校验错误（需 `spaceId`/`sessionId`/`contentBlocks`）；`ListSpaces`/`CreateSpace`/`GetUserSettings` 存在但要求真实 `csrfToken`；`GenerateAssistantResponse` 等 5 个猜测名均 404。

注：#2 首轮 `GetUserInfo` 返回 `status: Stale`、Usage/Models 401，原因是其 accessToken 已过期而非路径差异；经服务强刷后复测三项全部 200。

### 4.12 `csrfToken` 来源与 `SessionToken` 门槛（五次修订新增）

分析对象：用户导出的门户 HAR（`app.kiro.dev`，82 条目，31 个门户请求）+ 服务器实测。

1. **HAR 中 `x-csrf-token` 在全部 28 个携带请求中值相同**（会话级常量），与请求路径无关。
2. **来源定位**：首页响应 `GET /` 的 HTML 中 `<meta name="csrf-token" content="YF+W5vD9uI+W849l4FscyhxUNwQCyMEbjUaEBzPSQZ8=">`，同时还有 `user-id`、`idp=BuilderId`、`user-status=active`、`kiro-regions=us-east-1,eu-central-1`、16 个 featureFlags。前端 `main.js` 从该 meta 读取（`csrfToken: e ?? ""`）并广播到各组件。
3. **设备码 cookie 可取 csrf**：实测 `GET /` 与 `GetRootPage`（CBOR 接口）在 `AccessToken`+`Idp` cookie 下均 200 并返回**新鲜的** `csrf-token`（每次渲染不同）。
4. **但携带该真值打写操作仍失败**：`ListSpaces`/`CreateSpace`/`GetToken` 带首页返回的真 `csrfToken`（头+体双带）→ `401 Invalid CSRF token`。
5. **门槛定位**：`Logout`（无会话）报 `401 Session token is required`；CSRF 校验需要 `csrfToken` 与会话级 `SessionToken` cookie（浏览器登录流程种下的 **HttpOnly** cookie，HAR 未导出故不可见）配对。`SessionToken` 以 `AAAADmtleS0x…` 开头（用户此前粘贴的抓包中可见）。
6. 门户登录类命令探测：`AuthorizeDevice` 要求 `userCode`（门户设备码登录的确认端点，走 `app.kiro.dev/account/device` 页面）；`InitiateLogin` 要求 `redirectUri`/`codeChallenge` 等 4 项；`ExchangeToken` 要求 `code`/`idp` 等 3 项；`GetLoginMetadata(domainName=view.awsapps.com)` 返回 `200 {"found":true}`。
7. **结论**：门户对话面的最后一道门槛是 `SessionToken` cookie 的获取；它产生于浏览器登录重定向流程，代理侧目前无法凭设备码 accessToken 直接换取。

### 4.13 HAR 对话链路与响应协议（五次修订新增）

HAR 中捕获的完整对话链路（时间序，全部 200）：

| 步骤 | 操作 | 请求体（关键字段） | 响应 |
| --- | --- | --- | --- |
| 1 | `GetToken` | `{profileArn}` | `{accessToken: "aoa…"}`（门户专用 token 换发） |
| 2 | `GetUserInfo` / `GetUserUsageAndLimits` / `ListSpaces` | `{origin/profileArn}` | 用户信息 / 额度 / 空间列表 |
| 3 | `ListAvailableModels` / `GetUserSettings` / `ListAvailableProviders` / `GetPendingInvitations` / `GetNetworkConfiguration` | 同上形态 | 模型目录等 |
| 4 | `CreateSpace` | `{spaceType:"VIBE", csrfToken, profileArn}` | `{spaceId}` |
| 5 | `GetMainChatSession` | `{spaceId, profileArn}` | `{hasMainChat:true, sessionId}` |
| 6 | `StreamSendMessage` | `{spaceId, sessionId, modelId:"claude-opus-4.8", agentMode:"VIBE", csrfToken, profileArn, contentBlocks:[{text:{text:"…"}}]}` | **200 eventstream** |

`StreamSendMessage` 响应协议实测：

- `content-type: application/vnd.amazon.eventstream`，payload 为 CBOR（与现有 IDE 数据面 `generateAssistantResponse` 的 eventstream 同族，解析器可复用）
- 事件类型分布（一次 "hi 你是什么模型" 请求）：`agent_message_chunk`×27、`session_info_update`×11、`_kiro/mcp/status`×5、`_kiro/tools/didChange`×5、`_kiro/sessions/changed`×4、`tool_call`×3、`tool_call_update`×3、`config_option_update`×3、`_kiro/governance/state`×3、`_kiro/sandbox/status`×2、`pong`×1、`_kiro/hooks/didChange`×1、`_kiro/policy/changed`×1
- 首个事件 `initial-response` 含 `messageId`/`sessionId`；末尾事件 `done` 带 `kiroSessionId` 与 `stopReason:"end_turn"`
- 该事件面比 IDE 数据面丰富（sandbox/hooks/policy 等 IDE 协议没有的事件），代理适配时只需提取 `agent_message_chunk`/`tool_call`/`session_info_update` 三类
- `agent_message_chunk` 的 `payload` 为 **JSON 字符串**（非 CBOR）：`{"content":{"text":"…","type":"text"},"sessionUpdate":"agent_message_chunk",…}`（HAR 与六修实测一致）

### 4.14 门户对话面全链路打通（六次修订新增）

方法：本机以 `--remote-debugging-port=9222` 启动 Chrome 调试实例，`--user-data-dir` 指向**复制自用户主 profile 的最小配置**（Cookies 库 + `Local State` + 本地存储），通过 CDP `Network.getAllCookies` 读取 `app.kiro.dev` 的全部 cookie（含 HttpOnly）。用户主浏览器不受影响；会话文件用后即覆写销毁，未入仓。

1. **cookie 全集确认**：`SessionToken`（HttpOnly，1284 字符）、`AccessToken`（HttpOnly，`aoa…`）、`Idp=BuilderId`、`UserId`、`kiro-visitor-id`；`SessionToken` 与 `AccessToken` cookie 有效期均至 2026-09-01（**7 天**），`secure; HttpOnly`。
2. **写操作打通**：以完整 cookie 套实测 `ListSpaces` → **200**（3 个既有空间）；`GetMainChatSession` → **200**（`sessionId`）；`StreamSendMessage` → **200**，75KB eventstream，`stopReason:"end_turn"`，助手回复文本 `"OK"` 正确解析（`agent_message_chunk.payload` 内 `content.text`）。
3. **会话可续期（关键）**：`RefreshToken`（无参数）→ **200**，响应体含新 `accessToken` 与 `csrfToken`，且 `Set-Cookie: AccessToken=…` 续发；`GetToken`（带会话）→ 200 同形态。即会话建立后，`accessToken`/`csrfToken` 均可程序化续期，**`SessionToken` 7 天内无需人工介入**。
4. **对照**：同一 `csrfToken` 真值，无 `SessionToken` 时写操作恒 `401 Invalid CSRF token`（4.12），有则全通——确认 CSRF 校验绑定的是会话而非请求本身。
5. **代价**：消耗少量对话 credits（实测请求各约 1 credit 内）；`StreamSendMessage` 使用既有空间会话，未新建空间。
6. **安全边界**：会话凭证仅在用户本机内存/临时文件流转，未写入任何仓库文件与日志；会话文件在使用后立即覆写。

## 5. 发现归纳

1. **ARN ↔ token 身份绑定**：占位 ARN 与账号类型不匹配时报 `bearer token invalid` 而非权限错误，说明 ARN 参与鉴权计算，不是普通业务参数；对 API Key 路径同样生效（8c/8d）。
2. **Builder ID 占位 ARN 是 AWS 为共享账号预留的合法值**，能通过计费面鉴权；代码中 `profile.rs` 将其列为「已知占位、永不信任/持久化」的策略只覆盖了对话面场景，未考虑计费面可用。
3. **（修订）对话面 403 的根因是 token 类型，不是账号类型**：设备码流程产出的 OIDC accessToken 在对话面恒 403 且无法解析 profile ARN；同账号的 API Key（`tokentype: API_KEY`）无需 ARN 即全通。代码对 API Key 路径的支持已完整（`is_api_key_credential`、`tokentype` 头、跳过 profile 解析，见 `credentials.rs:505`、`endpoint/ide.rs:85/103`、`models_api.rs:112`、`profile.rs:45/163/248`），因此这是**引导问题**而非实现缺口。
4. **（四次修订）门户面是第三条鉴权模型**：既不看 `tokentype` 头，也不强制 `profileArn`，认的是 `AccessToken`+`Idp` cookie；只读操作（用户信息、用量、模型）对设备码 token 全通且免 ARN，说明「对话面 403」是 `q.*` 数据面按 token 类型做的定向限制，不是账号或 ARN 的普遍性限制。门户对话路径（`SendMessage`）鉴权同样通过，但需要真实 `csrfToken`，其签发方式未明（见 6.6）。（六次修订补记：该门槛已打通——CSRF 校验绑定的是会话级 `SessionToken` cookie，经本机 Chrome CDP 取得完整会话后 `StreamSendMessage` 全链路 200 出字，见 4.14/6.7。）
5. **当前代码的可见症状**：
   - 入库时 `GetUserInfo failed: 403`（`user_info.rs` 不带 `profileArn`）→ 订阅等级、邮箱、额度全部缺失，Admin 显示为「无」
   - 启动预热与模型刷新对 BuilderId 凭据恒失败（`启动预热：凭据 #N 模型刷新失败: HTTP 403`）
   - profileArn 解析对 BuilderId 每次都白发一次必败的 `ListAvailableProfiles`（虽有冷却，但首轮与冷却到期后仍会发）

## 6. 优化方案

以下各项均为**行为变化**，按项目纪律需各自（或合并为一个）OpenSpec change 落地；本节为设计输入。

### 6.1 P0：BuilderId 凭据一键升级 API Key（三次修订后首选方案；六次修订起与 6.7 并列双 P0）

- 依据：4.7/4.8 实测闭环。上游接口：
  - `POST https://management.{region}.kiro.dev/`，`x-amz-target: KiroControlPlaneBearerService.CreateApiKey`（以及 `ListApiKeys`/`DeleteApiKey` 用于幂等与清理）
  - 请求体 `{"profileArn":"arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX","label":"…"}`；`content-type: application/x-amz-json-1.0`
  - 鉴权：设备码 accessToken（过期先刷新；管理面报 400 Invalid token 即触发刷新重试）
- 流程（Admin UI「一键升级」按钮 + 可选自动模式）：
  1. 强刷该凭据 accessToken（复用既有 `/credentials/{id}/refresh`）
  2. 调 `ListApiKeys` 检查是否已有本代理创建的 key（用固定 `label` 前缀如 `kiro-rs` 识别，避免重复创建/覆盖用户自建 key）
  3. 无则 `CreateApiKey`，`rawKey` **当场**作为新的 `api_key` 凭据入库（复用既有导入路径，`auth_method=api_key`），并将原设备码凭据标记为「已升级、仅计费可见」或直接禁用
  4. 用新 key 发起一次 `ListAvailableModels` 自检，成功后才提示完成
- 幂等与失败语义：升级中途失败不删除原凭据；重复点击只复用既有 `kiro-rs` 前缀 key；key 数量受上游配额约束时把上游错误原样透出
- 安全：全程不引入用户浏览器会话依赖（无 CSRF/visitorId）；`rawKey` 不落日志；文档与提交不携带任何真实 key
- 收益：把「引导用户手动去页面建 key」变成一步完成，彻底绕开设备码 token 的对话面 403

### 6.2 P1：`getUsageLimits` 路径对无 ARN 凭据注入类型匹配的占位 ARN

- 范围：仅 `user_info.rs::get_user_info` 与 `token_manager::get_usage_limits` 的 URL 构造；**不写回、不持久化**到 `credentials.json`，不触碰 `profile.rs` 现有「占位不可信」策略。
- 规则：凭据无 `profile_arn` 时，按 provider 注入固定占位（BuilderId → Builder 占位；Github/Google → Social 占位；其余类型不注入）。已有可信 `profile_arn` 的凭据行为不变。
- 收益：BuilderId 凭据的订阅等级（PRO+）、额度、邮箱恢复可见；`添加凭据后获取订阅等级失败` 告警消失。
- 验证：单测覆盖 URL 构造矩阵（无 ARN × provider 三分支）；受控环境复跑本文 4.1 矩阵。
- 风险：上游若未来按接口收紧占位 ARN，行为退化为现状（403），无数据损坏风险；需在该路径保留「403 时降级为无 ARN 重试一次」的既有模式（参考 `models_api.rs` 的 stripped 重试）。
- **备选实现（四次修订补充）**：改走门户面 `GetUserUsageAndLimits`（4.11 实测）：设备码凭据用 `AccessToken`+`Idp` cookie 调用，**无需任何 `profileArn`**，且额外获得 `status`/featureFlags。代价是新增 CBOR 编解码与一条新上游依赖；若 6.1 一键升级落地后 BuilderId 凭据大多转为 API Key，本项价值下降，可降级为「计费信息展示的兜底通道」。两条路线二选一，不建议并存。

### 6.3 P2：BuilderId 凭据跳过必败的上游探测

- `profile.rs::resolve_profile_arn`：provider 为 BuilderId 时短路返回 `SoftUnavailable`，不再发 `ListAvailableProfiles`（该调用对 BuilderId 恒 `403 unsupported`，当前实现中它在 `decide_profile_action` 之前无条件执行）。
- `spawn_warmup_models` / 模型刷新：对 BuilderId 凭据跳过或降频，避免每次启动必打一条 403 告警日志。
- 收益：每凭据每次解析省一次完整 TLS 往返；日志降噪。
- 注意：须保留 `SoftUnavailable`（而非 `Unsupported`）语义，与现有「带不上 ARN 硬着头皮继续」的路径一致；`decide_profile_action` 的既有 spec 场景不受影响（该短路发生在 list 阶段之前）。

### 6.4 P3：Admin UI 凭据状态标注与升级引导

- 对 provider=BuilderId 的凭据，在列表与详情标注「设备码 Token 仅可用于计费查询」，并直接提供 6.1 的「一键升级」按钮；未完成升级时避免用户把对话 403 误判为配额不足或代理故障。
- 依据字段：`provider`、`auth_method`、`has_profile_arn`、订阅信息。
- 调度层行为：升级完成后原设备码凭据不再参与对话调度（6.1 流程第 3 步已含）；未升级前是否排除由配置开关决定。

### 6.5 不做的事

- 不把占位 ARN 注入对话接口请求体以「碰运气」：实测恒 403（8c 证明 ARN 合法但仍被拒），只会增加上游噪音。
- 不对 BuilderId 强刷 Token 以获取 ARN：IdC 刷新端点响应不含 `profileArn`（`refresh_routes_to_idc` 已有注释论证），现有代码已正确规避。
- 不尝试把设备码 token「伪装」成 API Key：8b 实测上游按 token 本体识别类型，伪装直接 `bearer token invalid`。
- **（五次修订提出，六次修订修订）不让用户手工复制/粘贴浏览器会话 cookie**（`SessionToken`/`AccessToken` cookie 等 HttpOnly 值）：手工粘贴易错、易泄露、不可审计。六次修订给出了可接受的替代：本机 Chrome 调试实例经 CDP 直接读取 cookie（含 HttpOnly，自动化、无需用户触碰敏感值，见 4.14/6.7）。约束仍然成立：会话凭证只在用户本机流转，不上传服务器部署、不入仓、不落日志；会话寿命（7 天）与轮换行为不受代理控制，续期语义见 6.6。

### 6.6 遗留疑问（后续可验证）

- 设备码注册的 `scopes`（`codewhisperer:completions/analysis/conversations/transformations/taskassist`，`online_auth.rs:20`）是否缺少对话面所需的 scope，尚无法在不重走完整设备码授权的前提下验证；若未来 Kiro 官方 Builder ID 登录对话可用，可对照其注册参数修订。当前结论以实测为准：token 类型路径差异是确定事实，scope 差异只是待验证假设。
- **（四次修订提出，五次修订定位，六次修订打通）** 门户对话的 `SessionToken` 门槛已通过本机 Chrome CDP 读取 cookie 克服（见 4.14），剩余疑问收敛为「全自动建会话」与「续期语义」三点：(a) `InitiateLogin`/`ExchangeToken` 能否在 PKCE 参数齐备时不依赖浏览器换发会话；(b) 门户 `AuthorizeDevice` 设备码确认流程能否种下 `SessionToken`（需一次真实人工确认配合抓包）；(c) `RefreshToken` 能否滑动 `SessionToken` 自身有效期——实测仅观察到 `Set-Cookie: AccessToken` 续发与响应体返回新 `accessToken`/`csrfToken`，`SessionToken` 的 7 天有效期是否随刷新顺延未验证；若不顺延，则每 7 天仍需一次人工浏览器登录（或 (a)/(b) 成立后改为全自动）。在 (a)/(b)/(c) 任一证实前，按「每 7 天一次人工引导 + 程序化续期」落地（见 6.7）。
- 门户接口非代理现有调用面，形态可能随 Web 端迭代变化（HAR 中的前端版本 `9fdb6af3f508f3ae` 仅为快照）；若引入需评估版本耦合与降级策略。

### 6.7 P0（并列候选，六次修订新增）：门户直连对话（KiroWebPortalService + 会话 cookie）

- 依据：4.14 六次修订实测全通。门户面不按 token 类型歧视设备码凭据，BuilderId 账号天然可用，**无需创建 API Key**；对话事件结构与现有 IDE 数据面同构（4.13），解析器可复用。
- 会话建立（唯一人工步骤，约每 7 天一次）：
  1. 用户在本机浏览器登录一次 `app.kiro.dev`（Builder ID 设备码确认）
  2. 代理侧以 Chrome 调试实例（`--remote-debugging-port`）经 CDP `Network.getAllCookies` 读取 `app.kiro.dev` 的 cookie 套（`SessionToken`/`AccessToken`/`Idp`/`UserId`，含 HttpOnly），用户无需触碰任何敏感值
  3. 会话凭证存入与 `credentials.json` 同级的本地存储并适用同样的忽略/保密规则；**不随部署上传服务器**（门户会话与用户本机绑定，服务器部署场景不适用本方案）
- 对话路径：首页 `<meta name="csrf-token">` 或 `GetToken` 取 csrf → `ListSpaces`/`GetMainChatSession` 取会话 → `StreamSendMessage`（请求 `application/vnd.amazon.cbor` + `smithy-protocol: rpc-v2-cbor`，响应 `application/vnd.amazon.eventstream`，事件类型 `agent_message_chunk`/`tool_call`/`session_info_update`/`stopReason`）
- 续期与探活：周期性调用 `RefreshToken`（无参数）换新 `accessToken`/`csrfToken` 并续发 `AccessToken` cookie；以 `ListSpaces` 探活 `SessionToken`，失败即提示重新登录
- 成本：新增 CBOR 编解码、eventstream 解析、会话生命周期管理；引入对门户协议（Web 前端快照版本）的新上游依赖（见 6.6 第 3 条）
- 与 6.1 的取舍：6.1 纯服务端、无会话依赖、管理面接口语义稳定，作为**默认推荐**；6.7 保留设备码身份、无 key 管理成本，但依赖本机浏览器环境与 7 天一次的人工引导。两者都落地时，Admin UI 对 BuilderId 凭据默认引导 6.1，6.7 作为「不创建 API Key」场景（如企业策略限制建 key）的高级选项

## 7. 证据留档说明

- 所有请求于 2026-08-25 在 `172.20.66.24` 上以各凭据实时 accessToken 直发上游；accessToken、refreshToken、邮箱、userId 等敏感值未入档（证据存档中 `rawKey` 已按前缀脱敏）。
- 对照原则：每个结论至少一个 #3（Github）对照组，且请求体/请求头经对照组验证合法（200 出流）后才对 #1 下结论。
- 刷新类端点未单独触发的原因：会轮转运行中服务的 refreshToken，影响生产实例；以实例日志中同日成功记录为证（复现脚本中的强刷属正常运行行为）。
- **可复现**：`scripts/repro_builderid_upstream_matrix.py` 覆盖 4.1–4.11 全部 61 项实验（含负例与隔离变量对照），逐项编号与本文档矩阵一致；最近一次全绿实跑证据见 `docs/builderid-403-repro-evidence-2026-08-25.txt`。上游行为可能随时间变化，重跑时以脚本输出的实时状态为准。4.12/4.13 基于用户导出的门户 HAR（本机 `~/Desktop/app.kiro.dev.har`，2026-08-26；含浏览器会话凭证，**不入仓**）与当日服务器实测。
- 4.14（六次修订）于 2026-08-26 在用户本机执行：Chrome 调试实例经 CDP 读取会话 cookie 后直调门户对话链路，真实账号、真实出字；会话凭证文件用后覆写销毁，未入仓。该节依赖人工登录与本机浏览器，复现成本高，暂不纳入复现脚本。
- 已知边界：`#3` social 对照组偶发 `bearer token invalid`，原因均为 accessToken 过期（social 刷新写回有延迟，脚本已改为轮询 `expiresAt` 直至新鲜）；`11q` 的 `status=Stale` 是门户对 Social 凭据的正常语义，非失败。
