## Why

OpenAI Chat Completions 是覆盖面最广的客户端协议（OpenAI SDK、Cursor、OpenWebUI、LiteLLM 等），kiro.rs 目前完全不支持：只有模型别名映射（`gpt-4o` → Claude，`converter.rs builtin_compat_aliases`），没有路由、类型或格式转换。这是 `docs/multi-protocol-api-design.md` 里的 P1。

Phase A 已把 `/v1/chat/completions` 登记进 Public API Catalog 并标为 `planned`，Admin 面板显示「未启用」，请求返回 404。本 change 把它变成 `live`。

底层可 100% 复用：`resolve_model`、`convert_request_with_policy`、`KiroProvider`、`EventStreamDecoder`。新增工作集中在协议类型、请求映射、响应/流转换、handler、路由五处。

设计输入：`docs/multi-protocol-api-design.md`（§6 Chat Completions、§8 路由挂载、§10 决策 D1/D8/D9/D12），经 Kiro-Go（`proxy/handler.go:1568 handleOpenAIChat`、`translator.go:1155 OpenAIToKiro`、`translator.go:1096 OpenAITool.UnmarshalJSON`）源码对照复核。

## What Changes

- **新增 `POST /v1/chat/completions`**：流式 + 非流式，含 function tools
- **新增 `src/openai/` 模块**：types / converter / stream / error / handlers，与 `src/anthropic/` 平级、互不侵入
- **请求映射走适配层**（D1）：OpenAI → 内存 `MessagesRequest` → 复用 `convert_request_with_policy`，不写第二套直达 Kiro 转换器
- **双形状 tool 反序列化**：同时接受 Chat 的 `{type,function:{name,parameters}}` 与 Responses 的顶层 `{type,name,parameters}`（对齐 `translator.go:1096`，也是 Phase C 复用本转换器的前提）
- **实现 `stream_options.include_usage`**（D12）：末尾追加 `choices: []` 且带 usage 的 chunk，再发 `[DONE]`
- **OpenAI error shape**：与 Anthropic 端点的 error shape 严格分家
- **路由挂载补齐三个 layer**（§8）：`auth_middleware`、`cors_layer()`、`DefaultBodyLimit::max(50MB)` —— `merge` 不传播 layer
- **catalog 中 `openai.chat.completions` 改 Live**：由 Phase A 建立的 `live ⊆ routes` 单测强制路由已挂载
- **不改任何 Anthropic 行为**：`/v1/messages`、`/cc/v1/messages`、`/v1/models` 的 handler、SSE 事件、鉴权模型零改动

## Capabilities

### New Capabilities

- `openai-chat-completions`：`POST /v1/chat/completions` 的请求契约、映射规则、非流式响应、流式 chunk 序列、usage 语义、错误方言、鉴权与 body 限制

### Modified Capabilities

无。`public-api-catalog` 能力（Phase A）尚未归档进 `openspec/specs/`，因此 `openai.chat.completions` 的 `planned → live` 状态变更以本能力内的「与端点注册表状态一致」需求表达。

补充（2026-07-28 verify 复核）：原先设想「待 Phase A 归档后由 `openspec-sync-specs` 统一收敛」**不成立**——Phase A/B/C 各自的 `specs/` 目录互不包含对方的能力文件，sync 没有可覆盖的目标。实际处置是让每个能力只断言自己持有的端点状态：Phase A 的 spec 不再断言 OpenAI 端点的具体状态，本 change 的 spec 不再断言 Responses 的状态。详见各自 `evidence/openspec-verify-report.md`。

## Impact

- **代码**：新增 `src/openai/{mod,types,converter,stream,error,handlers}.rs`；`src/main.rs`（`mod openai;` + merge 挂载）；`src/public_api/catalog.rs`（status 改 Live）；`src/anthropic/mod.rs`（按需 `pub(crate)` 导出 `convert_request_with_policy`、`override_thinking_from_model_name`、`get_context_window_size`、`extract_thinking_from_complete_text`）
- **API**：新增对外端点 `POST /v1/chat/completions`。无既有端点变更
- **配置**：无新增配置项
- **风险类型**：协议 / SSE（强制 OpenSpec）、认证 / API Key（新路由必须自带 auth layer）、模型映射（复用 `resolve_model`）
- **非目标**：
  - 不实现 `/v1/responses`（Phase C 独立 change）
  - 不支持服务端 `web_search` 工具（仅 Responses 端点提供，见 D10）；名为 `web_search` 的普通 function tool 走正常 tools 路径
  - 不实现 `logprobs`、`n>1`、`seed`、`presence_penalty`、`frequency_penalty`、`stop`、`logit_bias`
  - 不做路径别名 `/chat/completions`（D5）
  - 不改 Anthropic 侧 `override_thinking_from_model_name` 的调用方式（D8：不抽共享前置层）
  - 不重复注册 `/v1/models`
  - 不实现 `thinking` 的标签格式输出（默认 `reasoning_content`，标签格式后置）
