//! kiro-account-manager（KAM）导出文件适配器
//!
//! 以 `serde_json::Value` 做结构判别，输出规范化 `Vec<KiroCredentials>`。
//!
//! 不用 `#[serde(untagged)]`：`KiroCredentials` 零必填字段（全部 `Option` 或
//! `#[serde(default)]`）且无 `deny_unknown_fields`，untagged 会把任意 JSON 对象
//! 静默匹配成一条字段全空的凭据，而不是报错。Admin 导入与启动加载共用本模块，
//! 使同一份文件在两个入口得到等价结果。

use serde_json::Value;

use crate::kiro::model::credentials::{parse_auth_method, AuthMethod, KiroCredentials};

/// 容器格式判别失败
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrecognizedContainer {
    /// JSON 位置（如 `$` 或 `$.accounts[2]`）
    pub path: String,
    /// 该对象的顶层 key 名（仅 key 名，不含任何值）
    pub keys: Vec<String>,
}

impl std::fmt::Display for UnrecognizedContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} 处的 JSON 结构无法识别：既非原生凭据格式，也非已知 KAM 容器格式。\
             该对象的顶层字段为 [{}]。支持的形态：平铺单对象、平铺数组、\
             {{ version, accounts: [...] }}、{{ credentials: {{...}} }}",
            self.path,
            self.keys.join(", ")
        )
    }
}

impl std::error::Error for UnrecognizedContainer {}

/// 单条记录规范化失败
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRejected {
    /// JSON 位置
    pub path: String,
    /// 原因（面向操作者，不含密钥材料）
    pub reason: String,
}

impl std::fmt::Display for RecordRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for RecordRejected {}

/// 适配错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KamAdaptError {
    /// 整个文档的容器格式无法识别
    Container(UnrecognizedContainer),
    /// 文档不是对象也不是数组
    NotObjectOrArray { path: String },
}

impl std::fmt::Display for KamAdaptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Container(c) => c.fmt(f),
            Self::NotObjectOrArray { path } => write!(
                f,
                "{} 处应为 JSON 对象或数组，实际不是",
                path
            ),
        }
    }
}

impl std::error::Error for KamAdaptError {}

/// 单条记录的适配结果
pub type RecordResult = Result<KiroCredentials, RecordRejected>;

/// 识别出的容器形态（用于判断是否需要迁移写回）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerShape {
    /// 平铺数组 `[{...}]`
    FlatArray,
    /// 平铺单对象 `{...}`
    FlatObject,
    /// `{ version, accounts: [...] }`
    Wrapper,
    /// 旧版嵌套 `{ credentials: {...} }`
    LegacyNested,
}

impl ContainerShape {
    /// 该形态是否为本项目的原生格式（原生格式无需迁移写回）
    pub fn is_native(self) -> bool {
        matches!(self, Self::FlatArray | Self::FlatObject)
    }
}

/// 适配结果
#[derive(Debug)]
pub struct AdaptedDocument {
    pub shape: ContainerShape,
    /// 逐条结果：Ok 为规范化凭据，Err 为该条的失败原因
    pub records: Vec<RecordResult>,
}

impl AdaptedDocument {
    /// 仅取成功的凭据
    pub fn credentials(&self) -> Vec<KiroCredentials> {
        self.records.iter().filter_map(|r| r.as_ref().ok()).cloned().collect()
    }

    /// 是否存在失败记录
    pub fn has_failures(&self) -> bool {
        self.records.iter().any(|r| r.is_err())
    }
}

/// 判断对象的某个 key 是否存在且有实际值
///
/// KAM 只对 `email` 与 `proxyConfig` 使用 `skip_serializing_if`，其余可选字段
/// 一律输出显式 `null`。因此不能用 `contains_key` 判断有值。
///
/// 空白字符串也视为无值，与 `str_field` 的提取语义保持一致——否则容器识别键
/// 会被 `"  "` 满足，但后续字段提取又得到 `None`。
fn has_value(obj: &serde_json::Map<String, Value>, key: &str) -> bool {
    match obj.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(_) => true,
    }
}

