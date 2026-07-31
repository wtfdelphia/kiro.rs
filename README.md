# kiro-rs

一个用 Rust 编写的 Anthropic Claude API 兼容代理服务，将 Anthropic API 请求转换为 Kiro API 请求。

---

<table>
<tr>
<td>
<b>特别感谢</b>：<a href="https://co.yes.vg/register?ref=hank9999">YesCode</a> 为本项目提供了 AI API 额度赞助, YesCode 作为一家低调务实的 AI API 中转服务商 <br>
长期以来提供稳定高可用的服务, 如您有意体验, 请点击链接注册体验 → <a href="https://co.yes.vg/register?ref=hank9999">立即访问</a>
</td>
</tr>
</table>

---

#### [LINUX DO 讨论帖](https://linux.do/t/topic/1571986)

## 免责声明

本项目仅供研究使用, Use at your own risk, 使用本项目所导致的任何后果由使用人承担, 与本项目无关。
本项目与 AWS/KIRO/Anthropic/Claude 等官方无关, 本项目不代表官方立场。

## 注意！

因 TLS 默认从 native-tls 切换至 rustls，你可能需要专门安装证书后才能配置 HTTP 代理。可通过 `config.json` 的 `tlsBackend` 切回 `native-tls`。
如果遇到请求报错, 尤其是无法刷新 token, 或者是直接返回 error request, 请尝试切换 tls 后端为 `native-tls`, 一般即可解决。

