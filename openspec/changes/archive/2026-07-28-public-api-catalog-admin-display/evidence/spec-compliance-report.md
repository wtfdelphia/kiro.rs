# Spec Compliance Report: public-api-catalog-admin-display

审查日期：2026-07-27
审查范围：工作区未提交改动（该 change 与后续三个 change 共处同一工作区，按文件归属拆分核对）

## 总体状态：**WARN**

两项已在本次审查中修复（依赖登记、README TOC），无 CRITICAL 未处理。

## 六维审查

| 维度 | 状态 | 说明 |
| --- | --- | --- |
| Scope | PASS | 新增 `src/public_api/`、`admin-ui` 两个新文件；修改 `main.rs`（mod + 启动日志）、`src/admin/{router,handlers,service}.rs`（只读接口）、`admin-ui/{dashboard.tsx,types/api.ts}`。均在 proposal Impact 声明内。未触碰上游链路（converter / provider / stream） |
| Design | PASS | catalog 为静态常量表；`live ⊆ routes` + `planned ∉ routes` 双向断言按 design §4 实现；DTO 只回 mask；启动日志遍历 catalog |
| Scenarios | PASS | 9 Requirements / 23 Scenarios 全部有实现或测试对应（见下表抽样） |
| Project Rules | WARN→PASS | 新增 dev-dependency 未登记（已修复，见发现 1） |
| Verification | PASS | `openspec validate --all` 15 passed；`cargo test` 498 passed；`pnpm build` 通过；Playwright 面板渲染验证；防漂移门禁有效性已破坏验证 |
| README/AGENTS Sync | WARN→PASS | README 已记录新端点与 Admin 接口；TOC 缺两项（已修复，见发现 2）。AGENTS.md 无需改（未引入新纪律或新验证命令） |

## Scenario 覆盖抽样

| Scenario | 证据 |
| --- | --- |
| 启动日志来自注册表 | `main.rs` 遍历 `live_endpoints()`；evidence §4 实测输出 5→6→7 条随 catalog 变化 |
| live 条目可被路由命中 | `routes_test.rs:test_live_endpoints_are_mounted`（401 算命中） |
| planned 条目不可被路由命中 | `routes_test.rs:test_planned_endpoints_are_not_mounted` |
| planned 不做占位实现 | 同上（断言必须 404，501 会失败） |
| alias 字段存在但为空 | `catalog.rs:test_aliases_empty_in_first_version` |
| 不挂载别名路由 | `routes_test.rs:test_no_alias_routes_mounted` |
| 只返回掩码 | `service.rs:test_get_public_api_never_leaks_full_key`（正则断言全文无完整 key） |
| 示例使用占位符 | `dto.rs:test_examples_use_placeholder_only` |
| 未配置时为 null | `dto.rs:test_suggested_base_url_null_when_unconfigured` |
| planned 端点标注为未启用 | `public-api-panel.tsx` STATUS_META + Playwright 断言「未启用」文案命中 |
| 区分对外 API 与上游端点 | 面板 DialogDescription + 接入须知首条；Playwright 断言「Kiro 上游端点」命中 |
| Models 端点标注需鉴权 | `catalog.rs:test_models_auth_hint_present` + Playwright 断言「需鉴权」 |
| 复制内容使用当前 Base URL | `public-api-panel.tsx:curlWithBase`；Playwright 实测改 Base URL 后配方含新值 |

## 发现项

### 1. 新增 dev-dependency 未登记（WARN，已修复）

`Cargo.toml` 新增 `tower = { version = "0.5.2", features = ["util"] }` 作为
dev-dependency，但未登记到 `docs/tooling-sources.md`，违反 AGENTS.md
「工具来源：如引入新依赖需登记」。

**处置**：已在 `docs/tooling-sources.md` 新增「Cargo 依赖登记（非 CLI）」小节并
登记该项；已在本 change 的 proposal Impact 中补充依赖声明。

核实：`git show HEAD:Cargo.lock | grep -c '^name = "tower"'` 返回 1 ——
tower 原本已是 axum 的传递依赖，`Cargo.lock` 仅多一行依赖边，无新 crate 引入，
不增加运行时依赖。

### 2. README TOC 缺两项（WARN，已修复）

「API 端点」章节新增的 `### WebSearch 工具` 与 `### 端点清单的单一事实源`
未加入目录。

**处置**：已补入 TOC。

### 3. `.github/workflows/_runs.json` 未被忽略（WARN，未处置）

该文件是 GitHub API 查询缓存，非本 change 产生，也不在任何 change 的范围内。
`git check-ignore` 确认未被忽略，因此 `git add .` 会把它带入提交。

**建议**：提交时改用显式 `git add <path>` 逐文件暂存，或由仓库维护者决定是否
加入 `.gitignore`。本 change 不擅自处置他人文件。

## 证据路径

- `openspec/changes/public-api-catalog-admin-display/evidence/verification.md`

## 剩余风险

- Playwright 渲染验证仅覆盖本次新增面板；未做跨浏览器与响应式断点验证
- `suggestedBaseUrl` 的配置项仅预留字段，未实际接入 `Config`，待有需求时补
