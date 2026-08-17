//! Admin API 类型定义

use serde::{Deserialize, Serialize};

use crate::kiro::model::credentials::{parse_auth_method, AuthMethod};

// ============ 凭据状态 ============

/// 所有凭据状态响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialsStatusResponse {
    /// 凭据总数
    pub total: usize,
    /// 可用凭据数量（未禁用）
    pub available: usize,
    /// 当前活跃凭据 ID
    pub current_id: u64,
    /// 各凭据状态列表
    pub credentials: Vec<CredentialStatusItem>,
}

/// 单个凭据的状态信息
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusItem {
    /// 凭据唯一 ID
    pub id: u64,
    /// 优先级（数字越小优先级越高）
    pub priority: u32,
    /// 是否被禁用
    pub disabled: bool,
    /// 连续失败次数
    pub failure_count: u32,
    /// 是否为当前活跃凭据
    pub is_current: bool,
    /// Token 过期时间（RFC3339 格式）
    pub expires_at: Option<String>,
    /// 认证方式
    pub auth_method: Option<String>,
    /// 是否有 Profile ARN
    pub has_profile_arn: bool,
    /// Identity provider（BuilderId / Github / ...）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// refreshToken 的 SHA-256 哈希（仅 OAuth 凭据，用于前端去重）
    pub refresh_token_hash: Option<String>,
    /// kiroApiKey 的 SHA-256 哈希（仅 API Key 凭据，用于前端去重）
    pub api_key_hash: Option<String>,
    /// kiroApiKey 的脱敏展示（仅 API Key 凭据，用于前端显示）
    pub masked_api_key: Option<String>,
    /// 用户邮箱（用于前端显示）
    pub email: Option<String>,
    /// Kiro 稳定用户 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// 展示名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// API 调用成功次数
    pub success_count: u64,
    /// 最后一次 API 调用时间（RFC3339 格式）
    pub last_used_at: Option<String>,
    /// 是否配置了凭据级代理
    pub has_proxy: bool,
    /// 代理 URL（用于前端展示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// Token 刷新连续失败次数
    pub refresh_failure_count: u32,
    /// 禁用原因
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    /// 端点名称（决定该凭据走哪套 Kiro API，已回退到默认端点）
    pub endpoint: String,
    /// 凭据模型缓存数量（无缓存时为 0）
    #[serde(default)]
    pub model_count: u32,
    /// 模型缓存更新时间（RFC3339）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models_updated_at: Option<String>,
    /// 最近一次模型刷新错误
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models_last_error: Option<String>,
}

/// 全局模型 catalog 摘要（Admin）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalModelsCatalogResponse {
    pub success: bool,
    pub count: usize,
    pub models: Vec<String>,
    /// 带解析元数据的模型列表（与 models 同序子集；兼容旧客户端）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_items: Vec<ModelCatalogItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Admin 模型列表条目（raw + 解析元数据）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogItem {
    pub id: String,
    pub resolvable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_kind: Option<String>,
    pub testable: bool,
}

/// 余额查询参数
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BalanceQuery {
    /// true 时跳过 TTL 缓存强制刷新
    #[serde(default)]
    pub force: bool,
}

// ============ 操作请求 ============

/// 启用/禁用凭据请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDisabledRequest {
    /// 是否禁用
    pub disabled: bool,
}

/// 修改优先级请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPriorityRequest {
    /// 新优先级值
    pub priority: u32,
}

