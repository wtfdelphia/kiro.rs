# Design: ai-engineering-baseline

## 当前实现

无 AGENTS/spec/openspec/skills（接入前）；gitignore 曾忽略 AGENTS/CLAUDE；CodeGraph 本地已索引但未约定。

## 目标设计

见 docs/superpowers/specs/2026-07-20-ai-engineering-baseline-design.md 目录与组件设计。

## CodeGraph 影响面

- 查询命令：codegraph status
- 入口：无运行时入口变更
- 调用链：无
- 影响面：文档与规则文件
- 候选测试：路径存在性、openspec validate、git status

## 盲区补充

- rg：.gitignore、example 凭据、Dockerfile、workflows
- 确认无真实 credentials 被 add

## 异常路径

- openspec validate 失败：根据错误修工件后再继续
- 工作区脏且含用户密钥文件：停止并人工确认

## 回滚策略

删除新增规则/规格/skills；恢复 gitignore 相关行。

## 验证策略

openspec validate --all；文件树检查；git status；codegraph status；四门禁 evidence。
