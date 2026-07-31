# Bridge Plan: kam-external-idp-import-compat

> 生成时间：2026-07-30
> 分支：`dev`；工作区：仅 2 个未跟踪项（分析文档 + 本 change 目录），无真实凭据待提交
> OpenSpec 状态：`isComplete: true`，四工件（proposal/design/specs/tasks）均 `done`，**非 blocked**
> **结论（更新后）：第 8 节的 1 项停止条件与 3 项规格缺口已全部决议并回写工件，
> `openspec validate --all` 复跑通过，可以开始实现。**

## 1. 范围

把 KAM（kiro-account-manager）导出文件的 Microsoft Entra ID / Azure AD 账号
（`authMethod = external_idp`）做成可导入、可长期刷新，并统一两个导入入口的解析语义。

覆盖 proposal 的 Phase 0-3 全部 9 项改动：模型扩展、认证类型规范化、Microsoft OAuth2
刷新分支、服务端 KAM adapter、启动加载 wrapper/nested 支持与备份后原子回写、
导入反馈逐条化、四处既存缺陷修复、测试基础设施、文档同步。

## 2. 非目标（摘 proposal，实现时的硬边界）

- 不改 Social 与 IdC 既有刷新行为（端点、请求体、错误分类逐位不变）
- 不引入新运行时配置项；endpoint 白名单硬编码，不可配置
- 不迁移 KAM 账号级 `proxyConfig`、`password`、`usageData` 等非登录字段
- 不改 KAM 侧任何代码（外部只读参考）
- 不实现 external_idp 登录流程（授权码换 token），只做导入 + 刷新
- 不改 `profileArn` 固定占位表与占位识别逻辑
- 不改 `refresh_lock` 粒度、`force_refresh_token_for` 语义

## 3. 关键设计决策

| 决策 | 取舍理由 |
| --- | --- |
| `AuthMethod` 用内部枚举，持久化字段仍是 `Option<String>` | 改成枚举会让任何历史脏值导致整个 `credentials.json` 反序列化失败 |
| 新增 `parse_auth_method`，不改 `canonicalize_auth_method_value` | 后者契约是「未知原样透传」，被落盘路径依赖（`token_manager.rs:1211`），改成报错会让历史脏数据落盘失败 |
| endpoint 白名单不可配置 | 可配置的白名单等于可绕过的白名单；攻击面会从导入文件扩到配置文件（Docker 挂载 / CI 注入） |
| 显式拒 IP/loopback，不照抄 KAM 的隐式拦截 | KAM 靠域名白名单隐式挡住 IP，意图不可读、不可断言；本项目显式检查 |
| KAM adapter 用 `serde_json::Value` 判别，不用 untagged | untagged + 零必填字段 = 静默吞下任何对象（缺陷二本体） |
| `scopes` 用 `Option<String>` 而非 `Vec<String>` | 与 KAM `Account.scopes`（`kam/core/account.rs:211-212`）逐位对齐，导入无损 |
| external 判定必须先于 idc | external 账号也可能同时有 clientId + clientSecret；顺序反了则机密客户端永远进不了 external 分支 |

## 4. 高风险项

| # | 风险 | 等级 | 缓解 |
| --- | --- | --- | --- |
| R1 | 信任导出文件的 token endpoint → SSRF，泄露 refresh token | **最高** | HTTPS + 精确 hostname 白名单 + 显式 IP/loopback 拒绝 + 派生后复检 + 7 个绕过测试；白名单不可配置 |
| R2 | 扩展 `KiroCredentials` 影响面 | 高 | CodeGraph 实测 **151 个受影响符号**（见 5.1）；新增字段全可选、不改既有语义、全量回归 |
| R3 | 启动加载回写迁移损坏用户凭据文件 | 高 | 备份 → 临时文件 → 原子替换；失败不覆盖、不阻止启动 |
| R4 | `effective_api_region` 语义变更 | **停止条件** | 见 8.1 —— 该项已被证伪，不是缺陷 |
| R5 | `persist_credentials` 原子化牵连 13 个调用点 | 中 | 见 5.2；实现前需确认范围 |
| R6 | 导入强制刷新触发上游限流 | 中 | 沿用既有 batch 默认并发 1；`invalid_grant` 不重试 |
| R7 | 前端新增 vitest 依赖 | 中 | 见 8.3 —— CI 用 `--frozen-lockfile`，必须同步 lockfile |
| R8 | 错误体回显上游响应泄露 token/租户 | 中 | 结构化脱敏；禁止记录请求 form 与完整响应 |

