# Capability: release-version-governance

## Purpose

Bind the project's formal release identity to a single machine-enforced source of truth. `Cargo.toml [package].version` is the source declaration; a matching annotated `vYYYY.MM.MICRO` tag is the formal release identity, where the third segment is the release sequence within the month rather than a calendar day. `main` is the only stable release landing point, and every formal artifact surface -- binary `--version`, the first startup info log, Release asset names, image tags, and the OCI `org.opencontainers.image.version` label -- MUST express that same version.

Enforcement happens before any artifact is produced. A read-only reusable version gate validates tag format, month range, tag object type, Cargo consistency, and reachability from `origin/main`, running in parallel with the warning gate so a version error never waits on compilation. Manual image publishing cannot bypass the identity: `publish=true` derives the version from the single annotated `v*` tag on the current commit, while `publish=false` stays a side-effect-free dry run. Informal builds (branch artifacts, `dev-latest`, dry runs) stay traceable to a commit but are never required to match the Cargo version, and are never presented as formal releases.

## Requirements

### Requirement: 正式版本必须具有唯一且一致的发布身份

项目 MUST 以 `Cargo.toml [package].version` 作为源码版本声明，以匹配的附注 git tag 作为正式发布
身份。正式 tag MUST 严格使用 `vYYYY.MM.MICRO`，其中 `YYYY` 为四位年份，`MM` 为 1 至 12 的月份，
`MICRO` 为该年月内的发布序号（从 1 开始）。月份与序号 MUST NOT 补零，去掉 `v` 后 MUST 与 Cargo
版本完全一致。正式版本 MUST NOT 使用修订后缀。

第三段是**当月发布序号，不是日历日**。同一年月内 MUST 允许发布多个正式版本，其序号 MUST 严格
递增且 MUST NOT 复用。因此同一自然日 MAY 发布多个正式版本。

> 纠正说明：本要求早期版本曾将第三段定义为日历日（`vYYYY.M.D`）并禁止同日发布第二个正式版。该
> 定义源于只抽样最近 6 个 tag 的误判：项目全部历史 tag（含 `2025.12.1`–`2025.12.7` 在 4 天内发布、
> `2026.2.1`–`2026.2.3` 同日发布）表明原约定始终是当月序号。`v2026.7.27` 起第三段与日期重合属
> 一天一发的巧合。历史 tag MUST NOT 被追溯改写。

#### Scenario: 合法正式版本通过身份校验

- **WHEN** 当前提交具有唯一附注 tag `v2026.8.11`，Cargo 版本为 `2026.8.11`，且提交可从稳定发布分支到达
- **THEN** 版本身份门禁 MUST 通过

#### Scenario: 同月第二个正式版本被接受

- **WHEN** 同一年月内已存在正式版本 `v2026.8.11`，新提交带附注 tag `v2026.8.12` 且 Cargo 版本为 `2026.8.12`
- **THEN** 版本身份门禁 MUST 通过
- **AND** MUST NOT 因两者属于同一自然日或同一月份而拒绝

#### Scenario: 序号无需对应日历日

- **WHEN** 正式 tag 的第三段不是有效日历日（如 `v2026.2.30`）但为合法序号，且其余身份检查通过
- **THEN** 版本身份门禁 MUST 通过
- **AND** MUST NOT 报告「非法日历日期」

#### Scenario: Cargo 与 tag 漂移时拒绝发布

- **WHEN** 正式 tag 去掉 `v` 后与 Cargo 版本不一致
- **THEN** 版本身份门禁 MUST 失败并输出具体修复指引
- **AND** 发布产物构建、Release、镜像与 manifest MUST NOT 启动或创建

#### Scenario: 非法 CalVer 或轻量 tag 被拒绝

- **WHEN** tag 含修订后缀、月份或序号补零、月份超出 1-12、序号为 0，或不是附注 tag
- **THEN** 版本身份门禁 MUST 在构建前失败

### Requirement: 发布序号必须由维护者显式决定

版本身份门禁 MUST 只校验单个 tag 自身的合法性与其提交的 `origin/main` 可达性，MUST NOT 依据远端
历史 tag 列表推导或强制「序号等于上一个序号加一」。序号的选择 MUST 由维护者在创建 tag 时显式决定。

跳号 MUST NOT 被视为错误。同名 tag 的唯一性由 git 保证。

#### Scenario: 跳号不影响门禁

- **WHEN** 同月上一个正式版本为 `v2026.8.11`，新 tag 为 `v2026.8.15` 且其余身份检查通过
- **THEN** 版本身份门禁 MUST 通过

#### Scenario: 门禁不因缺少历史 tag 而失败

- **WHEN** 门禁运行环境未获取完整历史 tag 列表
- **THEN** 序号合法性判定 MUST NOT 受影响
- **AND** 门禁 MUST NOT 因无法比较历史序号而拒绝合法 tag

### Requirement: 稳定发布必须以 main 为唯一落点

`main` MUST 是唯一稳定发布分支。正式 tag 指向的提交 MUST 可从 `origin/main` 到达。正式产物 workflow 的稳定分支 push 触发器 MUST 监听 `main`，MUST NOT 继续把 `master` 作为稳定发布触发器。

#### Scenario: main 上的正式提交允许发布

- **WHEN** 正式 tag 指向的提交可从 `origin/main` 到达且其他版本身份检查通过
- **THEN** 正式发布构建 MAY 启动