/// 适配 KAM 或原生格式文档
///
/// 判别顺序有意义：wrapper 必须先于平铺单对象判定，否则一个同时含 `accounts`
/// 与 `refreshToken` 的畸形对象会被误判为单条凭据。
pub fn adapt(doc: &Value) -> Result<AdaptedDocument, KamAdaptError> {
    match doc {
        Value::Array(items) => Ok(AdaptedDocument {
            shape: ContainerShape::FlatArray,
            records: items
                .iter()
                .enumerate()
                .map(|(i, item)| normalize_record(item, &format!("$[{i}]")))
                .collect(),
        }),
        Value::Object(obj) => {
            // 1. wrapper：{ version, accounts: [...] }
            if let Some(Value::Array(items)) = obj.get("accounts") {
                return Ok(AdaptedDocument {
                    shape: ContainerShape::Wrapper,
                    records: items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| {
                            normalize_record(item, &format!("$.accounts[{i}]"))
                        })
                        .collect(),
                });
            }

            // 2. 旧版嵌套：{ credentials: {...} }
            if obj.get("credentials").map(Value::is_object).unwrap_or(false) {
                return Ok(AdaptedDocument {
                    shape: ContainerShape::LegacyNested,
                    records: vec![normalize_record(doc, "$")],
                });
            }

            // 3. 平铺单对象：需带凭据识别键，比 untagged 严格
            if has_value(obj, "refreshToken") || has_value(obj, "kiroApiKey") {
                return Ok(AdaptedDocument {
                    shape: ContainerShape::FlatObject,
                    records: vec![normalize_record(doc, "$")],
                });
            }

            Err(KamAdaptError::Container(UnrecognizedContainer {
                path: "$".to_string(),
                keys: obj.keys().cloned().collect(),
            }))
        }
        _ => Err(KamAdaptError::NotObjectOrArray {
            path: "$".to_string(),
        }),
    }
}