## 5. CodeGraph 证据

索引状态：129 文件 / 2,488 节点 / 6,610 边 / Rust 73 文件（`codegraph status`）。

### 5.1 `codegraph impact "KiroCredentials"` → **151 affected symbols**

proposal 中「CodeGraph 显示约 151 个符号」在本次实测中**精确复现**（此前分析文档
无法复现，已在该文档中替换为文本引用统计；此处以 CodeGraph 实测为准）。

关键受影响符号（与本 change 直接相关）：

```
src/kiro/model/credentials.rs
  struct KiroCredentials:16          ← 新增 3 字段
  enum   CredentialsConfig:153       ← 容器判别改造
  method effective_auth_region:220
  method effective_api_region:229    ← 见 8.1
  method canonicalize_auth_method:254
  method is_api_key_credential:284
  method into_sorted_credentials:186
  method from_json:296
```

### 5.2 `codegraph callers "persist_credentials"` → **13 个调用点**

```
new:674 / set_profile_arn:834 / clear_profile_arn:864 / try_ensure_token:1094
set_disabled:1679 / set_priority:1705 / reset_and_enable:1722 / get_usage_limits_for
（余下 5 个未在 head 截断内显示）
```

结论：改 `persist_credentials` 为原子写是**函数体内部改动**，13 个调用点均不需改签名，
风险低于 proposal 的预估。tasks 9.2 的「若牵连过多测试则停下」仍保留，但预期不会触发。

### 5.3 `codegraph callers "refresh_token"` → **7 个调用点**

```
try_ensure_token:1094 / get_usage_limits_for:1746 / ingest_credential:1881
force_refresh_token_for:2219 / ensure_access_token:2353 / refresh_models_for:2413
（+1 未显示）
```

结论：分派从两路扩四路是 `refresh_token` **函数体内部**改动，7 个调用点零改动。
与 proposal 判断一致。

### 5.4 `codegraph callers "effective_api_region"` → **3 个调用点**

```
get_usage_limits:337
test_api_call_uses_effective_api_region:3257   ← 关键
test_api_call_uses_credential_api_region:3273
```

**这是本次 bridge 最重要的发现。** 详见 8.1。

### 5.5 `codegraph impact "refresh_token"` → 17 symbols，含 `profile.rs:resolve_profile_arn:195`

确认 design「未决与已知遗留」第 1 条的耦合真实存在：改 `refresh_token` 的分派语义
会波及 profile ARN 解析。本 change 维持「只加测试记录、不修」的决定。

## 6. rg / 源码补盲（CodeGraph 不覆盖）

### 6.1 README 已文档化两条不同的 region 优先级链

```
README.md:456   凭据.authRegion > 凭据.region > config.authRegion > config.region
README.md:459   凭据.apiRegion > config.apiRegion > config.region
README.md:386   | apiRegion | string | 凭据级 API Region，用于 API 请求 |
```

注意 :456 显式含 `凭据.region`，:459 显式**不含**。这不是文档疏漏，
与代码（`credentials.rs:229-233`）和测试注释（`token_manager.rs:3265`）三重一致。
→ 直接导致 8.1。

### 6.2 `url` crate 不是直接依赖

`Cargo.toml` 的 `[dependencies]` 无 `url`；`Cargo.lock:1992` 显示 `url 2.5.7`
仅作为 reqwest 的传递依赖存在。design 的 endpoint 校验方案基于 `Url::parse`，
需新增直接依赖。→ 见 8.2。

### 6.3 CI 使用 `--frozen-lockfile`

```
.github/workflows/build.yaml:107          pnpm install --frozen-lockfile
.github/workflows/build-dev-release.yaml:99  pnpm install --frozen-lockfile
```