#### Scenario: main 不可达的 tag 被拒绝

- **WHEN** 正式 tag 指向的提交不可从 `origin/main` 到达
- **THEN** 版本身份门禁 MUST 失败
- **AND** 发布产物 MUST NOT 创建

#### Scenario: 分支迁移不得遗漏历史

- **WHEN** 正式 workflow 的稳定触发器从 `master` 迁移到 `main`
- **THEN** 维护者 MUST 先以非破坏方式把 master 所需独有提交纳入 main 并验证可达性
- **AND** 迁移 MUST NOT 使用强推、reset 或分支删除

### Requirement: 人工镜像发布不得绕过正式版本身份

人工触发 Docker workflow 且 `publish=true` 时，发布版本 MUST 从当前提交上的唯一附注 `v*` tag 推导，并通过与自动 tag 发布相同的版本身份门禁。自由输入的 version MUST NOT 覆盖已解析的正式发布身份。`publish=false` MUST 保持无发布副作用的 dry run，并 MAY 使用自由构建标签。

#### Scenario: 有唯一合法 tag 时允许人工发布

- **WHEN** 人工触发设置 `publish=true`，当前提交具有唯一合法附注 `v*` tag，且所有身份检查通过
- **THEN** workflow MUST 使用该 tag 作为镜像版本
- **AND** 发布步骤 MAY 执行

#### Scenario: 无 tag 或多 tag 时拒绝人工发布

- **WHEN** 人工触发设置 `publish=true`，但当前提交没有 `v*` tag 或存在多个候选 `v*` tag
- **THEN** workflow MUST 在登录镜像仓库前失败
- **AND** MUST NOT 推送镜像或创建 manifest

#### Scenario: dry run 保持无副作用

- **WHEN** 人工触发未设置 `publish=true`
- **THEN** workflow MAY 使用自由 version 输入完成构建
- **AND** MUST NOT 登录镜像仓库、推送镜像、创建 manifest 或移动别名 tag

### Requirement: 正式版本必须在运行时与容器元数据中可观测

正式构建的 `--version`、启动首条 info 日志、Release 资产名、Docker 镜像 tag 和 OCI `org.opencontainers.image.version` MUST 表达同一个正式版本。启动版本日志 MUST 在凭据加载或备份相关 info 日志之前输出。

#### Scenario: 正式二进制报告匹配版本

- **WHEN** 从正式 tag 构建并执行 `kiro-rs --version`
- **THEN** 输出版本 MUST 等于正式 tag 去掉 `v` 后的值

#### Scenario: 启动日志先报告项目版本

- **WHEN** 服务完成 tracing 初始化并开始启动
- **THEN** 第一条应用 info 日志 MUST 为 `kiro-rs v<version>`
- **AND** 该日志 MUST 早于凭据加载或备份日志

#### Scenario: 镜像携带一致 OCI 版本

- **WHEN** 构建正式 Docker 镜像
- **THEN** `org.opencontainers.image.version` label MUST 等于正式 tag

### Requirement: 非正式构建必须可追溯且不得冒充正式版本

分支 artifact、`dev-latest` 与 `publish=false` 的人工构建 MUST 被视为非正式构建。它们 MUST 通过 artifact 标签、Release 元数据或 Actions run 保留 commit 可追溯性，但 MUST NOT 被要求让滚动/自由标签等于 Cargo 版本，也 MUST NOT 被描述为正式 Release。

#### Scenario: dev-latest 可追溯到 commit

- **WHEN** dev 分支更新滚动 prerelease
- **THEN** Release 标题或正文 MUST 包含可解析的 commit 标识
- **AND** Actions run MUST 能定位到完整源码提交

#### Scenario: 手工 artifact 不创建正式发布

- **WHEN** build workflow 通过 workflow_dispatch 使用自由 version 标签运行
- **THEN** workflow MAY 上传带该标签的 artifact
- **AND** MUST NOT 因该标签创建正式 Release

### Requirement: 项目最低支持 Rust 版本必须为 1.97.1

`Cargo.toml` MUST 声明 `rust-version = "1.97.1"`。项目 MUST 在 Rust 1.97.1 上通过默认 feature 与 `--no-default-features` 两种 `cargo check --release --all-targets --locked` 判定面。Rust 1.96 及以下 MUST 被视为不受支持。

#### Scenario: MSRV 构建面通过

- **WHEN** 使用 Rust 1.97.1 和锁定依赖执行默认与无默认 feature 两种全目标检查
- **THEN** 两种检查 MUST 成功且不得产生项目告警

#### Scenario: 调整 MSRV 必须重新验证

- **WHEN** `rust-version` 被调整
- **THEN** 维护者 MUST 在新的目标版本上重新执行两种锁定检查并报告告警数
- **AND** README 与长期规格 MUST 同步更新

### Requirement: admin-ui 不独立跟随主项目版本

内嵌的 admin-ui MUST 被视为主二进制的一部分，`admin-ui/package.json` 的版本 MUST NOT 被用作发布身份，也无需随主项目 CalVer 同步。

#### Scenario: 主项目版本更新时保持前端版本策略

- **WHEN** 主项目 Cargo 版本更新并创建正式发布
- **THEN** admin-ui MUST 随主二进制构建和内嵌
- **AND** `admin-ui/package.json` 版本 MAY 保持不变
