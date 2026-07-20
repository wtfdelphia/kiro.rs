# AGENTS.md

## 回答语言

默认使用中文回答。

## 项目上下文

- 名称：kiro-rs
- 定位：Rust/Axum Anthropic Claude API 兼容代理，将请求转换为 Kiro API
- 技术栈：Rust 2024、Axum 0.8、Tokio、Reqwest、Serde、tracing；Admin UI 为 Vite + pnpm
- 关键入口：`src/main.rs`、`src/anthropic/`、`src/kiro/`、`src/admin/`、`src/model/`
- 长期事实：`spec/`
- 单次变更：`openspec/changes/<change-name>/`
- 工具来源：`docs/tooling-sources.md`
- 白皮书：`docs/AI 辅助开发工程化落地白皮书.md`

## AI 协作纪律（Karpathy）

- Think Before Coding：不静默猜测需求、接口、凭据、协议或发布影响；多解时先列假设或提问
- Simplicity First：只做当前规格范围内的最小改动
- Surgical Changes：只改与当前 change/task 直接相关的文件；不顺手重构或无关格式化
- Goal-Driven Execution：先定义成功标准与验证命令；未实际运行不得声称通过

一句话：不确定就先澄清；能简单就不抽象；非本任务不改；没有证据不说完成。

## OpenSpec 条件

以下必须先建立 OpenSpec change：

- 新业务能力或跨模块变更
- Anthropic/Kiro 协议、SSE 流式、转换逻辑
- Token 刷新、多凭据、负载均衡
- API Key / 认证中间件
- Admin API 或凭据管理
- 模型映射
- Docker / 发布 / CI 部署脚本
- 配置 schema 行为变化
- 大范围重构

可豁免但仍须遵守验证纪律：纯拼写、注释、单行且无行为变化的修复。

## Skills 门禁

优先使用 `.codex/skills/`。客户端不支持时必须等价输出同名证据。

OpenSpec 官方 init 已提供：`openspec-propose`、`openspec-apply-change`、`openspec-archive-change`、`openspec-explore`、`openspec-sync-specs`。

项目补充门禁（必须遵循或等价产出证据）：

| 场景 | Skill |
| --- | --- |
| 新建变更 | openspec-new-change 或 openspec-propose |
| 开始实现前 | openspec-superpowers-bridge |
| 实现后/审查前 | spec-compliance-check |
| 归档前 | openspec-verify-change |
| 最终回复/PR/归档/合并前 | verification-before-completion |

## CodeGraph

用于入口、调用链、影响面、候选测试。不替代 OpenSpec、rg、源码精读与测试。

常用：`codegraph status|context|query|callers|callees|impact|sync`

补盲：配置、Docker、脚本、示例凭据路径、CI、运行时注入。

## 高风险检查矩阵

| 变更类型 | 推荐验证 |
| --- | --- |
| 协议 / SSE | `cargo test` 相关模块；必要时本地 curl（禁用真实密钥入库） |
| Token / 多凭据 | token_manager 相关测试；example 配置完整性 |
| 认证 / API Key | auth/middleware 测试 |
| Admin / 凭据 CRUD | admin 测试；禁止真实凭据 |
| 模型映射 | 映射/converter 相关测试 |
| Docker / 发布 | Dockerfile、compose、workflows 审查 |
| admin-ui | `pnpm build`（及已有测试） |
| OpenSpec | `openspec validate --all` |

## 验证纪律

- 只报告本会话真实运行过的命令与结果
- 未运行必须写原因与剩余风险
- 不隐藏失败
- 不粘贴真实 token、账号、Cookie
- 完成前 `git status --short`，防止密钥与 `.codegraph/` 误入

## README / AGENTS / spec 同步纪律

- 影响启动、构建、部署、测试、API 入口、AI 纪律、验证命令时必须同步对应入口
- 单次变更过程只写在 `openspec/changes/<name>/`
- 无需更新时最终报告说明原因

## 安全

- 忽略并永不提交：`config.json`、`credentials.json`、`credentials.*`
- 示例仅用 `*.example.json`
- 文档与 PR 中禁止真实密钥
