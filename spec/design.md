# Design（长期架构事实）

## 架构风格

单进程 Rust 异步服务：Axum 提供 HTTP 路由；内部将 Anthropic 请求转换为 Kiro 协议，经 TokenManager 选择凭据并出站调用；响应再转换回 Anthropic SSE/JSON。

## 模块边界

| 模块 | 路径 | 职责 |
| --- | --- | --- |
| 入口 | `src/main.rs` | 配置加载、组装 Provider/TokenManager、挂载路由 |
| 配置 | `src/model/` | `Config`、CLI Args |
| Anthropic 兼容层 | `src/anthropic/` | 路由、鉴权中间件、handlers、协议转换、流式、WebSearch |
| Kiro 客户端 | `src/kiro/` | provider、token_manager、endpoint、parser、凭据模型 |
| Admin | `src/admin/` | 凭据管理 API |
| Admin UI 嵌入 | `src/admin_ui/` | 静态资源路由（构建自 `admin-ui/`） |
| 公共 | `src/common/` | 认证工具等 |
| HTTP 出站 | `src/http_client.rs` | 客户端构建（代理/TLS） |
| 前端工程 | `admin-ui/` | Vite + pnpm 管理界面 |

## 关键数据流

1. 客户端 -> Anthropic 兼容路由（API Key 校验）
2. handlers/converter 构造 Kiro 请求
3. MultiTokenManager 选择凭据并确保 access token 可用
4. provider/endpoint 出站调用 Kiro
5. parser 解析 event stream
6. stream/converter 输出 Anthropic SSE 或 JSON

## 安全与机密

- `config.json` / `credentials.json` 仅本地，git 忽略
- 仓库仅提供 `*.example.json`
- Admin 能力由非空 `adminApiKey` 启用
- AI/文档中禁止粘贴真实 token

## 构建与测试策略

- 后端：`cargo test` / `cargo build --release`（release 前需 admin-ui build 以嵌入静态资源，见 README）
- 前端：`cd admin-ui && pnpm install && pnpm build`
- Docker：docker-compose / Dockerfile / CI workflows
- AI 变更：OpenSpec validate + 对应高风险验证矩阵（见 `AGENTS.md`）

## CodeGraph

本地索引目录 `.codegraph/`（不入库）。用于入口、调用链、影响面、候选测试发现；配置/Docker/脚本/密钥路径必须再用 rg 与源码精读补盲。
