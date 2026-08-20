
# Qoder2Api 深度分析与 kiro-rs 可借鉴点

> 日期：2026-08-20
> 对象：`/home/openclaw/wtf_workspace/github/Qoder2Api`（Go 1.24+，v1.2.0，MIT）
> 性质：竞品/同类项目分析文档，未改动任何代码
> 分析手段：CodeGraph 索引（22 文件 / 487 节点 / 1296 边）+ 源码精读（含 `file:line` 复核）+ kiro-rs 现状对照

Qoder2Api 是把 **Qoder IDE 账号**（PAT）桥接为 **OpenAI `/v1/chat/completions` + Anthropic `/v1/messages`**
兼容网关的单二进制 Go 程序，内置账号池、故障转移和嵌入式 dashboard。它与 kiro-rs 同属
「把 IDE 订阅额度反向代理成标准 LLM API」这一品类，目标形态高度重合，因此值得逐项对照。

## 一、先说结论

Qoder2Api 整体工程成熟度**低于** kiro-rs（零测试、dashboard API 无鉴权、硬编码设备指纹），
多凭据管理、协议矩阵、测试纪律都是 kiro-rs 更强。但它有两个 kiro-rs 目前没有的机制，
证据强度足以立项：

| # | 建议 | 优先级 | 代价 | OpenSpec |
| --- | --- | --- | --- | --- |
| 1 | 流内错误可见性：`Event::Error` 产出协议错误事件而非静默吞掉 | **P0 必做**（正确性缺陷，已确认错误流被包装成 `end_turn` 假成功） | 小 | 需要（SSE 行为） |
| 2 | 首个内容事件提交前的透明故障转移（缓冲-嗅探-重放） | **条件立项**：等建议 1 的遥测给出流内凭据错误频率证据 | 中-大 | 需要（协议/SSE） |
| 3 | 配额错误解析重置窗口：定时冷却自动恢复，替代一刀切永久禁用 | **P2 缓做**：触发面仅 402 月度配额，先验证错误体有无窗口字段 | 中 | 需要（多凭据行为） |
| 4 | Admin 展示 `exhausted_until` 倒计时 | 随建议 3 | 小 | 随建议 3 |

收益量化与排序依据见第五节开头的「收益估算与优先级」。

其余亮点（协议枢纽转换、账号运行时管理、配额可视化、模型别名表、日志脱敏）kiro-rs
已有等价或更强的实现，不需要借鉴；个别小模式（`[1M]` 后缀上下文变体、每模型默认参数注入、
首启资产自释放）列为低优先级参考，不建议当前投入。

## 二、项目概览

### 2.1 架构

```
客户端（OpenAI / Anthropic 协议）
  -> BridgePool.dispatch            server/pool.go:298
       ├─ injectDefaults            server/pool.go:386   （注入默认 model/context/thinking）
       ├─ pick(attempt)             server/pool.go:250   （跳过 exhaustedUntil 未过期的账号）
       └─ runRequest -> failoverWriter（缓冲，直到首个真实内容才提交）
            ├─ OpenAiBridge.handleChat      server/openai_bridge.go:279
            └─ OpenAiBridge.handleMessages  server/anthropic_handler.go:11
                 └─ prepareQoderUpstream    server/qoder_upstream.go:20
                      └─ BearerApiClient.OpenStreamLines  api/bearer_client.go:75
                           （请求体自定义编码 + 签名 + cosy-* 设备指纹头 -> Qoder SSE）
```

### 2.2 模块职责

| 包 | 文件 | 职责 |
| --- | --- | --- |
| `server` | `pool.go`(630 行) | 账号池、failoverWriter、dispatch 重试环、dashboard 账号 CRUD、配额拉取 |
| `server` | `openai_bridge.go`(1692 行) | OpenAI 协议桥核心：请求构造、流式累加器、工具调用兜底、统一错误类型 |
| `server` | `anthropic_handler.go` / `anthropic.go` | Anthropic 兼容层：入口转内部 ChatRequest，出口按 Anthropic SSE 事件重放 |
| `server` | `models.go` | 模型别名表、反向映射、`[1M]` 上下文变体后缀、live catalog |
| `server` | `dashboard.go` / `dashboard_html.go` / `config.go` | 嵌入式 HTML dashboard + `/api/*` 路由 + `config.json` 持久化（0600） |
| `api` | `bearer_client.go` 等 | 上游客户端：请求签名、自定义编码、SSE 按行回调 |
| `auth` | `signature.go` 等 | cosy 签名（MD5）、PAT→jt 交换、本机 Qoder IDE 凭据读取（AES-CBC） |
| `encoding` | `qoder_encoding.go` | 上游私有线路编码：自定义 base64 字母表 + 三段轮转 |
| `httputil` | `client.go` | 共享 transport：出站代理、可选跳过 TLS 校验 |
| `logx` | `logx.go`(59 行) | 环境变量控级的极简日志 |

