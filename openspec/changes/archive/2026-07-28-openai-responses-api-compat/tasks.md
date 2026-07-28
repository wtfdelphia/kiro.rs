## 1. 类型与导出

- [x] 1.1 `src/openai/responses_types.rs`：`ResponsesRequest` / `ResponsesObject` / `ResponseOutputItem` / `ResponseContentPart` / `ResponsesUsage` / `ResponsesError`
- [x] 1.2 `src/anthropic/mod.rs` 追加 websearch 相关 `pub(crate)` 导出（不改实现）
- [x] 1.3 `src/anthropic/websearch.rs`：`call_mcp_api` / `create_mcp_request` / `parse_search_results` / `generate_search_summary` 提 `pub(crate)`
- [x] 1.4 verify：`cargo build` 无新警告

## 2. input 归一（responses.rs）

- [x] 2.1 三种顶层形状分派（string / array / object）
- [x] 2.2 item 类型分派：message / function_call / function_call_output / input_text / input_image / output_text / 带 role 兜底
- [x] 2.3 pending user parts 累积与 flush 机制
- [x] 2.4 连续 function_call 合并进同一 assistant 消息
- [x] 2.5 `instructions` → system；`model` 缺省默认值
- [x] 2.6 `previous_response_id` 非空 → 400；`store` 忽略
- [x] 2.7 校验：归一为空 / 无 user 上下文 → 400
- [x] 2.8 单测：design §10 前 9 行
- [x] 2.9 verify：`cargo test openai::responses`

## 3. 非流式响应

- [x] 3.1 `build_responses_object`：message item + function_call items
  - 命名注记：实现未抽出该名字的函数，改为在 `handlers.rs:1097 handle_responses_non_stream` 内联构造（items 组装见 `:1145-1157`）
- [x] 3.2 工具名还原；usage 优先 contextUsage 回落估算；metadata 回显
- [x] 3.3 单测：output 结构、工具名、usage 优先级
  - 覆盖方式注记：非流式构造路径无直接单测（无测试调用 `handle_responses_non_stream`）；断言落在 `responses_types.rs` 的序列化形状测试与流式侧 `responses_stream.rs`，另由真实凭据 curl 覆盖（`live-upstream-verification.md` §1，2026-07-28 复测 HTTP 200 且 output items 为 `['message']`/`['web_search_call','message']`）
- [x] 3.4 verify：`cargo test`

## 4. 流式语义事件（responses_stream.rs）

- [x] 4.1 `ResponsesStreamContext`：created / in_progress / completed 骨架
- [x] 4.2 message item：output_item.added → content_part.added → output_text.delta → content_part.done → output_item.done
- [x] 4.3 function_call item：added(in_progress) → function_call_arguments.delta → done(completed)
- [x] 4.4 文本后接工具调用时先关闭 message item，output_index 自增
- [x] 4.5 `response.failed` 路径
- [x] 4.6 SSE 格式含 `event:` 行；`[DONE]` 收尾；保活用注释行
- [x] 4.7 单测：完整事件顺序、index 管理、SSE 格式、failed
- [x] 4.8 verify：`cargo test openai::responses_stream`

## 5. web_search（websearch.rs）

- [x] 5.1 `is_web_search_tool` 宽判定（type 前缀 / google_search / name 三值，大小写不敏感）
- [x] 5.2 拦截条件：恰好一个 tool 且命中；混合工具不拦截
- [x] 5.3 `extract_search_query`：取最后一条 user 文本，不剥 Anthropic 前缀
- [x] 5.4 非流式：web_search_call item + message item
- [x] 5.5 流式：完整事件序列
- [x] 5.6 搜索失败 / 无结果的明确表达
- [x] 5.7 usage 用本地估算（不经上游 generate）
- [x] 5.8 单测：判定矩阵、查询提取、两种输出形态、失败路径
- [x] 5.9 verify：`cargo test openai::websearch`

## 6. 运行时开关

- [x] 6.1 `src/model/config.rs` 增加 `webSearchEmulation`（默认 true）
- [x] 6.2 Admin `GET/PUT /api/admin/settings/websearch`（热更新 + 落盘 + 鉴权）
- [x] 6.3 关闭时 web_search 走正常 tools 路径
- [x] 6.4 单测：默认启用、关闭后不拦截、未认证 401
- [x] 6.5 admin-ui 设置面板增加开关
- [x] 6.6 verify：`cargo test admin` + `pnpm build`
- [x] 6.7 `specs/admin-runtime-settings/spec.md`：MODIFIED「设置变更安全与校验」+ ADDED「Admin 可读写 web 搜索代执行开关」（核验补漏，见 openspec-verify-report 发现 1）
- [x] 6.8 单测锁定新增 Scenario：响应无无关密钥、Anthropic 侧判定不受开关影响

## 7. handler 与路由

- [x] 7.1 `post_responses`：复用 Phase B 的 `prepare`（D8 四项）
- [x] 7.2 web_search 分支前置判定（在 prepare 之前，避免无谓转换）
- [x] 7.3 `/responses` 挂到既有 `v1_routes`（自动继承三个 layer）
- [x] 7.4 单测：auth 矩阵与 body limit 扩展到新路径
- [x] 7.5 单测：thinking 后缀锁定（新端点同样适用）
- [x] 7.6 verify：`cargo test openai`

## 8. Catalog 与展示

- [x] 8.1 `openai.responses` status 改 Live；`retrieve` 仍 planned
- [x] 8.2 `client_hints` 更新：web_search 支持、两端点判定差异（D11）、无状态说明
- [x] 8.3 catalog 单测同步（live 数量、planned 范围）
- [x] 8.4 verify：`cargo test public_api` 防漂移双向断言通过

## 9. 端到端与门禁

- [x] 9.1 本地 curl：非流式基础对话（真实凭据下已验证，见 evidence/live-upstream-verification.md）
- [x] 9.2 本地 curl：流式语义事件（真实凭据下已验证，见 evidence/live-upstream-verification.md）
- [x] 9.3 本地 curl：`previous_response_id` 得 400
- [x] 9.4 本地 curl：web_search 开/关对比
- [x] 9.5 回归：`/v1/messages`、`/cc/v1/messages`、`/v1/chat/completions`、`/v1/models`
- [x] 9.6 启动日志确认新增端点
- [x] 9.7 Admin 面板确认 responses 显示为「可用」
- [x] 9.8 `openspec validate --all`
- [x] 9.9 `cargo build` + `cargo test` 全量
- [x] 9.10 `git status --short` 无密钥文件
- [x] 9.11 README 同步新增端点与开关
- [x] 9.12 `docs/multi-protocol-api-design.md` 状态更新
- [x] 9.13 evidence/ 落盘真实命令输出
