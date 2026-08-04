//! Kiro OAuth 凭证数据模型
//!
//! 支持从 Kiro IDE 的凭证文件加载，使用 Social 认证方式
//! 支持单凭据和多凭据配置格式

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::http_client::ProxyConfig;
use crate::model::config::Config;

/// Kiro OAuth 凭证
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroCredentials {
    /// 凭据唯一标识符（自增 ID）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,

    /// 访问令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,

    /// 刷新令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Profile ARN
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,

    /// 过期时间 (RFC3339 格式)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,

    /// 认证方式 (social / idc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,

    /// Identity provider name (e.g. BuilderId, Github, Google, Enterprise)
    /// Used for fixed profileArn resolution and supports_profiles decisions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// OIDC Client ID (IdC 认证需要)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// OIDC Client Secret (IdC 认证需要)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// 凭据优先级（数字越小优先级越高，默认为 0）
    #[serde(default)]
    #[serde(skip_serializing_if = "is_zero")]
    pub priority: u32,

    /// 凭据级 Region 配置（用于 OIDC token 刷新）
    /// 未配置时回退到 config.json 的全局 region
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// 凭据级 Auth Region（用于 Token 刷新）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_region: Option<String>,

    /// 凭据级 API Region（用于 API 请求）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_region: Option<String>,

    /// 凭据级 Machine ID 配置（可选）
    /// 未配置时回退到 config.json 的 machineId；都未配置时由 refreshToken 派生
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,

    /// 用户邮箱（从 Anthropic API 获取 / GetUserInfo）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Kiro 稳定用户 ID（用于 upsert 去重）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// 展示名（KAM nickname / label）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

    /// IAM SSO start URL（便于再次登录）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_url: Option<String>,

    /// 订阅等级（KIRO PRO+ / KIRO FREE 等）
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub subscription_title: Option<String>,

    /// 凭据级代理 URL（可选）
    /// 支持 http/https/socks5 协议
    /// 特殊值 "direct" 表示显式不使用代理（即使全局配置了代理）
    /// 未配置时回退到全局代理配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,

    /// 凭据级代理认证用户名（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_username: Option<String>,

    /// 凭据级代理认证密码（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_password: Option<String>,

    /// 凭据是否被禁用（默认为 false）
    #[serde(default)]
    pub disabled: bool,

    /// Kiro API Key（headless 模式）
    /// 格式: ksk_xxxxxxxx
    /// 设置后直接作为 Bearer Token 使用，无需 refreshToken
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiro_api_key: Option<String>,

    /// 端点名称（可选）
    ///
    /// 决定该凭据走哪套 Kiro API。未配置时回退到 `config.defaultEndpoint`（默认 "ide"）。
    /// 端点名必须在启动时注册的端点 registry 中存在。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// external_idp 的 OAuth2 token 端点（可选）
    ///
    /// Microsoft Entra ID / Azure AD 账号刷新时 POST 到此地址，而非 AWS OIDC。
    /// 使用前必须通过 `crate::kiro::external_idp::validate_token_endpoint` 校验。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,

    /// external_idp 的 issuer URL（可选）
    ///
    /// 未提供 `token_endpoint` 时据此派生，派生结果同样需通过校验。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,

    /// external_idp 的 OAuth2 scopes（可选，空格分隔单串）
    ///
    /// 用 String 而非 Vec<String>：与来源导出格式逐位对齐，导入无损。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<String>,
}

/// 判断是否为零（用于跳过序列化）
fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// 落盘前的 authMethod 归一：已知别名归一，未知值原样透传
///
/// 契约「未知值透传」被落盘路径依赖（见 `canonicalize_auth_method` 的调用点），
/// 历史脏值不得因归一而导致落盘失败。需要拒绝未知值的场景用
/// [`parse_auth_method`]，二者职责不同，不要合并。
fn canonicalize_auth_method_value(value: &str) -> &str {
    if value.eq_ignore_ascii_case("builder-id") || value.eq_ignore_ascii_case("iam") {
        "idc"
    } else if value.eq_ignore_ascii_case("api_key") || value.eq_ignore_ascii_case("apikey") {
        "api_key"
    } else {
        value
    }
}

/// 规范化的认证方式
///
/// 仅用于内部决策。持久化字段 `KiroCredentials::auth_method` 保持
/// `Option<String>`：改成枚举会让任何历史脏值导致整个 `credentials.json`
/// 反序列化失败，影响面远超收益。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    Social,
    Idc,
    ExternalIdp,
    ApiKey,
}

impl AuthMethod {
    /// 规范值字面量（与持久化字段取值一致）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Social => "social",
            Self::Idc => "idc",
            Self::ExternalIdp => "external_idp",
            Self::ApiKey => "api_key",
        }
    }
}

/// 显式 authMethod 不在别名表内
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownAuthMethod {
    pub value: String,
}

impl UnknownAuthMethod {
    /// 合法取值列表，用于错误提示
    pub const ACCEPTED: &'static str = "social, idc (builder-id, iam), external_idp (external-idp, azure, azuread, azure_ad), api_key (apikey)";
}

impl std::fmt::Display for UnknownAuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "不支持的 authMethod \"{}\"，合法取值：{}",
            self.value,
            Self::ACCEPTED
        )
    }
}

impl std::error::Error for UnknownAuthMethod {}

/// 解析显式 authMethod，未知值报错而非降级
///
/// 与 [`canonicalize_auth_method_value`] 的区别：后者对未知值原样透传（落盘用），
/// 本函数拒绝未知值（入口校验用）。
pub fn parse_auth_method(value: &str) -> Result<AuthMethod, UnknownAuthMethod> {
    let v = value.trim();
    let eq = |target: &str| v.eq_ignore_ascii_case(target);

    if eq("social") {
        Ok(AuthMethod::Social)
    } else if eq("idc") || eq("builder-id") || eq("iam") {
        Ok(AuthMethod::Idc)
    } else if eq("external_idp")
        || eq("external-idp")
        || eq("externalidp")
        || eq("azure")
        || eq("azuread")
        || eq("azure_ad")
    {
        Ok(AuthMethod::ExternalIdp)
    } else if eq("api_key") || eq("apikey") {
        Ok(AuthMethod::ApiKey)
    } else {
        Err(UnknownAuthMethod {
            value: v.to_string(),
        })
    }
}

