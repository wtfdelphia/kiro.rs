# 工具来源与核验记录

核验日期：2026-07-20
环境：Windows / PowerShell / 本机全局 CLI

| 工具 | 来源 | 核验版本 | 用途 | 不应提交 |
| --- | --- | --- | --- | --- |
| OpenSpec | https://github.com/Fission-AI/OpenSpec | 1.4.0 | 规格驱动与变更归档 | token、本机缓存 |
| CodeGraph | https://github.com/colbymchenry/codegraph | 0.9.8 | 本地代码图谱与影响面 | `.codegraph/` |
| ripgrep | https://github.com/BurntSushi/ripgrep | 14.0.3 | 文本补盲 | 无 |
| Node.js | 本机运行时 | 25.0.0 | 运行 OpenSpec / CodeGraph CLI | 无 |
| Rust / Cargo | 本机工具链 | 1.94.1 | 构建与测试 | `target/` |
| pnpm | 本机 | 11.1.3 | admin-ui 构建 | `admin-ui/node_modules/` |
| ECC | https://github.com/affaan-m/ECC | 仅参考，不安装 | rules / skills 结构借鉴 | 用户级配置、密钥 |
| Karpathy skills | https://github.com/multica-ai/andrej-karpathy-skills | 仅参考 | 行为纪律项目化 | 未裁剪外部配置 |

## 安装核验命令（示例）

```bash
openspec --version
codegraph --version
rg --version
node -v
rustc --version
cargo --version
pnpm -v
```

## 项目内入口

- AI 规则：`AGENTS.md`
- 长期事实：`spec/`
- 单次变更：`openspec/changes/<change-name>/`
- 白皮书：`docs/AI 辅助开发工程化落地白皮书.md`
- 设计规格：`docs/superpowers/specs/2026-07-20-ai-engineering-baseline-design.md`
- 实现计划：`docs/superpowers/plans/2026-07-20-ai-engineering-baseline.md`

## 企业网络说明

如需代理或镜像安装 CLI，仅写在个人环境或私有运维文档；不要把个人代理账号、token 写入本仓库。
