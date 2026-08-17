## 1. 请求侧归一（responses_tools.rs）

- [x] 1.1 namespace 分支接受 `child_type == "custom"` 的内层工具：校验 name 非空，按 `flatten_namespace_name` 生成展平名，产出 `{type:"function", name: 展平名, description: freeform_description(child_obj), parameters: freeform_schema()}`
- [x] 1.2 展平名同时记入 `rewrite.namespaces`（`(ns, child_name)`）与 `rewrite.freeform`
- [x] 1.3 冲突检查与 function 内层共用同一路径：与顶层工具冲突、与其他展平名冲突均返回 400，错误消息格式不变
- [x] 1.4 内层 custom 缺少 name 时输出含 namespace 的 warning 并丢弃该工具；丢弃告警收窄到 function/custom 之外的未知形状

## 2. 测试

- [x] 2.1 responses_tools 单测：内层 custom 展平并降级——名字为 `<ns>__<name>`，schema 为单个必填字符串属性 `input`，description 含调用约定说明、原描述与文法定义，且不触发丢弃告警
- [x] 2.2 responses_tools 单测：两级映射注册——展平名同时存在于 `freeform` 与 `namespaces`，逆映射值为 `(ns, child_name)`
- [x] 2.3 responses_tools 单测：内层 custom 的展平名与顶层工具冲突、与另一展平名冲突，均返回 400 且消息含冲突双方
- [x] 2.4 responses_tools 单测：内层 custom 缺少 name 时留痕丢弃，其余内层工具不受影响
- [x] 2.5 handlers 非流式测试：上游以展平名回传调用时，输出 item 为 custom-tool-call（带 `input`），name 为内层原名且带 `namespace` 字段；若暴露组合缺口，做最小修复并记录
- [x] 2.6 responses_stream 流式测试：同上组合形状，且 freeform 参数缓冲语义不变（增量不透传、完成时一次性发出）；若暴露组合缺口，做最小修复并记录
- [x] 2.7 convert_items 回放测试：input 含带 `namespace` 的 `custom_tool_call` item 时，归一为展平名的 assistant 工具调用（对齐既有 function_call 的 namespace 展平行为）

## 3. 验证与收尾

- [x] 3.1 `cargo test` openai 相关模块全部通过
- [x] 3.2 `cargo check --release --all-targets` 零新增告警
- [x] 3.3 `openspec validate --all` 通过
- [x] 3.4 真实流量回归：以含 namespace + custom 工具的客户端请求（Codex app）验证 pm2 日志不再出现该丢弃告警、`工具已送达上游` 含展平名；有调用回传时核对输出 item 形状（claude-tap trace 可作旁证）
