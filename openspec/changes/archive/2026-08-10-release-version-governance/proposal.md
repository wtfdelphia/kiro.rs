## Why

项目自 2026-03-30 后连续五个正式版本未同步 `Cargo.toml`，导致 Release 文件名和镜像 tag 已前进，而二进制 `--version` 仍报告 `2026.3.1`。当前发布身份还分散在 Cargo、git tag、workflow 输入和镜像标签中，默认分支 `main` 与发布分支 `master` 也已分叉，单靠人工清单无法阻止再次漂移或人工发布绕过。

## What Changes

- 将 `Cargo.toml [package].version` 定义为源码版本声明源，将匹配的附注 `vYYYY.M.D` tag 定义为正式发布身份；同一自然日只允许一个正式版。
- 新增 reusable version gate，在正式二进制和镜像构建前校验 tag、Cargo 版本、tag 类型和稳定分支可达性；失败时阻止所有产物 job 并给出修复指引。
- 将 `main` 统一为稳定发版落点；先以普通合并收敛 `main/master` 独有提交，再把正式 workflow 的稳定分支触发器从 `master` 迁至 `main`。
- 收紧人工镜像发布：`publish=true` 只能从当前提交的唯一附注 `v*` tag 推导版本，不能使用自由输入绕过正式版本身份；`publish=false` 继续作为无副作用 dry run。
- 让正式版本在 `--version`、启动首条 info 日志、Release 资产名、镜像 tag 和 OCI `org.opencontainers.image.version` 中一致。
- 将 `rust-version = "1.97.1"` 声明为项目 MSRV，并在该版本上验证默认与无默认 feature 两种锁定检查。
- 保留 `dev-latest`、分支 artifact 和 dry-run dispatch 的非正式语义；它们不要求外部标签等于 Cargo 版本，但必须通过 CI/Release 元数据追溯到 commit。
- 在 README 和 OpenSpec 长期规格中记录版本规则、发布清单、admin-ui 不独立版本化策略和验证要求。

## Capabilities

### New Capabilities

- `release-version-governance`: 规定正式版本身份、CalVer/tag 规则、稳定发布分支、机器门禁、人工发布约束、运行时与镜像版本可观测性、非正式构建追溯和 MSRV 契约。

### Modified Capabilities

无。现有 `build-warning-hygiene` 的告警门禁和人工发布默认 dry-run 契约保持不变；本 change 新增的版本身份约束与其并列。

## Impact

- 配置与代码：`Cargo.toml`、`Cargo.lock`、`src/main.rs`。
- CI/发布：新增 `.github/workflows/version-gate.yaml`；修改 `build.yaml` 与 `docker-build.yaml`；不修改 `build-dev-release.yaml` 和 `warning-gate.yaml`。
- 容器：`Dockerfile` 与 OCI 版本 label。
- 分支治理：实施前需由维护者以非破坏方式把 `master` 独有提交纳入 `main`；不强推、不删除分支。
- 文档与规格：`README.md`、`docs/version-governance-optimization-design.md`、新 capability delta spec；归档后同步到 `openspec/specs/release-version-governance/`。
- 外部验证：CI 红路径临时 tag 由维护者创建和删除并提供两个 Actions run URL，实施代理只核验证据。
- 不影响 Anthropic/Kiro/OpenAI 协议、认证、凭据、Admin API 或 admin-ui 构建/内嵌机制。
