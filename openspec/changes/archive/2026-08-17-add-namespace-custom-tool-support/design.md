## Context

工具归一入口是 `src/openai/responses_tools.rs` 的 `normalize_tools`。现状三个事实（见 proposal.md - Why 的动机，此处只列约束）：

1. namespace 分支对内层子工具只接受 `type == "function"`（或空 type），其余形状告警丢弃；`custom` 子工具因此消失。
2. 顶层 `custom` 已有完整 freeform 降级：`freeform_description`（调用约定说明 + 原描述 + `format.definition` 文法）+ `freeform_schema`（单个必填字符串属性 `input`），并把名字记入 `ToolRewriteMap.freeform`。
3. `ToolRewriteMap` 的两个映射相互独立，输出侧已分别消费并可组合：
   - 非流式 `handlers.rs::build_tool_call_item`：先按 freeform 决定 custom-tool-call 形状，再按 namespaces 逆映射 `with_namespace` 还原名字。
   - 流式 `responses_stream.rs::close_tool_item`：freeform 命中走 `close_freeform_tool_item`，其收尾同样调用 `restore_name` 并写回 `item.namespace`。

历史回放侧（`responses.rs::convert_items`）的 `"function_call" | "custom_tool_call"` 分支对两者统一做 namespace 展平；`custom_tool_call_output` 按 call_id 配对。均无需改动。

## Goals / Non-Goals

**Goals:**

- namespace 内层 `custom` 子工具以展平名 + freeform 降级送达上游，并记入两级映射。
- 响应侧对这类工具的调用还原为「custom-tool-call + 原名 + namespace」的组合形状。
- 冲突检查、超长截断、可观测性与内层 `function` 完全一致。

**Non-Goals:**

- 不支持 namespace 内层除 function/custom 之外的其他形状（继续告警丢弃）。
- 不改动顶层 custom、顶层 function、`additional_tools` 承载等既有路径。
- 不改动 Anthropic 端点的工具归一。

## Decisions

- **D1：复用顶层 freeform 降级，注册展平名到两级映射。** namespace 分支遇到 `child_type == "custom"` 时：校验 name 非空（空则告警丢弃），`flat = flatten_namespace_name(ns, child_name)`，执行与 function 子工具相同的冲突检查，随后 `rewrite.namespaces.insert(flat, (ns, child_name))`、`rewrite.freeform.insert(flat)`，产出 `{type:"function", name: flat, description: freeform_description(child_obj), parameters: freeform_schema()}`。
  - 备选：为 namespace 内 custom 设计独立映射——拒绝。输出侧两个映射已能组合，新增映射只增加维护面。
  - 备选：把内层 custom 原样透传——拒绝。上游只接受命名 function 工具（claude-tap 实采与生产流量双重证据）。
- **D2：冲突语义与 function 内层完全一致。** 复用 `top_level` 集合与 `flattened_seen` 的两段检查，错误消息格式不变，spec 的 400 场景自然覆盖。
- **D3：告警面收窄但不清零。** 丢弃告警仅保留给 function/custom 之外的未知内层形状；「缺少 name」告警对 custom 内层同样适用（带 namespace 字段）。符合既有「工具丢弃必须留痕」要求。
- **D4：输出侧预期零改动，以测试验证组合行为。** 流式与非流式的两级映射组合在代码上已成立；任务里以新增测试固化该行为，若测试暴露缺口再做最小修复。
- **D5：freeform 固定 schema 不参与 `strip_encrypted`。** encrypted 标记只可能出现在客户端提供的 JSON Schema 中，freeform schema 是本端常量，无需处理；`format.definition` 仅进描述文本。

## Risks / Trade-offs

- [真实客户端对「namespace + custom」调用的回传形状只观察到请求侧，响应侧组合是按两级已验证映射推导] → 用流式/非流式两级测试覆盖组合路径；实现后以 Codex app 真实流量回归（claude-tap 已在链路中，可直接比对 trace）。
- [展平名超长时与短名映射叠加] → 既有「两级映射叠加」场景与 `tool_name_map` 机制已覆盖，本变更不引入新截断路径，测试沿用同一构造。
- [告警文案变化影响既有日志检索习惯] → 保留原句式主干，仅收窄触发条件；变更说明写入 tasks 的验证项。

## Migration Plan

纯转换层行为扩展，无配置、无数据迁移。发布即生效；回滚即回退二进制。验证门槛：`cargo test`（openai 模块）与 `cargo check --release --all-targets` 零新增告警。
