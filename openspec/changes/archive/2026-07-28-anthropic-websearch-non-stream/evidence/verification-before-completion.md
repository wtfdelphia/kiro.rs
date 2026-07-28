# Verification Before Completion

Change: `anthropic-websearch-non-stream`
Date: 2026-07-28

## Verification

以下命令均于本会话真实运行：

| Command | Result | Conclusion |
| --- | --- | --- |
| `cargo test`（全量） | PASS: 500 passed, 0 failed, 0 ignored | 全仓回归绿；仅 10 条既有 dead_code warning（非本 change 引入） |
| `cargo test websearch` | PASS: 47 passed, 0 failed（453 filtered） | Anthropic + OpenAI 两侧 websearch 全绿 |
| `cargo build --release` | PASS: Finished，0 error | release 产物可用 |
| `pnpm build`（admin-ui） | PASS: 1777 modules, built in 6.96s | 前端构建通过（与本 change 无关，为部署二进制而跑） |
| `openspec validate anthropic-websearch-non-stream --strict` | PASS: change is valid | 严格校验通过 |
| `openspec validate --all` | PASS: 15 passed, 0 failed | 全部 spec/change 校验通过 |
| `git status --short` + 敏感文件扫描 | PASS: 候选中无 `config.json`/`credentials*`/`.env`/`.codegraph/` | 无凭据入库 |
| `git check-ignore config.json credentials.json .codegraph/` | PASS: 三者均被忽略 | .gitignore 覆盖到位 |
| `grep -c TEMP-DIAG src/anthropic/websearch.rs` | PASS: 0 | 排查 U+FFFD 时的临时插桩已全部撤除 |

### Live Smoke（2026-07-28，真实上游）

release 二进制部署至 `C:\Users\wtf5058\Downloads\kiro_release`（旧 exe 备份为 `kiro-rs.exe.bak-20260728-160208`），`kiro-rs.exe -c ./config.json` 启动，端口 18990，凭据 #1 Token 刷新成功、模型缓存 19 项。密钥仅通过 shell 变量读取，未回显、未落盘。

| 场景 | 结果 |
| --- | --- |
| 非流式 web_search（`stream` 省略） | HTTP 200，`content-type: application/json`，四块结构 `[text, server_tool_use, web_search_tool_result, text]` |
| 流式 web_search（`stream:true`） | SSE 事件序列正常，`message_start` → 四块 start/stop → `message_delta` → `message_stop` |
| 关闭 `webSearchEmulation` 后的 Anthropic 侧 | 四块结构**不受影响**（该开关仅作用于 `/v1/responses`），验证了本 change 与 Phase C 开关的隔离 |
| `POST /v1/messages`、`/cc/v1/messages` 及两个 count_tokens | 均 HTTP 200 |

### 附带查证：搜索结果 title 中的 U+FFFD（结论：非本 change 缺陷）

实测发现部分 `title` 含替换字符（如 `L'\u{FFFD}quipe`）。临时插桩定位结论：

- MCP 原始响应字节整体 `utf8=VALID`，`content-type: application/json`
- 在原始字节上统计 `EF BF BD` 序列，与最终发给客户端的 U+FFFD 计数**精确相等**：34→34、0→0、1→1

替换字符是上游 Kiro MCP 抓取非 UTF-8 页面（Latin-1 法语/意大利语站点）时自己解码坏后写入 JSON 的合法内容，本 change 的透传路径无损，不修改代码。插桩已撤除（见上表）。

## Documentation Sync

| 入口 | 状态 |
| --- | --- |
| `README.md:564-578` + TOC `:69` | 已同步「WebSearch 工具」两种模式说明 |
| `docs/multi-protocol-api-design.md:652-655` | 相关待办已标为「已修复」 |
| `spec/` 长期事实 | 本 change 不改变对外协议入口清单，无需改动（OpenAI 两端点与新模块的同步由 Phase B/C 承担，已于本会话补入 `spec/requirements.md`、`spec/structure.md`） |
| `AGENTS.md` | 无需改动（不涉及 AI 纪律或验证命令变化） |

## 剩余风险

1. Scenario 3.7「查询无法提取时两种模式均 400」缺少验证物：生产代码在 `websearch.rs:469-481`（分派之前、两模式共用），结构上成立，但无单测覆盖 `extract_search_query` 返回 `None` 的分支，历史 curl 也未覆盖该场景。
2. Scenario 3.2「stream:true 仍为 SSE」的单测支撑为间接（`wants_stream(true)==true` + 事件名序列），无单测直接断言流式分支的 content-type；由本次 Live Smoke 的实测覆盖。
3. `bridge-plan.md` 缺失且**不补写**（实现前检查点，事后补写会伪造流程时序）。本 change 改动范围为单文件，实现前已用 `git show HEAD:` 对照与源码精读定位全部调用点。

## 结论

PASS。可归档。本 change 与其余三个无归档次序依赖（建立独立能力，不涉及 catalog status）。