## 三、亮点逐项拆解

### 3.1 failoverWriter：首内容提交前的透明故障转移（最有价值）

位置：`server/pool.go:28-160`。

机制：dispatch 不直接把 handler 输出写给客户端，而是套一层 `failoverWriter`：

1. **缓冲阶段**：所有 Write 进入内存 buf，不触碰真实 ResponseWriter；
2. **嗅探提交**：每次写入后 `sniff()` 检测首个「真实内容」信号——OpenAI 流嗅
   `"choices"`、Anthropic 流嗅 `content_block_delta`（`pool.go:72-89`）；命中即 `commit()`，
   此后直通，不再缓冲；
3. **可重试判定**：handler 返回后若尚未提交，`finalize()` 检查缓冲内容是否为可重试错误：
   配额错误（code 115，含俄文 marker 与结构化 SSE error 两种识别路径）或鉴权过期
   （code 105 / "Login expired"），并记录 `retryKind` 与 `limitReset`（`pool.go:107-145`）；
4. **重放**：dispatch 环据此切换账号重放整个请求（`pool.go:298-360`）。auth 类错误还会先
   `refreshSession()` 在**同账号**上重试一次再放弃。客户端全程无感。

配套设计：`markExhausted` 用服务端返回的 `agentLimitResetTime` 作为冷却终点，无则默认
10 分钟（`pool.go:268-277`）；`markAuthExpired` 给 24 小时长冷却（`pool.go:283-288`）；
`pick(attempt)` 从 active 起环形跳过冷却中的账号（`pool.go:250-266`）。

**对照 kiro-rs**：kiro-rs 的故障转移全部发生在 HTTP 状态码阶段
（`src/kiro/provider.rs:363` `call_api_with_retry`：402 额度、401/403 bearer、429/5xx 瞬态），
流一旦开始，上游 EventStream 中的 `Event::Error` 只打日志、**不产出任何 SSE 事件**：

```rust
// src/anthropic/stream.rs:658-664
Event::Error { error_code, error_message } => {
    tracing::error!("收到错误事件: {} - {}", error_code, error_message);
    Vec::new()
}
```

即客户端要么看到流戛然而止（无 `error` 事件、无 stop_reason），要么收到一个看似正常实则
残缺的响应。这是两个独立问题：

- **P1a 流内错误可见性**：`Event::Error` / `Event::Exception` 应映射为协议错误事件
  （Anthropic SSE `error` 事件；OpenAI/Responses 对应的错误帧），而不是 `Vec::new()`。
  投入小，属于缺陷修复性质。
- **P1b 首内容前透明故障转移**：kiro-rs 的「首个内容事件」有结构化判据
  （第一个 `AssistantResponseEvent`），比 Qoder2Api 的字符串嗅探更可靠。在该事件到达前，
  配额/鉴权类流内错误可以触发换凭据重放；一旦产出过内容事件则只能透传错误。
  注意 Kiro 上游是 AWS event-stream 二进制帧且请求成本更高，缓冲窗口应严格限定在
  首内容事件前，并对重放次数设硬上限（复用现有 `MAX_TOTAL_RETRIES` 语义）。

### 3.2 配额重置时间解析与自动恢复冷却

位置：`server/openai_bridge.go:1235` `parseAgentLimitReset`（从错误 message 内嵌 JSON 提取
`agentLimitResetTime` 毫秒时间戳）+ `server/pool.go:268` `markExhausted`。

要点：配额耗尽不是「禁用」，而是「冷却到服务端明确给出的重置时刻」，到期后 `pick()`
自动让账号重新可选，无需人工干预。dashboard 的 `accountsJSON` 同时把
`exhausted_until` 暴露给 UI（`server/dashboard.go:113-115`）。

**对照 kiro-rs**：`report_quota_exhausted`（`src/kiro/token_manager.rs:1801-1843`）对
402 `MONTHLY_REQUEST_COUNT` 的处理是 `disabled = true` + `failure_count` 打满，必须人工
`reset_and_enable`。对**月度**配额而言这大体合理（冷却一个月和永久禁用差别不大），
但该模式的价值在于：

