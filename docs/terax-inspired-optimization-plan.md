# 借鉴 terax-ai 的 kiro-rs 优化方案

来源：`docs/terax-ai-src-tauri-architecture-review.md` 的架构分析
对象：kiro-rs（当前工作区）
性质：方案文档，未改动任何代码

## 一、先说结论

分析 terax 时我把 SSRF 防护列为最高价值借鉴点，那时的判断依据是「kiro-rs 的出站目标可被配置影响」。回到本项目逐行核实后，这个判断需要修正得更具体：**风险不在 base URL 被整体替换，而在 `region` 字符串被无校验地拼进 host**。这是一个可验证的注入面，不是理论推演。

五项建议按证据强度和投入产出排序：

| # | 建议 | 证据强度 | 代价 | OpenSpec |
| --- | --- | --- | --- | --- |
| 1 | `region` 白名单校验 | 已确认可注入 | 小 | 需要 |
| 2 | Admin 路由补 body limit | 已确认缺失 | 极小 | 需要 |
| 3 | 出站 client 加重定向策略 | 已确认缺失 | 小 | 需要 |
| 4 | 细分 `AdminServiceError` | 已确认塌缩 | 中 | 需要 |
| 5 | 拆分 `token_manager.rs` | 结构判断 | 大 | 需要 |

第 5 项我倾向于**暂不推进**，理由在第七节说明。

## 二、region 无校验拼接（建议 1）

### 现状证据

`region` 从配置和凭据流向 URL host，全程没有格式校验：

```
src/kiro/model/credentials.rs:409  effective_api_region()  凭据.api_region > config.api_region > config.region
src/kiro/endpoint/ide.rs:31        host()      format!("q.{}.amazonaws.com", region)
src/kiro/endpoint/ide.rs:67        api_url()   format!("https://q.{}.amazonaws.com/generateAssistantResponse", region)
src/kiro/token_manager.rs:457      host        format!("q.{}.amazonaws.com", region)
src/kiro/online_auth.rs:464        oidc_base() format!("https://oidc.{region}.amazonaws.com")
```

写入路径是打通的：`src/admin/types.rs:169-176` 定义了凭据级 `region` / `auth_region` / `api_region` 三个可选字段，经 Admin API 的凭据导入与设置接口落库。`src/admin/handlers.rs:188,216,246` 三个登录端点也直接接收 `payload.region`。

一个形如 `us-east-1.attacker.com` 的取值会让 `api_url()` 产出 `https://q.us-east-1.attacker.com.amazonaws.com/...`；而含 `@` 或 `/` 的取值可以改变 URL 的 authority 解析结果。凭 Kiro 的 Bearer token 会随请求头一起发出，这是凭据外泄路径，不只是连错地址。

### terax 的对应做法

