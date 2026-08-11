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

## 零新增编译告警（硬性）

任何代码实现都不得引入新的编译告警，**与是否走 OpenSpec 流程无关**——包括下方「可豁免」的拼写/注释/单行修复。

- 判定命令：`cargo check --release --all-targets`（本项目唯一准绳；`cargo build --release` 漏测试目标告警，门槛更松）
- 消除手段限于修正真实问题：移动导入、删除死代码、把测试专用符号收敛到 `#[cfg(test)]`
- 禁止 crate/module 级 `#![allow(dead_code)]`；禁止为消除告警构造假数据
- 确有正当保留理由时，用最小范围 `#[allow(...)]` 并在紧邻位置注明理由
- 提交前若告警数高于变更基线，视为未完成

### 两道防线

- **第一防线（本地，非强制）**：`scripts/git-hooks/pre-push`，装法 `git config core.hooksPath scripts/git-hooks`。跑原始准绳命令，有告警则拒绝推送。需一次性配置才生效，新克隆默认未启用，`--no-verify` 可绕过
- **第二防线（CI，硬失败）**：`.github/workflows/warning-gate.yaml`，挂在三条流水线的产物构建之前。判定命令与准绳逐 flag 一致，额外附加 `-D warnings` 与 `--locked`，并覆盖 default 与 `--no-default-features` 两种组合。门禁红则不产出 release、镜像与 manifest
- 门禁工具链**钉版**（`dtolnay/rust-toolchain@1.97.1`），而 7 腿产物与 Docker **浮动 stable 且不带 `-D warnings`**：Rust 的兼容承诺不覆盖「不产生新告警」，若发布路径升级告警，编译器发版即可在零代码变更下中断发布
- 告警**计数**始终取自原始准绳命令；门禁只回答「有无告警」，其提前中止不作为计数依据

背景与首轮清零方案见 `docs/release-build-warnings-cleanup-design.md`；两道防线的完整论证见 `docs/warning-gate-two-line-defense-design.md`。

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
| 起草提交信息 / PR 标题 / squash 信息 | caveman-commit |

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
| CI / 告警门禁 | `warning-gate.yaml` 与三条流水线接线审查；YAML 可解析；绿路径与**红路径**都要有 run 证据（只验证绿路径不能证明门禁会拦） |
| admin-ui | `pnpm build`（及已有测试） |
| OpenSpec | `openspec validate --all` |
| 任意代码改动 | `cargo check --release --all-targets` 无新增告警（CI 门禁在此基础上附加 `-D warnings` 与 `--locked`） |

## 验证纪律

- 只报告本会话真实运行过的命令与结果
- 未运行必须写原因与剩余风险
- 代码改动必须报告 `cargo check --release --all-targets` 的告警数，并确认无新增
- 不隐藏失败
- 不粘贴真实 token、账号、Cookie
- 完成前 `git status --short`，防止密钥与 `.codegraph/` 误入

## 提交信息纪律

提交信息、PR 标题、squash 信息统一走 `caveman-commit` skill（`.codex/skills/caveman-commit/SKILL.md`，`.claude/skills/` 有等价镜像）。客户端不支持 skill 时按该文件规则等价产出。

要点（完整规则见 skill）：

- Conventional Commits：`<type>(<scope>): <祈使式摘要>`，摘要用中文祈使式，代码标识符保持原文
- 长度按显示宽度计（CJK 算 2 宽度）：目标 ≤50，硬上限 72；结尾不加句号
- scope 复用既有词表，不新造同义词
- 正文默认省略，只写非显而易见的 why；breaking change、安全修复、schema 迁移、revert、告警门禁/发布路径/凭据相关改动必须写正文
- 禁止「本次提交做了 X」式复述、emoji、在正文里叙述 AI 参与过程
- 提交信息、PR 标题、squash 信息不写 `Assisted-by`、`Co-Authored-By`、`Co-authored-by` 等 AI 归属；真人共同作者的 `Co-authored-by` trailer 按实际协作保留

## README / AGENTS / spec 同步纪律

- 影响启动、构建、部署、测试、API 入口、AI 纪律、验证命令时必须同步对应入口
- 单次变更过程只写在 `openspec/changes/<name>/`
- 无需更新时最终报告说明原因

## 安全

- 忽略并永不提交：`config.json`、`credentials.json`、`credentials.*`
- 示例仅用 `*.example.json`
- 文档与 PR 中禁止真实密钥