新增 vitest 若不同步 `admin-ui/pnpm-lock.yaml`，两个 workflow 都会失败。→ 见 8.3。

### 6.4 `.gitignore` 覆盖充分

```
/config.json  /credentials.json  /credentials.*  /test.json
/kiro_balance_cache.json  /kiro_stats.json  .codegraph/  *.log
```

`credentials.*` 通配会同时忽略迁移备份文件（`credentials.json.kam-backup-*`）——
这是期望行为，备份不应入库。但**注意 `credentials.example.*.json` 也匹配 `/credentials.*`**，
需确认既有 example 文件是如何被跟踪的（它们已在库中，说明是历史 `git add -f`
或 gitignore 对已跟踪文件无效）。新增 `credentials.example.external.json`
（tasks 12.2）**将需要 `git add -f`**，否则会被忽略。→ 补入 8.4。

### 6.5 Admin 路由现状

```
src/admin/router.rs:32  /credentials/import        → import_credential
src/admin/router.rs:33  /credentials/import/batch  → import_credentials_batch
```

新增 `/credentials/import/kam` 是纯增量挂载，不改既有两条。

### 6.6 `src/kiro/mod.rs` 需新增两个模块声明

现有 11 个 `pub mod`，需加 `kam_adapter` 与 `external_idp`。tasks 未显式列出，
但属实现细节，不构成缺口。

### 6.7 release 构建依赖 admin-ui

`spec/design.md`：「release 前需 admin-ui build 以嵌入静态资源」；
`src/admin_ui/` 用 `rust-embed`。前端改动必须先 `pnpm build` 才能被 release 二进制包含。
验证顺序：`pnpm test` → `pnpm build` → `cargo build --release`。

## 7. 任务到执行步骤映射

| tasks | 执行步骤 | 验证 | 何时停止 |
| --- | --- | --- | --- |
| 1.1-1.4 | 读规则、核对既有调用点 | 能陈述高风险类型与验证命令 | 工件矛盾 |
| 2.1-2.4 | 认证类型规范化（纯函数，先做） | 别名表全覆盖；external 先于 idc；`canonicalize_auth_method_value` diff 为空 | 既有落盘测试失败 |
| **3.0** | **`Cargo.toml` 加 `url` 直接依赖** | `cargo tree -i url` 显示直接依赖 | — |
| 3.1-3.5 | endpoint 校验（安全关键） | 4 域 + 子域通过；7 类绕过全拒；错误不含 userinfo | 任一绕过用例通过 |
| 4.1-4.3 | 模型加 3 字段 + round-trip | 序列化不丢；旧文件可加载 | 既有 credentials 测试失败 |
| **4.4（已反转）** | **确认 region 解析函数未被修改** | `token_manager.rs:3256-3273` 两测试继续通过；两函数体 diff 为空 | 任一测试失败 → 动了不该动的地方 |
| 4.5 | 全量回归 credentials | `cargo test kiro::model::credentials` 全绿 | 任一既有测试失败 |
| 5.1-5.6 | 刷新分派扩四路 | external 带 secret 仍选 external；Social/IdC diff 为空 | `refresh_social_token`/`refresh_idc_token` 函数体被改动 |
| 6.1-6.8 | KAM adapter | 4 容器 × 4 格式；label/enabled 两形态；全 null 样本 | 容器判别顺序错 |
| 7.1-7.3 | 脱敏 fixtures | 无真实 token；`git status` 干净 | 出现疑似真实凭据 |
| 8.1-8.8 | Admin 契约与 KAM 端点 | authMethod 校验；公共客户端通过；batch 契约未变 | 既有 batch 测试失败 |
| 9.1-9.9 | 原子写 + 启动加载 | Windows 覆盖实测；wrapper 不再产生空凭据；迁移失败不覆盖 | `fs::rename` 覆盖行为与假设不符 |
| 10.1-10.6 | 前端 | `install --frozen-lockfile` + `pnpm test` + `pnpm build` 均通过 | CI `--frozen-lockfile` 失败 |
| 11.1-11.3 | profile 回归测试 | `git diff src/kiro/profile.rs` 只含 `#[cfg(test)]` | 需改 profile 逻辑 |
| **12.1a** | **`.gitignore` 加 example 例外** | `git check-ignore -v` 两向验证 | — |
| 12.1-12.6 | 样例与文档 | 占位 ARN 已换；README 四值；region 章节不改 | — |
| 13.1-13.10 | 全量验证 | 见第 9 节 | 任一命令失败 |
| 14.1-14.4 | 交付门禁 | 三份 evidence 报告齐 | — |

