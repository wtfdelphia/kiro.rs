# Design

事实依据：`docs/codex-responses-lite-wire-analysis.md`（wire 实测 4 份 + 权威源码核对）。
本文只写决策与实现结构，不重复事实。

## D1. 方向：一切降级为 function tool

Kiro 上游工具模型只有 `toolSpecification` 与 `cachePoint`（分析 §4.1）。
因此**不追求「完整支持 Codex 全部 ToolSpec」**——该目标在协议层不成立。
照 Kiro IDE 官方归一函数 `Tt4` 的思路，把每种形状降级成普通 function + JSON Schema。

## D2. 归一层位置：`src/openai/responses_tools.rs`（新增）

不改 Chat 端点共享的 `convert_tools` 语义。`custom` / `namespace` / `additional_tools`
都是 Responses 协议概念，Chat Completions 协议没有它们，客户端也不会发。
唯一跨端点改动是 `converter.rs` 补 warn（无行为变化）。

将来若发现有客户端向 Chat 端点发这些形状，再独立评估。

## D3. 状态传递：归一层直接返回，不穿 `prepare`

`prepare`（`src/openai/handlers.rs:103`）是 Chat 端点共享函数，不给它注入 Responses 专属概念。

`to_chat_request_json` 签名改为返回 `(Value, ToolRewriteMap)`：

```rust
pub struct ToolRewriteMap {
    /// 降级前的工具名 -> freeform 标记
    pub freeform: HashSet<String>,
    /// 展平名 -> (namespace, 原名)
    pub namespaces: HashMap<String, (String, String)>,
}
```

由 `post_responses` 接住，绕过 `prepare` 直接交给 `handle_responses_stream` /
`handle_responses_non_stream`。

### D3.1 key 用展平名，不用缩短名

超长工具名的缩短发生在 `prepare` 内部的 `map_tool_name`，但**还原也已经在既有代码里做了**：
`aggregate`（`handlers.rs:335-339`）与流式 `responses_stream.rs:324-326` 都在
产出工具调用前用 `tool_name_map` 把缩短名还原回原始名。

因此响应侧拿到的名字已经是展平名（`collaboration__spawn_agent`），
`ToolRewriteMap` 的 key 就用它，归一层**不需要预测缩短结果**。

映射链条（第一级为既有机制）：

```
上游名 --(tool_name_map，既有)--> 展平名 --(namespaces)--> (namespace, 原名)
```

> 修正记录：本文早期版本写「key 必须用缩短后的名字」，理由是「上游回传的是缩短名」。
> 该判断漏了 `tool_name_map` 的还原步骤，方向是反的——若存缩短名，
> 查找必然失配。实现阶段发现并更正，同时撤回了为预测缩短而做的
> `shorten_tool_name` / `TOOL_NAME_MAX_LEN` 可见性提升（不再需要）。

**仍需锁定测试的理由**：两级都漏则工具调用链断，且失败是静默的
（编译器不报，只表现为客户端拒绝自己的工具调用）。锁定测试见 tasks §7。

## D4. `additional_tools` 提取

`convert_items` 增加 `"additional_tools"` 分支：读 `tools[]` 收集到一个待合并列表，
item 本身不产生任何消息。

### D4.1 归一层统一收 `Value`，不经 `OpenAiTool`

实现时发现的结构约束：`req.tools` 的类型是 `Option<Vec<OpenAiTool>>`，
而 `OpenAiTool` 的手写 `Deserialize`（`src/openai/types.rs:106-154`）只保留
`name` / `description` / `parameters` / `tool_type` 四个字段。
**`custom` 的 `format` 与 `namespace` 的 `tools[]` 在反序列化时即被丢弃**，
归一层拿不到降级所需的信息。

`additional_tools` 位于 `req.input`（`serde_json::Value`，原始 JSON 保真），信息完整。
因此提取出的工具必须以 `Value` 形式直接进 `normalize_tools`，不得先转 `OpenAiTool`。

