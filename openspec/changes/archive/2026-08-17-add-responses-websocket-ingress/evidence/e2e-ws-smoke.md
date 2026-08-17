# e2e 冒烟证据：真实二进制 WS ingress（任务 9.3 / 9.4）

日期：2026-08-14。

## 方式

- 二进制：`cargo build --release` 产物 `target/release/kiro-rs`。
- 配置：临时目录 `/tmp/kiro-ws-e2e/`（config.json：port 18991、requireApiKey、
  websocket 块 maxConnections=4；credentials.json：**显式伪造**的 api_key 凭据 +
  预置 profileArn，避免触发真实凭据与 profile 解析网络流）。
- 客户端：`python3 + websockets 16.0`（环境未安装 websocat/wscat，用等价 WS 客户端；
  脚本 `/tmp/kiro-ws-e2e/e2e.py`）。
- 上游：无真实凭据，turn 预期失败——用于演练握手/首帧/错误事件化/连接存活/热加载全链路。

## 结果（10/10 通过）

```text
[PASS] non_upgrade_get_426 status=426
[PASS] auth_missing_key_401 server rejected WebSocket connection: HTTP 401
[PASS] ws_upgrade_ok
[PASS] turn_events_received error
[PASS] turn_reached_terminal_or_error terminal=error
[PASS] connection_alive_after_turn type=error
[PASS] admin_get_ws_settings status=200 body={"enabled":true,"mode":"http_bridge","maxConnections":4,...}
[PASS] admin_put_disable {"success":true,"message":"WebSocket 设置已更新并落盘"}
[PASS] disabled_new_conn_503 server rejected WebSocket connection: HTTP 503
[PASS] admin_put_reenable {"success":true,"message":"WebSocket 设置已更新并落盘"}
```

服务端日志（/tmp/kiro-ws-e2e/server.log 摘录）：

```text
INFO kiro_rs::openai::ws_ingress: WS ingress: 新连接升级 mode=HttpBridge
WARN kiro_rs::openai::ws_ingress: WS turn: 上游失败且未写出下游事件，换凭据重试一次 stage="call_upstream" ...
WARN kiro_rs::openai::ws_ingress: WS turn: 失败，以 error 事件表达 stage="call_upstream" ...
```

额外收获：真实上游 400 触发了「未写出事件 → 换凭据重试一次 → error 事件 → 连接存活」
完整链路（design §4 异常路径表的线上演练）。

启动日志确认 catalog 派生：`GET  /v1/responses` 出现在「可用 API」，
`GET/PUT /api/admin/settings/websocket` 出现在 Admin 清单。

## 与 tasks.md 的差异说明

- 9.3 要求 websocat/wscat：环境未安装，改用 python websockets 等价完成，覆盖项一致。
- 无真实凭据，未能完成「一轮完整成功对话」；成功 turn 的全事件序列由集成测试
  （mock 上游，`ws_integration::multi_turn_reuses_connection` 等 16 项）锚定。

## 9.4 Codex CLI 实测：无法执行

- 原因：工作区无可用 Kiro 凭据（credentials.json 不存在，仅有 *.example.*）；
  Codex CLI 指向本代理还需有效会话与模型配额。
- 剩余风险：Codex 实际 WS 客户端行为（beta 头、帧时序、重连策略）未经真机验证；
  协议形状以 sub2api 已验证行为为基线（design §5），上线前建议补一轮真机会话。
