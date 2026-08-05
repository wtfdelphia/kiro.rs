## 1. 抽取共享块构造

- [x] 1.1 新增 `build_websearch_blocks(query, tool_use_id, results) -> (Vec<Value>, String)`
- [x] 1.2 `generate_websearch_events` 改为调用它拆分成 SSE 事件（事件序列与字段不变）
- [x] 1.3 verify：现有流式相关单测全绿（回归保护）

## 2. 非流式路径

- [x] 2.1 `handle_websearch_request` 读取 `payload.stream` 并分派
- [x] 2.2 非流式：构造 Anthropic message 对象（四个块 + usage + stop_reason + model 回显）
- [x] 2.3 usage 含 `server_tool_use.web_search_requests`
- [x] 2.4 verify：`cargo build`

## 3. 测试

- [x] 3.1 `stream: false` → content-type 为 JSON，响应体为 message 对象
- [x] 3.2 `stream: true` → 仍为 SSE（回归）
- [x] 3.3 非流式四个块的类型与顺序
- [x] 3.4 两路径块内容逐字段相等
- [x] 3.5 usage 字段完备、`stop_reason` 为 `end_turn`、model 回显原值
- [x] 3.6 无结果时摘要明确、结果列表为空
- [x] 3.7 查询无法提取时两种模式均 400
- [x] 3.8 verify：`cargo test websearch`

## 4. 门禁

- [x] 4.1 `openspec validate --all`
- [x] 4.2 `cargo build` + `cargo test` 全量
- [x] 4.3 端到端：`stream:false` 与 `stream:true` 各跑一次 curl
- [x] 4.4 回归：`/v1/messages`、`/cc/v1/messages`、`/v1/chat/completions`、`/v1/responses`
- [x] 4.5 `git status --short` 无密钥文件
- [x] 4.6 README 说明 web_search 两种模式
- [x] 4.7 `docs/multi-protocol-api-design.md` 移除该项待办
- [x] 4.8 evidence/ 落盘真实命令输出