/// 添加凭据请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCredentialRequest {
    /// 刷新令牌（OAuth 凭据必填，API Key 凭据不需要）
    pub refresh_token: Option<String>,

    /// 认证方式（可选，默认 social）
    #[serde(default = "default_auth_method")]
    pub auth_method: String,

    /// Identity provider（可选，IdC 导入默认 BuilderId）
    pub provider: Option<String>,

    /// Profile ARN（可选，导入时若已知应保留）
    pub profile_arn: Option<String>,

    /// OIDC Client ID（IdC 认证需要）
    pub client_id: Option<String>,

    /// OIDC Client Secret（IdC 认证需要）
    pub client_secret: Option<String>,

    /// 优先级（可选，默认 0）
    #[serde(default)]
    pub priority: u32,

    /// 凭据级 Region 配置（用于 OIDC token 刷新）
    /// 未配置时回退到 config.json 的全局 region
    pub region: Option<String>,

    /// 凭据级 Auth Region（用于 Token 刷新）
    pub auth_region: Option<String>,

    /// 凭据级 API Region（用于 API 请求）
    pub api_region: Option<String>,

    /// 凭据级 Machine ID（可选，64 位字符串）
    /// 未配置时回退到 config.json 的 machineId
    pub machine_id: Option<String>,

    /// 用户邮箱（可选，用于前端显示）
    pub email: Option<String>,

    /// Kiro 稳定用户 ID（可选，用于 upsert）
    pub user_id: Option<String>,

    /// 展示名（可选）
    pub nickname: Option<String>,

    /// IAM SSO start URL（可选）
    pub start_url: Option<String>,

    /// 冲突策略：reject | upsert | replace_token_only（可选）
    pub on_conflict: Option<String>,

    /// 凭据级代理 URL（可选，特殊值 "direct" 表示不使用代理）
    pub proxy_url: Option<String>,

    /// 凭据级代理认证用户名（可选）
    pub proxy_username: Option<String>,

    /// 凭据级代理认证密码（可选）
    pub proxy_password: Option<String>,

    /// Kiro API Key（API Key 凭据必填，格式: ksk_xxxxxxxx）
    /// 设置后直接作为 Bearer Token 使用，无需 refreshToken
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiro_api_key: Option<String>,

    /// 端点名称（可选，未配置时使用 config.defaultEndpoint）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// external_idp 的 OAuth2 token 端点（external_idp 需要，或用 issuerUrl 派生）
    pub token_endpoint: Option<String>,

    /// external_idp 的 issuer URL（可选，用于派生 tokenEndpoint）
    pub issuer_url: Option<String>,

    /// external_idp 的 OAuth2 scopes（可选，空格分隔单串）
    pub scopes: Option<String>,
}

fn default_auth_method() -> String {
    "social".to_string()
}

impl AddCredentialRequest {
    /// 按认证族校验必需字段
    ///
    /// 返回 Err 时错误信息面向操作者，不含任何密钥材料。
    ///
    /// external_idp **不要求** clientSecret：公共客户端本就没有 secret，
    /// 强制要求会逼用户伪造，既过不了上游也污染存储。
    pub fn validate_shape(&self) -> Result<AuthMethod, String> {
        let method = parse_auth_method(&self.auth_method).map_err(|e| e.to_string())?;

        let non_empty = |v: &Option<String>| {
            v.as_deref()
                .map(str::trim)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        };

        match method {
            AuthMethod::ApiKey => {
                if !non_empty(&self.kiro_api_key) {
                    return Err("api_key 凭据需要非空 kiroApiKey".to_string());
                }
            }
            AuthMethod::Social => {
                if !non_empty(&self.refresh_token) {
                    return Err("social 凭据需要 refreshToken".to_string());
                }
            }
            AuthMethod::Idc => {
                if !non_empty(&self.refresh_token) {
                    return Err("idc 凭据需要 refreshToken".to_string());
                }
                if !non_empty(&self.client_id) || !non_empty(&self.client_secret) {
                    return Err(
                        "idc 凭据需要同时提供 clientId 和 clientSecret".to_string()
                    );
                }
            }
            AuthMethod::ExternalIdp => {
                if !non_empty(&self.refresh_token) {
                    return Err("external_idp 凭据需要 refreshToken".to_string());
                }
                if !non_empty(&self.client_id) {
                    return Err("external_idp 凭据需要 clientId".to_string());
                }
                if !non_empty(&self.token_endpoint) && !non_empty(&self.issuer_url) {
                    return Err(
                        "external_idp 凭据需要 tokenEndpoint 或 issuerUrl 之一".to_string(),
                    );
                }
                // 校验 endpoint 合法性：不合法则不得进入后续任何出站请求
                crate::kiro::external_idp::resolve_token_endpoint(
                    self.token_endpoint.as_deref(),
                    self.issuer_url.as_deref(),
                )
                .map_err(|e| e.to_string())?;
            }
        }

        Ok(method)
    }
}