## 8. 停止条件命中与规格缺口（全部已决议）

> 决议时间：2026-07-30，同一会话内。用户对 8.1 批准「照列出范围执行撤销」，
> 对 8.4 选择「`.gitignore` 加例外规则」。8.2 / 8.3 为无争议的缺口补齐。
> 四项均已回写工件，`openspec validate --all` 复跑 19/19 通过。

### 8.1 【停止条件 → 已撤销】`effective_api_region` 补 region 回退是错的

**状态：已决议，改动线整条撤销。**

proposal「缺陷四第 3 项」、design「`effective_api_region` 补 region 回退」、
tasks 4.4、`credential-ingest` spec 的 ADDED requirement
「有效 API region 与 auth region 的回退语义必须一致」——**这一整条线基于错误前提。**

三重反证：

1. **既有测试明确断言当前行为，且带解释性注释**
   （`src/kiro/token_manager.rs:3256-3270`）：
   ```rust
   #[test]
   fn test_api_call_uses_effective_api_region() {
       config.region = "us-west-2";
       credentials.region = Some("eu-west-1");
       // 凭据.region 不参与 api_region 回退链
       assert_eq!(api_host, "q.us-west-2.amazonaws.com");
   }
   ```
   这不是偶然通过的测试，是**有意设计的断言**。

2. **README 分两行文档化了两条不同的链**（`README.md:456` 含 `凭据.region`，
   `README.md:459` 刻意不含）。

3. **`Config` 层也是两条独立链**（`src/model/config.rs:267-274`），
   `effective_auth_region` 与 `effective_api_region` 各自回退到 `config.region`，
   互不引用。

结论：auth region 与 api region 的回退链**故意不同**——auth region 是刷新端点的
region，api region 是数据面端点的 region，两者可以合法不同。我此前把它当缺陷，
是只看了函数体没看测试与文档。

**已执行的撤销（逐项核对）**：

| 位置 | 处理 |
| --- | --- |
| proposal 缺陷四第 3 项 | 改写为「region 只写 auth 专用字段」，并显式声明**不含** api region 回退主张，附三重反证 |
| proposal What Changes 第 7 项 | 「四处既存缺陷」→「三处」，第 3 条删除，补不改解析函数的说明 |
| proposal Impact 表 | `credentials.rs` 行去掉 api region 回退，加「region 解析函数不改」 |
| proposal Risks | 整条替换为「本 change 不含任何影响非 KAM 账号的语义变更」，并要求 `token_manager.rs:3256-3273` 两测试继续通过 |
| design「`region` 为何同时写两个字段」 | 标题改「`region` 写哪个字段」，结论仍选 B，删去「配合补 api region 回退」 |
| design「`effective_api_region` 补 region 回退」 | 整节替换为「不改 `effective_api_region`（撤销的早期决策）」，列三重反证与语义解释 |
| design 回滚 | 删「api region 语义变更随 revert 回退」，改为「不含影响非 KAM 账号的语义变更」 |
| design 未决第 2 条 | 改写为「api region 仍取全局配置是既有设计而非遗留缺陷」 |
| tasks 4.4 | 从「补回退」反转为「确认两函数**未被修改**」，验证条件为两个既有测试继续通过 |
| tasks 6.6 | 补断言「`effective_api_region` 仍取全局配置」 |
| tasks 12.4 | 补「`README.md:456-459` 两条 region 链保持原样不改」 |
| `credential-ingest` spec ADDED requirement | 从「回退语义必须一致」**反转**为「region 解析链不得因导入能力而改变」，4 个 scenario 全部重写 |
| `credential-import` spec「region 写入通用字段」 | 删去「auth 与 api 都能从该字段派生」，改为「auth-specific 字段保持为空 + 有效 auth region 经既有链取到」，并新增 scenario「导入不改变 region 解析行为」 |