`req.tools` 侧沿用既有做法：`to_chat_request_json` 本来就把它转回 `Value`
（`responses.rs:65-77`），转换后与 `additional_tools` 的 `Value` 合并，一起喂给归一层。
合并顺序：顶层 `req.tools` 在前，`additional_tools` 在后。

**已知局限**：顶层 `tools` 里若出现 `custom` / `namespace`，其 `format` / 内层 `tools[]`
仍会在 `OpenAiTool` 反序列化时丢失，降级退化为空 schema。
不修的理由：lite 模式下顶层 `tools` 恒为空（4 份抓包确认），
非 lite 模式不会出现 `additional_tools`；顶层直接发 `custom` / `namespace`
属未观察到的情形。修它需要改 `OpenAiTool`（Chat 端点共享路径）或
把 `ResponsesRequest.tools` 换成 `Vec<Value>`（牵动 `should_emulate` 与 websearch 既有测试），
两者都在无真实驱动时扩大回归面。将来观察到该形状再独立评估。

### D4.2 `should_emulate` 不受影响

`should_emulate(tools: Option<&[OpenAiTool]>, ...)`（`websearch.rs:45`）继续只看
`req.tools`。`additional_tools` 提取出的工具**不参与**该判定。
lite 模式下 `req.tools` 恒为 `None`，代执行分支永不触发——
这与「多工具客户端无法使用 web_search 代执行」的结论一致，无需额外处理。

`instructions` 无需特殊处理：lite 模式把它变成 `role: "developer"` 的 message，
而 `converter.rs:32-33` 已把 `developer` 当 system 别名。这条链路本来就是通的
（模型能报出技能名即为证据）。

## D5. `custom` → function 降级

```
{"type":"object",
 "properties":{"input":{"type":"string",
   "description":"The raw input for this tool, passed through verbatim."}},
 "required":["input"]}
```

**description 前插覆盖说明**，化解「别用 JSON」与 JSON `arguments` 的冲突（分析 §2.4）：

```
[Invocation note: call this tool as a normal JSON function call and put the raw
tool input verbatim in the `input` field. The guidance below describes the format
of that raw input, not the shape of this function call.]

<原 description>
```

`format.definition`（lark 文法）追加到 description 末尾，使模型仍能看到语法约束。
上游无 grammar 概念，这是唯一能保留该信息的位置。

## D6. `namespace` 展平

命名 `<namespace>__<name>`，对齐 Codex 自身的 `code_mode_name_for_tool_name`
（`codex-rs/tools/src/code_mode.rs:155-162`）。双下划线可逆——工具名本身含下划线时
（`spawn_agent`）单下划线无法反推分割点。

内层 `tools` 字段名兼容 `children` 回落（sub2api 的 `namespaceChildren` 如此，
实测未见 `children`，成本两行）。

**超长不在归一层截断**：只拼展平名并记逆映射，超长交给下游现有 `map_tool_name`。
归一层不新写第二套哈希规则——两套并存会让同一个名字被截断两次、逆向变三层。

**还原两级**：上游名 →（`tool_name_map`，既有）展平名 →（`namespaces`）`(namespace, 原名)`。
两级都漏则工具调用链断，须有叠加测试（见 D3.1）。

### D6.1 冲突报 400，不自动改名

| 冲突 | 处理 |
| --- | --- |
| 展平名与顶层工具同名 | 400 |
| 两个 namespace 展平到同一名字 | 400 |

错误信息含冲突双方名字，照 sub2api 措辞译：

```
namespace tool "collaboration"/"spawn_agent" flattens to "collaboration__spawn_agent"
which conflicts with a top-level tool of the same name; this upstream cannot
disambiguate them, rename one of the tools
```

**不用随机后缀消歧**：随机后缀会让同一工具在不同轮次得到不同名字，
破坏 `call_id` 之外的工具身份一致性，客户端也无法预期。
冲突是客户端的命名问题，应由客户端改名，代理不该猜。

## D7. 剔除 `encrypted`

递归删除 schema 中所有 `encrypted` 键（分析 §6.2）。作用域仅限本归一层，
**不动** `src/anthropic/converter.rs` 的 `normalize_json_schema`——那在 `/v1/messages`
共享路径上，回归面覆盖全部现有流量，且与官方 `W19` 的其他差异缺本地故障证据。

