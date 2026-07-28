## 1. 模块骨架与导出

- [x] 1.1 `src/main.rs` 加 `mod openai;`
- [x] 1.2 `src/openai/mod.rs`：模块声明 + `create_openai_routes` 占位
- [x] 1.3 `src/anthropic/mod.rs` 追加 `pub(crate)` 导出（design §2 清单）；`MAX_BODY_SIZE` 提 `pub(crate)`
- [x] 1.4 verify：`cargo build` 通过，无新警告

## 2. 请求类型（types.rs）

- [x] 2.1 `ChatCompletionRequest` / `ChatMessage` / `StreamOptions` / `OpenAiToolCall`
- [x] 2.2 `OpenAiTool` 手写 `Deserialize`：Chat 嵌套 + Responses 顶层双形状
- [x] 2.3 响应类型（`ChatCompletion` / `ChatCompletionChunk` / `Usage`）
- [x] 2.4 单测：双形状解析等价；未知字段被忽略；`content` 为 null/string/array 三态
- [x] 2.5 verify：`cargo test openai::types`

## 3. 请求映射（converter.rs）

- [x] 3.1 `to_messages_request`：system/developer 合并、user/assistant 文本
- [x] 3.2 content parts：text part、image data URL → `ImageSource`、远程 URL 跳过并 warn
- [x] 3.3 `assistant.tool_calls` → tool_use block（arguments 解析失败退化为 `{}` 并 warn）
- [x] 3.4 `tool` role → tool_result block，连续多条归集到同一 user 消息
- [x] 3.5 `tools[]` → Anthropic `Tool`；`max_tokens` / `max_completion_tokens` 优先级 + 默认 64000
- [x] 3.6 校验：messages 空 / 无 user 消息 → 400
- [x] 3.7 `temperature` / `top_p` 接受但不透传
- [x] 3.8 单测：design §10 前 6 行全部覆盖
- [x] 3.9 verify：`cargo test openai::converter`

## 4. 错误方言（error.rs）

- [x] 4.1 `OpenAiError` 枚举 + `{error:{message,type}}` 序列化
- [x] 4.2 映射表（design §7）：400 / 502 / 503 / 500
- [x] 4.3 单测：error shape 非 Anthropic shape；无凭据信息泄漏
- [x] 4.4 verify：`cargo test openai::error`

## 5. 流式状态机（stream.rs）

- [x] 5.1 `OpenAiStreamContext`：首块 role、文本增量、finish_reason 末块、`[DONE]`
- [x] 5.2 tool_calls 增量：首次带 id+name+index，后续仅 arguments 片段；index 稳定
- [x] 5.3 thinking 标签跨 chunk 检测 → `reasoning_content`，不污染 content
- [x] 5.4 `include_usage` 为 true 时追加 `choices:[]` + usage chunk（false 时不发）
- [x] 5.5 ping 保活用 SSE 注释行，不发伪 chunk
- [x] 5.6 单测：给定 Event 序列断言完整 chunk 序列；多工具并发 index 稳定；include_usage 开关
- [x] 5.7 verify：`cargo test openai::stream`

## 6. handler（handlers.rs）

- [x] 6.1 `post_chat_completions`：design §8 六步前置逻辑（provider / 映射 / thinking / convert / tokens / thinking_enabled）
- [x] 6.2 非流式：聚合 Event，`finish_reason` 三态、`prompt_tokens` 优先 contextUsage、工具名还原
- [x] 6.3 非流式 thinking：`extract_thinking_from_complete_text` → `reasoning_content`
- [x] 6.4 流式：`OpenAiStreamContext` + SSE 响应
- [x] 6.5 model 回显原始请求值（D9）
- [x] 6.6 **不抄 websearch 分支**（D10）
- [x] 6.7 单测：thinking 锁定、tool_name_map 锁定、model 回显、web_search 不劫持
- [x] 6.8 verify：`cargo test openai::handlers`

## 7. 路由挂载

- [x] 7.1 `create_openai_routes`：`/v1/chat/completions` + **三个 layer**（auth / cors / body limit）
- [x] 7.2 `src/main.rs` merge 挂载，位置在 admin nest 之前
- [x] 7.3 单测：auth 矩阵（requireApiKey 开/关 × 有/无 key）
- [x] 7.4 单测：body limit 大于 2MB 不返回 413
- [x] 7.5 verify：`cargo test` + curl 无 key 得 401

## 8. Catalog 状态切换

- [x] 8.1 `openai.chat.completions` status 改 Live
- [x] 8.2 `client_hints` 复核（OPENAI_BASE_URL、model 回显、include_usage、无 web_search）
- [x] 8.3 catalog 单测同步（`test_expected_live_set` 数量、`test_openai_endpoints_planned` 范围）
  - 时点注记：`test_openai_endpoints_planned` 已在 Phase C 被替换为 `catalog.rs:294 test_chat_completions_live` 与 `:303 test_responses_live_retrieve_still_planned`；`test_expected_live_set` 现断言 `live.len() == 7`（本任务完成时为 6）
- [x] 8.4 verify：`cargo test public_api` 全绿，防漂移双向断言通过

## 9. 端到端与门禁

- [x] 9.1 本地 curl：非流式基础对话（真实凭据下已验证，见 evidence/live-upstream-verification.md）
- [x] 9.2 本地 curl：`stream:true` 流式（真实凭据下已验证，见 evidence/live-upstream-verification.md）
- [x] 9.3 本地 curl：include_usage 开/关对比（真实凭据下已验证，见 evidence/live-upstream-verification.md）
- [x] 9.4 本地 curl：function tools 主路径（真实凭据下已验证，见 evidence/live-upstream-verification.md）
- [x] 9.5 Anthropic 回归：`/v1/messages`、`/cc/v1/messages`、`/v1/models` 行为不变
- [x] 9.6 启动日志确认新增 6 条 live 端点
  - 时点注记：本任务完成时 live 共 6 条；Phase C 挂载 `/v1/responses` 后现为 7 条（实测启动日志与 `GET /api/admin/public-api` 均为 7 live / 1 planned）
- [x] 9.7 Admin 面板确认 chat completions 显示为「可用」
- [x] 9.8 `openspec validate --all`
- [x] 9.9 `cargo build` + `cargo test` 全量
- [x] 9.10 `git status --short` 无密钥文件
- [x] 9.11 README 同步新增端点
- [x] 9.12 `docs/multi-protocol-api-design.md` 状态更新（Phase B 标已完成）
- [x] 9.13 evidence/ 落盘真实命令输出