1. Kiro 的错误体可能携带其他限流类型（按分钟/按日的窗口），一刀切永久禁用会让可自动
   恢复的凭据也要人工捞；
2. 「禁用但带到期时间」在 Admin 面板可以显示倒计时与重置时刻，运维语义更准确；
3. `report_quota_exhausted` 目前不读错误体里的任何窗口信息，`default_is_monthly_request_limit`
   （`src/kiro/endpoint/mod.rs:76`）只做布尔判定——结构化解析错误体是自然延伸。

落地前需要验证：Kiro 402 响应体是否真的携带重置时间字段（Qoder 的
`agentLimitResetTime` 是 Qoder 特有设计，不能假设 Kiro 有）。若无窗口字段，可对
非月度限流采用保守默认窗口，月度维持现状。

### 3.3 协议枢纽式转换（hub-and-spoke）

位置：`server/anthropic.go:30` `anthropicToChatRequest` + `server/anthropic_handler.go`。

Anthropic 入口先降级为内部 OpenAI 形态的 `ChatRequest`，与原生 OpenAI 请求共用
`prepareQoderUpstream` 一条上游构造路径；出口再按目标协议重放（OpenAI chunk /
Anthropic content_block_* 事件）。新增协议只需要「入口转换 + 出口重放」两段，
不碰上游逻辑。

**对照 kiro-rs**：kiro-rs 走的是同一思路——Anthropic / OpenAI Chat / Responses 三协议
各自收敛到 Kiro `ConversationState`（见 `docs/multi-protocol-api-design.md`），且请求级
错误已有协议自适应映射（`src/openai/error.rs:89`）。此点已覆盖，无需借鉴；
唯一残留缺口是 3.1 的流内错误渲染。

### 3.4 统一上游错误类型 + 多形态渲染

位置：`server/openai_bridge.go:1173-1313`。`QoderAPIError{Code, Message, ResetAt}` 一个类型
挂三个渲染面：`HTTPStatus()`（115→429，其余→502）、`OpenAIType()`（115→rate_limit_error）、
`Error()`；配合 `writeJSONAPIError` / `writeStreamAPIError` / `writeAnthropicJSONError` /
Anthropic SSE `error` 事件，池耗尽时也能按请求协议输出对应格式的 429
（`server/pool.go:363-384`）。

**对照 kiro-rs**：请求级已有等价物；流内错误渲染缺失（见 3.1）。若做 P1a，可参照该形态：
一个上游错误结构（code + message + 可选 reset 窗口）驱动三种协议渲染，避免在
`anthropic/stream.rs`、`openai/stream.rs`、`responses_stream.rs` 三处各写一份。

### 3.5 dashboard 即控制面：运行时账号管理与每模型默认参数

位置：`server/pool.go:429-525`（AddPAT/RemoveIndex/SelectIndex/SetDefaultModel/SetRuntime）、
`server/pool.go:386`（injectDefaults）、`server/config.go`（config.json 持久化，0600）。

要点：

- 账号增删切换全部运行时生效并即时落盘，自动故障转移也会回写 active 账号
  （`pool.go:238-248` `setActive`），UI 与内部状态始终一致；
- `injectDefaults` 在 dispatch 时为缺省请求注入 default model 与每模型 context/thinking
  设置——「面板设置 → 请求注入」闭环；
- `main.go:62-77` `ensureFiles`：裸二进制首启自动从 go:embed 释放 `.env.example` 与
  baseprompt 模板，开箱即用。

**对照 kiro-rs**：Admin 凭据 CRUD、启用/禁用、优先级、负载均衡模式均已具备且更强
（`src/admin/service.rs`、`token_manager.rs` 的 `set_priority`/`set_load_balancing_mode`）。
差异点只有「每模型默认参数注入」——kiro-rs 没有 `default_model` 或每模型 context/thinking
默认值的请求注入机制。考虑 kiro-rs 客户端（Claude Code 等）通常自带完整参数，此项价值
中等偏低，列为可选。

### 3.6 流式健壮性：pending 缓冲与工具调用文本兜底

位置：`server/openai_bridge.go:1419-1560`（streamAccumulator）、`1662`（parseToolCallsText）。

上游模型偶发把工具调用以 JSON 文本形式吐进 content 时，`isPotentialToolCallText`
（`openai_bridge.go:1540`）让累加器先扣住 pending 文本，判定后再决定按文本下发还是转成
结构化 `tool_calls`；非流式路径同样有 `parseToolCallsText` 兜底。