/// 取字符串字段，trim 后为空视为缺失
fn str_field(obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 规范化单条记录
///
/// 同时支持平铺形态与旧版 `{ credentials: {...} }` 嵌套形态。嵌套形态下认证字段
/// 取自内层，身份字段（email / userId / label / machineId / enabled）取自外层。
fn normalize_record(item: &Value, path: &str) -> RecordResult {
    let reject = |reason: &str| {
        Err(RecordRejected {
            path: path.to_string(),
            reason: reason.to_string(),
        })
    };

    let outer = match item.as_object() {
        Some(o) => o,
        None => return reject("记录应为 JSON 对象"),
    };

    // 嵌套形态时认证字段在内层；否则内外同源
    let inner = outer
        .get("credentials")
        .and_then(Value::as_object)
        .unwrap_or(outer);

    let mut cred = KiroCredentials::default();

    // ---- 认证字段（取内层）----
    cred.refresh_token = str_field(inner, "refreshToken");
    cred.access_token = str_field(inner, "accessToken");
    cred.expires_at = str_field(inner, "expiresAt");
    cred.client_id = str_field(inner, "clientId");
    cred.client_secret = str_field(inner, "clientSecret");
    cred.profile_arn = str_field(inner, "profileArn");
    cred.start_url = str_field(inner, "startUrl");
    cred.provider = str_field(inner, "provider");
    cred.token_endpoint = str_field(inner, "tokenEndpoint");
    cred.issuer_url = str_field(inner, "issuerUrl");
    cred.scopes = str_field(inner, "scopes");
    cred.kiro_api_key = str_field(inner, "kiroApiKey");

    // region 只写通用字段：auth region 经既有回退链派生，不存重复值以免日后漂移。
    // api region 的回退链故意不含凭据级 region（认证区与数据面区可以合法不同），
    // 需要凭据级 api region 时由操作者在 Admin UI 显式设置。
    cred.region = str_field(inner, "region").or_else(|| str_field(outer, "region"));
    cred.auth_region = None;
    cred.api_region = str_field(inner, "apiRegion").or_else(|| str_field(outer, "apiRegion"));

    // ---- 身份字段（内外都找，内层优先）----
    cred.email = str_field(inner, "email").or_else(|| str_field(outer, "email"));
    cred.user_id = str_field(inner, "userId").or_else(|| str_field(outer, "userId"));
    cred.machine_id = str_field(inner, "machineId").or_else(|| str_field(outer, "machineId"));

    // label → nickname：平铺与嵌套两条路径都必须映射。
    // 显式 nickname 优先于 label。
    cred.nickname = str_field(inner, "nickname")
        .or_else(|| str_field(outer, "nickname"))
        .or_else(|| str_field(inner, "label"))
        .or_else(|| str_field(outer, "label"));

    // ---- enabled → disabled（语义相反）----
    // KAM 的 enabled 默认 true；本项目 disabled 默认 false。丢弃该字段会让
    // 上游已禁用的账号静默变为启用。
    let enabled = inner
        .get("enabled")
        .or_else(|| outer.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    cred.disabled = !enabled;

    // 显式 disabled 优先于 enabled 派生（原生格式用 disabled）
    if let Some(disabled) = inner
        .get("disabled")
        .or_else(|| outer.get("disabled"))
        .and_then(Value::as_bool)
    {
        cred.disabled = disabled;
    }

    // ---- priority / endpoint（原生格式字段，KAM 无）----
    if let Some(p) = inner
        .get("priority")
        .or_else(|| outer.get("priority"))
        .and_then(Value::as_u64)
    {
        cred.priority = p as u32;
    }
    cred.endpoint = str_field(inner, "endpoint").or_else(|| str_field(outer, "endpoint"));
    cred.subscription_title = str_field(inner, "subscriptionTitle");
    cred.proxy_url = str_field(inner, "proxyUrl");
    cred.proxy_username = str_field(inner, "proxyUsername");
    cred.proxy_password = str_field(inner, "proxyPassword");

    // ---- authMethod：显式优先，未知拒绝 ----
    let explicit = str_field(inner, "authMethod").or_else(|| str_field(outer, "authMethod"));
    let method = match &explicit {
        Some(raw) => match parse_auth_method(raw) {
            Ok(m) => m,
            Err(e) => return reject(&e.to_string()),
        },
        None => {
            // 缺省时由凭据形态推断（external 判定先于 idc）
            match cred.classify_auth_method() {
                Ok(m) => m,
                Err(e) => return reject(&e.to_string()),
            }
        }
    };
    cred.auth_method = Some(method.as_str().to_string());

    // ---- 按族校验必需字段 ----
    match method {
        AuthMethod::ApiKey => {
            if cred.kiro_api_key.is_none() {
                return reject("api_key 凭据需要非空 kiroApiKey");
            }
        }
        AuthMethod::Social => {
            if cred.refresh_token.is_none() {
                return reject("social 凭据需要 refreshToken");
            }
        }
        AuthMethod::Idc => {
            if cred.refresh_token.is_none() {
                return reject("idc 凭据需要 refreshToken");
            }
            if cred.client_id.is_none() || cred.client_secret.is_none() {
                return reject("idc 凭据需要同时提供 clientId 和 clientSecret");
            }
            // IdC 缺省 provider 填 BuilderId，使固定 ARN 相关路径可用
            if cred.provider.is_none() {
                cred.provider = Some("BuilderId".to_string());
            }
        }
        AuthMethod::ExternalIdp => {
            if cred.refresh_token.is_none() {
                return reject("external_idp 凭据需要 refreshToken");
            }
            if cred.client_id.is_none() {
                return reject("external_idp 凭据需要 clientId");
            }
            // clientSecret 可选：公共客户端本就没有 secret
            if cred.token_endpoint.is_none() && cred.issuer_url.is_none() {
                return reject("external_idp 凭据需要 tokenEndpoint 或 issuerUrl 之一");
            }
            if let Err(e) = crate::kiro::external_idp::resolve_token_endpoint(
                cred.token_endpoint.as_deref(),
                cred.issuer_url.as_deref(),
            ) {
                return reject(&e.to_string());
            }
            // KAM 对 external 刻意置 provider = null，不得回填 BuilderId
        }
    }

    Ok(cred)
}

#[cfg(test)]
mod fixtures {
    //! 完全虚构的脱敏 KAM fixtures。
    //!
    //! 所有 token / secret / 租户 ID 均为明显占位值，不含任何真实凭据材料。
    //! refreshToken 长度需超过 100 字符以通过 `validate_refresh_token` 的截断检查。

    /// 占位 refresh token（长度足够，形态明显是假的）
    pub fn fake_rt(tag: &str) -> String {
        format!("fake-refresh-token-{tag}-{}", "0".repeat(120))
    }

    /// Google Social 账号（平铺）
    pub fn social_google() -> serde_json::Value {
        serde_json::json!({
            "id": "acct-1",
            "label": "Google 主号",
            "status": "active",
            "addedAt": "2026-01-01T00:00:00Z",
            "email": "placeholder-google@example.invalid",
            "userId": "fake-user-google",
            "authMethod": "social",
            "provider": "Google",
            "refreshToken": fake_rt("google"),
            "accessToken": null,
            "expiresAt": null,
            "clientId": null,
            "clientSecret": null,
            "region": null,
            "startUrl": null,
            "profileArn": null,
            "tokenEndpoint": null,
            "issuerUrl": null,
            "scopes": null,
            "machineId": null,
            "enabled": true
        })
    }

    /// BuilderId IdC 账号（平铺）
    pub fn idc_builder_id() -> serde_json::Value {
        serde_json::json!({
            "id": "acct-2",
            "label": "BuilderId 号",
            "status": "active",
            "addedAt": "2026-01-01T00:00:00Z",
            "email": "placeholder-builder@example.invalid",
            "userId": "fake-user-builder",
            "authMethod": "IdC",
            "provider": "BuilderId",
            "refreshToken": fake_rt("builder"),
            "clientId": "fake-client-id-builder",
            "clientSecret": "fake-client-secret-builder",
            "region": "us-east-1",
            "machineId": "f".repeat(64),
            "enabled": true
        })
    }

    /// Enterprise IdC 账号（平铺，带 startUrl）
    pub fn idc_enterprise() -> serde_json::Value {
        serde_json::json!({
            "id": "acct-3",
            "label": "企业号",
            "status": "active",
            "addedAt": "2026-01-01T00:00:00Z",
            "userId": "fake-user-enterprise",
            "email": null,
            "authMethod": "IdC",
            "provider": "Enterprise",
            "refreshToken": fake_rt("enterprise"),
            "clientId": "fake-client-id-ent",
            "clientSecret": "fake-client-secret-ent",
            "region": "eu-west-1",
            "startUrl": "https://placeholder.awsapps.com/start",
            "enabled": true
        })
    }

    /// external_idp 机密客户端（有 clientSecret，provider 为 null）
    pub fn external_confidential() -> serde_json::Value {
        serde_json::json!({
            "id": "acct-4",
            "label": "Entra 机密客户端",
            "status": "active",
            "addedAt": "2026-01-01T00:00:00Z",
            "email": "placeholder-entra@example.invalid",
            "userId": "fake-user-entra",
            "authMethod": "external_idp",
            "provider": null,
            "refreshToken": fake_rt("entra-conf"),
            "clientId": "fake-ms-client-id",
            "clientSecret": "fake-ms-client-secret",
            "tokenEndpoint": "https://login.microsoftonline.com/fake-tenant/oauth2/v2.0/token",
            "issuerUrl": "https://login.microsoftonline.com/fake-tenant",
            "scopes": "openid profile offline_access",
            "profileArn": "arn:aws:codewhisperer:us-east-1:000000000000:profile/FAKEEXTERNAL",
            "region": "us-east-1",
            "enabled": true
        })
    }

    /// external_idp 公共客户端（无 clientSecret）
    pub fn external_public() -> serde_json::Value {
        serde_json::json!({
            "id": "acct-5",
            "label": "Entra 公共客户端",
            "status": "active",
            "addedAt": "2026-01-01T00:00:00Z",
            "userId": "fake-user-entra-pub",
            "authMethod": "external_idp",
            "provider": null,
            "refreshToken": fake_rt("entra-pub"),
            "clientId": "fake-ms-public-client-id",
            "clientSecret": null,
            "tokenEndpoint": "https://login.microsoftonline.com/fake-tenant/oauth2/v2.0/token",
            "issuerUrl": null,
            "scopes": null,
            "enabled": true
        })
    }

    /// 可选字段全为显式 null（仅 refreshToken 有值）
    pub fn all_nulls_except_refresh_token() -> serde_json::Value {
        serde_json::json!({
            "id": "acct-6",
            "label": "全 null 样本",
            "status": "active",
            "addedAt": "2026-01-01T00:00:00Z",
            "refreshToken": fake_rt("all-nulls"),
            "email": null,
            "password": null,
            "accessToken": null,
            "expiresAt": null,
            "provider": null,
            "userId": null,
            "authMethod": null,
            "clientId": null,
            "clientSecret": null,
            "region": null,
            "clientIdHash": null,
            "ssoSessionId": null,
            "idToken": null,
            "startUrl": null,
            "profileArn": null,
            "tokenEndpoint": null,
            "issuerUrl": null,
            "scopes": null,
            "usageData": null,
            "groupId": null,
            "machineId": null,
            "availableModelsCache": null,
            "lastFailureAt": null,
            "disabledReason": null,
            "proxyConfig": null
        })
    }

    /// 含全部敏感字段的账号（用于断言这些字段被丢弃）
    pub fn with_sensitive_extras() -> serde_json::Value {
        serde_json::json!({
            "id": "acct-7",
            "label": "含敏感字段",
            "status": "active",
            "addedAt": "2026-01-01T00:00:00Z",
            "authMethod": "social",
            "provider": "Github",
            "refreshToken": fake_rt("sensitive"),
            "password": "fake-account-password",
            "usageData": {"used": 1},
            "groupId": "grp-1",
            "tagLinks": [{"tagId": "t1", "linkedAt": "2026-01-01T00:00:00Z"}],
            "availableModelsCache": {"models": []},
            "failureCount": 3,
            "successCount": 10,
            "lastFailureAt": "2026-01-01T00:00:00Z",
            "disabledReason": "手动禁用",
            "proxyConfig": {
                "enabled": true,
                "protocol": "socks5",
                "host": "127.0.0.1",
                "port": 1080,
                "username": "fake-proxy-user",
                "password": "fake-proxy-password"
            },
            "enabled": true
        })
    }

    /// 旧版嵌套形态：认证字段在 credentials 内，label 在外层
    pub fn legacy_nested() -> serde_json::Value {
        serde_json::json!({
            "label": "嵌套形态号",
            "email": "placeholder-nested@example.invalid",
            "userId": "fake-user-nested",
            "machineId": "a".repeat(64),
            "credentials": {
                "authMethod": "social",
                "provider": "Google",
                "refreshToken": fake_rt("nested")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use serde_json::json;

    // ============ 容器格式判别 ============

    #[test]
    fn adapts_flat_array() {
        let doc = json!([social_google(), idc_builder_id()]);
        let out = adapt(&doc).expect("平铺数组应被识别");
        assert_eq!(out.shape, ContainerShape::FlatArray);
        assert_eq!(out.records.len(), 2);
        assert!(!out.has_failures());
    }

    #[test]
    fn adapts_flat_single_object() {
        let out = adapt(&social_google()).expect("平铺单对象应被识别");
        assert_eq!(out.shape, ContainerShape::FlatObject);
        assert_eq!(out.records.len(), 1);
    }

    #[test]
    fn adapts_wrapper() {
        let doc = json!({
            "version": "1.9.2",
            "accounts": [social_google(), external_confidential()]
        });
        let out = adapt(&doc).expect("wrapper 应被识别");
        assert_eq!(out.shape, ContainerShape::Wrapper);
        assert_eq!(out.records.len(), 2);
        assert!(!out.has_failures());
    }

    #[test]
    fn adapts_legacy_nested() {
        let out = adapt(&legacy_nested()).expect("旧版嵌套应被识别");
        assert_eq!(out.shape, ContainerShape::LegacyNested);
        assert_eq!(out.records.len(), 1);
    }

    #[test]
    fn adapts_array_of_legacy_nested() {
        let doc = json!([legacy_nested(), legacy_nested()]);
        let out = adapt(&doc).expect("嵌套账号数组应被识别");
        assert_eq!(out.shape, ContainerShape::FlatArray);
        assert_eq!(out.records.len(), 2);
        // 数组内的嵌套形态由 normalize_record 处理
        for r in &out.records {
            let c = r.as_ref().expect("嵌套记录应成功");
            assert_eq!(c.nickname.as_deref(), Some("嵌套形态号"));
        }
    }

    #[test]
    fn wrapper_detection_precedes_flat_single() {
        // 畸形对象同时含 accounts 与顶层 refreshToken：必须按 wrapper 处理
        let doc = json!({
            "version": "1.9.2",
            "refreshToken": fake_rt("decoy"),
            "accounts": [social_google()]
        });
        let out = adapt(&doc).expect("应识别为 wrapper");
        assert_eq!(
            out.shape,
            ContainerShape::Wrapper,
            "wrapper 判定必须先于平铺单条，否则 accounts 会被忽略"
        );
        assert_eq!(out.records.len(), 1);
        assert_eq!(
            out.records[0].as_ref().unwrap().nickname.as_deref(),
            Some("Google 主号")
        );
    }

    #[test]
    fn rejects_unknown_container_with_diagnosable_error() {
        let doc = json!({ "version": "1.0", "data": [], "meta": {} });
        let err = adapt(&doc).expect_err("未知包装对象必须 fail fast");
        match err {
            KamAdaptError::Container(ref c) => {
                assert_eq!(c.path, "$");
                assert!(c.keys.contains(&"version".to_string()));
                assert!(c.keys.contains(&"data".to_string()));
                assert!(c.keys.contains(&"meta".to_string()));
            }
            other => panic!("期望 Container 错误，实际: {other:?}"),
        }
        // 错误信息只含 key 名，不含字段值
        assert!(err.to_string().contains("version"));
        assert!(!err.to_string().contains("1.0"));
    }

    #[test]
    fn rejects_non_object_non_array() {
        assert!(matches!(
            adapt(&json!("a string")),
            Err(KamAdaptError::NotObjectOrArray { .. })
        ));
        assert!(matches!(
            adapt(&json!(42)),
            Err(KamAdaptError::NotObjectOrArray { .. })
        ));
    }

    #[test]
    fn empty_array_is_valid_and_empty() {
        let out = adapt(&json!([])).expect("空数组是合法的");
        assert_eq!(out.shape, ContainerShape::FlatArray);
        assert!(out.records.is_empty());
    }

    // ============ 字段映射 ============

    #[test]
    fn maps_social_fields() {
        let out = adapt(&social_google()).unwrap();
        let c = out.records[0].as_ref().unwrap();
        assert_eq!(c.auth_method.as_deref(), Some("social"));
        assert_eq!(c.provider.as_deref(), Some("Google"));
        assert_eq!(c.email.as_deref(), Some("placeholder-google@example.invalid"));
        assert_eq!(c.user_id.as_deref(), Some("fake-user-google"));
        assert_eq!(c.nickname.as_deref(), Some("Google 主号"));
        assert!(c.refresh_token.is_some());
        assert!(!c.disabled);
    }

    #[test]
    fn maps_idc_fields_and_canonicalizes_alias() {
        let out = adapt(&idc_builder_id()).unwrap();
        let c = out.records[0].as_ref().unwrap();
        // KAM 写 "IdC"，规范化后应为 "idc"
        assert_eq!(c.auth_method.as_deref(), Some("idc"));
        assert_eq!(c.provider.as_deref(), Some("BuilderId"));
        assert_eq!(c.client_id.as_deref(), Some("fake-client-id-builder"));
        assert_eq!(c.client_secret.as_deref(), Some("fake-client-secret-builder"));
        assert_eq!(c.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn maps_enterprise_start_url() {
        let out = adapt(&idc_enterprise()).unwrap();
        let c = out.records[0].as_ref().unwrap();
        assert_eq!(c.provider.as_deref(), Some("Enterprise"));
        assert_eq!(
            c.start_url.as_deref(),
            Some("https://placeholder.awsapps.com/start")
        );
    }

    #[test]
    fn maps_external_confidential_fields() {
        let out = adapt(&external_confidential()).unwrap();
        let c = out.records[0].as_ref().unwrap();
        assert_eq!(c.auth_method.as_deref(), Some("external_idp"));
        assert_eq!(
            c.token_endpoint.as_deref(),
            Some("https://login.microsoftonline.com/fake-tenant/oauth2/v2.0/token")
        );
        assert_eq!(
            c.issuer_url.as_deref(),
            Some("https://login.microsoftonline.com/fake-tenant")
        );
        assert_eq!(c.scopes.as_deref(), Some("openid profile offline_access"));
        // 真实 profileArn 必须保留
        assert!(c.profile_arn.as_deref().unwrap().contains("FAKEEXTERNAL"));
        // external 不得回填 provider
        assert!(
            c.provider.is_none(),
            "external 账号不得被填 provider，实际: {:?}",
            c.provider
        );
    }

    #[test]
    fn maps_external_public_client_without_secret() {
        let out = adapt(&external_public()).unwrap();
        let c = out.records[0]
            .as_ref()
            .expect("公共客户端不得因缺 clientSecret 而失败");
        assert_eq!(c.auth_method.as_deref(), Some("external_idp"));
        assert!(c.client_secret.is_none());
        assert!(c.provider.is_none());
    }

    #[test]
    fn label_maps_to_nickname_in_both_shapes() {
        // 平铺形态
        let flat = adapt(&social_google()).unwrap();
        assert_eq!(
            flat.records[0].as_ref().unwrap().nickname.as_deref(),
            Some("Google 主号")
        );

        // 嵌套形态：label 在外层，认证字段在内层
        let nested = adapt(&legacy_nested()).unwrap();
        assert_eq!(
            nested.records[0].as_ref().unwrap().nickname.as_deref(),
            Some("嵌套形态号"),
            "嵌套形态的 label 也必须映射为 nickname"
        );
    }

    #[test]
    fn nested_shape_maps_identity_from_outer() {
        let out = adapt(&legacy_nested()).unwrap();
        let c = out.records[0].as_ref().unwrap();
        assert_eq!(c.email.as_deref(), Some("placeholder-nested@example.invalid"));
        assert_eq!(c.user_id.as_deref(), Some("fake-user-nested"));
        assert_eq!(c.machine_id.as_deref(), Some(&"a".repeat(64)[..]));
        assert_eq!(c.auth_method.as_deref(), Some("social"));
        assert_eq!(c.provider.as_deref(), Some("Google"));
    }

    #[test]
    fn enabled_maps_to_disabled_inverted() {
        let mut disabled_acct = social_google();
        disabled_acct["enabled"] = json!(false);
        let out = adapt(&disabled_acct).unwrap();
        assert!(
            out.records[0].as_ref().unwrap().disabled,
            "enabled: false 必须映射为 disabled: true"
        );

        let mut enabled_acct = social_google();
        enabled_acct["enabled"] = json!(true);
        assert!(!adapt(&enabled_acct).unwrap().records[0]
            .as_ref()
            .unwrap()
            .disabled);
    }

    #[test]
    fn missing_enabled_defaults_to_not_disabled() {
        let mut acct = social_google();
        acct.as_object_mut().unwrap().remove("enabled");
        assert!(
            !adapt(&acct).unwrap().records[0].as_ref().unwrap().disabled,
            "enabled 缺失应视为 true，即 disabled: false"
        );
    }

    #[test]
    fn explicit_disabled_takes_precedence() {
        // 原生格式用 disabled 字段
        let doc = json!({
            "refreshToken": fake_rt("native"),
            "authMethod": "social",
            "disabled": true
        });
        assert!(adapt(&doc).unwrap().records[0].as_ref().unwrap().disabled);
    }

    #[test]
    fn region_writes_general_field_only() {
        let out = adapt(&idc_builder_id()).unwrap();
        let c = out.records[0].as_ref().unwrap();
        assert_eq!(c.region.as_deref(), Some("us-east-1"));
        assert!(
            c.auth_region.is_none(),
            "不写 authRegion：避免与 region 形成可漂移的重复值"
        );

        // auth region 经既有回退链取到；api region 的链故意不含凭据级 region
        let mut config = crate::model::config::Config::default();
        config.region = "us-west-2".to_string();
        assert_eq!(c.effective_auth_region(&config), "us-east-1");
        assert_eq!(
            c.effective_api_region(&config),
            "us-west-2",
            "api region 不回退到凭据级 region（认证区与数据面区可以合法不同）"
        );
    }

    #[test]
    fn drops_sensitive_and_non_login_fields() {
        let out = adapt(&with_sensitive_extras()).unwrap();
        let c = out.records[0].as_ref().unwrap();

        // 序列化后不得出现任何被丢弃字段的值
        let json_out = serde_json::to_string(c).unwrap();
        for leaked in [
            "fake-account-password",
            "fake-proxy-user",
            "fake-proxy-password",
            "grp-1",
            "手动禁用",
        ] {
            assert!(
                !json_out.contains(leaked),
                "不得携带被丢弃字段的值: {leaked}"
            );
        }
        // 代理配置不迁移（本项目用另一套字段）
        assert!(c.proxy_url.is_none());
        assert!(c.proxy_username.is_none());
        assert!(c.proxy_password.is_none());
    }

    #[test]
    fn tolerates_all_explicit_nulls() {
        let out = adapt(&all_nulls_except_refresh_token()).expect("全 null 样本应可解析");
        let c = out.records[0]
            .as_ref()
            .expect("仅 refreshToken 有值时应判为 social");
        assert_eq!(c.auth_method.as_deref(), Some("social"));
        assert!(c.client_id.is_none());
        assert!(c.token_endpoint.is_none());
        assert!(c.issuer_url.is_none());
        assert!(c.scopes.is_none());
        assert!(c.profile_arn.is_none());
        assert!(c.machine_id.is_none());
    }

    #[test]
    fn null_valued_key_does_not_count_as_present() {
        // has_value 必须区分「key 不存在」与「key 存在但为 null」
        let doc = json!({ "refreshToken": null, "accounts": null });
        let err = adapt(&doc).expect_err("refreshToken 为 null 不构成平铺单条");
        assert!(matches!(err, KamAdaptError::Container(_)));
    }

    // ============ 逐条失败 ============

    #[test]
    fn rejects_unknown_auth_method_per_record() {
        let mut bad = social_google();
        bad["authMethod"] = json!("oauth2");
        let doc = json!([social_google(), bad, idc_builder_id()]);
        let out = adapt(&doc).unwrap();

        assert_eq!(out.records.len(), 3);
        assert!(out.records[0].is_ok(), "同批次其他记录应照常处理");
        assert!(out.records[2].is_ok());

        let err = out.records[1].as_ref().unwrap_err();
        assert_eq!(err.path, "$[1]");
        assert!(err.reason.contains("oauth2"));
        assert!(err.reason.contains("social"), "错误应列出合法取值");
        assert!(err.reason.contains("external_idp"));
    }

    #[test]
    fn rejects_external_without_endpoint() {
        let mut bad = external_public();
        bad["tokenEndpoint"] = json!(null);
        bad["issuerUrl"] = json!(null);
        let out = adapt(&bad).unwrap();
        let err = out.records[0].as_ref().unwrap_err();
        assert!(
            err.reason.contains("tokenEndpoint 或 issuerUrl"),
            "实际: {}",
            err.reason
        );
    }

    #[test]
    fn rejects_external_with_non_whitelisted_endpoint() {
        let mut bad = external_public();
        bad["tokenEndpoint"] = json!("https://attacker.example/token");
        let out = adapt(&bad).unwrap();
        let err = out.records[0].as_ref().unwrap_err();
        assert!(
            err.reason.contains("Microsoft 登录域"),
            "非白名单 endpoint 应被拒，实际: {}",
            err.reason
        );
        // 错误不得携带 token 材料
        assert!(!err.reason.contains("fake-refresh-token"));
    }

    #[test]
    fn rejects_external_without_client_id() {
        let mut bad = external_public();
        bad["clientId"] = json!(null);
        let out = adapt(&bad).unwrap();
        assert!(out.records[0]
            .as_ref()
            .unwrap_err()
            .reason
            .contains("clientId"));
    }

    #[test]
    fn rejects_idc_missing_client_secret() {
        let mut bad = idc_builder_id();
        bad["clientSecret"] = json!(null);
        let out = adapt(&bad).unwrap();
        assert!(out.records[0]
            .as_ref()
            .unwrap_err()
            .reason
            .contains("clientId 和 clientSecret"));
    }

    #[test]
    fn rejects_social_without_refresh_token() {
        let doc = json!({ "refreshToken": "  ", "authMethod": "social", "label": "x" });
        // refreshToken 为空白 → 不构成平铺单条识别键
        assert!(adapt(&doc).is_err());

        // 显式 social 但 refreshToken 缺失（借 accounts 容器进入）
        let wrapped = json!({
            "accounts": [{ "authMethod": "social", "label": "x" }]
        });
        let out = adapt(&wrapped).unwrap();
        assert!(out.records[0]
            .as_ref()
            .unwrap_err()
            .reason
            .contains("refreshToken"));
    }

    #[test]
    fn rejects_non_object_record() {
        let doc = json!(["not an object", social_google()]);
        let out = adapt(&doc).unwrap();
        assert_eq!(out.records[0].as_ref().unwrap_err().path, "$[0]");
        assert!(out.records[1].is_ok());
    }

    #[test]
    fn record_paths_are_precise_in_wrapper() {
        let mut bad = social_google();
        bad["authMethod"] = json!("nope");
        let doc = json!({ "accounts": [social_google(), bad] });
        let out = adapt(&doc).unwrap();
        assert_eq!(out.records[1].as_ref().unwrap_err().path, "$.accounts[1]");
    }

    // ============ 容器 × 登录格式矩阵 ============

    #[test]
    fn matrix_all_containers_times_all_login_formats() {
        let accounts = vec![
            ("social", social_google()),
            ("idc-builder", idc_builder_id()),
            ("idc-enterprise", idc_enterprise()),
            ("external-conf", external_confidential()),
            ("external-pub", external_public()),
        ];

        for (name, acct) in &accounts {
            // 平铺单对象
            let flat = adapt(acct).unwrap_or_else(|e| panic!("{name} 平铺单对象: {e}"));
            assert_eq!(flat.shape, ContainerShape::FlatObject);
            assert!(flat.records[0].is_ok(), "{name} 平铺单对象应成功");

            // 平铺数组
            let arr = adapt(&json!([acct])).unwrap();
            assert_eq!(arr.shape, ContainerShape::FlatArray);
            assert!(arr.records[0].is_ok(), "{name} 平铺数组应成功");

            // wrapper
            let wrapper = adapt(&json!({ "version": "1.9.2", "accounts": [acct] })).unwrap();
            assert_eq!(wrapper.shape, ContainerShape::Wrapper);
            assert!(wrapper.records[0].is_ok(), "{name} wrapper 应成功");

            // 旧版嵌套
            let nested = adapt(&json!({
                "label": format!("{name} 嵌套"),
                "credentials": acct
            }))
            .unwrap();
            assert_eq!(nested.shape, ContainerShape::LegacyNested);
            assert!(nested.records[0].is_ok(), "{name} 嵌套形态应成功");

            // 四种容器应产出等价的认证类型
            let expected = flat.records[0].as_ref().unwrap().auth_method.clone();
            for (label, out) in [("array", &arr), ("wrapper", &wrapper), ("nested", &nested)] {
                assert_eq!(
                    out.records[0].as_ref().unwrap().auth_method,
                    expected,
                    "{name} 在 {label} 容器中的认证类型应与平铺一致"
                );
            }
        }
    }

    #[test]
    fn native_shapes_do_not_require_migration() {
        assert!(ContainerShape::FlatArray.is_native());
        assert!(ContainerShape::FlatObject.is_native());
        assert!(!ContainerShape::Wrapper.is_native());
        assert!(!ContainerShape::LegacyNested.is_native());
    }

    #[test]
    fn fixtures_contain_no_realistic_credentials() {
        // 所有 fixture 的 token 类字段必须是明显占位值
        for acct in [
            social_google(),
            idc_builder_id(),
            idc_enterprise(),
            external_confidential(),
            external_public(),
            all_nulls_except_refresh_token(),
            with_sensitive_extras(),
            legacy_nested(),
        ] {
            let s = acct.to_string();
            if s.contains("refreshToken") && s.contains("fake-refresh-token") {
                // ok
            }
            assert!(
                !s.contains("ksk_") || s.contains("fake"),
                "fixture 含疑似真实 API Key"
            );
        }
        // refreshToken 占位值形态明确
        assert!(fake_rt("t").starts_with("fake-refresh-token-"));
        assert!(fake_rt("t").len() > 100, "长度需通过截断检查");
    }
}