/// 添加凭据成功响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCredentialResponse {
    pub success: bool,
    pub message: String,
    /// 新添加的凭据 ID
    pub credential_id: u64,
    /// 用户邮箱（如果获取成功）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// created | updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Kiro 稳定用户 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

// ============ 余额查询 ============

/// 余额查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResponse {
    /// 凭据 ID
    pub id: u64,
    /// 订阅类型
    pub subscription_title: Option<String>,
    /// 当前使用量
    pub current_usage: f64,
    /// 使用限额
    pub usage_limit: f64,
    /// 剩余额度
    pub remaining: f64,
    /// 使用百分比
    pub usage_percentage: f64,
    /// 下次重置时间（Unix 时间戳）
    pub next_reset_at: Option<f64>,
}

// ============ 模型目录 / 测试 ============

/// 单凭据模型刷新响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsRefreshResponse {
    pub success: bool,
    pub credential_id: u64,
    pub count: usize,
    pub models: Vec<String>,
    pub updated_at: String,
}

/// 全量模型刷新响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsRefreshAllResponse {
    pub success: bool,
    pub refreshed: usize,
    pub failed: usize,
    pub global_count: usize,
    pub errors: Vec<ModelsRefreshErrorItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsRefreshErrorItem {
    pub credential_id: u64,
    pub error: String,
}

/// 凭据模型列表响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialModelsResponse {
    pub success: bool,
    pub models: Vec<String>,
    /// 带解析元数据的模型列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_items: Vec<ModelCatalogItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// 凭据测试请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCredentialRequest {
    /// 可选模型名（客户端 id 或上游 id）；默认 claude-sonnet-4.6
    #[serde(default)]
    pub model: Option<String>,
}

/// 凭据测试响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCredentialResponse {
    pub success: bool,
    /// 客户端请求的 model（或默认）
    pub model: String,
    /// 实际发送上游的 modelId
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_model: Option<String>,
    /// alias | normalized | passthrough
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    pub latency_ms: u64,
}

// ============ 负载均衡配置 ============

/// 负载均衡模式响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancingModeResponse {
    /// 当前模式（"priority" 或 "balanced"）
    pub mode: String,
}

/// 设置负载均衡模式请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLoadBalancingModeRequest {
    /// 模式（"priority" 或 "balanced"）
    pub mode: String,
}

// ============ 通用响应 ============

/// 操作成功响应
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

impl SuccessResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }
}

/// 错误响应
#[derive(Debug, Serialize)]
pub struct AdminErrorResponse {
    pub error: AdminError,
}

#[derive(Debug, Serialize)]
pub struct AdminError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

impl AdminErrorResponse {
    pub fn new(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: AdminError {
                error_type: error_type.into(),
                message: message.into(),
            },
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message)
    }

    pub fn authentication_error() -> Self {
        Self::new("authentication_error", "Invalid or missing admin API key")
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub fn api_error(message: impl Into<String>) -> Self {
        Self::new("api_error", message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new("internal_error", message)
    }
}

// ============ 批量导入 ============

/// 批量导入选项
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportOptions {
    #[serde(default)]
    pub on_conflict: Option<String>,
    #[serde(default)]
    pub stop_on_error: Option<bool>,
    #[serde(default)]
    pub fetch_balance: Option<bool>,
    #[serde(default)]
    pub concurrency: Option<u32>,
}

/// 批量导入请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportRequest {
    pub items: Vec<AddCredentialRequest>,
    #[serde(default)]
    pub options: Option<BatchImportOptions>,
}

/// 批量导入单条结果
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportItemResult {
    pub index: usize,
    /// created | updated | duplicate | failed
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<BalanceResponse>,
    /// profile 未解析等非致命警告
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// 批量导入汇总
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportSummary {
    pub created: usize,
    pub updated: usize,
    pub duplicate: usize,
    pub failed: usize,
}

/// 批量导入响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportResponse {
    pub success: bool,
    pub summary: BatchImportSummary,
    pub results: Vec<BatchImportItemResult>,
}