## D8. 历史 item 改写（与降级同批，不可延后）

只改 `tools` 数组是不够的：客户端把上一轮调用记录放回 `input`，
其中 item type 仍是客户端方言。漏掉这步的后果是第二轮里工具调用与结果双双消失
（Kiro 要求 `tool_use`/`tool_result` 同轮配对），模型以为自己从没调过 `exec`，重复执行。

在 `convert_items` 的 match 里添分支，复用现有逻辑：

| 客户端 item | 归并到 | 附带处理 |
| --- | --- | --- |
| `custom_tool_call` | 现有 `"function_call"` 分支 | `input` → `arguments = {"input": <值>}` |
| `custom_tool_call_output` | 现有 `"function_call_output"` 分支 | `output` 非字符串时 JSON 字符串化 |

归并到现有分支而非另写预处理 pass，可直接复用已有的并行调用合并与 `tool_call_id` 配对逻辑。

`function_call` 分支增强：带 `namespace` 字段时**无条件**拼成展平名。
不检查该 namespace 是否还在本轮 tools 里——历史与当前工具集未必一致
（客户端可能中途改配置），但同一个工具在历史与当下必须同名，否则配对断。

`tool_choice` 改写：`{type:"custom", name:X}` → `{type:"function", name:X}`；
`{type:"namespace"}` → `"auto"`。

## D9. 响应侧还原

### D9.1 非流式

按 `freeform` 集合分派：命中则产出 `custom_tool_call` item（`input` 字段），
否则维持 `function_call`。

`input` 提取规则（顺序敏感，对齐 sub2api `extractCustomToolCallInput`）：

| `arguments` | 返回 |
| --- | --- |
| 空白字符串 | `""` |
| 非法 JSON | **原始字符串整体**（模型直接回裸源码的情况） |
| 合法 JSON 且含 `input` 键 | `input` 的字符串值；`input` 非 string 则返回原始字符串 |
| 合法 JSON、无 `input` 键、空对象 `{}` | `""` |
| 合法 JSON、无 `input` 键、非空 | 原始字符串整体 |

「非法 JSON → 原样返回」这条分支是必需的：`exec` 的 description 明确要求裸源码，
模型很可能照做。

namespace 还原：命中 `namespaces` 则 item 的 `name` 改回原名、增加 `namespace` 字段。

### D9.2 流式：参数必须缓冲到 done

这是最容易漏的一环。`custom_tool_call_input.delta` 的载荷是**已提取的 `input`**，
而 `input` 只有在参数 JSON 完整后才能提取，因此不能逐条改名转发：

| 上游事件 | 处理 |
| --- | --- |
| `output_item.added`（freeform 工具） | item 改 `custom_tool_call`，`input` 置 `""`，清 `arguments` |
| 参数增量事件 | **吞掉不转发**，累加进缓冲区 |
| 参数完成事件 | 提取 `input`，发 `custom_tool_call_input.delta`（非空时）+ `.done` |
| `output_item.done`（freeform 工具） | item 改 `custom_tool_call`，`input` 填提取结果 |

一个上游事件可能对应 0 个或 2 个下游事件。目标序列见分析 §5.3。

**`sequence_number` 不实现**：kiro.rs 的 `ResponsesSseEvent` 当前不发该字段，
Codex 的 `process_responses_event` 也未读取它（分析 §7）。吞/扩事件后无需重排。

> 已由端到端验证确认（2026-07-29）：客户端在不收到 `sequence_number` 的情况下
> 正常接受了 `custom_tool_call` 事件序列并执行了工具。

## D10. `text.format` 只补 warn

上游 `userInputMessageContext` 只有 `toolResults` 与 `tools`（分析 §4.2），
无处透传。**不做 prompt 层模拟**：模拟会把 `strict: true` 降格为「尽力」，
模型可能回 markdown 包裹的 JSON，属假装支持。

`ResponsesRequest` 增 `text` 字段仅为能读到它并 warn，不参与转换。