**对照 kiro-rs**：Kiro 上游有结构化 `tool_use` 事件，不存在该问题，不需要借鉴。
但「对歧义内容先扣住、判定后再下发」与 3.1 的缓冲思想同源，可作为 P1b 设计的参考语汇。

### 3.7 模型别名表 + `[1M]` 上下文变体后缀

位置：`server/models.go:10-82`。`knownAliases`（对外 id → 上游 key）+ `init()` 构建的
`reverseKnown` 反向表（含冲突消解 `preferAlias`）；`stripModel1MSuffix` 支持
`model[1M]` 写法选择 1M 上下文变体（同一模型的不同上下文窗口作为「伪模型名」暴露）。

**对照 kiro-rs**：别名与 catalog 路由已有专门设计（`docs/model-alias-and-catalog-routing-optimization-design.md`）。
「用后缀标记上下文变体」是个小记法，若将来 Kiro 同模型出现多上下文档位可参考，当前无需求。

### 3.8 逆向工程卫生与日志脱敏

- 溯源注释：`Wire format from 1608.har`（`qoder_upstream.go:76`）、`from CLI sniff`
  （`api/cosy_headers.go:12`）——每个逆向常量都标明抓包来源；
- 日志脱敏：`maskPAT`（`pool.go:229-234`，前 6 + … + 后 4）在所有日志路径统一使用。

**对照 kiro-rs**：两项都已覆盖且更强——wire 分析有专门文档
（`docs/codex-responses-lite-wire-analysis.md`、`docs/kiro-rs-eventstream-mapping-analysis.md`），
脱敏有 `mask_api_key`（`src/kiro/token_manager.rs:63`）与 EventStream 脱敏诊断摘要
（`src/anthropic/stream.rs:541`）。维持现状。

## 四、对照总表

| 能力项 | Qoder2Api | kiro-rs | 结论 |
| --- | --- | --- | --- |
| 多凭据池 | PAT 列表 + active 指针 | MultiTokenManager：优先级/负载均衡模式/统计/禁用原因 | kiro-rs 更强 |
| 请求前故障转移 | dispatch 环 + pick 跳过冷却 | call_api_with_retry（402/401/403/429/5xx 分类） | kiro-rs 更强 |
| 流内透明故障转移 | failoverWriter 缓冲-嗅探-重放 | 无 | **借鉴（建议 2）** |
| 流内错误可见性 | 协议化 error 事件 | Event::Error 静默吞掉 | **借鉴（建议 1）** |
| 配额恢复 | reset 时间冷却、自动恢复 | 永久禁用 + 人工 reset_and_enable | **借鉴（建议 3）** |
| 鉴权过期处理 | 会话重建 + 同账号重试，再 24h 冷却 | 401 强制刷新 token 重试（provider.rs:310/568） | 已覆盖 |
| 协议矩阵 | OpenAI + Anthropic | Anthropic + OpenAI Chat + Responses（+ WS ingress） | kiro-rs 更强 |
| 协议错误渲染 | QoderAPIError 多形态 | OpenAiError/ConversionError 映射 | 请求级已覆盖 |
| 运行时账号管理 | /api/accounts 增删切 + 落盘 | Admin CRUD + 优先级 + 启停 | 已覆盖 |
| 配额可视化 | /v1/usage + dashboard quota/exhausted_until | usage_limits + Admin（无倒计时） | 大部分已覆盖 |
| 模型别名/catalog | aliases + reverse + [1M] 后缀 | alias + catalog 设计已落地 | 已覆盖 |
| 每模型默认参数注入 | injectDefaults | 无 | 可选（低优先级） |
| 日志脱敏 | maskPAT | mask_api_key + 脱敏诊断摘要 | 已覆盖 |
| 测试 | **0 个测试文件** | 全量测试 + 告警门禁 | 反面教材 |
| 管理面鉴权 | 无（端口可达即可改账号） | Admin 鉴权中间件 | kiro-rs 更强 |

## 五、借鉴建议（按 ROI 排序）

### 收益估算与优先级

三项建议的收益不在一个量级上，排序为 **1 >> 2（条件） > 3**：

