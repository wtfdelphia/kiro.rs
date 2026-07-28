# Capability: admin-public-api-view

## Purpose

Expose the Public API registry to operators through a read-only Admin endpoint (`GET /api/admin/public-api`) and a corresponding Admin UI panel, so the served client-facing surface can be inspected without reading source. The view never returns full client keys, distinguishes the outward Public API from the upstream Kiro endpoint, and marks `planned` entries as not yet usable.

## Requirements

### Requirement: Admin 只读对外 API 目录接口

The system MUST expose a read-only Admin endpoint `GET /api/admin/public-api` protected by the existing Admin authentication middleware. The response MUST contain a server summary section and endpoint entries grouped by protocol family, derived from the Public API registry.

#### Scenario: 未认证请求被拒绝

- **WHEN** 未携带有效 admin key 请求 `GET /api/admin/public-api`
- **THEN** 响应 MUST 为 401，且响应体 MUST NOT 包含任何端点清单或密钥信息

#### Scenario: 认证后返回服务概要与分组端点

- **WHEN** 携带有效 admin key 请求 `GET /api/admin/public-api`
- **THEN** 响应 MUST 包含监听主机、端口、requireApiKey 标志、apiKey 掩码、支持的鉴权头列表，以及按协议族分组的端点条目（每条含 method、path、aliases、status、stream、summary、clientHints）

#### Scenario: 与注册表同源

- **WHEN** 注册表中某端点的 status 或 path 发生变化
- **THEN** 该接口的响应 MUST 随之变化，MUST NOT 存在独立维护的第二份清单

### Requirement: 永不回传完整客户端密钥

The response MUST NOT contain the full client API key in any field or example. The key MUST be represented only as a mask. Generated client-side examples MUST use a literal placeholder for the key, and MUST NOT embed the Admin key.

#### Scenario: 只返回掩码

- **WHEN** 已配置非空 client apiKey 并请求该接口
- **THEN** 响应中 MUST 只出现掩码形式（部分字符 + 掩码标记），完整 key MUST NOT 出现在任何字段

#### Scenario: 示例使用占位符

- **WHEN** 响应包含 curl 或 SDK 配置示例
- **THEN** 示例中的密钥位置 MUST 为固定占位符文本，MUST NOT 是真实 client key 或 admin key

### Requirement: Base URL 建议值可缺省

The server summary MUST include a suggested base URL field. When no explicit public base URL is configured, this field MUST be null so that the client can fall back to the browser origin. The service MUST NOT fabricate a base URL from the listen address when that address is a wildcard.

#### Scenario: 未配置时为 null

- **WHEN** 未配置公开 Base URL
- **THEN** 响应中的建议 Base URL 字段 MUST 为 null

### Requirement: Admin UI 对外 API 面板

The Admin UI MUST provide an entry point in the dashboard header, alongside the runtime settings entry, that opens a panel displaying the public API catalog. The panel MUST show a service summary, endpoint cards grouped by protocol family with status indication, per-client configuration recipes, and copy actions.

#### Scenario: planned 端点标注为未启用

- **WHEN** 面板展示 status 为 planned 的端点
- **THEN** 该端点 MUST 带有明确的未启用标记，MUST NOT 呈现为当前可调用

#### Scenario: 区分对外 API 与上游端点

- **WHEN** 用户查看该面板
- **THEN** 面板 MUST 含有明确文案区分「对外 Public API」与「Kiro 上游 endpoint」，避免把上游端点误当作客户端 Base URL

#### Scenario: OpenAI 与 Anthropic 的 Base URL 差异提示

- **WHEN** 面板展示客户端配方
- **THEN** MUST 明确标注 OpenAI 客户端的 Base URL 需带 `/v1` 后缀，而 Anthropic 客户端的 Base URL 不带

#### Scenario: Models 端点标注需鉴权

- **WHEN** 面板展示 `GET /v1/models`
- **THEN** MUST 标注该端点受 requireApiKey 约束需要鉴权，以便用户理解未配置 key 的客户端探测会得到 401

#### Scenario: 复制内容使用当前 Base URL

- **WHEN** 用户在面板中修改 Base URL 显示值并复制某端点配置
- **THEN** 复制文本 MUST 使用该 Base URL，且该修改 MUST 只影响展示与复制文本，MUST NOT 改变服务端任何配置