**Write Failed/会话卡死**: 如果遇到持续的 Write File / Write Failed 并导致会话不可用，参考 Issue [#22](https://github.com/hank9999/kiro.rs/issues/22) 和 [#49](https://github.com/hank9999/kiro.rs/issues/49) 的说明与临时解决方案（通常与输出过长被截断有关，可尝试调低输出相关 token 上限）

## 功能特性

- **Anthropic API 兼容**: 完整支持 Anthropic Claude API 格式
- **OpenAI API 兼容**: `/v1/chat/completions` 与 `/v1/responses`（无状态），可直接对接 OpenAI SDK
- **流式响应**: 支持 SSE (Server-Sent Events) 流式输出；Responses 端点为命名语义事件
- **Token 自动刷新**: 自动管理和刷新 OAuth Token
- **多凭据支持**: 支持配置多个凭据，按优先级自动故障转移
- **负载均衡**: 支持 `priority`（按优先级）和 `balanced`（均衡分配）两种模式
- **智能重试**: 单凭据最多重试 3 次，单请求最多重试 9 次
- **凭据回写**: 多凭据格式下自动回写刷新后的 Token
- **Thinking 模式**: 支持 Claude 的 extended thinking 功能
- **工具调用**: 完整支持 function calling / tool use；`/v1/responses` 额外兼容 Codex 的
  `additional_tools` / `custom` / `namespace` 工具方言（降级送达上游并在响应侧还原原形状）
- **WebSearch**: 内置 WebSearch 代执行（Anthropic 端点按 `stream` 返回 SSE 或 JSON；Responses 端点判定更宽且可开关）
- **端点注册表**: 对外端点由单一事实源登记，单测强制 `live` 可路由 / `planned` 必 404，防止清单漂移
- **多模型支持**: 支持 Sonnet、Opus、Haiku 系列模型
- **Admin 管理**: 可选的 Web 管理界面和 API，支持凭据管理、余额查询、运行时设置热更新、对外端点目录等
- **多级 Region 配置**: 支持全局和凭据级别的 Auth Region / API Region 配置
- **凭据级代理**: 支持为每个凭据单独配置 HTTP/SOCKS5 代理，优先级：凭据代理 > 全局代理 > 无代理

---

- [开始](#开始)
  - [1. 编译](#1-编译)
  - [2. 最小配置](#2-最小配置)
  - [3. 启动](#3-启动)
  - [4. 验证](#4-验证)
  - [下载 / Releases](#下载--releases)
- [Docker](#docker)
- [配置详解](#配置详解)
  - [config.json](#configjson)
  - [credentials.json](#credentialsjson)
  - [Region 配置](#region-配置)
  - [代理配置](#代理配置)
  - [认证方式](#认证方式)
  - [环境变量](#环境变量)
- [API 端点](#api-端点)
  - [标准端点 (/v1)](#标准端点-v1)
  - [Claude Code 兼容端点 (/cc/v1)](#claude-code-兼容端点-ccv1)
  - [OpenAI 兼容端点](#openai-兼容端点)
  - [WebSearch 工具](#websearch-工具)
  - [端点清单的单一事实源](#端点清单的单一事实源)
  - [Thinking 模式](#thinking-模式)
  - [工具调用](#工具调用)
- [模型映射](#模型映射)
- [Admin（可选）](#admin可选)
- [注意事项](#注意事项)
- [SpecCoding / OpenSpec 工作流](#speccoding--openspec-工作流)
- [项目结构](#项目结构)
- [技术栈](#技术栈)
- [License](#license)
- [致谢](#致谢)

## 开始

### 1. 编译

> PS: 不想本地编译时可直接下载发布包，详见 [下载 / Releases](#下载--releases)。
>
> - 正式版：<https://github.com/wtfdelphia/kiro.rs/releases/latest>
> - 开发滚动包：<https://github.com/wtfdelphia/kiro.rs/releases/tag/dev-latest>

> **前置步骤**：编译前需要先构建前端 Admin UI（用于嵌入到二进制中）：
> ```bash
> cd admin-ui && pnpm install && pnpm build
> ```

```bash
cargo build --release
```

### 2. 最小配置

创建 `config.json`：

```json
{
   "host": "127.0.0.1",
   "port": 8990,
   "apiKey": "sk-kiro-rs-qazWSXedcRFV123456",
   "region": "us-east-1"
}
```
> PS: 如果你需要 Web 管理面板, 请注意配置 `adminApiKey`

创建 `credentials.json`（从 Kiro IDE 等中获取凭证信息）：
> PS: 可以前往 Web 管理面板配置跳过本步骤
> 如果你对凭据地域有疑惑, 请查看 [Region 配置](#region-配置)

Social 认证：
```json
{
   "refreshToken": "你的刷新token",
   "expiresAt": "2025-12-31T02:32:45.144Z",
   "authMethod": "social"
}
```

IdC 认证：
```json
{
   "refreshToken": "你的刷新token",
   "expiresAt": "2025-12-31T02:32:45.144Z",
   "authMethod": "idc",
   "clientId": "你的clientId",
   "clientSecret": "你的clientSecret"
}
```

### 3. 启动

```bash
./target/release/kiro-rs
```

或指定配置文件路径：

```bash
./target/release/kiro-rs -c /path/to/config.json --credentials /path/to/credentials.json
```

### 4. 验证

```bash
curl http://127.0.0.1:8990/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: sk-kiro-rs-qazWSXedcRFV123456" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 1024,
    "stream": true,
    "messages": [
      {"role": "user", "content": "Hello, Claude!"}
    ]
  }'
```

#
### 运行时设置（Admin）

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET/PUT | `/api/admin/settings/proxy` | 全局出站代理（热更新 + 落盘；不回传明文密码） |
| GET/PUT | `/api/admin/settings/endpoint` | 默认 Kiro 端点（仅已注册名，当前 `ide`） |
| GET/PUT | `/api/admin/settings/auth` | 客户端 `requireApiKey` / `apiKey`（mask 读；热更新） |
| GET | `/api/admin/models/catalog` | 全局模型 catalog 摘要 |
| GET | `/api/admin/public-api` | 对外 API 端点目录（只读；Base URL / 鉴权 / status / curl 示例） |
| GET/PUT | `/api/admin/settings/websearch` | web_search 代执行开关（默认开启；仅影响 `/v1/responses`） |
| GET | `/api/admin/credentials/{id}/balance?force=true` | 强制刷新余额（跳过 TTL 缓存） |

> `settings/endpoint` 指的是**本代理访问上游 Kiro** 的端点（当前 `ide`）；
> `public-api` 指的是**客户端访问本代理**的对外端点。两者不可混用。
> Admin UI 顶栏「对外 API 端点」按钮可查看后者并一键复制配置。

`GET /v1/models`：优先全局模型缓存；仅暴露 `map_model` 可映射的 id；缓存为空时静态 fallback（不阻塞上游全量刷新）。

## 下载 / Releases

构建产物会发布到 GitHub Releases，可直接下载多平台二进制。

### 入口

| 类型 | 说明 | 链接 |
| --- | --- | --- |
| Releases 列表 | 所有正式版 / prerelease | <https://github.com/wtfdelphia/kiro.rs/releases> |
| 最新正式版 | 最近一次 `v*` 自动发布 | <https://github.com/wtfdelphia/kiro.rs/releases/latest> |
| 开发滚动包 | `dev` 分支滚动 prerelease | <https://github.com/wtfdelphia/kiro.rs/releases/tag/dev-latest> |
| 指定正式版示例 | 如 `v2026.7.27` | <https://github.com/wtfdelphia/kiro.rs/releases/tag/v2026.7.27> |

> 说明：`push v*` 后，`build.yaml` 会在多平台构建成功后自动创建正式 Release 并挂载二进制。  
> `dev` 分支成功构建后会更新滚动 prerelease `dev-latest`（不覆盖正式 Latest）。

### 资源命名

```text
kiro-rs-<version>-<platform>[.exe]
```

常见 platform：

- `Windows-x64`（`.exe`）
- `Linux-x64` / `Linux-arm64`
- `Linux-musl-x64` / `Linux-musl-arm64`
- `macOS-x64` / `macOS-arm64`

### 直链下载示例

正式版（把版本号换成实际 tag）：

```bash
# Linux x64
curl -fL -O https://github.com/wtfdelphia/kiro.rs/releases/download/v2026.7.27/kiro-rs-v2026.7.27-Linux-x64

# Windows x64
curl -fL -O https://github.com/wtfdelphia/kiro.rs/releases/download/v2026.7.27/kiro-rs-v2026.7.27-Windows-x64.exe

# macOS arm64
curl -fL -O https://github.com/wtfdelphia/kiro.rs/releases/download/v2026.7.27/kiro-rs-v2026.7.27-macOS-arm64
```

开发滚动包：

```bash
# Linux x64
curl -fL -O https://github.com/wtfdelphia/kiro.rs/releases/download/dev-latest/kiro-rs-dev-latest-Linux-x64

# Windows x64
curl -fL -O https://github.com/wtfdelphia/kiro.rs/releases/download/dev-latest/kiro-rs-dev-latest-Windows-x64.exe
```

通用模板：

```text
https://github.com/wtfdelphia/kiro.rs/releases/download/<tag>/kiro-rs-<tag>-<platform>[.exe]
```

> Windows / 部分环境下请使用 `curl.exe -fL -O ...`，并确保跟随重定向（`-L`）。

### 和 Docker 镜像的关系

- 二进制：GitHub Releases
- 容器镜像：GHCR 包页 <https://github.com/wtfdelphia/kiro.rs/pkgs/container/kiro-rs>
- 镜像名：`ghcr.io/wtfdelphia/kiro-rs`

## Docker

本仓库构建的镜像推送到 GHCR：

- 包页面：<https://github.com/wtfdelphia/kiro.rs/pkgs/container/kiro-rs>
- 镜像：`ghcr.io/wtfdelphia/kiro-rs`

常用 tag：

| Tag | 含义 |
| --- | --- |
| `latest` | 最近一次正式 `v*` 构建 |
| `v2026.7.27` 等 | 指定正式版本 |
| `beta` | `master` 分支 beta 构建 |
| `dev-latest` 相关 | 仅二进制 prerelease；Docker 工作流当前不发 dev 滚动镜像 |

拉取示例：

```bash
docker pull ghcr.io/wtfdelphia/kiro-rs:latest
# 或指定版本
docker pull ghcr.io/wtfdelphia/kiro-rs:v2026.7.27
```

也可以通过 Docker Compose 启动：

```bash
# 如使用本仓库镜像，可显式指定 owner/tag
# Linux/macOS:
#   IMAGE_OWNER=wtfdelphia IMAGE_TAG=latest docker compose up -d
# PowerShell:
#   $env:IMAGE_OWNER="wtfdelphia"; $env:IMAGE_TAG="latest"; docker compose up -d

docker compose up
```

需要将 `config.json` 和 `credentials.json` 挂载到容器中，具体参见 `docker-compose.yml`。

## 配置详解

### config.json

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `host` | string | `127.0.0.1` | 服务监听地址 |
| `port` | number | `8080` | 服务监听端口 |
| `apiKey` | string | - | 自定义 API Key（用于客户端认证；`requireApiKey=true` 时建议配置） |
| `requireApiKey` | bool | `true` | 是否要求客户端 API Key。`true` 且 `apiKey` 为空时 fail-closed（一律 401）；`false` 时客户端可不带 key（Admin 仍需 `adminApiKey`） |
| `region` | string | `us-east-1` | AWS 区域 |
| `authRegion` | string | - | Auth Region（用于 Token 刷新），未配置时回退到 region |
| `apiRegion` | string | - | API Region（用于 API 请求），未配置时回退到 region |
| `kiroVersion` | string | `0.11.107` | Kiro 版本号；可在 Admin「运行时设置 / 客户端标识」热更新 |
| `machineId` | string | - | 自定义机器码（64位十六进制），不定义则自动生成 |
| `systemVersion` | string | 随机 | 系统版本标识；可在 Admin「运行时设置 / 客户端标识」热更新 |
| `nodeVersion` | string | `22.22.0` | Node.js 版本标识；可在 Admin「运行时设置 / 客户端标识」热更新 |
| `tlsBackend` | string | `rustls` | TLS 后端：`rustls` 或 `native-tls` |
| `countTokensApiUrl` | string | - | 外部 count_tokens API 地址 |
| `countTokensApiKey` | string | - | 外部 count_tokens API 密钥 |
| `countTokensAuthType` | string | `x-api-key` | 外部 API 认证类型：`x-api-key` 或 `bearer` |
| `proxyUrl` | string | - | HTTP/SOCKS5 代理地址 |
| `proxyUsername` | string | - | 代理用户名 |
| `proxyPassword` | string | - | 代理密码 |
| `adminApiKey` | string | - | Admin API 密钥，配置后启用凭据管理 API 和 Web 管理界面 |
| `loadBalancingMode` | string | `priority` | 负载均衡模式：`priority`（按优先级）或 `balanced`（均衡分配） |
| `extractThinking` | boolean | `true` | 非流式响应的 thinking 块提取。启用后 `<thinking>` 标签会被解析为独立的 `thinking` 内容块 |
| `defaultEndpoint` | string | `ide` | 默认 Kiro 端点。凭据未显式指定 `endpoint` 时使用。当前支持：`ide` |
| `modelResolution.defaultChatModel` | string | `claude-sonnet-4.6` | `auto` 解析到的默认上游聊天模型 |
| `modelResolution.allowCatalogPassthrough` | bool | `true` | 允许命中 Kiro catalog 的上游模型 ID（如 `gpt-5.6-sol`）透传 |
| `modelResolution.exposeCompatAliasesInModels` | bool | `false` | 是否在公开 `/v1/models` 额外暴露 `auto` / `gpt-4o` / `gpt-4` 等兼容别名 |
| `modelResolution.compatAliases` | object | `{}` | 自定义兼容别名，key 为客户端 model，value 为上游 model |
| `webSearchEmulation` | bool | `true` | `/v1/responses` 的 web 搜索代执行开关。关闭后该端点的 `web_search` 工具走普通工具路径，不代替客户端执行搜索；不影响 Anthropic 端点。可在 Admin「运行时设置」热更新 |

完整配置示例：

```json
{
   "host": "127.0.0.1",
   "port": 8990,
   "apiKey": "sk-kiro-rs-qazWSXedcRFV123456",
   "region": "us-east-1",
   "tlsBackend": "rustls",
   "kiroVersion": "0.11.107",
   "machineId": "64位十六进制机器码",
   "systemVersion": "darwin#24.6.0",
   "nodeVersion": "22.22.0",
   "authRegion": "us-east-1",
   "apiRegion": "us-east-1",
   "countTokensApiUrl": "https://api.example.com/v1/messages/count_tokens",
   "countTokensApiKey": "sk-your-count-tokens-api-key",
   "countTokensAuthType": "x-api-key",
   "proxyUrl": "http://127.0.0.1:7890",
   "proxyUsername": "user",
   "proxyPassword": "pass",
   "adminApiKey": "sk-admin-your-secret-key",
   "loadBalancingMode": "priority",
   "extractThinking": true,
   "defaultEndpoint": "ide",
   "modelResolution": {
     "defaultChatModel": "claude-sonnet-4.6",
     "allowCatalogPassthrough": true,
     "exposeCompatAliasesInModels": false,
     "compatAliases": {
       "my-client-model": "claude-sonnet-4.6"
     }
   }
}
```

### credentials.json

可选身份字段：`userId`、`nickname`、`startUrl`（Admin 展示与导入 upsert）。Admin 另提供 `POST /api/admin/credentials/import`（默认按 userId upsert；裸 `POST /credentials` 默认 reject 重复）。

支持单对象格式（向后兼容）或数组格式（多凭据）。

#### 字段说明

| 字段             | 类型     | 描述                                          |
|----------------|--------|---------------------------------------------|
| `id`           | number | 凭据唯一 ID（可选，仅用于 Admin API 管理；手写文件可不填）        |
| `accessToken`  | string | OAuth 访问令牌（可选，可自动刷新）                        |
| `refreshToken` | string | OAuth 刷新令牌                                  |
| `profileArn`   | string | AWS Profile ARN（可选，登录时返回）                   |
| `expiresAt`    | string | Token 过期时间 (RFC3339)                        |
| `authMethod`   | string | 认证方式：`social` / `idc` / `external_idp` / `api_key`  |
| `provider`     | string | 身份提供方：`BuilderId` / `Github` / `Google` / `Enterprise` 等（可选，用于 profileArn 固定表） |
| `clientId`     | string | 客户端 ID（IdC 与 external_idp 认证必填）                |
| `clientSecret` | string | 客户端密钥（IdC 必填；external_idp 公共客户端可不填）        |
| `tokenEndpoint`| string | external_idp 的 OAuth2 token 端点（与 `issuerUrl` 二选一必填） |
| `issuerUrl`    | string | external_idp 的 issuer URL（未给 `tokenEndpoint` 时据此派生） |
| `scopes`       | string | external_idp 的 OAuth2 scopes，空格分隔（可选）          |
| `priority`     | number | 凭据优先级，数字越小越优先，默认为 0                         |
| `region`       | string | 凭据级 Auth Region, 兼容字段                       |
| `authRegion`   | string | 凭据级 Auth Region，用于 Token 刷新, 未配置时回退到 region |
| `apiRegion`    | string | 凭据级 API Region，用于 API 请求                    |
| `machineId`    | string | 凭据级机器码（64位十六进制）                             |
| `email`        | string | 用户邮箱（可选，从 API 获取）                           |
| `proxyUrl`     | string | 凭据级代理 URL（可选，特殊值 `direct` 表示不使用代理）       |
| `proxyUsername`| string | 凭据级代理用户名（可选）                                |
| `proxyPassword`| string | 凭据级代理密码（可选）                                 |
| `endpoint`     | string | 凭据级端点名称（可选，未配置时使用 `config.defaultEndpoint`）|

说明：
- IdC / Builder-ID / IAM 在本项目里属于同一种登录方式，配置时统一使用 `authMethod: "idc"`
- 为兼容旧配置，`builder-id` / `iam` 仍可被识别，但会按 `idc` 处理
- `external_idp` 亦可写作 `external-idp` / `azure` / `azuread` / `azure_ad`（大小写不敏感）
- **不在上述取值内的 `authMethod` 会被拒绝**，并在错误中列出合法取值；不会静默按 `social` 处理
- KAM 导入的 IdC 账号建议带 `provider`（缺省按 `BuilderId`）与可选 `profileArn`；服务会在请求前自动解析并缓存 `profileArn`（固定表 / ListAvailableProfiles / refresh fallback）
- `external_idp` 账号的 `provider` 应留空，`profileArn` 若有真实值会原样保留

##### external_idp（Microsoft Entra ID / Azure AD）

刷新走 Microsoft OAuth2 `refresh_token` grant，而非 AWS SSO OIDC。安全限制：

- `tokenEndpoint` 只接受 HTTPS，且域名必须是 `login.microsoftonline.com`、
  `login.microsoftonline.us`、`login.partner.microsoftonline.cn`、
  `login.chinacloudapi.cn` 之一或其子域
- 域名判定基于 URL 解析结果，不做字符串匹配；含 userinfo、IP 地址、`localhost` 的
  endpoint 一律拒绝
- 只给了 `issuerUrl` 时按 `{issuer}/oauth2/v2.0/token` 派生，派生结果再次校验
- 白名单硬编码、不可通过配置修改：可配置的白名单等于可绕过的白名单

公共客户端（无 `clientSecret`）可直接导入，不需要伪造密钥。

示例见 `credentials.example.external.json`。

#### KAM（kiro-account-manager）导出文件导入

支持四种容器格式：平铺单对象、平铺数组、`{ version, accounts: [...] }`、
旧版 `{ credentials: {...} }` 嵌套（单个或数组）。

**推荐路径是 Admin UI 的 KAM 导入**（`POST /api/admin/credentials/import/kam`）：
服务端完成容器判别与认证分类，逐条返回预检与导入结果，失败记录带明确原因。

也可以直接把 KAM 导出文件当作 `credentials.json` 使用，属离线迁移能力：

- 启动时识别到 KAM 容器格式后，会先备份原文件（`credentials.json.kam-backup-<时间戳>`），
  再原子写回为原生数组格式
- 迁移失败不影响启动，原文件保持不变，下次启动重试
- 无法识别的 JSON 结构会明确报错并指出位置与顶层字段名，不会静默加载为空凭据

导入时的字段映射需注意两点：

- KAM 的 `enabled`（默认 `true`）与本项目的 `disabled`（默认 `false`）语义相反，
  导入时会取反映射，上游禁用的账号不会变成启用
- KAM 的 `label` 映射为 `nickname`；`region` 写入通用 `region` 字段
  （Auth Region 经回退链派生；API Region 需要时请显式设置 `apiRegion`）

KAM 导出文件包含明文 `refreshToken`、`clientSecret` 以及账号与代理密码。
导入预览只显示字段是否已配置，不显示这些值；账号密码与代理配置不会被导入。

#### 单凭据格式（旧格式，向后兼容）

```json
{
   "accessToken": "请求token，一般有效期一小时，可选",
   "refreshToken": "刷新token，一般有效期7-30天不等",
   "profileArn": "arn:aws:codewhisperer:us-east-1:111112222233:profile/QWER1QAZSDFGH",
   "expiresAt": "2025-12-31T02:32:45.144Z",
   "authMethod": "social",
   "clientId": "IdC 登录需要",
   "clientSecret": "IdC 登录需要"
}
```

#### 多凭据格式（支持故障转移和自动回写）

```json
[
   {
      "refreshToken": "第一个凭据的刷新token",
      "expiresAt": "2025-12-31T02:32:45.144Z",
      "authMethod": "social",
      "priority": 0
   },
   {
      "refreshToken": "第二个凭据的刷新token",
      "expiresAt": "2025-12-31T02:32:45.144Z",
      "authMethod": "idc",
      "clientId": "xxxxxxxxx",
      "clientSecret": "xxxxxxxxx",
      "region": "us-east-2",
      "priority": 1,
      "proxyUrl": "socks5://proxy.example.com:1080",
      "proxyUsername": "user",
      "proxyPassword": "pass"
   },
   {
      "refreshToken": "第三个凭据（显式不走代理）",
      "expiresAt": "2025-12-31T02:32:45.144Z",
      "authMethod": "social",
      "priority": 2,
      "proxyUrl": "direct"
   }
]
```

多凭据特性：
- 按 `priority` 字段排序，数字越小优先级越高（默认为 0）
- 单凭据最多重试 3 次，单请求最多重试 9 次
- 自动故障转移到下一个可用凭据
- 多凭据格式下 Token 刷新后自动回写到源文件

### Region 配置

支持多级 Region 配置，分别控制 Token 刷新和 API 请求使用的区域。

**Auth Region**（Token 刷新）优先级：
`凭据.authRegion` > `凭据.region` > `config.authRegion` > `config.region`

**API Region**（API 请求）优先级：
`凭据.apiRegion` > `config.apiRegion` > `config.region`

### 代理配置

支持全局代理和凭据级代理，凭据级代理会覆盖该凭据产生的所有出站连接（API 请求、Token 刷新、额度查询）。

**代理优先级**：`凭据.proxyUrl` > `config.proxyUrl` > 无代理

| 凭据 `proxyUrl` 值 | 行为 |
|---|---|
| 具体 URL（如 `http://proxy:8080`、`socks5://proxy:1080`） | 使用凭据指定的代理 |
| `direct` | 显式不使用代理（即使全局配置了代理） |
| 未配置（留空） | 回退到全局代理配置 |

凭据级代理示例：

```json
[
   {
      "refreshToken": "凭据A：使用自己的代理",
      "authMethod": "social",
      "proxyUrl": "socks5://proxy-a.example.com:1080",
      "proxyUsername": "user_a",
      "proxyPassword": "pass_a"
   },
   {
      "refreshToken": "凭据B：显式不走代理（直连）",
      "authMethod": "social",
      "proxyUrl": "direct"
   },
   {
      "refreshToken": "凭据C：使用全局代理（或直连，取决于 config.json）",
      "authMethod": "social"
   }
]
```

### 认证方式

客户端请求本服务时，支持两种认证方式：

1. **x-api-key Header**
   ```
   x-api-key: sk-your-api-key
   ```

2. **Authorization Bearer**
   ```
   Authorization: Bearer sk-your-api-key
   ```

### 环境变量

可通过环境变量配置日志级别：

```bash
RUST_LOG=debug ./target/release/kiro-rs
```

## API 端点

### 标准端点 (/v1)

| 端点 | 方法 | 描述 |
|------|------|------|
| /v1/models | GET | 获取可用模型列表（优先全局模型缓存并附 -thinking 变体；缓存为空时回退内置静态列表） |
| `/v1/messages` | POST | 创建消息（对话） |
| `/v1/messages/count_tokens` | POST | 估算 Token 数量 |

### Claude Code 兼容端点 (/cc/v1)

| 端点 | 方法 | 描述 |
|------|------|------|
| `/cc/v1/messages` | POST | 创建消息（缓冲模式，确保 `input_tokens` 准确） |
| `/cc/v1/messages/count_tokens` | POST | 估算 Token 数量（与 `/v1` 相同） |

### OpenAI 兼容端点

| 端点 | 方法 | 描述 |
|------|------|------|
| `/v1/chat/completions` | POST | OpenAI Chat Completions（流式 + 非流式，含 function tools） |
| `/v1/responses` | POST | OpenAI Responses（语义事件流 + 非流式，无状态；支持 web_search 代执行与 Codex 工具方言兼容） |

接入注意：

- `OPENAI_BASE_URL` 需带 `/v1` 后缀（`ANTHROPIC_BASE_URL` 不带），这是最高频的配置错误
- 响应回显的 `model` 为客户端请求的原始名（如 `gpt-4o`），不是实际执行的 Claude 模型
- 流式 `usage` 需客户端传 `stream_options: {"include_usage": true}` 才在末尾返回
- `temperature` / `top_p` / `tool_choice` 接受但不透传（Kiro 上游无对应字段）
- 图片仅支持 base64 data URL；远程 http(s) 图片 URL 会被跳过
- `/v1/chat/completions` 不劫持 `web_search`：名为 `web_search` 的工具作为普通 function tool
  透传上游（代执行仅存在于 `/v1/responses`，见下文）
- 未实现 `logprobs`、`n>1`、`seed`、`stop`、`logit_bias`

`/v1/responses` 额外注意：

- **无状态**：携带 `previous_response_id` 返回 400，请在 `input` 中带上完整对话；`store` 被忽略
- SSE 为命名语义事件（`event: response.*`），与 `/v1/chat/completions` 的纯 `data:` 行不同
- `input` 支持字符串 / item 数组 / 单个 item 对象三种形状
- 顶层 `tools` 只声明**单个** web_search 工具时由本代理执行搜索并返回 `web_search_call` +
  `message` 输出；可在 Admin 运行时设置中关闭（关闭后该工具走正常工具路径）
- 该端点的 web_search 判定比 `/v1/messages` 宽（含 `web_search_20250305` 等形状），
  两端点行为差异是有意选择
- 未实现 `GET /v1/responses/{id}`、`include`、`truncation`、`parallel_tool_calls`
- 不支持结构化输出：请求携带 `text.format` 时该字段被忽略并打 warning。
  Kiro 上游的消息上下文只有工具定义与工具结果，没有 response format 概念，无处透传；
  不做 prompt 层模拟（模拟会把 `strict: true` 降级成「尽力」，属假装支持）

**客户端工具方言兼容**（面向 OpenAI Codex 等发送非标准工具形状的客户端）：

| 客户端形状 | 本代理的处理 |
| --- | --- |
| `input[0]` 的 `additional_tools` item | 提取其 `tools[]` 并入顶层工具列表（该 item 不产生对话消息）。部分模型不发顶层 `tools` 字段，工具全在这里 |
| `type: "custom"`（裸文本输入工具） | 降级为 schema 为 `{input: string}` 的 function；原 description 保留并在前面加调用约定说明，`format.definition`（如 lark 文法）追加到末尾 |
| `type: "namespace"`（工具分组） | 内层工具展平为 `<namespace>__<name>` 的独立工具；响应侧还原为 `(namespace, 原名)` |
| 历史 `custom_tool_call` / `custom_tool_call_output` | 改写为 function 调用与 tool 结果，保证上一轮的调用与结果成对存活 |

- 由 `custom` 降级而来的工具，其调用在响应中还原为 `custom_tool_call`（带 `input` 而非
  `arguments`），流式与非流式都做。这不是可选的美化：客户端为此类工具登记的 payload
  类型只接受裸文本，收到 `function_call` 会拒绝自己的工具调用并触发模型重试
- 展平名或原工具名超长时沿用既有工具名缩短机制，响应侧按两级映射还原
- 展平后与顶层工具重名、或两个 namespace 展平到同名时返回 400 并指出冲突双方，
  不自动改名（自动改名会让同一工具在不同轮次有不同名字，客户端无法预测）
- 一次发送多个工具的客户端（如 Codex）不会触发 web_search 代执行：
  代执行要求顶层 `tools` 恰好一个工具，而这类客户端的工具通常经 `additional_tools` 承载、
  顶层 `tools` 为空。这是确定结果而非边界情况

> **`/cc/v1/messages` 与 `/v1/messages` 的区别**：
> - `/v1/messages`：实时流式返回，`message_start` 中的 `input_tokens` 是估算值
> - `/cc/v1/messages`：缓冲模式，等待上游流完成后，用从 `contextUsageEvent` 计算的准确 `input_tokens` 更正 `message_start`，然后一次性返回所有事件
> - 等待期间会每 25 秒发送 `ping` 事件保活

### WebSearch 工具

`/v1/messages` 与 `/cc/v1/messages` 在请求**恰好声明一个** `web_search` 工具时，
由本代理通过 Kiro MCP 执行搜索并直接构造响应（不走模型生成）。
响应形态跟随请求的 `stream` 字段：

- `stream: true` → SSE 事件流
- `stream: false`（或缺省）→ 标准 message JSON 对象

两种模式的内容块一致：`text`（搜索说明）、`server_tool_use`、
`web_search_tool_result`、`text`（结果摘要），`usage` 中带
`server_tool_use.web_search_requests`。

混合工具（web_search 与其它工具同时声明）不触发代执行，按普通工具转发上游。
`/v1/responses` 的 web_search 判定更宽且可开关，见上文 OpenAI 兼容端点。

### 端点清单的单一事实源

上表的对外端点由 `src/public_api/catalog.rs` 统一登记，启动日志的「可用 API」列表与
Admin `GET /api/admin/public-api` 均由它派生，避免多处手写清单互相漂移。
单测强制 `status=live` 的端点必须能被真实路由命中，`status=planned` 的必须命中不到（404）。

已登记但尚未实现（`planned`，当前请求返回 404）：`GET /v1/responses/{id}`。

### Thinking 模式

支持 Claude 的 extended thinking 功能：

```json
{
  "model": "claude-sonnet-4-20250514",
  "max_tokens": 16000,
  "thinking": {
    "type": "enabled",
    "budget_tokens": 10000
  },
  "messages": [...]
}
```

### 工具调用

完整支持 Anthropic 的 tool use 功能：

```json
{
  "model": "claude-sonnet-4-20250514",
  "max_tokens": 1024,
  "tools": [
    {
      "name": "get_weather",
      "description": "获取指定城市的天气",
      "input_schema": {
        "type": "object",
        "properties": {
          "city": {"type": "string"}
        },
        "required": ["city"]
      }
    }
  ],
  "messages": [...]
}
```

## 模型映射

| Anthropic 模型 | Kiro 模型 |
|----------------|-----------|
| `*sonnet-5*` | `claude-sonnet-5` |
| `*sonnet*`（含 4.6/4-6） | `claude-sonnet-4.6` |
| `*sonnet*`（含 4.5/4-5） | `claude-sonnet-4.5` |
| `*opus*`（含 4.8/4-8） | `claude-opus-4.8` |
| `*opus*`（含 4.7/4-7） | `claude-opus-4.7` |
| `*opus*`（含 4.6/4-6） | `claude-opus-4.6` |
| `*opus*`（含 4.5/4-5） | `claude-opus-4.5` |
| `*haiku*` | `claude-haiku-4.5` |

Sonnet 5 的 thinking 行为与已知限制见 [docs/claude-sonnet-5.md](docs/claude-sonnet-5.md)。

## Admin（可选）

当 `config.json` 配置了非空 `adminApiKey` 时，会启用：

Admin API 使用**独立的** `adminApiKey` 认证（不是客户端 `apiKey`），全部接口都在鉴权中间件之内，未携带有效 key 一律 401。

- **凭据管理**
  - `GET /api/admin/credentials` - 获取所有凭据状态
  - `POST /api/admin/credentials` - 添加新凭据
  - `DELETE /api/admin/credentials/:id` - 删除凭据
  - `POST /api/admin/credentials/import` - 导入单个凭据
  - `POST /api/admin/credentials/import/batch` - 批量导入凭据
  - `POST /api/admin/credentials/:id/disabled` - 设置凭据禁用状态
  - `POST /api/admin/credentials/:id/priority` - 设置凭据优先级
  - `POST /api/admin/credentials/:id/reset` - 重置失败计数
  - `GET /api/admin/credentials/:id/balance` - 获取凭据余额（`?force=true` 跳过 TTL 缓存）
  - `POST /api/admin/credentials/:id/refresh` - 强制刷新 Token

- **模型目录**
  - `POST /api/admin/credentials/models/refresh` - 刷新全部启用凭据的模型目录
  - `POST /api/admin/credentials/:id/models/refresh` - 刷新单凭据模型目录
  - `GET /api/admin/credentials/:id/models` - 查看凭据模型缓存（`?live=true` 时先刷新）
  - `POST /api/admin/credentials/:id/test` - 对凭据做最小真实推理探测（可选 body model，默认 claude-sonnet-4.6）
  - `GET /api/admin/models/catalog` - 全局模型 catalog 摘要

- **在线认证**
  - `POST /api/admin/auth/builderid/start` / `poll` - Builder ID 设备码流程
  - `POST /api/admin/auth/iam-sso/start` / `complete` - IAM Identity Center (SSO) 流程
  - `POST /api/admin/auth/sso-token` - 直接导入 SSO Token

- **运行时设置（均为热更新 + 落盘，详见[运行时设置（Admin）](#运行时设置admin)）**
  - `GET/PUT /api/admin/settings/proxy` - 全局出站代理（不回传明文密码）
  - `GET/PUT /api/admin/settings/endpoint` - 默认 Kiro **上游**端点
  - `GET/PUT /api/admin/settings/auth` - 客户端 `requireApiKey` / `apiKey`（读取为掩码）
  - `GET/PUT /api/admin/settings/websearch` - web_search 代执行开关（默认开启，仅影响 `/v1/responses`）
  - `GET/PUT /api/admin/settings/client-identity` - Kiro / System / Node 版本标识
  - `GET/PUT /api/admin/config/load-balancing` - 负载均衡模式

- **对外端点目录**
  - `GET /api/admin/public-api` - 只读的对外 API 端点清单（Base URL、鉴权方式、status、curl 示例）。永不回传完整客户端 key，只给掩码

- **Admin UI**
  - `GET /admin` - 访问管理页面（需要在编译前构建 `admin-ui/dist`）
  - 顶栏入口：「运行时设置」（代理 / 上游端点 / 客户端鉴权 / websearch 开关 / 客户端标识）、「对外 API 端点」（分组端点卡 + 客户端配方 + 一键复制）
  - 列表区「刷新全部模型」；凭据卡片「查看模型 / 刷新模型 / 测试」

## 注意事项

1. **凭证安全**: 请妥善保管 `credentials.json` 文件，不要提交到版本控制
2. **Token 刷新**: 服务会自动刷新过期的 Token，无需手动干预
3. **WebSearch 工具**: 当 `tools` 列表仅包含一个 `web_search` 工具时，由本代理代执行搜索并直接构造响应（不走模型生成）。混合工具不触发。Anthropic 端点的响应形态跟随请求的 `stream` 字段；`/v1/responses` 的判定更宽且可在 Admin 关闭
4. **Admin 密钥独立**: Admin API 用 `adminApiKey` 认证，与客户端 `apiKey` 是两把不同的钥匙


## SpecCoding / OpenSpec 工作流

本仓库的 AI 辅助开发入口以项目内规则为准：

- [AGENTS.md](AGENTS.md)：Codex / 通用 Agent 主规则，包含 OpenSpec 条件、门禁 skill、验证纪律与安全要求
- [CLAUDE.md](CLAUDE.md)：Claude Code 最小入口，指向同一套主规则
- [spec/](spec/)：长期需求、设计、目录归属事实
- [openspec/changes/<change-name>/](openspec/changes/)：单次变更 proposal / design / tasks / specs / evidence
- [docs/tooling-sources.md](docs/tooling-sources.md)：OpenSpec、CodeGraph、rg、Node、Rust、pnpm 等工具来源与核验版本
- [docs/multi-protocol-api-design.md](docs/multi-protocol-api-design.md)：多协议对外 API 的设计定稿（OpenAI 兼容层的决策记录与分期落地）
- [docs/AI 辅助开发工程化落地白皮书.md](docs/AI%20辅助开发工程化落地白皮书.md)：本次工程化落地参考

推荐闭环：

```text
openspec new change / 补齐工件
  -> openspec-superpowers-bridge（Bridge Plan）
  -> 小步实现并更新 tasks.md
  -> spec-compliance-check
  -> openspec-verify-change
  -> README/AGENTS/spec 同步判断
  -> verification-before-completion
  -> openspec archive（人工确认后）
```

## 项目结构

```
kiro-rs/
├── src/
│   ├── main.rs                 # 程序入口
│   ├── http_client.rs          # HTTP 客户端构建
│   ├── token.rs                # Token 计算模块
│   ├── debug.rs                # 调试工具
│   ├── test.rs                 # 测试
│   ├── model/                  # 配置和参数模型
│   │   ├── config.rs           # 应用配置
│   │   └── arg.rs              # 命令行参数
│   ├── anthropic/              # Anthropic API 兼容层
│   │   ├── router.rs           # 路由配置
│   │   ├── handlers.rs         # 请求处理器
│   │   ├── middleware.rs       # 认证中间件
│   │   ├── types.rs            # 类型定义
│   │   ├── converter.rs        # 协议转换器
│   │   ├── stream.rs           # 流式响应处理
│   │   └── websearch.rs        # WebSearch 工具处理（按 stream 返回 SSE 或 JSON）
│   ├── openai/                 # OpenAI API 兼容层
│   │   ├── mod.rs              # 路由与鉴权/CORS/体积上限 layer
│   │   ├── types.rs            # Chat Completions 类型（工具定义双形状）
│   │   ├── converter.rs        # 映射到内部 Anthropic 形状后复用既有转换核
│   │   ├── handlers.rs         # 请求处理器（prepare 前置为两端点共用）
│   │   ├── stream.rs           # Chat Completions chunk 流
│   │   ├── responses.rs        # Responses 入参归一（含无状态校验）
│   │   ├── responses_types.rs  # Responses 类型
│   │   ├── responses_stream.rs # Responses 命名语义事件流
│   │   ├── websearch.rs        # Responses 端点的 web_search 代执行
│   │   └── error.rs            # OpenAI 错误方言
│   ├── public_api/             # 对外端点注册表
│   │   ├── catalog.rs          # canonical 端点清单（单一事实源）
│   │   ├── dto.rs              # Admin 只读视图 DTO（密钥仅掩码）
│   │   └── routes_test.rs      # 双向防漂移断言（live 可路由 / planned 必 404）
│   ├── kiro/                   # Kiro API 客户端
│   │   ├── provider.rs         # API 提供者
│   │   ├── token_manager.rs    # Token 管理
│   │   ├── machine_id.rs       # 设备指纹生成
│   │   ├── model/              # 数据模型
│   │   │   ├── credentials.rs  # OAuth 凭证
│   │   │   ├── events/         # 响应事件类型
│   │   │   ├── requests/       # 请求类型
│   │   │   ├── common/         # 共享类型
│   │   │   ├── token_refresh.rs # Token 刷新模型
│   │   │   └── usage_limits.rs # 使用额度模型
│   │   └── parser/             # AWS Event Stream 解析器
│   │       ├── decoder.rs      # 流式解码器
│   │       ├── frame.rs        # 帧解析
│   │       ├── header.rs       # 头部解析
│   │       ├── error.rs        # 错误类型
│   │       └── crc.rs          # CRC 校验
│   ├── admin/                  # Admin API 模块
│   │   ├── router.rs           # 路由配置
│   │   ├── handlers.rs         # 请求处理器
│   │   ├── service.rs          # 业务逻辑服务
│   │   ├── types.rs            # 类型定义
│   │   ├── middleware.rs       # 认证中间件
│   │   └── error.rs            # 错误处理
│   ├── admin_ui/               # Admin UI 静态文件嵌入
│   │   └── router.rs           # 静态文件路由
│   └── common/                 # 公共模块
│       └── auth.rs             # 认证工具函数
├── admin-ui/                   # Admin UI 前端工程（构建产物会嵌入二进制）
├── tools/                      # 辅助工具
├── Cargo.toml                  # 项目配置
├── config.example.json         # 配置示例
├── docker-compose.yml          # Docker Compose 配置
└── Dockerfile                  # Docker 构建文件
```

## 技术栈

- **Web 框架**: [Axum](https://github.com/tokio-rs/axum) 0.8
- **异步运行时**: [Tokio](https://tokio.rs/)
- **HTTP 客户端**: [Reqwest](https://github.com/seanmonstar/reqwest)
- **序列化**: [Serde](https://serde.rs/)
- **日志**: [tracing](https://github.com/tokio-rs/tracing)
- **命令行**: [Clap](https://github.com/clap-rs/clap)

## License

MIT

## 致谢

本项目的实现离不开前辈的努力:  
 - [kiro2api](https://github.com/caidaoli/kiro2api)
 - [proxycast](https://github.com/aiclientproxy/proxycast)

本项目部分逻辑参考了以上的项目, 再次由衷的感谢!