| 建议 | 收益性质 | 收益量级 | 关键依据 |
| --- | --- | --- | --- |
| 1 | 正确性修复：错误流目前被包装成假成功 | **高** | 上游 `Event::Error` 被吞掉后流照常收尾，`get_stop_reason()` 兜底返回 `end_turn`（`src/anthropic/stream.rs:341-347`），客户端收到 `message_stop` 的**成功响应**，内容却截断在上游出错点。Claude Code 等 agent 会当成正常回合结束：不重试、不报错、带着残缺答案继续下游逻辑。这是代理类产品最坏的失效形态——静默截断且无法自愈。频率中低，但每次触发都是脏数据 |
| 2 | 可用性优化：把可见失败变无感恢复 | **待测** | 增量收益只覆盖「200 之后以流内事件到达的凭据类错误」这一子类：状态码级失败（402/401/403/429/5xx）已被 `call_api_with_retry` 覆盖；内容类流内错误（如 `ContentLengthExceededException`）换凭据也救不了。该子类的真实频率目前无法测量——恰恰因为建议 1 没做，错误全被吞了。先做建议 1 让错误可见、积累频率证据，再决定建议 2 是否立项 |
| 3 | 运维减负：配额凭据自动恢复 | **低（当前证据下）** | 全库唯一的凭据禁用触发点是 402 `MONTHLY_REQUEST_COUNT`（`src/kiro/provider.rs:246,483`）；429/408/5xx 已按瞬态处理、不禁用（`provider.rs:593-595`）。因此自动恢复当前能省的只是「每凭据每月一次人工 reset_and_enable」。且 Kiro 402 错误体是否携带重置时间字段未验证——若没有，本建议收益趋近于零，只剩 Admin 倒计时展示的面子价值 |

执行顺序：建议 1 立项实施 → 观察一段时间流内错误的类型与频率 → 凭据类流内错误显著则立项建议 2；
建议 3 独立推进的前提是先抓到真实 402 响应体样本、确认存在窗口字段。

### 建议 1：流内错误可见性（P0，小改，必做）

把 `Event::Error` / `Event::Exception` 映射为协议错误事件而不是 `Vec::new()`：
Anthropic 输出 SSE `error` 事件后正常收尾；OpenAI/Responses 输出各自错误帧。参照
Qoder2Api「一个错误类型、多协议渲染」的形态，错误结构至少携带 code/message，
为建议 3 预留 reset 窗口字段。属于 SSE 协议行为变化，需要 OpenSpec change。

除协议事件外，`generate_final_events()` 的收尾路径（`src/anthropic/handlers.rs:628-644`）
需要区分「正常结束」与「错误截断」：错误发生后不得再以 `end_turn` 收尾。

### 建议 2：首内容事件前的透明故障转移（条件立项，中-大改，缓做）

在流式管线引入「提交点」概念：第一个 `AssistantResponseEvent`（或等价内容事件）产出前，
配额/鉴权类上游错误触发换凭据重放，客户端无感；产出后错误走建议 1 的可见性通道。
设计时注意：

- 判据用结构化事件类型，**不要**学 Qoder2Api 的字符串嗅探（`"choices"` 匹配脆弱且有
  误判面——错误事件 JSON 里也可能出现该词）；
- 重放次数并入现有重试预算，设硬上限；
- 缓冲窗口内的上游响应体要完整消费完再重放，避免连接泄漏；
- count_tokens 等非流式路径不受影响。
- **立项门槛**：建议 1 落地后，统计流内错误中凭据类（配额/鉴权）占比与整体频率；
  若频率低或多为内容类错误，本建议不做。

### 建议 3：配额重置窗口与定时冷却（P2，中改，先验证）

先验证 Kiro 402/限流错误体是否携带重置时间字段；有则解析并落为凭据的
`disabled_until`（到期自动恢复可选），无则对非月度限流给保守默认窗口、月度维持永久禁用。
Admin 面板随之展示 `exhausted_until` 倒计时（Qoder2Api `accountsJSON` 的做法可直接参照）。
涉及 token/多凭据行为，需要 OpenSpec change。

注意当前触发面：只有 402 月度配额会禁用凭据，429/5xx 走瞬态重试不禁用
（`src/kiro/provider.rs:593-595`），所以本建议的自动恢复收益上限有限；
若验证发现 Kiro 还有更短窗口的限流类型（日/小时），优先级再上调。

### 不建议现在做的

- 每模型默认参数注入：客户端自带参数为主，收益不明确；
- `[1M]` 变体后缀记法：Kiro 当前无同模型多上下文档位；
- 首启资产自释放：kiro-rs 的 `*.example.json` 与发布包已覆盖该场景。

