## Why

Codex app 的 Responses 请求里，`namespace` 工具的内层子工具并不全是 `function` 形状：真实流量（2026-08-17 pm2 日志，`functions` 命名空间）已经出现 `custom`（freeform）子工具。当前归一层只接受 `function` 内层，`custom` 子工具被丢弃并留 warning，导致客户端注册的该工具从上游视野中消失——模型无法调用它，且客户端无任何报错可查。顶层 `custom` 已有完整降级支持，namespace 内层缺这一半。

## What Changes

- namespace 的内层 `custom` 子工具不再丢弃：复用顶层 custom 的 freeform 降级（单 `input` 属性 schema + 调用约定说明 + 文法定义追加），以展平名 `<namespace>__<name>` 送达上游。
- 展平名同时记入 freeform 集合与 namespace 逆映射，使响应侧还原同时命中两级映射：输出为 custom-tool-call（带 `input`），且名字还原为原名并携带 `namespace` 字段。
- 内层 `custom` 的命名冲突检查与 `function` 内层一致：与顶层工具或其他展平名冲突时返回 400。
- 历史回放侧无需改动：`custom_tool_call` 输入 item 的 namespace 展平已由现有分派分支覆盖。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `openai-responses`: 「namespace 工具展平与命名冲突」扩展为接受内层 custom 形状；「客户端方言工具的响应侧还原」明确 namespace 与 freeform 两级映射叠加的还原行为。

## Impact

- 代码：`src/openai/responses_tools.rs`（normalize_tools 的 namespace 分支）；流式与非流式输出侧的还原逻辑已具备组合能力，预期零改动或仅补齐测试。
- 协议面：Responses 端点的工具归一行为；对既有 function 内层、顶层 custom、命名冲突语义无回归。
- 可观测性：`namespace 内层工具形状非 function，已丢弃` 告警仅对 custom 之外的未知形状保留。