/// 凭据配置（支持单对象或数组格式）
///
/// 自动识别配置文件格式：
/// - 单对象格式（旧格式，向后兼容）
/// - 数组格式（新格式，支持多凭据）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CredentialsConfig {
    /// 单个凭据（旧格式）
    Single(KiroCredentials),
    /// 多凭据数组（新格式）
    Multiple(Vec<KiroCredentials>),
}

/// 加载结果：除凭据配置外，附带是否需要格式迁移写回
#[derive(Debug)]
pub struct LoadedCredentials {
    pub config: CredentialsConfig,
    /// 源文件为导入工具容器格式（wrapper / 旧版嵌套），需规范化写回
    pub needs_migration: bool,
}

impl CredentialsConfig {
    /// 从文件加载，并区分原生格式与导入工具容器格式
    ///
    /// 判别用 `serde_json::Value` 结构化进行，不依赖 `#[serde(untagged)]`：
    /// `KiroCredentials` 零必填字段且无 `deny_unknown_fields`，untagged 会把任意
    /// JSON 对象静默匹配成一条字段全空的凭据，导致「加载成功但一个账号都不能用」
    /// 且日志无任何指向 JSON 结构的线索。
    pub fn load_detailed<P: AsRef<Path>>(path: P) -> anyhow::Result<LoadedCredentials> {
        let path = path.as_ref();

        // 文件不存在时返回空数组
        if !path.exists() {
            return Ok(LoadedCredentials {
                config: CredentialsConfig::Multiple(vec![]),
                needs_migration: false,
            });
        }

        let content = fs::read_to_string(path)?;

        // 文件为空时返回空数组
        if content.trim().is_empty() {
            return Ok(LoadedCredentials {
                config: CredentialsConfig::Multiple(vec![]),
                needs_migration: false,
            });
        }

        let doc: serde_json::Value = serde_json::from_str(&content)?;
        Self::from_value(&doc)
    }

    /// 从已解析的 JSON 判别容器格式并构造配置
    pub fn from_value(doc: &serde_json::Value) -> anyhow::Result<LoadedCredentials> {
        use crate::kiro::kam_adapter::{self, ContainerShape};

        let adapted = kam_adapter::adapt(doc)?;

        // 逐条错误必须可定位：未知 authMethod、缺字段、非法 endpoint 都在此暴露
        let mut creds = Vec::with_capacity(adapted.records.len());
        for (index, record) in adapted.records.iter().enumerate() {
            match record {
                Ok(c) => creds.push(c.clone()),
                Err(rejected) => {
                    anyhow::bail!(
                        "凭据文件第 {} 条记录（{}）无效: {}",
                        index,
                        rejected.path,
                        rejected.reason
                    );
                }
            }
        }

        let needs_migration = !adapted.shape.is_native();

        // 原生单对象格式保持 Single，以维持 is_multiple_format 的既有语义
        let config = match adapted.shape {
            ContainerShape::FlatObject => CredentialsConfig::Single(
                creds.into_iter().next().unwrap_or_default(),
            ),
            _ => CredentialsConfig::Multiple(creds),
        };

        Ok(LoadedCredentials {
            config,
            needs_migration,
        })
    }

    /// 将导入工具容器格式的凭据文件规范化写回为原生格式
    ///
    /// 顺序：备份原文件 → 写临时文件 → 原子替换。任一步失败都保留原文件不变。
    /// 返回备份文件路径。
    pub fn migrate_to_native<P: AsRef<Path>>(
        path: P,
        config: &CredentialsConfig,
    ) -> anyhow::Result<std::path::PathBuf> {
        let path = path.as_ref();

        // 先序列化：序列化失败时不应已经产生备份文件
        let creds: Vec<KiroCredentials> = match config {
            CredentialsConfig::Single(c) => vec![c.clone()],
            CredentialsConfig::Multiple(list) => list.clone(),
        };
        let json = serde_json::to_string_pretty(&creds)?;

        // 备份失败则不写回：宁可下次启动重试，也不冒无备份改写用户凭据的风险
        let backup = crate::common::atomic_file::backup_file(path, "kam-backup")?;

        crate::common::atomic_file::write_atomic(path, &json)?;

        Ok(backup)
    }

    /// 转换为按优先级排序的凭据列表
    pub fn into_sorted_credentials(self) -> Vec<KiroCredentials> {
        match self {
            CredentialsConfig::Single(mut cred) => {
                cred.canonicalize_auth_method();
                vec![cred]
            }
            CredentialsConfig::Multiple(mut creds) => {
                // 按优先级排序（数字越小优先级越高）
                creds.sort_by_key(|c| c.priority);
                for cred in &mut creds {
                    cred.canonicalize_auth_method();
                }
                creds
            }
        }
    }

    /// 判断是否为多凭据格式（数组格式）
    pub fn is_multiple(&self) -> bool {
        matches!(self, CredentialsConfig::Multiple(_))
    }
}

impl KiroCredentials {
    /// 特殊值：显式不使用代理
    pub const PROXY_DIRECT: &'static str = "direct";

