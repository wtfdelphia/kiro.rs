## 1. 前置：可见性与类型

- [x] ~~1.1 `src/anthropic/converter.rs`：`shorten_tool_name` 与 `TOOL_NAME_MAX_LEN` 提 `pub(crate)`~~
  **已撤回**：任务组 7 实现时发现 `tool_name_map` 的还原发生在响应侧分派之前
  （`handlers.rs:335-339` / `responses_stream.rs:324-326`），`ToolRewriteMap` 的 key 应用展平名而非缩短名，
  归一层无需预测缩短。两处可见性已恢复私有，见 design D3.1 修正记录
- [x] ~~1.2 `src/anthropic/mod.rs`：追加对应 `pub(crate)` 导出~~ **已撤回**（同上）
- [x] 1.3 `src/openai/responses_types.rs`：`ResponsesRequest` 增 `text` 字段（仅为可读到并 warn，不参与转换）
- [x] 1.4 `src/openai/responses_types.rs`：`ResponseOutputItem` 增 `input` / `namespace` 字段（`skip_serializing_if`）与 `custom_tool_call` 构造
  - 实现注记：`ResponseOutputItem` 的三个既有构造用全字段字面量，已同步补 `input: None` / `namespace: None`；另加 `with_namespace` 链式方法供响应侧还原
- [x] 1.5 verify：`cargo build` 通过。`TOOL_NAME_MAX_LEN` / `shorten_tool_name` 的 unused 警告为预期（任务组 2 消费），其余警告均为既有

## 2. 归一层骨架（新增 responses_tools.rs）

- [x] 2.1 `ToolRewriteMap { freeform: HashSet<String>, namespaces: HashMap<String,(String,String)> }`
- [x] 2.2 `normalize_tools(tools: Vec<Value>) -> Result<(Vec<Value>, ToolRewriteMap), OpenAiError>`：五形状分派入口
- [x] 2.3 `strip_encrypted`：递归删除 schema 中的 `encrypted` 键（D7）
- [x] ~~2.4 `upstream_visible_name`：复用 `shorten_tool_name` 预测上游可见名~~
  **已撤回**：key 改用展平名后不再需要预测缩短（见 1.1 与 design D3.1）
- [x] 2.5 单测：`encrypted` 剔除（含嵌套 properties 与数组）；展平名双下划线
- [x] 2.6 verify：`cargo test --bin kiro-rs openai::responses_tools` → 通过
  - 实现注记：同批写入 `extract_custom_input`（五分支，供任务组 7 复用）与 `rewrite_tool_choice`（供 6.4），
    因它们与降级规则同属一个内聚单元，拆开会让 `freeform_schema` 等常量跨模块暴露

## 3. additional_tools 提取（D4）

- [x] 3.1 `src/openai/responses.rs`：`to_chat_request_json` 签名改为返回 `(Value, ToolRewriteMap)`
- [x] 3.2 `convert_items` 增 `"additional_tools"` 分支：收集 `tools[]`，item 本身不产生消息
  - 连带：`parse_input` / `convert_items` 签名改为回传 `(messages, tools)`
  - 连带：`_ =>` 分支在 `build_message` 返回 `None` 时补 warn（`additional_tools` 曾因此静默消失）
- [x] 3.3 合并顺序：顶层 `req.tools` 在前，`additional_tools` 在后
  - **设计修正**：`additional_tools` 必须走 `Value` 保真路径，不经 `OpenAiTool`（其手写 `Deserialize` 会丢弃 `custom.format` 与 `namespace.tools[]`）。已更新 design D4.1/D4.2 与 spec 场景
- [x] 3.4 更新调用点：`handlers.rs:728`（测试）、`handlers.rs:832`（生产）、`responses.rs` 测试辅助 `chat()`；
  其余 5 处为 `unwrap_err()` / `is_ok()`，不解构返回值，无需改
- [x] 3.5 单测 6 项：真实抓包形状（含 custom + namespace + encrypted）工具全部到达（5 个）且 schema 非空；
  `additional_tools` 不产生消息；顶层与 input 工具合并顺序；无工具时不写 `tools` 字段；`text.format` 不致请求失败
- [x] 3.6 verify：`cargo test --bin kiro-rs openai::responses::` → 34 passed（含既有 28 项零回归）

## 4. custom 降级（D5）