/// KAM 导出文件导入请求
///
/// 直接接收原始 KAM 文档，容器判别与认证分类均在服务端完成——客户端再实现一套
/// 判别规则会让同一文件在不同入口得到不同结果。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KamImportRequest {
    /// 原始 KAM 文档（平铺单对象 / 平铺数组 / wrapper / 旧版嵌套）
    pub document: serde_json::Value,
    #[serde(default)]
    pub options: Option<BatchImportOptions>,
    /// 仅预检不入库
    #[serde(default)]
    pub dry_run: bool,
}

/// KAM 导入的逐条预检结果（不含任何密钥材料）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KamPreviewItem {
    pub index: usize,
    /// JSON 位置，便于定位源文件中的记录
    pub path: String,
    /// 识别出的认证方式（失败时为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// 字段完整性：是否具备各类必需/可选字段（不回传字段值）
    pub has_refresh_token: bool,
    pub has_client_id: bool,
    pub has_client_secret: bool,
    pub has_token_endpoint: bool,
    pub has_issuer_url: bool,
    pub has_scopes: bool,
    pub has_profile_arn: bool,
    /// 该记录是否会被禁用（来自 enabled 取反）
    pub disabled: bool,
    /// 预检是否通过
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// KAM 导入响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KamImportResponse {
    pub success: bool,
    /// 识别出的容器形态
    pub container: String,
    /// 逐条预检结果
    pub preview: Vec<KamPreviewItem>,
    /// 实际入库汇总（dryRun 时为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<BatchImportSummary>,
    /// 实际入库逐条结果（dryRun 时为空）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<BatchImportItemResult>,
}

