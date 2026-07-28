# Structure（目录与归属）

```text
kiro-rs/
├── src/                    # Rust 服务端源码
│   ├── main.rs
│   ├── anthropic/          # Anthropic API 兼容
│   ├── openai/             # OpenAI 兼容（Chat Completions / Responses）
│   ├── public_api/         # 对外端点注册表（单一事实源 + 防漂移断言）
│   ├── kiro/               # Kiro 上游与解析
│   ├── admin/              # Admin API
│   ├── admin_ui/           # 嵌入 UI 路由
│   ├── model/              # 配置与参数
│   └── common/             # 公共工具
├── admin-ui/               # 前端工程（构建产物嵌入二进制）
├── tools/                  # 辅助工具（非运行时核心）
├── docs/                   # 专题文档、白皮书、工具来源、superpowers
├── spec/                   # 长期需求/设计/结构事实
├── openspec/               # OpenSpec 项目与变更
├── .codex/skills/          # 项目内 AI skills 门禁
├── AGENTS.md               # AI agent 通用规则
├── CLAUDE.md               # Claude Code 入口
├── README.md               # 人类入口
├── Cargo.toml
├── Dockerfile
├── docker-compose.yml
└── *.example.json          # 配置/凭据示例
```

## 配置文件归属

| 文件 | 归属 | 是否入库 |
| --- | --- | --- |
| `config.example.json` | 示例 | 是 |
| `credentials.example.*.json` | 示例 | 是 |
| `config.json` | 本地运行 | 否 |
| `credentials.json` | 本地运行 | 否 |
| `.codegraph/` | 本地图谱 | 否 |