## D11. 不纳入本次范围

- **`tool_search` proxy**：4 份抓包零出现，无真实驱动。将来遇到再独立立 change
- **`web_search`**：lite 模式下 `hosted_model_tool_specs` 直接返回空
  （`spec_plan.rs:303-305`），hosted 工具根本不发。既有 `should_emulate` 的
  单工具约束在 Codex 场景下必然不满足（一次发 4 个工具），须在 spec 中写明该限制
- **`normalize_json_schema` 对齐官方 `W19`**：见 D7
- **SSE `event:` 行的强约束**：`openspec/specs/openai-responses/spec.md` 比实际客户端
  需求严格（Codex 只读 `data:`），但无害，不动

## D12. 验证策略

### 锁定测试（每项对应一个静默失败模式）

| 目标 | 断言 |
| --- | --- |
| `additional_tools` 提取 | 4 份抓包形状为输入，上游 tools 含全部工具且 schema 非空 |
| 提取后无残留消息 | `additional_tools` item 不产生 user/assistant 消息 |
| `custom` 降级 | schema 为 `{input:string}`；lark 文法出现在 description；覆盖说明在最前 |
| **freeform 集合 key** | **超长 freeform 工具名往返后仍产出 `custom_tool_call`** |
| freeform 参数提取五分支 | 空白 / 非法 JSON / 有 `input` / 空对象 / 无 `input` 非空，逐分支 |
| 含换行与引号的源码往返 | JS 源码含 `\n` 与 `"` 时无损 |
| namespace 展平 | 6 个内层工具独立出现，名字为 `collaboration__*` |
| **namespace 回程还原** | 上游回展平名 → item 的 `name` 为原名、`namespace` 为原 namespace |
| **两级映射叠加** | 超长展平名经 `tool_name_map` 缩短后仍能还原为 `(namespace, 原名)` |
| 冲突报错 | 两类冲突各一测试，断言 400 且错误信息含双方名字 |
| `encrypted` 剔除 | 内层 `message` 属性的 `encrypted` 不出现在上游 schema |
| 历史 item 改写 | `custom_tool_call` → `arguments={"input":…}`；`custom_tool_call_output` → tool 消息 |
| 历史 namespace 拼接 | `function_call{namespace:"collaboration", name:"spawn_agent"}` → 展平名 |
| `tool_choice` 改写 | `{type:"custom"}` → function；`{type:"namespace"}` → `"auto"` |
| **流式缓冲** | 上游参数增量不得透传；`input.delta` 只在参数完成后出现一次 |
| 无 name 工具 | 被丢弃时有 warn，不影响其余工具 |
| `text.format` | 收到时有 warn，请求仍正常处理 |

### 零回归

`cargo test`（`--bin kiro-rs`，本仓库无 lib target）全绿；
`/v1/messages`、`/v1/chat/completions` 既有测试不得转红。

### 端到端

真实 Codex 客户端打通一次「执行命令」，确认模型能实际调用 `exec` 并拿到输出。
单测无法覆盖「客户端是否接受还原后的 item 形状」。

**已完成**（2026-07-29）：见 `evidence/end-to-end-verification.md`。
判定依据是工具调用分派后客户端**发起了下一轮请求**——若还原形状有误，
客户端会拒绝自己的调用而不会有第二轮。

### 诊断日志（实现期新增）

端到端取证时发现 info 级别看不到工具透传结果，完整请求体又只在 debug。
补两条 info 级日志，非临时探针，长期保留：

| 位置 | 内容 | 诊断价值 |
| --- | --- | --- |
| `handlers.rs` `prepare` | 上游工具清单的数量与名字（空时也打） | 工具透传是最易静默失效的环节，客户端可能用 `tools` 字段也可能藏在 `additional_tools` 里 |
| `handlers.rs` `build_tool_call_item` / `responses_stream.rs` | 工具调用的分派结果（item 类型 + namespace） | 分派错误直接导致客户端拒绝执行，且无其他外部可见症状 |

两者都只打名字与类型，不含 schema、参数或指令内容。