- [x] 4.1 schema 固定为 `{input: string}`
- [x] 4.2 description 前插调用约定覆盖说明
- [x] 4.3 `format.definition`（lark 文法）追加到 description 末尾，带 syntax 名
- [x] 4.4 工具名记入 `freeform` 集合，**key 用工具原名，不缩短**（design D3.1）
- [x] 4.5 单测 6 项：schema 形状；覆盖说明在最前且原描述保留；lark 文法存在；
  **锁定测试：超长名时集合 key 仍为原名**；无 name 时丢弃；无 format 时仍降级
- [x] 4.6 verify：`cargo test --bin kiro-rs openai::responses_tools` → 10 passed

## 5. namespace 展平（D6）

- [x] 5.1 内层 `tools` 提升为顶层，名字 `<namespace>__<name>`；`children` 字段名回落
- [x] 5.2 逆映射记入 `namespaces`，key 用展平名，**不缩短**（design D3.1）
- [x] 5.3 冲突检测：展平名与顶层同名、两个 namespace 展平同名 → 400，错误信息含双方名字与 `rename` 建议
- [x] 5.4 单测 8 项：展平为独立工具且无空壳残留、内层 `encrypted` 剔除、`children` 回落、
  两类冲突各一（断言 400 + 双方名字）、同一工具重复去重不报错、
  **锁定测试：逆映射 key 为展平名（超长也不缩短）**、namespace 缺 name 时丢弃、无名工具丢弃不影响其余
- [x] 5.5 verify：`cargo test --bin kiro-rs openai::responses_tools` → 29 passed
  - 同批完成 `extract_custom_input` 五分支测试（7 项，含换行引号无损）与 `rewrite_tool_choice` 测试（3 项），
    对应任务 6.4 与 7.2/7.5 的单测部分

## 6. 历史 item 与 tool_choice 改写（D8）

- [x] 6.1 `custom_tool_call` 归并到现有 `"function_call"` 分支：`input` → `arguments={"input":…}`
- [x] 6.2 `custom_tool_call_output` 归并到 `"function_call_output"` 分支：output 非字符串时 JSON 字符串化（复用既有 `stringify`）
- [x] 6.3 `function_call` 分支：带 `namespace` 时无条件拼展平名（`custom_tool_call` 同样适用）
- [x] 6.4 `tool_choice` 改写：`custom` → function；`namespace` → `"auto"`（在 `to_chat_request_json` 内接入）
- [x] 6.5 单测 12 项：input 包装、换行引号无损、output 转 tool 消息、非字符串 output 字符串化、
  null output 为空串、namespace 拼接（`function_call` 与 `custom_tool_call` 各一）、无 namespace 保留裸名、
  **调用与结果成对存活**、tool_choice 三形状
- [x] 6.6 verify：`cargo test --bin kiro-rs openai::responses::` → 46 passed

## 7. 响应侧还原（D9.1，非流式）

- [x] 7.1 `build_tool_call_item`：按 `freeform` 集合分派 item 类型（非流式与流式共用）
- [x] 7.2 `extract_custom_input`：五分支提取（顺序敏感，实现与单测在任务组 2/5 完成）
- [x] 7.3 namespace 还原：`name` 改原名 + 增 `namespace` 字段（`with_namespace`）
- [x] 7.4 `ToolRewriteMap` 从 `post_responses` 传入两个 handler（绕过 `prepare`）
  - 实现注记：新增 `ResponsesContext` 收拢 `echo_model` / `instructions` / `metadata` / `tool_rewrite`，
    两个 handler 的 5 参数签名收为 3 个；`ResponsesStreamContext::new` 增第 8 参数（3 处测试构造点已补 `Default::default()`）
- [x] 7.5 单测：普通工具不变、freeform 转 `custom_tool_call`、裸源码透传、换行引号无损、
  两项还原叠加、序列化形状（不出 `arguments` / 无 namespace 时不出该字段）
- [x] 7.6 **锁定测试：超长 freeform 工具名往返后仍产出 `custom_tool_call`**
- [x] 7.7 **锁定测试：超长展平名的两级映射叠加，还原为 `(namespace, 原名)`**
- [x] 7.8 verify：`cargo test --bin kiro-rs openai::handlers` → 31 passed

## 8. 响应侧还原（D9.2，流式）

