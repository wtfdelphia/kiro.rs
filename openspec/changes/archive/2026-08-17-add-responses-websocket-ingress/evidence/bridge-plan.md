# Bridge Plan: add-responses-websocket-ingress

> 日期：2026-08-14（openspec-superpowers-bridge 产出）
> 依据：proposal.md / design.md / tasks.md / specs/**、AGENTS.md、spec/design.md、
> openspec/project.md、CodeGraph 与 rg 补盲证据（见 §4/§5）

## 1. 范围、非目标与关键设计决策

**范围**：`GET /v1/responses` WebSocket ingress（http_bridge 模式）+ 事件源双 sink
重构 + 模式路由/透传预留缝 + 配置化开关与热加载（Admin API）+ 端点目录登记。
详见 proposal.md。

**非目标**（实现期遇到以下需求一律记录并停，不顺手做）：透传实现、上游连接池/
Redis 粘连、`previous_response_id` 续链、permessage-deflate、Admin UI 页面、
文件监听式热加载。

**关键设计决策**（design.md 浓缩）：

1. 复用既有链路：WS turn → `to_chat_request_json` → `prepare` → `call_api_stream` →
   `ResponsesStreamContext`，只新增 WS sink；
2. SSE/WS parity 是重构的合并前置（同输入下事件序列逐项一致，不含 keepalive）；
3. 模式握手时解析并冻结；passthrough 预留分支 upgrade 前 501；未知 mode 回落并 warn；
4. 准入用 AtomicUsize+Notify（不用 Semaphore，容量需热改）；
5. 热加载语义矩阵：`enabled` 只拦新连接、`mode` 冻结、`max_connections` 只影响新准入、
   超时类每个等待边界重读快照；
6. JSON 命名 camelCase（config.json 与 Admin API），Rust 字段 snake_case。

## 2. 工件一致性检查

- `openspec change show add-responses-websocket-ingress --json`：解析出 9 个 delta，
  状态正常（非 blocked）；`openspec validate --all` 23 项通过。
- proposal 非目标 ↔ specs：specs 中无任何透传实现、续链、压缩相关 Requirement ✅
- specs 7 个 Requirement（新 capability）+ admin 1 个 + catalog 1 个 ↔ tasks 覆盖：
  见 §6 映射表，逐项有对应任务 ✅
- 发现并已修正的偏差：设计文档 §4.11 原 JSON 示例为 snake_case，与
  `Config` 的 `rename_all = "camelCase"` 约定不符；已回写 docs 与 change design.md。

## 3. 高风险项

| # | 风险 | 等级 | 处置 |
| --- | --- | --- | --- |
| H1 | 协议/SSE 流式变更（AGENTS.md 高风险矩阵行） | 高 | parity 测试前置 + websocat/Codex 实测证据 |
| H2 | 零新增编译告警（binary-only target，`pub` 无豁免） | 高 | 新增 trait/struct 必须有 crate 内真实调用点；每完成一组任务跑判定命令 |
| H3 | 工作区存在他人未提交改动：`src/openai/handlers.rs`、`src/openai/responses_stream.rs` 等（推测来自 analyze-kiro-eventstream-diagnostics，28/29） | 高 | **不得 revert**；实现前先读当前工作区版本，在其之上重构；冲突无法调和时停止并询问 |
| H4 | axum `ws` feature 与 `--no-default-features` 组合 | 中 | CI warning-gate 跑 default 与 no-default 两种组合；本地两种都验证 |
| H5 | 热加载语义实现错误（enabled 误杀存量 / mode 未冻结） | 中 | 语义矩阵写成单测（tasks 7.2/7.4） |
| H6 | 端点目录唯一性（GET/POST 同路径） | 低 | (method, path) 组合唯一是既有不变式，登记时测试断言 |

## 4. CodeGraph 证据（命令与结论）

索引：`codegraph status` → 150 文件 / 2,937 节点 / 8,878 边，up to date。

| 命令 | 结论 |
| --- | --- |
| `codegraph impact create_responses_sse_stream` | 仅 3 个符号受影响（自身、`handle_responses_stream:1291`、`post_responses:1120`）→ 事件源重构影响面收敛在 `src/openai/handlers.rs` |
| `codegraph impact AppState` | 56 个符号，绝大多数是 `AppState::new`/`with_auth_runtime` 的测试调用 → 新增 `ws` 字段在构造函数内默认初始化即可，测试无需改 |
| `codegraph callers prepare` | 12 个调用方（含 8+ 个测试）→ 抽取 handler 无关函数时保持签名稳定，避免大面积改测试 |
| `codegraph impact update_auth_settings` | service.rs:1300 → handlers.rs:340 → router `PUT /settings/auth`:70 三层 → WS 设置端点照此三层复制（service 方法 + handler + router 挂载） |

CodeGraph 不覆盖配置/Docker/CI/示例凭据，§5 用 rg 补盲。

## 5. rg / 源码补盲

| 检查 | 命令/位置 | 结论 |
| --- | --- | --- |
| 配置 schema 命名 | `rg rename_all src/model/config.rs` → :22 `camelCase`；`config.example.json` keys 全 camelCase | websocket 块 JSON 用 camelCase；`config.example.json` 需补示例块（任务 8.x 隐含，实现时勿漏） |
| admin types 命名 | `src/admin/types.rs:11` `camelCase` | Admin API 请求/响应 camelCase |
| 启动日志来源 | `src/main.rs:238-240` 由 `public_api::live_endpoints()` 生成，注释明示勿手写第二份 | 登记 catalog 后启动日志自动覆盖，无需改 main.rs 日志代码 |
| catalog 条目字段 | `src/public_api/catalog.rs` 有 `stream: bool`、`client_hints: &'static [&'static str]` | 满足 spec 登记需求 |
| Docker | `Dockerfile` 仅 `EXPOSE 8990`，无协议特定配置 | 无需改动 |
| CI | `.github/workflows/warning-gate.yaml` 跑 `cargo check --release --all-targets --locked` 与 `--no-default-features` 两组合 | 本地验证须覆盖两种组合（H4） |
| 密钥卫生 | `.gitignore` 含 `/config.json`、`/credentials.json`、`/credentials.*`；`git status` 未见真实凭据文件 | 满足停止条件检查；evidence 中禁止粘贴真实 token |
| admin-ui | 前端仅调用既有 admin 端点 | 新增 settings/websocket 端点不破坏 UI；UI 页面为后续项（非目标） |

## 6. 任务 → 执行步骤映射

执行顺序：1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9（2 与 3 可并行；6 依赖 2/3/4/5）。

| 任务 | 执行步骤 | 验证 | 停止点 |
| --- | --- | --- | --- |
| 1.1 axum ws feature | 改 Cargo.toml，`cargo check` 两组合 | 零新增告警 | 依赖解析失败 → 报告版本冲突 |
| 1.2 WsSettings 配置块 | `src/model/config.rs` 加结构体+字段（serde default + camelCase）；`config.example.json` 补示例 | 旧配置加载单测 | — |
| 1.3 未知 mode 回落 | Deserialize 自定义或枚举兜底 | 单测 + warn 日志断言 | — |
| 2.1-2.3 事件源重构 | 先读工作区当前 handlers.rs（H3），抽 `ResponsesEventSource`，SSE sink 接回 | 既有 openai 测试全绿 + parity 测试 | parity 不一致且原因不明 → 停 |
| 3.1 prepare 抽取 | 保持签名稳定抽函数（codegraph callers 12 处不动） | 既有测试绿 | — |
| 4.1-4.3 模式缝 | `ws_transport.rs`：mode 枚举 + trait + resolve_mode；passthrough 501 | 集成测试（oneshot + upgrade） | — |
| 5.1-5.2 错误分类 | `ws_error.rs` + 关闭码映射 | 单测 | — |
| 6.1-6.7 WS handler | `ws_ingress.rs`：挂载 → 握手准入 → 首帧契约 → turn 循环 → 保护 | 各场景集成测试 | 工作区改动与重构冲突不可调和 → 停并询问 |
| 7.1-7.5 热加载 + Admin | AppState.ws 句柄；AtomicUsize 准入；三层 admin 端点（照 update_auth_settings） | 热加载语义单测 + 落盘/重启恢复 | — |
| 8.1-8.3 目录与文档 | catalog 登记 + dto 断言 + README | public_api 测试；启动日志自动含新端点 | — |
| 9.1-9.7 验证与证据 | 按 §7 清单跑，证据写 `evidence/` | 见 §7 | 任一硬门槛失败 → 停 |

## 7. 必跑验证

1. `cargo check --release --all-targets` —— 零新增告警（硬门槛，报告告警数）
2. `cargo check --release --all-targets --no-default-features` —— CI 同组合
3. `cargo test` —— 全绿（含 parity / 热加载 / 准入 / 首帧 / 模式路由）
4. `openspec validate --all`
5. websocat/wscat 端到端一轮对话（证据入 evidence/）
6. Codex CLI ws 模式实测（无法执行时写明原因与剩余风险）
7. `git status --short` —— 无密钥、无 `.codegraph/`、无误提交

## 8. README / AGENTS / spec 同步判断

| 文档 | 判断 | 理由 |
| --- | --- | --- |
| README | **需同步**（任务 8.2） | 新增对外端点 `GET /v1/responses`（ws）与 admin settings 端点，属 API 入口变化 |
| AGENTS.md | 不需同步 | 无新增验证命令或纪律；高风险矩阵「协议/SSE」行已覆盖 |
| spec/design.md（长期事实） | **归档时同步一行** | 模块边界表的 OpenAI 兼容层描述追加 WS ingress；数据流无结构性变化。在 openspec-archive-change 阶段执行 |
| openspec/specs/ | 归档时自动合并（archive 流程） | 本 change 三个 delta spec |
| docs/websocket-support-optimization-design.md | 已回写 camelCase 修正；实现期新偏差按任务 8.3 回写 | — |

## 9. 停止条件

1. `src/openai/handlers.rs` / `responses_stream.rs` 的工作区未提交改动与重构冲突，
   且无法在不 revert 用户改动的前提下调和（AGENTS.md：不得 revert 他人改动）。
2. SSE parity 失败且定位不到原因（说明事件源抽取破坏了既有语义）。
3. default 或 no-default-features 任一组合出现编译失败/新增告警且无法以修正真实
   问题的方式消除。
4. Codex CLI / 手工实测发现协议形状与 spec 基线不符（协议漂移超出设计文档 §5
   容忍度）→ 记录差异，回 specs 修订后再继续。
5. 工作区或证据中出现真实凭据/token（立即停止并清理报告）。
6. 发现未写入 spec 的高风险影响面 → 先补 spec 再继续。
