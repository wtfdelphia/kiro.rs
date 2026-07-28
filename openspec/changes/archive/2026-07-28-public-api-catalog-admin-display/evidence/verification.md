# 验证记录（Phase A）

日期：2026-07-27。以下均为本会话真实运行的命令与输出摘录。

## 1. OpenSpec

```
$ openspec validate --all
✓ change/public-api-catalog-admin-display
Totals: 12 passed, 0 failed (12 items)
```

## 2. 后端测试

```
$ cargo test
test result: ok. 306 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

新增 23 项（原 283 → 306）：catalog 9、dto 8、routes 4、admin service 2。

```
$ cargo test public_api
test public_api::catalog::tests::test_aliases_empty_in_first_version ... ok
test public_api::catalog::tests::test_cc_stream_difference_documented ... ok
test public_api::catalog::tests::test_expected_live_set ... ok
test public_api::catalog::tests::test_families_order ... ok
test public_api::catalog::tests::test_ids_unique ... ok
test public_api::catalog::tests::test_live_entries_complete ... ok
test public_api::catalog::tests::test_method_path_unique ... ok
test public_api::catalog::tests::test_models_auth_hint_present ... ok
test public_api::catalog::tests::test_openai_endpoints_planned ... ok
test public_api::dto::tests::* (8 项) ... ok
test public_api::routes_test::test_live_endpoints_are_mounted ... ok
test public_api::routes_test::test_live_endpoints_require_auth ... ok
test public_api::routes_test::test_no_alias_routes_mounted ... ok
test public_api::routes_test::test_planned_endpoints_are_not_mounted ... ok
test result: ok. 21 passed; 0 failed
```

## 3. 防漂移门禁有效性验证（关键）

把 `openai.chat.completions` 的 status 由 Planned 临时改为 Live，确认测试转红：

```
test public_api::routes_test::test_live_endpoints_are_mounted ... FAILED
test public_api::routes_test::test_live_endpoints_require_auth ... FAILED
test public_api::catalog::tests::test_expected_live_set ... FAILED
test public_api::catalog::tests::test_openai_endpoints_planned ... FAILED
test public_api::dto::tests::test_status_serialized_lowercase ... FAILED

panicked: live 端点数量与预期不符: {..., ("POST", "/v1/chat/completions"), ...}
```

已恢复原值，`cargo test public_api` 重新全绿（21 passed）。这证明 `live ⊆ routes` 不是空转断言。

## 4. 启动日志

```
$ kiro-rs --config <临时配置> --credentials <示例凭据>
INFO kiro_rs: 可用 API:
INFO kiro_rs:   GET  /v1/models
INFO kiro_rs:   POST /v1/messages
INFO kiro_rs:   POST /v1/messages/count_tokens
INFO kiro_rs:   POST /cc/v1/messages
INFO kiro_rs:   POST /cc/v1/messages/count_tokens
```

改造前手写清单只有前 3 行，遗漏了 `/cc/v1` 两条 —— 这正是 P4 漂移的实例，现已由 catalog 消除。

## 5. Admin 接口

```
$ curl -o /dev/null -w "%{http_code}" http://127.0.0.1:18990/api/admin/public-api
401                                    # 未带 admin key

$ curl -H "x-api-key: <admin>" .../api/admin/public-api
http=200 size=5030
```

响应结构：

```
server: {"listenHost":"127.0.0.1","port":18990,"requireApiKey":true,
         "apiKeyMask":"sk-k***3456","hasApiKey":true,
         "authHeaders":["x-api-key","Authorization: Bearer"],
         "suggestedBaseUrl":null}

[models]            GET   /v1/models                      live     stream=False
[claude]            POST  /v1/messages                    live     stream=True
                    POST  /v1/messages/count_tokens       live     stream=False
                    POST  /cc/v1/messages                 live     stream=True
                    POST  /cc/v1/messages/count_tokens    live     stream=False
[openai-chat]       POST  /v1/chat/completions            planned  stream=True
[openai-responses]  POST  /v1/responses                   planned  stream=True
                    GET   /v1/responses/{id}              planned  stream=False
```

密钥检查（对 5030 字节响应全文）：

```
含完整 client key: False
含 admin key:      False
含占位符 API_KEY:  True
```

## 6. planned 端点与回归

```
POST /v1/chat/completions  -> 404      # planned，未挂载
POST /v1/responses         -> 404      # planned，未挂载
POST /messages             -> 404      # 首版无别名
GET  /models               -> 404      # 首版无别名

POST /v1/models      无 key -> 401     # 回归：鉴权行为不变
POST /v1/messages    无 key -> 401
POST /cc/v1/messages 无 key -> 401
GET  /v1/models      带 key -> 200
GET  /admin                 -> 200     # Admin UI 可访问
```

## 7. admin-ui

```
$ pnpm build
$ tsc -b && vite build
✓ 1777 modules transformed.
dist/assets/index-CIq8N_ga.js   468.79 kB │ gzip: 148.63 kB
✓ built in 49.97s
```

Playwright 渲染验证（headless chromium，真实服务）：

```
分组: 服务概要 / Models / Anthropic Messages / OpenAI Chat Completions
      / OpenAI Responses / 客户端配方 / 接入须知
状态徽章: {'可用': 5, '未启用': 3, '流式': 4, 'GET': 2, 'POST': 6}
URL 复制按钮数: 8
改 Base URL 后配方含新值: True
横向溢出: {'scrollW': 766, 'clientW': 766, 'overflowX': False}
暗色模式: dialog 配色 bg=rgb(2,8,23) fg=rgb(248,250,252)，内容可见
控制台错误: 无
```

文案检查（面板全文）：标题、上游概念区分、planned「未启用」标记、
`OPENAI_BASE_URL` 提示、Models「需鉴权」、`API_KEY` 占位符说明、
apiKey 掩码显示 —— 全部命中；完整 client key 与 admin key 均未出现。

## 8. 安全与卫生

```
$ git status --short
（无 config.json / credentials.json / .codegraph 等）
```

`.github/workflows/_runs.json` 为既有未跟踪文件（GitHub API 查询缓存），非本 change 产生，未改动。

临时验证文件（配置、凭据副本、Playwright 脚本、截图）已删除。

## 9. 未执行项

- README 同步：待 tasks 6.4 处理
- Phase B/C 相关约束（D8–D12）仅写入 design.md，本 change 未实现任何 OpenAI 协议代码