**KAM 导入的 region 处理最终形态**：写入凭据级通用 `region`，
靠 `effective_auth_region` 的既有回退链让刷新取到它；`effective_api_region`
仍取全局配置。若 KAM 账号需要区分 api region，由用户在 Admin UI 显式设置
`apiRegion`——这是既有设计意图，不是缺陷。

**遗留待办（不阻塞实现）**：分析文档
`docs/kiro-account-manager-export-compatibility-optimization.md` 第 4.3 节
关于 region 的表述仍是旧判断，需同步修正。该文档不在本 change 的工件范围内，
但会误导后续读者。

### 8.2 【规格缺口 → 已补】未声明新增 `url` crate 依赖

**状态：已决议，proposal Impact 表加 `Cargo.toml`，tasks 新增 3.0。**

design 的 endpoint 校验方案要求「用 URL 解析器取 hostname，不做字符串匹配」，
实现依赖 `Url::parse`。但 `url` 不在 `Cargo.toml` 直接依赖中
（`Cargo.lock:1992` 显示它只是 reqwest 的传递依赖）。

依赖传递依赖是脆弱的：reqwest 升级可能改变 `url` 版本或移除它。
需在 `Cargo.toml` 显式声明 `url = "2"`（AGENTS.md 无 pin 策略要求，
但用户全局 CLAUDE.md 无相关约束，按项目现有惯例用 caret range）。

**已执行**：proposal Impact 表新增 `Cargo.toml` 行；tasks 新增 3.0
「`Cargo.toml` 的 `[dependencies]` 新增 `url = "2"`」，验证条件为
`cargo tree -i url` 显示 kiro-rs 为直接依赖者。版本用 caret range，
与项目现有依赖惯例一致（`Cargo.toml` 无 pin 策略）。

顺带补入 tasks 3.1 与 6.1：`src/kiro/mod.rs` 需加两个 `pub mod` 声明
（现有 11 个，需加 `external_idp` 与 `kam_adapter`）。

### 8.3 【规格缺口 → 已补】未声明同步 `pnpm-lock.yaml`

**状态：已决议，tasks 10.1 补 lockfile 同步与 `--frozen-lockfile` 验证。**

两个 CI workflow 都用 `pnpm install --frozen-lockfile`
（`build.yaml:107`、`build-dev-release.yaml:99`）。tasks 10.1 只说「引入 vitest 与
`test` script」，未提 lockfile。不同步会让 CI 在 install 阶段直接失败。

**已执行**：tasks 10.1 补「同步 `admin-ui/pnpm-lock.yaml`」，
验证条件加 `pnpm --dir admin-ui install --frozen-lockfile` 通过；
proposal Impact 表新增 `admin-ui/pnpm-lock.yaml` 行。

### 8.4 【规格缺口 → 已决议】新增 example 文件会被 `.gitignore` 吞掉

**状态：用户选择「`.gitignore` 加例外规则」。已加 tasks 12.1a。**

`.gitignore` 有 `/credentials.*`，会匹配 `credentials.example.external.json`。
既有 4 个 example 文件已在库中（gitignore 对已跟踪文件无效），但**新增文件需
`git add -f`**，否则静默不入库，导致文档引用了一个仓库里不存在的文件。

**已执行**：tasks 新增 12.1a「`.gitignore` 在 `/credentials.*` 之后加
`!/credentials.example.*.json` 例外」，验证条件为
`git check-ignore -v credentials.example.external.json` 显示未被忽略
且 `git check-ignore -v credentials.json` 仍被忽略。
proposal Impact 表新增 `.gitignore` 行。

选例外规则而非每次 `git add -f`：显式例外是声明式的，不依赖操作者记得加 flag。
代价是可跟踪范围扩大——写入 proposal Risks 与 design 未决第 5 条：
例外模式收窄到 `credentials.example.*.json`（不是 `credentials.*`），
并依赖 AGENTS.md 纪律与提交前 `git status --short` 检查兜底。

## 9. 必跑验证

