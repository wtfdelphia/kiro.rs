## ADDED Requirements

### Requirement: 注册表登记 Responses WebSocket 端点

The registry MUST include the Responses WebSocket ingress as a `live` entry: method `GET`,
path `/v1/responses`, stream capability true, with client hints identifying the WebSocket
upgrade transport. The existing `POST /v1/responses` entry MUST remain unchanged; method
differentiation MUST keep the (method, path) uniqueness invariant. Non-upgrade requests to
the live GET entry return 426 and therefore count as routed (not 404).

#### Scenario: GET 条目登记为 live

- **WHEN** 读取注册表
- **THEN** MUST 存在 method 为 GET、path 为 `/v1/responses`、status 为 live 的条目，
  其 stream 标志为 true 且 client hints 标注 WebSocket upgrade

#### Scenario: live 条目可被路由命中

- **WHEN** 对 `GET /v1/responses` 发起非 upgrade 请求
- **THEN** 响应 MUST 为 426（视为路由命中），MUST NOT 为 404

#### Scenario: POST 条目不受影响

- **WHEN** 读取注册表中 `POST /v1/responses` 条目
- **THEN** 其字段与行为 MUST 与本变更前完全一致