    /// 获取默认凭证文件路径
    pub fn default_credentials_path() -> &'static str {
        "credentials.json"
    }

    /// 获取有效的 Auth Region（用于 Token 刷新）
    /// 优先级：凭据.auth_region > 凭据.region > config.auth_region > config.region
    pub fn effective_auth_region<'a>(&'a self, config: &'a Config) -> &'a str {
        self.auth_region
            .as_deref()
            .or(self.region.as_deref())
            .unwrap_or(config.effective_auth_region())
    }

    /// 获取有效的 API Region（用于 API 请求）
    /// 优先级：凭据.api_region > config.api_region > config.region
    pub fn effective_api_region<'a>(&'a self, config: &'a Config) -> &'a str {
        self.api_region
            .as_deref()
            .unwrap_or(config.effective_api_region())
    }

    /// 获取有效的代理配置
    /// 优先级：凭据代理 > 全局代理 > 无代理
    /// 特殊值 "direct" 表示显式不使用代理（即使全局配置了代理）
    pub fn effective_proxy(&self, global_proxy: Option<&ProxyConfig>) -> Option<ProxyConfig> {
        match self.proxy_url.as_deref() {
            Some(url) if url.eq_ignore_ascii_case(Self::PROXY_DIRECT) => None,
            Some(url) => {
                let mut proxy = ProxyConfig::new(url);
                if let (Some(username), Some(password)) =
                    (&self.proxy_username, &self.proxy_password)
                {
                    proxy = proxy.with_auth(username, password);
                }
                Some(proxy)
            }
            None => global_proxy.cloned(),
        }
    }

    pub fn canonicalize_auth_method(&mut self) {
        let auth_method = match &self.auth_method {
            Some(m) => m,
            None => return,
        };

        let canonical = canonicalize_auth_method_value(auth_method);
        if canonical != auth_method {
            self.auth_method = Some(canonical.to_string());
        }
    }

    /// 检查凭据是否支持 Opus 模型
    ///
    /// Free 账号不支持 Opus 模型，需要 PRO 或更高等级订阅
    pub fn supports_opus(&self) -> bool {
        match &self.subscription_title {
            Some(title) => {
                let title_upper = title.to_uppercase();
                // 如果包含 FREE，则不支持 Opus
                !title_upper.contains("FREE")
            }
            // 如果还没有获取订阅信息，暂时允许（首次使用时会获取）
            None => true,
        }
    }

    /// 判定凭据的规范化认证方式
    ///
    /// 判别优先级：
    /// 1. 合法显式 `auth_method`
    /// 2. 缺省时，白名单内的 `token_endpoint` / `issuer_url` → external_idp
    /// 3. 缺省时，`client_id` + `client_secret` 齐全 → idc
    /// 4. 缺省时，有 `kiro_api_key` → api_key
    /// 5. 其余 → social
    ///
    /// 第 2 步必须先于第 3 步：external 账号也可能同时有 client id 与 secret，
    /// 顺序反了则机密客户端永远进不了 external 分支。
    pub fn classify_auth_method(&self) -> Result<AuthMethod, UnknownAuthMethod> {
        if let Some(explicit) = self
            .auth_method
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return parse_auth_method(explicit);
        }

        let external_endpoint_ok = crate::kiro::external_idp::resolve_token_endpoint(
            self.token_endpoint.as_deref(),
            self.issuer_url.as_deref(),
        )
        .is_ok();
        if external_endpoint_ok {
            return Ok(AuthMethod::ExternalIdp);
        }

        if self.client_id.is_some() && self.client_secret.is_some() {
            return Ok(AuthMethod::Idc);
        }

        if self.kiro_api_key.is_some() {
            return Ok(AuthMethod::ApiKey);
        }

        Ok(AuthMethod::Social)
    }

    /// 检查是否为 API Key 凭据
    ///
    /// API Key 凭据直接使用 kiro_api_key 作为 Bearer Token，无需 refreshToken
    pub fn is_api_key_credential(&self) -> bool {
        self.kiro_api_key.is_some()
            || self
                .auth_method
                .as_deref()
                .map(|m| m.eq_ignore_ascii_case("api_key") || m.eq_ignore_ascii_case("apikey"))
                .unwrap_or(false)
    }
}

