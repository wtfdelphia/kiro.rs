# Requirements（长期需求事实）

## 产品定位

kiro-rs 是用 Rust 编写的 Anthropic Claude API 兼容代理服务，将 Anthropic API 请求转换为 Kiro API 请求。

## 核心能力（当前已存在）

- Anthropic Messages API 兼容（`/v1`）
- Claude Code 兼容端点（`/cc/v1`，缓冲模式以修正 input_tokens）
- OpenAI Chat Completions 兼容（`/v1/chat/completions`）
- OpenAI Responses 兼容（`/v1/responses`，无状态；可选 web 搜索代执行）
- 对外端点注册表：状态（live/planned）与真实路由双向防漂移，经 Admin 只读展示
- SSE 流式响应
- OAuth Token 自动刷新
- 多凭据：优先级 / 均衡负载、故障转移、多凭据格式下 token 回写
- Thinking / tool use / WebSearch 转换
- 多模型映射（Sonnet / Opus / Haiku 等）
- 可选 Admin API 与嵌入式 Admin UI（需 `adminApiKey`）
- 多级 Region 与凭据级代理

## 业务边界

- 本项目为研究用途代理，不代表 AWS / Kiro / Anthropic 官方
- 客户端认证使用配置的 `apiKey`（x-api-key 或 Bearer）
- 上游认证依赖用户提供的 Kiro 相关 OAuth 凭据

## 非目标

- 不实现完整的多租户 SaaS 控制面
- 不在仓库内保存或分发真实用户凭据
- 不替代官方 Claude / Kiro 客户端的全部产品能力
- 不在未建立 OpenSpec change 的情况下进行跨模块/高风险行为变更

## 质量与协作需求

- 高风险变更（协议、凭据、认证、Admin、模型映射、Docker/发布）必须可规格化、可验证、可审计
- AI 辅助开发必须遵循 `AGENTS.md` 与 OpenSpec 门禁