// ============ 在线授权 ============

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIdStartRequest {
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIdPollRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct BuilderIdPollCompletedResponse {
    pub success: bool,
    pub completed: bool,
    pub credential_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IamSsoStartRequest {
    pub start_url: String,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IamSsoCompleteRequest {
    pub session_id: String,
    pub callback_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoTokenImportRequest {
    pub bearer_token: String,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoTokenImportResponse {
    pub success: bool,
    pub accounts: Vec<SsoTokenAccountResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoTokenAccountResult {
    pub credential_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_import_request_deserializes_items() {
        let token = "x".repeat(150);
        let json = format!(
            r#"{{"items":[{{"refreshToken":"{}","authMethod":"social"}}]}}"#,
            token
        );
        let req: BatchImportRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.items.len(), 1);
        assert!(req.options.is_none());
        assert_eq!(req.items[0].auth_method, "social");
    }

    #[test]
    fn add_credential_request_accepts_identity_fields() {
        let token = "y".repeat(150);
        let json = format!(
            r#"{{"refreshToken":"{}","authMethod":"idc","userId":"u-1","nickname":"n1","startUrl":"https://example.awsapps.com/start","onConflict":"upsert","provider":"BuilderId","profileArn":"arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX"}}"#,
            token
        );
        let req: AddCredentialRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.user_id.as_deref(), Some("u-1"));
        assert_eq!(req.nickname.as_deref(), Some("n1"));
        assert_eq!(req.on_conflict.as_deref(), Some("upsert"));
        assert_eq!(req.provider.as_deref(), Some("BuilderId"));
        assert!(req.profile_arn.is_some());
    }

    #[test]
    fn add_credential_response_serializes_action() {
        let resp = AddCredentialResponse {
            success: true,
            message: "ok".into(),
            credential_id: 3,
            email: Some("a@b.c".into()),
            action: Some("created".into()),
            user_id: Some("u".into()),
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["action"], "created");
        assert_eq!(v["userId"], "u");
        assert_eq!(v["credentialId"], 3);
    }

    // ============ external_idp 契约 ============

    fn base_request(auth_method: &str) -> AddCredentialRequest {
        AddCredentialRequest {
            refresh_token: Some("z".repeat(150)),
            auth_method: auth_method.to_string(),
            provider: None,
            profile_arn: None,
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
            on_conflict: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            kiro_api_key: None,
            endpoint: None,
            token_endpoint: None,
            issuer_url: None,
            scopes: None,
        }
    }

    #[test]
    fn add_credential_request_accepts_external_fields() {
        let token = "e".repeat(150);
        let json = format!(
            r#"{{"refreshToken":"{}","authMethod":"external_idp","clientId":"ms-cid","tokenEndpoint":"https://login.microsoftonline.com/t/oauth2/v2.0/token","issuerUrl":"https://login.microsoftonline.com/t","scopes":"openid profile"}}"#,
            token
        );
        let req: AddCredentialRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.auth_method, "external_idp");
        assert_eq!(
            req.token_endpoint.as_deref(),
            Some("https://login.microsoftonline.com/t/oauth2/v2.0/token")
        );
        assert_eq!(
            req.issuer_url.as_deref(),
            Some("https://login.microsoftonline.com/t")
        );
        assert_eq!(req.scopes.as_deref(), Some("openid profile"));
    }

    #[test]
    fn default_auth_method_is_still_social() {
        let token = "d".repeat(150);
        let json = format!(r#"{{"refreshToken":"{}"}}"#, token);
        let req: AddCredentialRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.auth_method, "social", "缺省值不得改变");
    }

    #[test]
    fn validate_shape_rejects_unknown_auth_method() {
        let req = base_request("oauth2");
        let err = req.validate_shape().expect_err("未知 authMethod 应被拒");
        assert!(err.contains("oauth2"));
        assert!(err.contains("social"), "错误应列出合法取值");
        assert!(err.contains("external_idp"));
    }

    #[test]
    fn validate_shape_accepts_all_canonical_methods() {
        // social
        assert_eq!(base_request("social").validate_shape(), Ok(AuthMethod::Social));

        // idc
        let mut idc = base_request("idc");
        idc.client_id = Some("cid".into());
        idc.client_secret = Some("sec".into());
        assert_eq!(idc.validate_shape(), Ok(AuthMethod::Idc));

        // api_key
        let mut ak = base_request("api_key");
        ak.kiro_api_key = Some("ksk_x".into());
        assert_eq!(ak.validate_shape(), Ok(AuthMethod::ApiKey));

        // external_idp
        let mut ext = base_request("external_idp");
        ext.client_id = Some("ms-cid".into());
        ext.token_endpoint =
            Some("https://login.microsoftonline.com/t/oauth2/v2.0/token".into());
        assert_eq!(ext.validate_shape(), Ok(AuthMethod::ExternalIdp));
    }

    #[test]
    fn validate_shape_external_public_client_passes_without_secret() {
        let mut req = base_request("external_idp");
        req.client_id = Some("ms-public-cid".into());
        req.token_endpoint =
            Some("https://login.microsoftonline.com/t/oauth2/v2.0/token".into());
        // 公共客户端没有 clientSecret：不得因此被拒
        assert!(req.client_secret.is_none());
        assert_eq!(req.validate_shape(), Ok(AuthMethod::ExternalIdp));
    }

    #[test]
    fn validate_shape_external_requires_endpoint_or_issuer() {
        let mut req = base_request("external_idp");
        req.client_id = Some("ms-cid".into());
        let err = req.validate_shape().expect_err("缺 endpoint 应被拒");
        assert!(err.contains("tokenEndpoint 或 issuerUrl"), "实际: {err}");
    }

    #[test]
    fn validate_shape_external_requires_client_id() {
        let mut req = base_request("external_idp");
        req.token_endpoint =
            Some("https://login.microsoftonline.com/t/oauth2/v2.0/token".into());
        let err = req.validate_shape().expect_err("缺 clientId 应被拒");
        assert!(err.contains("clientId"), "实际: {err}");
    }

    #[test]
    fn validate_shape_external_rejects_non_whitelisted_endpoint() {
        let mut req = base_request("external_idp");
        req.client_id = Some("ms-cid".into());
        req.token_endpoint = Some("https://attacker.example/token".into());
        let err = req
            .validate_shape()
            .expect_err("非白名单 endpoint 应在入口被拒");
        assert!(err.contains("Microsoft 登录域"), "实际: {err}");
    }

    #[test]
    fn validate_shape_external_accepts_issuer_only() {
        let mut req = base_request("external_idp");
        req.client_id = Some("ms-cid".into());
        req.issuer_url = Some("https://login.microsoftonline.com/tenant".into());
        assert_eq!(req.validate_shape(), Ok(AuthMethod::ExternalIdp));
    }

    #[test]
    fn validate_shape_idc_still_requires_both_client_fields() {
        let mut req = base_request("idc");
        req.client_id = Some("cid".into());
        let err = req.validate_shape().expect_err("IdC 缺 secret 应被拒");
        assert!(err.contains("clientId 和 clientSecret"));
    }

    #[test]
    fn kam_import_request_deserializes() {
        let json = r#"{"document":{"accounts":[]},"dryRun":true}"#;
        let req: KamImportRequest = serde_json::from_str(json).unwrap();
        assert!(req.dry_run);
        assert!(req.document.get("accounts").is_some());
    }

    #[test]
    fn kam_preview_item_never_serializes_secrets() {
        let item = KamPreviewItem {
            index: 0,
            path: "$[0]".into(),
            auth_method: Some("external_idp".into()),
            provider: None,
            email: Some("a@b.c".into()),
            nickname: Some("n".into()),
            has_refresh_token: true,
            has_client_id: true,
            has_client_secret: false,
            has_token_endpoint: true,
            has_issuer_url: false,
            has_scopes: true,
            has_profile_arn: true,
            disabled: false,
            valid: true,
            error: None,
        };
        let v = serde_json::to_string(&item).unwrap();
        // 只有布尔状态，没有任何字段值
        assert!(v.contains("hasClientSecret"));
        assert!(v.contains("hasTokenEndpoint"));
        for forbidden in ["refreshToken\":\"", "clientSecret\":\"", "tokenEndpoint\":\""] {
            assert!(!v.contains(forbidden), "预检不得回传字段值: {forbidden}");
        }
    }
}

// ============ Runtime settings ============

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettingsResponse {
    pub proxy_url: Option<String>,
    pub has_proxy_auth: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_username: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProxySettingsRequest {
    pub proxy_url: Option<String>,
    pub proxy_username: Option<String>,
    pub proxy_password: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointSettingsResponse {
    pub default_endpoint: String,
    pub registered_endpoints: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEndpointSettingsRequest {
    pub default_endpoint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSettingsResponse {
    pub require_api_key: bool,
    pub has_api_key: bool,
    pub api_key_mask: Option<String>,
}

/// web_search 代执行设置（仅影响 /v1/responses 端点）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchSettingsResponse {
    pub web_search_emulation: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWebSearchSettingsRequest {
    pub web_search_emulation: bool,
}

/// WebSocket ingress 运行时设置（含活跃连接数）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsSettingsResponse {
    pub enabled: bool,
    /// `http_bridge` / `passthrough`（预留）
    pub mode: String,
    pub max_connections: usize,
    pub client_first_message_timeout_seconds: u64,
    pub inter_turn_idle_timeout_seconds: u64,
    pub max_message_bytes: usize,
    pub upstream_read_timeout_seconds: u64,
    /// 当前活跃 WS 连接数（准入计数器实时值）
    pub active_connections: usize,
}

/// WebSocket 设置部分更新请求：未携带字段保持当前值
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWsSettingsRequest {
    pub enabled: Option<bool>,
    /// `http_bridge` / `passthrough`；未知值返回 400
    pub mode: Option<String>,
    pub max_connections: Option<usize>,
    pub client_first_message_timeout_seconds: Option<u64>,
    pub inter_turn_idle_timeout_seconds: Option<u64>,
    pub max_message_bytes: Option<usize>,
    pub upstream_read_timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAuthSettingsRequest {
    pub require_api_key: Option<bool>,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientIdentitySettingsResponse {
    pub kiro_version: String,
    pub system_version: String,
    pub node_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateClientIdentitySettingsRequest {
    pub kiro_version: Option<String>,
    pub system_version: Option<String>,
    pub node_version: Option<String>,
}