#[cfg(test)]
impl KiroCredentials {
    fn from_json(json_string: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_string)
    }

    fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::Config;

    #[test]
    fn test_from_json() {
        let json = r#"{
            "accessToken": "test_token",
            "refreshToken": "test_refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2024-01-01T00:00:00Z",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2024-01-01T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("social".to_string()));
    }

    #[test]
    fn test_from_json_with_unknown_keys() {
        let json = r#"{
            "accessToken": "test_token",
            "unknownField": "should be ignored"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
    }

    #[test]
    fn test_to_json() {
        let creds = KiroCredentials {
            id: None,
            access_token: Some("token".to_string()),
            refresh_token: None,
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            provider: None,
            client_id: None,
            client_secret: None,
            priority: 0,
            region: None,
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            user_id: None,
            nickname: None,
            start_url: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            ..Default::default()
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("accessToken"));
        assert!(json.contains("authMethod"));
        assert!(!json.contains("refreshToken"));
        // priority 为 0 时不序列化
        assert!(!json.contains("priority"));
    }

    #[test]
    fn test_default_credentials_path() {
        assert_eq!(
            KiroCredentials::default_credentials_path(),
            "credentials.json"
        );
    }

    #[test]
    fn test_priority_default() {
        let json = r#"{"refreshToken": "test"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 0);
    }

    #[test]
    fn test_priority_explicit() {
        let json = r#"{"refreshToken": "test", "priority": 5}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 5);
    }

    #[test]
    fn test_credentials_config_single() {
        let json = r#"{"refreshToken": "test", "expiresAt": "2025-12-31T00:00:00Z"}"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Single(_)));
    }

    #[test]
    fn test_credentials_config_multiple() {
        let json = r#"[
            {"refreshToken": "test1", "priority": 1},
            {"refreshToken": "test2", "priority": 0}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Multiple(_)));
        assert_eq!(config.into_sorted_credentials().len(), 2);
    }

    #[test]
    fn test_credentials_config_priority_sorting() {
        let json = r#"[
            {"refreshToken": "t1", "priority": 2},
            {"refreshToken": "t2", "priority": 0},
            {"refreshToken": "t3", "priority": 1}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        // 验证按优先级排序
        assert_eq!(list[0].refresh_token, Some("t2".to_string())); // priority 0
        assert_eq!(list[1].refresh_token, Some("t3".to_string())); // priority 1
        assert_eq!(list[2].refresh_token, Some("t1".to_string())); // priority 2
    }

    // ============ Region 字段测试 ============

    #[test]
    fn test_region_field_parsing() {
        // 测试解析包含 region 字段的 JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "region": "us-east-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, Some("us-east-1".to_string()));
    }

    #[test]
    fn test_region_field_missing_backward_compat() {
        // 测试向后兼容：不包含 region 字段的旧格式 JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, None);
    }

    #[test]
    fn test_region_field_serialization() {
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            provider: None,
            client_id: None,
            client_secret: None,
            priority: 0,
            region: Some("eu-west-1".to_string()),
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            user_id: None,
            nickname: None,
            start_url: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            ..Default::default()
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("region"));
        assert!(json.contains("eu-west-1"));
    }

    #[test]
    fn test_region_field_none_not_serialized() {
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            provider: None,
            client_id: None,
            client_secret: None,
            priority: 0,
            region: None,
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            user_id: None,
            nickname: None,
            start_url: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            ..Default::default()
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("region"));
    }

    // ============ MachineId 字段测试 ============

    #[test]
    fn test_machine_id_field_parsing() {
        let machine_id = "a".repeat(64);
        let json = format!(
            r#"{{
                "refreshToken": "test_refresh",
                "machineId": "{machine_id}"
            }}"#
        );

        let creds = KiroCredentials::from_json(&json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.machine_id, Some(machine_id));
    }

    #[test]
    fn test_machine_id_field_serialization() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = Some("b".repeat(64));

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("machineId"));
    }

    #[test]
    fn test_machine_id_field_none_not_serialized() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = None;

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("machineId"));
    }

    #[test]
    fn test_multiple_credentials_with_different_regions() {
        // 测试多凭据场景下不同凭据使用各自的 region
        let json = r#"[
            {"refreshToken": "t1", "region": "us-east-1"},
            {"refreshToken": "t2", "region": "eu-west-1"},
            {"refreshToken": "t3"}
        ]"#;

        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        assert_eq!(list[0].region, Some("us-east-1".to_string()));
        assert_eq!(list[1].region, Some("eu-west-1".to_string()));
        assert_eq!(list[2].region, None);
    }

    #[test]
    fn test_region_field_with_all_fields() {
        // 测试包含所有字段的完整 JSON
        let json = r#"{
            "id": 1,
            "accessToken": "access",
            "refreshToken": "refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2025-12-31T00:00:00Z",
            "authMethod": "idc",
            "clientId": "client123",
            "clientSecret": "secret456",
            "priority": 5,
            "region": "ap-northeast-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.id, Some(1));
        assert_eq!(creds.access_token, Some("access".to_string()));
        assert_eq!(creds.refresh_token, Some("refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2025-12-31T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("idc".to_string()));
        assert_eq!(creds.client_id, Some("client123".to_string()));
        assert_eq!(creds.client_secret, Some("secret456".to_string()));
        assert_eq!(creds.priority, 5);
        assert_eq!(creds.region, Some("ap-northeast-1".to_string()));
    }

    #[test]
    fn test_region_roundtrip() {
        // 测试序列化和反序列化的往返一致性
        let original = KiroCredentials {
            id: Some(42),
            access_token: Some("token".to_string()),
            refresh_token: Some("refresh".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            provider: None,
            client_id: None,
            client_secret: None,
            priority: 3,
            region: Some("us-west-2".to_string()),
            auth_region: None,
            api_region: None,
            machine_id: Some("c".repeat(64)),
            email: None,
            user_id: None,
            nickname: None,
            start_url: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            kiro_api_key: None,
            endpoint: None,
            ..Default::default()
        };

        let json = original.to_pretty_json().unwrap();
        let parsed = KiroCredentials::from_json(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.access_token, original.access_token);
        assert_eq!(parsed.refresh_token, original.refresh_token);
        assert_eq!(parsed.priority, original.priority);
        assert_eq!(parsed.region, original.region);
        assert_eq!(parsed.machine_id, original.machine_id);
    }

    // ============ auth_region / api_region 字段测试 ============

    #[test]
    fn test_auth_region_field_parsing() {
        let json = r#"{
            "refreshToken": "test_refresh",
            "authRegion": "eu-central-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.auth_region, Some("eu-central-1".to_string()));
        assert_eq!(creds.api_region, None);
    }

    #[test]
    fn test_api_region_field_parsing() {
        let json = r#"{
            "refreshToken": "test_refresh",
            "apiRegion": "ap-southeast-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.api_region, Some("ap-southeast-1".to_string()));
        assert_eq!(creds.auth_region, None);
    }

    #[test]
    fn test_auth_api_region_serialization() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.auth_region = Some("eu-west-1".to_string());
        creds.api_region = Some("us-west-2".to_string());

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("authRegion"));
        assert!(json.contains("eu-west-1"));
        assert!(json.contains("apiRegion"));
        assert!(json.contains("us-west-2"));
    }

    #[test]
    fn test_auth_api_region_none_not_serialized() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.auth_region = None;
        creds.api_region = None;

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("authRegion"));
        assert!(!json.contains("apiRegion"));
    }

    #[test]
    fn test_auth_api_region_roundtrip() {
        let mut original = KiroCredentials::default();
        original.refresh_token = Some("refresh".to_string());
        original.region = Some("us-east-1".to_string());
        original.auth_region = Some("eu-west-1".to_string());
        original.api_region = Some("ap-northeast-1".to_string());

        let json = original.to_pretty_json().unwrap();
        let parsed = KiroCredentials::from_json(&json).unwrap();

        assert_eq!(parsed.region, original.region);
        assert_eq!(parsed.auth_region, original.auth_region);
        assert_eq!(parsed.api_region, original.api_region);
    }

    #[test]
    fn test_backward_compat_no_auth_api_region() {
        // 旧格式 JSON 不包含 authRegion/apiRegion，应正常解析
        let json = r#"{
            "refreshToken": "test_refresh",
            "region": "us-east-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.region, Some("us-east-1".to_string()));
        assert_eq!(creds.auth_region, None);
        assert_eq!(creds.api_region, None);
    }

    // ============ effective_auth_region / effective_api_region 优先级测试 ============

    #[test]
    fn test_effective_auth_region_credential_auth_region_highest() {
        // 凭据.auth_region > 凭据.region > config.auth_region > config.region
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());
        creds.auth_region = Some("cred-auth-region".to_string());

        assert_eq!(creds.effective_auth_region(&config), "cred-auth-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_credential_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());
        // auth_region 未设置

        assert_eq!(creds.effective_auth_region(&config), "cred-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_config_auth_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let creds = KiroCredentials::default();
        // auth_region 和 region 均未设置

        assert_eq!(creds.effective_auth_region(&config), "config-auth-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_config_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        // config.auth_region 未设置

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_auth_region(&config), "config-region");
    }

    #[test]
    fn test_effective_api_region_credential_api_region_highest() {
        // 凭据.api_region > config.api_region > config.region
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.api_region = Some("config-api-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.api_region = Some("cred-api-region".to_string());

        assert_eq!(creds.effective_api_region(&config), "cred-api-region");
    }

    #[test]
    fn test_effective_api_region_fallback_to_config_api_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.api_region = Some("config-api-region".to_string());

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_api_region(&config), "config-api-region");
    }

    #[test]
    fn test_effective_api_region_fallback_to_config_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_api_region(&config), "config-region");
    }

    #[test]
    fn test_effective_api_region_ignores_credential_region() {
        // 凭据.region 不参与 api_region 的回退链
        let mut config = Config::default();
        config.region = "config-region".to_string();

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());

        assert_eq!(creds.effective_api_region(&config), "config-region");
    }

    #[test]
    fn test_auth_and_api_region_independent() {
        // auth_region 和 api_region 互不影响
        let mut config = Config::default();
        config.region = "default".to_string();

        let mut creds = KiroCredentials::default();
        creds.auth_region = Some("auth-only".to_string());
        creds.api_region = Some("api-only".to_string());

        assert_eq!(creds.effective_auth_region(&config), "auth-only");
        assert_eq!(creds.effective_api_region(&config), "api-only");
    }

    // ============ 凭据级代理优先级测试 ============

    #[test]
    fn test_effective_proxy_credential_overrides_global() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("socks5://cred:1080".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, Some(ProxyConfig::new("socks5://cred:1080")));
    }

    #[test]
    fn test_effective_proxy_credential_with_auth() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("http://proxy:3128".to_string());
        creds.proxy_username = Some("user".to_string());
        creds.proxy_password = Some("pass".to_string());

        let result = creds.effective_proxy(Some(&global));
        let expected = ProxyConfig::new("http://proxy:3128").with_auth("user", "pass");
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_effective_proxy_direct_bypasses_global() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("direct".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, None);
    }

    #[test]
    fn test_effective_proxy_direct_case_insensitive() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("DIRECT".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, None);
    }

    #[test]
    fn test_effective_proxy_fallback_to_global() {
        let global = ProxyConfig::new("http://global:8080");
        let creds = KiroCredentials::default();

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, Some(ProxyConfig::new("http://global:8080")));
    }

    #[test]
    fn test_effective_proxy_none_when_no_proxy() {
        let creds = KiroCredentials::default();
        let result = creds.effective_proxy(None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_provider_field_roundtrip() {
        let mut original = KiroCredentials::default();
        original.refresh_token = Some("rt".to_string());
        original.auth_method = Some("idc".to_string());
        original.provider = Some("BuilderId".to_string());
        original.profile_arn = Some(
            "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX".to_string(),
        );
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("provider"));
        assert!(json.contains("BuilderId"));
        assert!(json.contains("profileArn"));
        let parsed: KiroCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, original.provider);
        assert_eq!(parsed.profile_arn, original.profile_arn);
    }

    #[test]
    fn test_provider_field_missing_backward_compat() {
        let json = r#"{"refreshToken": "t", "authMethod": "idc"}"#;
        let creds: KiroCredentials = serde_json::from_str(json).unwrap();
        assert_eq!(creds.provider, None);
    }

    #[test]
    fn test_identity_fields_roundtrip() {
        let json = r#"{
            "refreshToken": "rt",
            "email": "a@example.com",
            "userId": "user-123",
            "nickname": "nick",
            "startUrl": "https://sso.example.com/start"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.email.as_deref(), Some("a@example.com"));
        assert_eq!(creds.user_id.as_deref(), Some("user-123"));
        assert_eq!(creds.nickname.as_deref(), Some("nick"));
        assert_eq!(creds.start_url.as_deref(), Some("https://sso.example.com/start"));

        let out = creds.to_pretty_json().unwrap();
        assert!(out.contains("userId"));
        assert!(out.contains("nickname"));
        assert!(out.contains("startUrl"));

        let legacy = r#"{"refreshToken": "rt"}"#;
        let legacy_creds = KiroCredentials::from_json(legacy).unwrap();
        assert!(legacy_creds.user_id.is_none());
        assert!(legacy_creds.nickname.is_none());
        assert!(legacy_creds.start_url.is_none());
    }

    // ============ authMethod 规范化 ============

    #[test]
    fn test_parse_auth_method_all_aliases() {
        for v in ["social", "SOCIAL", " Social "] {
            assert_eq!(parse_auth_method(v), Ok(AuthMethod::Social), "输入: {v}");
        }
        for v in ["idc", "IdC", "IDC", "builder-id", "BUILDER-ID", "iam", "IAM"] {
            assert_eq!(parse_auth_method(v), Ok(AuthMethod::Idc), "输入: {v}");
        }
        for v in [
            "external_idp",
            "EXTERNAL_IDP",
            "external-idp",
            "externalidp",
            "azure",
            "AZURE",
            "azuread",
            "azure_ad",
            "AZURE_AD",
        ] {
            assert_eq!(parse_auth_method(v), Ok(AuthMethod::ExternalIdp), "输入: {v}");
        }
        for v in ["api_key", "API_KEY", "apikey", "APIKEY"] {
            assert_eq!(parse_auth_method(v), Ok(AuthMethod::ApiKey), "输入: {v}");
        }
    }

    #[test]
    fn test_parse_auth_method_rejects_unknown() {
        let err = parse_auth_method("oauth2").unwrap_err();
        assert_eq!(err.value, "oauth2");
        // 错误信息必须列出合法取值
        let shown = err.to_string();
        assert!(shown.contains("social"));
        assert!(shown.contains("external_idp"));
        assert!(shown.contains("api_key"));

        assert!(parse_auth_method("").is_err());
        assert!(parse_auth_method("microsoft").is_err());
    }

    #[test]
    fn test_auth_method_as_str_matches_persisted_literals() {
        assert_eq!(AuthMethod::Social.as_str(), "social");
        assert_eq!(AuthMethod::Idc.as_str(), "idc");
        assert_eq!(AuthMethod::ExternalIdp.as_str(), "external_idp");
        assert_eq!(AuthMethod::ApiKey.as_str(), "api_key");
    }

    #[test]
    fn test_classify_explicit_beats_field_inference() {
        // 显式 external_idp + client 字段齐全：不得因 client 字段被判为 idc
        let mut c = KiroCredentials::default();
        c.auth_method = Some("external_idp".to_string());
        c.client_id = Some("cid".to_string());
        c.client_secret = Some("sec".to_string());
        c.refresh_token = Some("rt".to_string());
        assert_eq!(c.classify_auth_method(), Ok(AuthMethod::ExternalIdp));
    }

    #[test]
    fn test_classify_external_inference_precedes_idc() {
        // 无 authMethod，但同时有 clientId + clientSecret + 白名单 tokenEndpoint
        // → 必须判为 external_idp（external 分支先于 idc）
        let mut c = KiroCredentials::default();
        c.client_id = Some("cid".to_string());
        c.client_secret = Some("sec".to_string());
        c.refresh_token = Some("rt".to_string());
        c.token_endpoint =
            Some("https://login.microsoftonline.com/tenant/oauth2/v2.0/token".to_string());
        assert_eq!(c.classify_auth_method(), Ok(AuthMethod::ExternalIdp));
    }

    #[test]
    fn test_classify_issuer_url_also_infers_external() {
        let mut c = KiroCredentials::default();
        c.client_id = Some("cid".to_string());
        c.client_secret = Some("sec".to_string());
        c.issuer_url = Some("https://login.microsoftonline.com/tenant".to_string());
        assert_eq!(c.classify_auth_method(), Ok(AuthMethod::ExternalIdp));
    }

    #[test]
    fn test_classify_non_whitelisted_endpoint_does_not_infer_external() {
        // 非白名单 endpoint 不构成 external 判据，回落到 idc
        let mut c = KiroCredentials::default();
        c.client_id = Some("cid".to_string());
        c.client_secret = Some("sec".to_string());
        c.token_endpoint = Some("https://attacker.example/token".to_string());
        assert_eq!(c.classify_auth_method(), Ok(AuthMethod::Idc));
    }

    #[test]
    fn test_classify_fallback_order() {
        // idc：client 字段齐全，无 endpoint
        let mut idc = KiroCredentials::default();
        idc.client_id = Some("cid".to_string());
        idc.client_secret = Some("sec".to_string());
        assert_eq!(idc.classify_auth_method(), Ok(AuthMethod::Idc));

        // api_key：无 client 字段，有 kiroApiKey
        let mut ak = KiroCredentials::default();
        ak.kiro_api_key = Some("ksk_x".to_string());
        assert_eq!(ak.classify_auth_method(), Ok(AuthMethod::ApiKey));

        // social：仅 refreshToken
        let mut social = KiroCredentials::default();
        social.refresh_token = Some("rt".to_string());
        assert_eq!(social.classify_auth_method(), Ok(AuthMethod::Social));
    }

    #[test]
    fn test_classify_rejects_explicit_unknown() {
        let mut c = KiroCredentials::default();
        c.auth_method = Some("oauth2".to_string());
        c.refresh_token = Some("rt".to_string());
        // 显式未知值必须报错，不得静默降级为 social
        assert!(c.classify_auth_method().is_err());
    }

    #[test]
    fn test_canonicalize_auth_method_value_still_passes_unknown_through() {
        // 落盘归一的契约不变：未知值原样透传，历史脏值不得导致落盘失败
        let mut c = KiroCredentials::default();
        c.auth_method = Some("oauth2".to_string());
        c.canonicalize_auth_method();
        assert_eq!(c.auth_method.as_deref(), Some("oauth2"));

        // 已知别名仍归一
        let mut b = KiroCredentials::default();
        b.auth_method = Some("builder-id".to_string());
        b.canonicalize_auth_method();
        assert_eq!(b.auth_method.as_deref(), Some("idc"));

        let mut a = KiroCredentials::default();
        a.auth_method = Some("APIKEY".to_string());
        a.canonicalize_auth_method();
        assert_eq!(a.auth_method.as_deref(), Some("api_key"));
    }

    // ============ external_idp 字段 round-trip ============

    #[test]
    fn test_external_fields_roundtrip() {
        let json = r#"{
            "refreshToken": "rt",
            "authMethod": "external_idp",
            "clientId": "cid",
            "tokenEndpoint": "https://login.microsoftonline.com/t/oauth2/v2.0/token",
            "issuerUrl": "https://login.microsoftonline.com/t",
            "scopes": "openid profile offline_access"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(
            creds.token_endpoint.as_deref(),
            Some("https://login.microsoftonline.com/t/oauth2/v2.0/token")
        );
        assert_eq!(
            creds.issuer_url.as_deref(),
            Some("https://login.microsoftonline.com/t")
        );
        assert_eq!(
            creds.scopes.as_deref(),
            Some("openid profile offline_access")
        );

        let out = creds.to_pretty_json().unwrap();
        assert!(out.contains("tokenEndpoint"));
        assert!(out.contains("issuerUrl"));
        assert!(out.contains("scopes"));

        let reparsed = KiroCredentials::from_json(&out).unwrap();
        assert_eq!(reparsed.token_endpoint, creds.token_endpoint);
        assert_eq!(reparsed.issuer_url, creds.issuer_url);
        assert_eq!(reparsed.scopes, creds.scopes);
    }

    #[test]
    fn test_external_fields_missing_backward_compat() {
        // 旧凭据文件缺三字段仍可加载
        let json = r#"{"refreshToken": "rt", "authMethod": "social"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(creds.token_endpoint.is_none());
        assert!(creds.issuer_url.is_none());
        assert!(creds.scopes.is_none());

        // 未设置时不应出现在序列化输出中
        let out = creds.to_pretty_json().unwrap();
        assert!(!out.contains("tokenEndpoint"));
        assert!(!out.contains("issuerUrl"));
        assert!(!out.contains("scopes"));
    }

    #[test]
    fn test_external_fields_explicit_null_treated_as_absent() {
        // KAM 导出对未设置的可选字段输出显式 null
        let json = r#"{
            "refreshToken": "rt",
            "tokenEndpoint": null,
            "issuerUrl": null,
            "scopes": null
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(creds.token_endpoint.is_none());
        assert!(creds.issuer_url.is_none());
        assert!(creds.scopes.is_none());
    }

    // ============ 容器判别与迁移 ============

    fn load_temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kiro-rs-load-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_rt(tag: &str) -> String {
        format!("fake-refresh-token-{tag}-{}", "0".repeat(120))
    }

    #[test]
    fn test_load_native_array_no_migration() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        let content = format!(
            r#"[{{"refreshToken":"{}","authMethod":"social","priority":1}}]"#,
            fake_rt("a")
        );
        std::fs::write(&path, &content).unwrap();

        let loaded = CredentialsConfig::load_detailed(&path).unwrap();
        assert!(!loaded.needs_migration, "原生数组不应触发迁移");
        assert!(loaded.config.is_multiple());
        assert_eq!(loaded.config.into_sorted_credentials().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_native_single_object_no_migration() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        std::fs::write(
            &path,
            format!(r#"{{"refreshToken":"{}","authMethod":"social"}}"#, fake_rt("b")),
        )
        .unwrap();

        let loaded = CredentialsConfig::load_detailed(&path).unwrap();
        assert!(!loaded.needs_migration);
        assert!(
            !loaded.config.is_multiple(),
            "原生单对象必须保持 Single，以维持 is_multiple_format 语义"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_wrapper_object_is_recognized_not_swallowed() {
        // 这是本 change 修复的核心缺陷：untagged 会把 wrapper 静默匹配成
        // 一条字段全空的凭据，日志显示「已加载 1 个凭据配置」但无账号可用。
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        let content = format!(
            r#"{{"version":"1.9.2","accounts":[
                {{"label":"号1","authMethod":"social","refreshToken":"{}"}},
                {{"label":"号2","authMethod":"social","refreshToken":"{}"}}
            ]}}"#,
            fake_rt("w1"),
            fake_rt("w2")
        );
        std::fs::write(&path, &content).unwrap();

        let loaded = CredentialsConfig::load_detailed(&path).unwrap();
        assert!(loaded.needs_migration, "wrapper 应标记为需迁移");
        let creds = loaded.config.into_sorted_credentials();
        assert_eq!(creds.len(), 2, "必须解析出 2 条，而非 1 条空凭据");
        assert!(creds[0].refresh_token.is_some());
        assert_eq!(creds[0].nickname.as_deref(), Some("号1"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_legacy_nested_is_recognized() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        std::fs::write(
            &path,
            format!(
                r#"{{"label":"嵌套号","credentials":{{"authMethod":"social","refreshToken":"{}"}}}}"#,
                fake_rt("n")
            ),
        )
        .unwrap();

        let loaded = CredentialsConfig::load_detailed(&path).unwrap();
        assert!(loaded.needs_migration);
        let creds = loaded.config.into_sorted_credentials();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].nickname.as_deref(), Some("嵌套号"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_unknown_wrapper_fails_fast_with_json_path() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        std::fs::write(&path, r#"{"version":"1.0","data":[],"meta":{}}"#).unwrap();

        let err = CredentialsConfig::load_detailed(&path)
            .expect_err("未知包装对象必须 fail fast");
        let msg = err.to_string();
        assert!(msg.contains("$"), "错误应含 JSON 位置: {msg}");
        assert!(msg.contains("version"), "错误应含顶层 key 名: {msg}");
        assert!(msg.contains("无法识别"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_unknown_auth_method_reports_index() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        let content = format!(
            r#"[{{"refreshToken":"{}","authMethod":"social"}},
                {{"refreshToken":"{}","authMethod":"oauth2"}}]"#,
            fake_rt("ok"),
            fake_rt("bad")
        );
        std::fs::write(&path, &content).unwrap();

        let err = CredentialsConfig::load_detailed(&path).expect_err("未知 authMethod 应报错");
        let msg = err.to_string();
        assert!(msg.contains("第 1 条"), "应指出凭据 index: {msg}");
        assert!(msg.contains("oauth2"));
        assert!(msg.contains("social"), "应列出合法取值: {msg}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_missing_and_empty_file_unchanged() {
        let dir = load_temp_dir();

        // 文件不存在
        let missing = dir.join("nope.json");
        let loaded = CredentialsConfig::load_detailed(&missing).unwrap();
        assert!(!loaded.needs_migration);
        assert!(loaded.config.into_sorted_credentials().is_empty());

        // 文件为空
        let empty = dir.join("empty.json");
        std::fs::write(&empty, "   \n  ").unwrap();
        let loaded = CredentialsConfig::load_detailed(&empty).unwrap();
        assert!(!loaded.needs_migration);
        assert!(loaded.config.into_sorted_credentials().is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_migrate_backs_up_and_writes_native_format() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        let original = format!(
            r#"{{"version":"1.9.2","accounts":[{{"label":"号1","authMethod":"social","refreshToken":"{}"}}]}}"#,
            fake_rt("m")
        );
        std::fs::write(&path, &original).unwrap();

        let loaded = CredentialsConfig::load_detailed(&path).unwrap();
        assert!(loaded.needs_migration);

        let backup = CredentialsConfig::migrate_to_native(&path, &loaded.config).unwrap();

        // 备份存在且内容为原始文档
        assert!(backup.exists());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);

        // 目标文件已是原生数组格式
        let migrated = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&migrated).unwrap();
        assert!(value.is_array(), "迁移后应为原生数组格式");

        // 再次加载不应触发迁移，且内容等价
        let reloaded = CredentialsConfig::load_detailed(&path).unwrap();
        assert!(!reloaded.needs_migration, "迁移后不应重复迁移");
        let creds = reloaded.config.into_sorted_credentials();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].nickname.as_deref(), Some("号1"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_migrate_failure_preserves_original_file() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        let original = format!(
            r#"{{"accounts":[{{"authMethod":"social","refreshToken":"{}"}}]}}"#,
            fake_rt("f")
        );
        std::fs::write(&path, &original).unwrap();
        let loaded = CredentialsConfig::load_detailed(&path).unwrap();

        // 备份阶段失败：源路径不存在
        let missing = dir.join("subdir-absent").join("credentials.json");
        assert!(CredentialsConfig::migrate_to_native(&missing, &loaded.config).is_err());

        // 原文件内容不变
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_migration_equivalence_between_containers() {
        // 同一份逻辑内容以四种容器写入，迁移后应得到等价凭据
        let dir = load_temp_dir();
        let rt = fake_rt("equiv");

        let docs = vec![
            ("flat_array", format!(r#"[{{"label":"X","authMethod":"social","refreshToken":"{rt}"}}]"#)),
            ("flat_object", format!(r#"{{"label":"X","authMethod":"social","refreshToken":"{rt}"}}"#)),
            ("wrapper", format!(r#"{{"version":"1.9.2","accounts":[{{"label":"X","authMethod":"social","refreshToken":"{rt}"}}]}}"#)),
            ("nested", format!(r#"{{"label":"X","credentials":{{"authMethod":"social","refreshToken":"{rt}"}}}}"#)),
        ];

        let mut all: Vec<KiroCredentials> = Vec::new();
        for (name, content) in &docs {
            let path = dir.join(format!("{name}.json"));
            std::fs::write(&path, content).unwrap();
            let loaded = CredentialsConfig::load_detailed(&path).unwrap();
            let mut creds = loaded.config.into_sorted_credentials();
            assert_eq!(creds.len(), 1, "{name} 应解析出 1 条");
            all.push(creds.remove(0));
        }

        // 四种容器的认证字段与身份字段逐一相等
        for (i, c) in all.iter().enumerate().skip(1) {
            let base = &all[0];
            assert_eq!(c.auth_method, base.auth_method, "容器 {i} 的 authMethod 不一致");
            assert_eq!(c.refresh_token, base.refresh_token, "容器 {i} 的 refreshToken 不一致");
            assert_eq!(c.nickname, base.nickname, "容器 {i} 的 nickname 不一致");
            assert_eq!(c.disabled, base.disabled, "容器 {i} 的 disabled 不一致");
            assert_eq!(c.region, base.region);
            assert_eq!(c.token_endpoint, base.token_endpoint);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_external_credential_from_file() {
        let dir = load_temp_dir();
        let path = dir.join("credentials.json");
        let content = format!(
            r#"[{{
                "refreshToken":"{}",
                "authMethod":"external_idp",
                "clientId":"ms-cid",
                "tokenEndpoint":"https://login.microsoftonline.com/t/oauth2/v2.0/token",
                "scopes":"openid profile"
            }}]"#,
            fake_rt("ext")
        );
        std::fs::write(&path, &content).unwrap();

        let loaded = CredentialsConfig::load_detailed(&path).unwrap();
        assert!(!loaded.needs_migration, "原生数组含 external 字段也是原生格式");
        let creds = loaded.config.into_sorted_credentials();
        assert_eq!(creds[0].auth_method.as_deref(), Some("external_idp"));
        assert!(creds[0].token_endpoint.is_some());
        assert_eq!(creds[0].scopes.as_deref(), Some("openid profile"));
        assert!(creds[0].provider.is_none(), "external 不得回填 provider");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_region_resolution_chains_remain_distinct() {
        // 两条 region 回退链故意不同：auth region 回退到凭据级 region，
        // api region 不回退。改动任一条都会破坏「A 区认证、B 区调用」的配置能力。
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut creds = KiroCredentials::default();
        creds.region = Some("eu-west-1".to_string());

        assert_eq!(creds.effective_auth_region(&config), "eu-west-1");
        assert_eq!(creds.effective_api_region(&config), "us-west-2");
    }
}