- [x] 8.1 `responses_stream.rs`：freeform 工具的参数缓冲状态机（复用既有 `tool_buffers`，按 name 查 `freeform` 集合判定）
- [x] 8.2 `output_item.added` / `.done`：item 改 `custom_tool_call`（added 时 `input` 为空串，不带 `arguments`）
- [x] 8.3 参数增量吞掉不转发；stop 时发 `custom_tool_call_input.delta`（非空）+ `.done`
- [x] 8.4 namespace 还原（`restore_name`，added / done / 非流式同规则）
- [x] 8.5 单测 7 项：**锁定测试：上游参数增量不得透传**、`input.delta` 只出现一次且序列为
  added → input.delta → input.done → item.done、item 形状（added/done）、裸源码提取、
  普通工具仍透传增量（零回归）、namespace 在 added 与 done 都还原、finish 收尾未完成的 freeform 工具
- [x] 8.6 verify：`cargo test --bin kiro-rs openai::responses_stream` → 27 passed

## 9. 可观测性（D10 + warn）

- [x] 9.1 `src/openai/converter.rs`：丢弃工具时 `warn` 带 name 与 type（唯一跨端点改动，无行为变化）
  - 连带：`OpenAiTool.tool_type` 的 `#[allow(dead_code)]` 可去（已有多处生产使用）
- [x] 9.2 归一层丢弃工具时 `warn`（非对象、custom/namespace 缺 name、内层形状非 function、无名工具，共 5 处）
- [x] 9.3 收到 `text.format` 时 `warn` 声明不支持（`to_chat_request_json` 入口）
- [x] 9.4 单测：Chat 侧无名工具丢弃不影响其余 + 全部无名时返回 None；
  归一层同类断言见 5.4；`text.format` 不致失败见 3.5
- [x] 9.5 verify：`cargo test --bin kiro-rs openai::` → 239 passed

## 10. 收尾验证

- [x] 10.1 `cargo test --bin kiro-rs` → **563 passed; 0 failed**
- [x] 10.2 零回归确认：`anthropic::` 103 / `openai::converter` 20 / `openai::stream` 22 / `kiro::` 172，全部 0 failed；
  编译警告 10 条，与改动前基线一致（新增的 `ToolRewriteMap::is_empty` 未被使用，已删除）
- [x] 10.3 `openspec validate --all` → 18 passed, 0 failed
- [x] 10.4 端到端：**通过**，证据见 `evidence/end-to-end-verification.md`
  - `工具已送达上游 count=9`（含 6 个 `collaboration__*`）；`item_type="custom_tool_call"`；
    分派后 1 秒内客户端发起下一轮且工具仍为 9 个 → 工具链未断
  - 连带新增两条 info 级诊断日志（非临时探针，长期保留）：
    `handlers.rs` 上游工具清单、`handlers.rs`/`responses_stream.rs` 工具调用分派结果
- [x] 10.5 移除临时探针（`grep "wire capture" src/` 无命中）；清理 `wire-capture/`
  - ~~剩余：`kiro_release/kiro-rs.exe` 仍是 debug 探针版，原版备份在 `kiro-rs.exe.bak-wire`（**待用户处理**，需停进程）~~
  - **已消解**（2026-07-29 合规审查核实）：`kiro_release/` 与 `wire-capture/` 均已不存在；
    `src/` 中 `dbg!` / `eprintln!` / `println!` / `wire capture` / `TODO` / `FIXME` 零命中
- [x] 10.6 `git status --short`：7 个修改文件 + `responses_tools.rs` + 文档与 change，无凭据、无 `.codegraph/`；
  `src/anthropic/` 未出现在改动列表（1.1/1.2 的可见性提升已完整撤回）
- [x] 10.7 `spec-compliance-check` → `openspec-verify-change` → `verification-before-completion`
  - `spec-compliance-check` → **PASS**，见 `evidence/spec-compliance-report.md`
  - `openspec-verify-change` → 见 `evidence/openspec-verify-report.md`
  - `verification-before-completion` → 待最终回复前执行
  - 计数校正（合规审查实跑）：10.1 全量为 **570 passed**（另两个 change 同期新增测试所致）；
    10.3 `openspec validate --all` 现为 **20 passed**；5.5 的 `responses_tools` 实为 **27 passed**（原写 29，累加笔误）