```powershell
# 后端分层
cargo test kiro::model::credentials
cargo test kiro::external_idp
cargo test kiro::kam_adapter
cargo test kiro::token_manager
cargo test kiro::profile
cargo test admin
cargo test

# 前端（顺序有意义：test → build，release 依赖 build 产物嵌入）
pnpm --dir admin-ui install --frozen-lockfile
pnpm --dir admin-ui test
pnpm --dir admin-ui build

# 规格与卫生
openspec validate --all
git status --short
```

端到端等价性（tasks 13.8）是本 change 的核心验收：同一脱敏 fixture 经
Admin 导入与启动加载，产出的规范化凭据字段逐一相等。

**不做真实账号在线验活。** 若本地验证，只用临时凭据，并确认
`config.json`、`credentials.json`、`credentials.*`、`.codegraph/` 不进 Git 候选。

## 10. README / AGENTS / spec 同步判断

| 文件 | 是否需改 | 依据 |
| --- | --- | --- |
| `README.md` | **需改** | `authMethod` 取值（:379 从「social 或 idc」扩四值）、凭据字段表加 3 字段、KAM 支持范围、endpoint 安全限制、example 文件清单 |
| `README.md:456-459` region 章节 | **不改** | 按 8.1，region 回退链维持现状 |
| `AGENTS.md` | 不需改 | 不涉及 AI 纪律或高风险矩阵变化；现有「Token/多凭据」「Admin/凭据 CRUD」「admin-ui」三项已覆盖本 change |
| `spec/design.md` | 不需改 | 模块边界未变（新增文件都在 `src/kiro/` 既有职责内）；构建测试策略未变 |
| `spec/requirements.md` / `spec/structure.md` | 待确认 | 本次未读，归档前由 `openspec-verify-change` 覆盖 |
| `openspec/project.md` | 不需改 | 技术栈与验证命令无实质变化（vitest 属 admin-ui 内部） |
| `openspec/specs/**` | 由 archive 流程更新 | 本 change 的 3 个 delta 文件在归档时合并 |
| `config.example.json` | 不需改 | 本 change 不引入运行时配置项 |
| `.gitignore` | **需改（已决议）** | 加 `!/credentials.example.*.json` 例外，见 8.4 |
| `Cargo.toml` | **需改** | 新增 `url` 直接依赖，见 8.2 |
| `admin-ui/pnpm-lock.yaml` | **需改** | 同步 vitest，见 8.3 |
| `docs/kiro-account-manager-export-compatibility-optimization.md` | **需改（不阻塞）** | §4.3 的 region 表述仍是被 8.1 证伪的旧判断 |

## 11. 停止条件

### 已命中并已解决

- **8.1 `effective_api_region`** —— 规格中一整条改动线基于错误前提，
  既有测试注释、README、Config 层三重反证。按 AGENTS.md「不静默猜测」与
  「不得改测试迁就实现」已停止并上报；用户批准撤销后，工件已按 8.1 表格逐项回写，
  `openspec validate --all` 复跑 19/19 通过。**该项现已关闭。**
- 8.2 / 8.3 / 8.4 —— 三项规格缺口已补齐（`Cargo.toml`、lockfile、`.gitignore`），
  对应 tasks 3.0 / 10.1 / 12.1a。**均已关闭。**

### 实现过程中若命中以下任一项，立即停止并报告

- 任一 endpoint 绕过测试通过（R1 是最高风险，零容忍）
- `refresh_social_token` / `refresh_idc_token` 函数体出现 diff（Non-Goals 硬边界）
- `src/kiro/profile.rs` 出现 `#[cfg(test)]` 之外的 diff
- Windows `fs::rename` 覆盖行为与 design 假设不符
- `persist_credentials` 原子化牵连超过 3 个既有测试需修改
- 发现需要新增运行时配置项（Non-Goals 明确排除，需回到 proposal）
- 工作区出现真实 `config.json`、`credentials.json`、token 或 Cookie
- 任一既有测试为了让新实现通过而被修改
- `test_api_call_uses_effective_api_region` 或
  `test_api_call_uses_credential_api_region` 失败（8.1 的哨兵测试）
- `git check-ignore` 显示 `credentials.json` 因例外规则而变为可跟踪