## 六、反面教材：不要借鉴的部分

1. **零测试**。全仓库没有一个 `*_test.go`；CodeGraph explore 的 blast radius 报告对每个
   关键符号都打出 `⚠️ no covering tests found`。failoverWriter 这种多状态机逻辑裸奔，
   是 kiro-rs 验证纪律（零告警门禁 + 测试覆盖 + evidence）的反面印证。
2. **字符串嗅探做状态机判据**。`sniff()` 靠 `"choices"` / `content_block_delta` 子串判定
   提交点；`limitErrorMarker` 甚至是俄文本地化文案 `"Лимит агента исчерпан"`
   （`pool.go:18`，`openai_bridge.go:1181`）——用本地化错误文案当故障转移判据，上游换语言即失效。
   kiro-rs 若实现同类机制必须依赖结构化字段（error code / reason）。
3. **硬编码逆向常量无版本策略**。`CosyVersionCLI = "1.1.20"`、`cosy-machineos = "x86_64_win32"`
   伪装 Windows（`api/cosy_headers.go:13,52`）、签名密钥 base64 明文常量
   （`auth/signature.go:11`）。上游 CLI 一升级就需要改代码重新发版。kiro-rs 对上游协议
   常量的处理（分析文档 + 可配置项）应保持。
4. **管理 API 无鉴权、无 body limit**。`/api/accounts/add|remove|select` 对任何能访问端口
   的人开放（`server/dashboard.go:20-31`），单二进制默认 127.0.0.1 缓解了远程风险，但绑定
   地址是可配置的。kiro-rs 的 Admin 鉴权中间件是正确方向，body limit 项已在
   `docs/terax-inspired-optimization-plan.md` 立项，继续推进即可。
5. **PAT 明文落 config.json（0600）**。本地工具的常见取舍，但 Qoder2Api 没有任何
   额外保护。kiro-rs `credentials.json` 同为明文方案，属已知共性风险，非本次借鉴议题。

## 七、附录：CodeGraph 证据与复现命令

索引状态（2026-08-20，`✓ Index is up to date`）：

```text
Project: /home/openclaw/wtf_workspace/github/Qoder2Api
Files: 22   Nodes: 487   Edges: 1,296   DB: 1.39 MB
Nodes: function 153 / import 140 / method 104 / struct 30 / constant 18 / route 13 / variable 7
Language: go 22
```

复现命令：

```bash
codegraph status -p /home/openclaw/wtf_workspace/github/Qoder2Api
codegraph files  -p /home/openclaw/wtf_workspace/github/Qoder2Api --no-metadata
codegraph explore "account pool failover on agent limit error" -p /home/openclaw/wtf_workspace/github/Qoder2Api
codegraph node -p /home/openclaw/wtf_workspace/github/Qoder2Api -f server/openai_bridge.go --symbols-only
```

关键符号定位（Qoder2Api）：

| 符号 | 位置 |
| --- | --- |
| `failoverWriter` / `sniff` / `commit` / `finalize` | `server/pool.go:28-160` |
| `dispatch`（重试环） | `server/pool.go:298-360` |
| `pick` / `markExhausted` / `markAuthExpired` | `server/pool.go:250-288` |
| `injectDefaults` | `server/pool.go:386-415` |
| `QoderAPIError` / `parseQoderStreamError` / `parseAgentLimitReset` | `server/openai_bridge.go:1166-1253` |
| `streamAccumulator` / `isPotentialToolCallText` / `parseToolCallsText` | `server/openai_bridge.go:1407-1692` |
| `anthropicStreamWriter`（thinking/text/tool_use 懒开块） | `server/anthropic_handler.go:225-374` |
| `knownAliases` / `stripModel1MSuffix` | `server/models.go:10-82` |
| `OpenStreamLines`（签名 + 编码 + SSE 行回调） | `api/bearer_client.go:75` |

kiro-rs 对照定位：

| 现状点 | 位置 |
| --- | --- |
| Event::Error 静默吞掉 | `src/anthropic/stream.rs:658-664` |
| 请求前分类重试 | `src/kiro/provider.rs:363`（`call_api_with_retry`） |
| 402 月度配额判定 | `src/kiro/endpoint/mod.rs:76` |
| 配额耗尽永久禁用 | `src/kiro/token_manager.rs:1801-1843` |
| 401 强制刷新重试 | `src/kiro/provider.rs:310,568` |
| 协议错误映射 | `src/openai/error.rs:89` |
