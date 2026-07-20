# kiro-rs OpenSpec Project

## 项目

kiro-rs：Rust/Axum 实现的 Anthropic Claude API 兼容代理，转发并转换为 Kiro API。

## 技术栈

- Rust edition 2024、Axum 0.8、Tokio、Reqwest、Serde
- Admin UI：Vite + pnpm（`admin-ui/`）
- 容器：Docker / docker-compose

## 事实源优先级

1. 当前 `openspec/changes/<change>/` 工件（单次变更范围与验收）
2. `AGENTS.md`（AI 纪律与高风险矩阵）
3. `spec/`（长期需求/设计/结构）
4. `README.md`（人类启动与使用）
5. 源码与测试

## 约束

- 不提交真实凭据与本地 config
- 高风险变更必须走 OpenSpec change
- 实现前输出 Bridge Plan；完成前真实验证
- 详情见仓库根目录 `AGENTS.md`

## 常用验证

- `openspec validate --all`
- `cargo test`
- `codegraph status` / `codegraph impact "<symbol>"`
- `rg` 补盲配置/Docker/脚本