`workspace.rs:is_safe_distro_name()` 处理的是同一类问题——一个字符串要被拼进 UNC 路径 `\\wsl.localhost\<distro>\`。它的三条原则值得照搬：

1. **白名单而非黑名单**：只允许字母数字与 `. _ -`，而不是逐个禁止危险字符
2. **在拼接点校验，不在入口校验**：入口可能有多个，拼接点只有一个
3. **失败时返回必然无效的哨兵值**，让下游的自然检查拒绝，而不是 panic 或静默放行

### 建议实现

AWS region 的实际格式是 `<group>-<direction>-<number>`，字符集比 WSL 发行版名更窄，白名单可以更严：

```rust
// 建议位置：src/kiro/endpoint/mod.rs 或 src/common/
/// AWS region 白名单校验。
///
/// region 会被拼进请求 host（`q.{region}.amazonaws.com`），未校验的取值
/// 可以把请求连同 Bearer token 导向任意主机。真实 region 形如
/// `us-east-1` / `ap-northeast-3`，只含小写字母、数字和连字符。
pub fn is_safe_region(region: &str) -> bool {
    !region.is_empty()
        && region.len() <= 32
        && region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !region.starts_with('-')
        && !region.ends_with('-')
        && !region.contains("--")
}
```

这条规则同时挡掉 `.`（阻断子域后缀嫁接）、`@`（阻断 userinfo 混淆）、`/`（阻断路径逃逸）和大写形态的绕过。

落点是三个拼接函数：`IdeEndpoint::host/api_url/mcp_url`、`token_manager` 里构造 `getUsageLimits` URL 处、`online_auth::oidc_base`。校验失败的处置需要按调用点区分：

- `oidc_base` 与 `token_manager` 返回 `Result`，直接返回明确错误
- `api_url` / `mcp_url` 返回 `String`，签名改 `Result` 会波及 trait 与全部实现。更小的改法是在凭据落库和配置加载时拒绝非法 region，拼接点作为第二道防线返回哨兵 host（如 `invalid-region.localhost`），让请求以连接失败告终而非发往攻击者

我倾向于**两处都做**：入口拒绝给出可读报错，拼接点兜底防止漏掉新增的写入路径。这正是 terax 的分层思路。

### 验证要求

单测覆盖：正常 region 通过（`us-east-1`、`ap-northeast-3`、`eu-central-1`）；注入形态被拒（`us-east-1.attacker.com`、`us-east-1@evil.com`、`../x`、`US-EAST-1`、空串、`-us-east-1`、`us--east-1`）。再加一条断言：非法 region 下 `api_url()` 的输出不包含攻击者域名。

## 三、Admin 路由缺 body limit（建议 2）

### 现状证据

`src/anthropic/router.rs:20` 定义 `MAX_BODY_SIZE = 50MB`，anthropic 与 openai 两个路由树都挂了 `DefaultBodyLimit::max(MAX_BODY_SIZE)`。`src/admin/router.rs` 的 `create_admin_router` 只挂了 `admin_auth_middleware`，没有 body limit,因此走 axum 默认 2MB。

这带来一个具体后果：`/credentials/import/batch` 和 `/credentials/import/kam` 是批量导入端点，KAM 文档或批量凭据超过 2MB 时会被 axum 以 413 拒绝，报错与「凭据格式问题」难以区分。AGENTS.md 里已经记过同类坑——「merge 不传播 layer，漏 body limit 会退回 axum 默认 2MB 导致带图请求 413」。Admin 侧是同一个坑的另一半。

### 建议实现

给 admin router 显式挂上限。数值不宜直接复用 50MB：Admin 端点收的是 JSON 凭据文档，不是带图对话，独立常量更能表达意图。

```rust
// src/admin/router.rs
/// Admin API 请求体上限。
///
/// 批量凭据与 KAM 文档导入会超过 axum 默认的 2MB；同时 Admin 收的是
/// JSON 文档而非多模态对话体，不必放到 anthropic 侧的 50MB。
const ADMIN_MAX_BODY_SIZE: usize = 8 * 1024 * 1024;
```

顺带值得考虑的是 terax `control.rs` 的另一半防护：它除了帧大小上限，还有 `MAX_CONNECTIONS` 32 与 `MAX_PENDING_REQUESTS` 32，超限直接回 `server_busy` 而不是排队堆积。kiro-rs 的 Admin 侧目前没有并发封顶,`/credentials/models/refresh` 这类全量刷新端点被并发调用时会同时打向上游。不过这需要先确认是否为真实痛点，我不建议在没有观测数据时就加限流——**这一项列为待观察，不进本轮方案**。

### 验证要求

集成测试：构造略超 2MB 的合法批量导入请求，断言不再返回 413。同时断言超过 `ADMIN_MAX_BODY_SIZE` 的请求仍被拒绝，避免上限形同虚设。

## 四、出站 client 缺重定向策略（建议 3）

### 现状证据

`src/http_client.rs:47` 的 `build_client()` 是全项目唯一的 client 构建点，17 处调用（token.rs、user_info.rs、token_manager.rs 四处、provider.rs 两处、profile.rs、online_auth.rs 五处、models_api.rs、admin/service.rs）。它设置了 `timeout`、TLS 后端、可选代理，**没有设置重定向策略**，因此走 reqwest 默认的「最多 10 跳自动跟随」。

调用点收敛到一个函数是好事：改造面小，收益覆盖全部出站请求。

### terax 的对应做法

`net.rs:build_safe_client()` 用 `redirect::Policy::custom` 在**每一跳**重跑判定：scheme 必须是 http/https、拒绝 URL 内嵌 userinfo、拒绝云元数据主机名、按 IP 分级决定是否放行。关键洞察是「只校验首个 URL 等于没校验」——攻击者控制的服务器可以用 302 把请求导向内网。

### 建议实现

kiro-rs 的上游都是已知的 AWS 域名，不需要 terax 那套完整的 IP 四级分类。够用且低风险的版本是：

```rust
// src/http_client.rs，在 build_client 内
builder = builder.redirect(reqwest::redirect::Policy::custom(|attempt| {
    if attempt.previous().len() > 5 {
        return attempt.error("重定向次数过多");
    }
    let next = attempt.url();
    // 上游是固定的 AWS HTTPS 端点；降级到 http 或出现 userinfo
    // 都意味着请求被引导到了非预期目标，Bearer token 不应继续发送。
    if next.scheme() != "https" {
        return attempt.stop();
    }
    if !next.username().is_empty() || next.password().is_some() {
        return attempt.stop();
    }
    attempt.follow()
}));
```

`attempt.stop()` 的语义是停止跟随并把当前响应返回给调用方，不是报错，因此不会把正常的 AWS 重定向变成失败。

这里有个需要提醒的取舍：如果某个上游确实依赖 http 跳转或跨域重定向，这条策略会改变现有行为。**上线前需要在真实凭据下跑一遍完整的登录与对话链路**，我无法通过静态阅读确认 AWS 侧不存在这类跳转。

### 验证要求

单测只能覆盖 client 构建成功。真正的验证需要本地起一个会 302 到 http 的测试服务，断言请求不跟随。这部分建议放在 OpenSpec change 的验证清单里，而不是留给 CI。

## 五、AdminServiceError 语义塌缩（建议 4）

### 现状证据

`src/admin/error.rs` 共 5 个变体，`UpstreamError(String)` 在 `service.rs` 里被使用 7 处（1059、1469、1485、1524、1550、1601、1637 行），承接的失败原因至少包括：Builder ID 登录启动失败、SSO 轮询上游错误、批量操作的聚合失败、凭据测试失败。

`service.rs:1481-1489` 那段尤其能说明问题——代码在用字符串匹配把 `UpstreamError` 拆开：

```rust
.map_err(|e| {
    let msg = e.to_string();
    if msg.contains("not found") || msg.contains("expired") {
        AdminServiceError::InvalidCredential(msg)
    } else {
        AdminServiceError::UpstreamError(msg)
    }
})
```

`service.rs:1521` 和 `1547` 还有 `msg.contains("startUrl")`、`msg.contains("无效")`、`msg.contains("状态")` 的匹配。**用中文错误消息的子串做控制流分支**，任何一次文案调整都会静默改变 HTTP 状态码。这是类型信息在跨层传递时丢失后的必然补偿手段。

### terax 的对应做法

`git/errors.rs` 的做法有两层。表层是 16 个语义变体，每个 `Display` 带可执行建议（`NoUpstream` 会说「Run `git push -u <remote> <branch>` in the terminal first」）。深层是 `classify_auth_error()`：把不可避免的 stderr 文本匹配**集中在一个函数里**，产出结构化变体，其余代码只跟枚举打交道。

第二层才是关键。kiro-rs 需要的不是「更多变体」，而是「字符串匹配只出现在一个地方」。

### 建议实现

分两步，第一步的收益大且风险低：

**第一步——把散落的匹配收敛成一个分类函数。** 不动枚举定义，只把 `service.rs` 里几处 `msg.contains(...)` 抽成 `classify_online_auth_error(&anyhow::Error) -> AdminServiceError`。收益是文案改动不再影响状态码，且分类规则可被单测直接覆盖。

**第二步——按可观测的处置差异补变体。** 我建议只加确实需要区别对待的：

```rust
/// 上游返回配额耗尽（月度请求数用尽），凭据会被禁用并转移
QuotaExhausted { credential_id: u64 },
/// 上游限流，调用方应退避重试
RateLimited { retry_after_secs: Option<u64> },
/// 上游请求超时（与「上游返回错误」区分：前者可重试）
UpstreamTimeout { operation: &'static str },
```

这三个的判据是「调用方的正确反应不同」：配额耗尽要换凭据，限流要退避，超时可以直接重试。至于「token 过期」，`token_manager` 内部已有 `MAX_FAILURES_PER_CREDENTIAL` 与强制刷新机制处理，暴露到 Admin 层未必有新增价值,**不建议为了枚举整齐而加**。

`status_code()` 相应映射：`QuotaExhausted` 与 `RateLimited` 应为 429 而非当前的 502，`UpstreamTimeout` 用 504。这是对外契约变化，必须走 OpenSpec。

### 验证要求

`error.rs` 现有 4 个测试已覆盖状态码映射与「响应不含密钥字段」，新变体需补齐同样两类断言。分类函数需要针对每条真实上游错误样本各一个用例。注意现有测试里 `upstream_error_response_has_no_secret_fields` 这类断言必须保留——错误细化很容易在消息里带出更多上游细节。

## 六、其余借鉴点的取舍

分析文档里列了 9 项，这里说明为何多数不进方案。

**有界环形缓冲（terax `ringbuffer.rs`）**：设计确实好，但 kiro-rs 的 SSE 是边到边转发，当前没有审计、断线续传或回放需求。为一个不存在的需求引入缓冲层是投机性抽象。**等真实需求出现再回来取用**。

**单一入口做路径规范化（`fs/mod.rs:to_canon`）**：kiro-rs 不做跨平台路径展示，只有 `atomic_file.rs` 处理文件路径，且已收敛。无落点。

**进程治理与 Job Object**：kiro-rs 不 spawn 子进程。完全不适用。

**平台密钥后端（`secrets.rs`）**：值得单独讨论。kiro-rs 目前把凭据存明文 `credentials.json`（`.gitignore` 已忽略）。terax 在 macOS/Windows 走系统钥匙串，Linux 退回 `0o600` 文件。这对 kiro-rs 是个真实的安全改进方向,但它会改变凭据存储格式、迁移路径和多实例共享行为，属于独立的大型变更,不适合塞进本轮。**建议单独立项评估**。

**纯函数化提升可测性**：这是写法习惯而非一次性改造。terax 的 `launcher_dir_is_stale(name, modified, now, is_alive)` 把时间和进程存活都注入为参数，于是判定逻辑可确定性测试。kiro-rs 后续新增逻辑时可以照此办理，不需要专门的改造 change。

## 七、关于拆分 token_manager.rs

`src/kiro/token_manager.rs` 有 3981 行、169 KB，是全项目最大的文件（第二名 `admin/service.rs` 2112 行）。terax 的 `git/` 目录把同等复杂度拆成 `errors / process / operations / parser / types / utils` 六个文件，读起来确实清爽。

但我不建议现在动，理由有三：

1. **它是高风险区**。AGENTS.md 明确把「Token 刷新、多凭据、负载均衡」列为必须走 OpenSpec 的高风险变更。纯结构性移动虽不改行为，却会让这个区域的 git blame 与后续 review 成本显著上升。
2. **没有由痛点驱动**。terax 的 git 模块拆分是因为 git 有天然的职责边界（错误类型、进程执行、命令实现、输出解析）。`token_manager` 的 3981 行是否存在同样清晰的边界，需要先读透再判断,而不是因为「行数多」就拆。
3. **本轮已有四项待做**。前四项都是有明确证据的具体缺口，先把它们做完更有价值。

如果之后要推进，合理的第一刀是先把测试代码分出去：`token_manager.rs` 里 `#[cfg(test)]` 部分占比不小，剥离到 `token_manager/tests.rs` 是零行为风险的纯移动，能立刻降低主文件的阅读负担,也能顺便看清实现部分的真实规模。

## 八、推进顺序与 OpenSpec 边界

建议拆成三个独立 change，不要合并：

1. **`harden-outbound-request-targets`**：建议 1 + 建议 3。都属于「出站请求安全」，共享同一套验证思路（构造恶意目标、断言请求不发出）。触及协议与配置 schema 行为。
2. **`admin-body-limit-alignment`**：建议 2。独立且小，可以先做完先验证。
3. **`admin-error-taxonomy`**：建议 4。改对外 HTTP 状态码契约，需要同步 admin-ui 的错误处理，验证面最广，放最后。

每个 change 都需要 `openspec validate --all`，代码改动都需要 `cargo check --release --all-targets` 并报告告警数无新增。建议 3 额外需要真实链路验证，建议 4 额外需要 `pnpm build`（若改动 admin-ui 错误处理）。

## 九、验证说明

本文档为方案设计，未改动任何代码，因此未运行 `cargo check --release --all-targets`。

本轮实际执行并用于支撑结论的命令：

- `rg -n "build_client"` — 确认 17 处调用点全部收敛到 `src/http_client.rs`
- `rg -n "UpstreamError\(" src/admin/service.rs` — 确认 7 处使用点
- `rg -n "effective_api_region" -A 15` — 确认 region 从凭据流向 host 拼接
- `rg -n "MAX_BODY_SIZE|DefaultBodyLimit"` — 确认 anthropic/openai 有、admin 无
- 精读 `src/admin/{error,router,middleware}.rs`、`src/http_client.rs`、`src/kiro/endpoint/{mod,ide}.rs`、`src/kiro/online_auth.rs` 相关段落、`src/kiro/model/credentials.rs` 的 region 解析

未验证的部分与残余风险：

- **region 注入未做端到端复现**。结论基于代码路径推导（Admin 写入 → 凭据存储 → `effective_api_region` → `format!` 拼接），未构造恶意 region 实际发请求确认。这需要真实凭据环境，我没有执行。
- **建议 3 可能改变现有行为**。若 AWS 上游存在 http 跳转或跨域重定向，收紧策略会导致请求提前终止。必须在真实链路验证后再合并。
- **建议 4 的变体清单是初稿**。哪些失败原因真正需要区分，取决于 admin-ui 与运维实际如何消费这些错误。落地前应对照前端错误处理代码复核。
- 未评估各项改动对现有测试的破坏面。`token_manager.rs` 与 `admin/service.rs` 的测试量大，region 校验可能让部分使用非法 region 的测试夹具失败,这属于预期内的修复成本，但规模未测算。
