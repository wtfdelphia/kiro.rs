//! Admin API 业务逻辑服务

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::token_manager::MultiTokenManager;

use super::error::AdminServiceError;
use super::types::{
    AddCredentialRequest, AddCredentialResponse, BalanceResponse, CredentialStatusItem,
    CredentialsStatusResponse, LoadBalancingModeResponse, SetLoadBalancingModeRequest,
};

/// 余额缓存过期时间（秒），5 分钟
const BALANCE_CACHE_TTL_SECS: i64 = 300;

/// 缓存的余额条目（含时间戳）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBalance {
    /// 缓存时间（Unix 秒）
    cached_at: f64,
    /// 缓存的余额数据
    data: BalanceResponse,
}

/// Admin 服务
///
/// 封装所有 Admin API 的业务逻辑
pub struct AdminService {
    token_manager: Arc<MultiTokenManager>,
    balance_cache: Mutex<HashMap<u64, CachedBalance>>,
    cache_path: Option<PathBuf>,
    /// 已注册的端点名称集合（用于 add_credential 校验）
    known_endpoints: HashSet<String>,
}

impl AdminService {
    pub fn new(
        token_manager: Arc<MultiTokenManager>,
        known_endpoints: impl IntoIterator<Item = String>,
    ) -> Self {
        let cache_path = token_manager
            .cache_dir()
            .map(|d| d.join("kiro_balance_cache.json"));

        let balance_cache = Self::load_balance_cache_from(&cache_path);

        Self {
            token_manager,
            balance_cache: Mutex::new(balance_cache),
            cache_path,
            known_endpoints: known_endpoints.into_iter().collect(),
        }
    }

    /// 获取所有凭据状态
    pub fn get_all_credentials(&self) -> CredentialsStatusResponse {
        let snapshot = self.token_manager.snapshot();
        let default_endpoint = self.token_manager.config().default_endpoint.clone();

        let mut credentials: Vec<CredentialStatusItem> = snapshot
            .entries
            .into_iter()
            .map(|entry| CredentialStatusItem {
                id: entry.id,
                priority: entry.priority,
                disabled: entry.disabled,
                failure_count: entry.failure_count,
                is_current: entry.id == snapshot.current_id,
                expires_at: entry.expires_at,
                auth_method: entry.auth_method,
                has_profile_arn: entry.has_profile_arn,
                provider: entry.provider,
                refresh_token_hash: entry.refresh_token_hash,
                api_key_hash: entry.api_key_hash,
                masked_api_key: entry.masked_api_key,
                email: entry.email,
                user_id: entry.user_id,
                nickname: entry.nickname,
                success_count: entry.success_count,
                last_used_at: entry.last_used_at.clone(),
                has_proxy: entry.has_proxy,
                proxy_url: entry.proxy_url,
                refresh_failure_count: entry.refresh_failure_count,
                disabled_reason: entry.disabled_reason,
                endpoint: entry.endpoint.unwrap_or_else(|| default_endpoint.clone()),
            })
            .collect();

        // 按优先级排序（数字越小优先级越高）
        credentials.sort_by_key(|c| c.priority);

        CredentialsStatusResponse {
            total: snapshot.total,
            available: snapshot.available,
            current_id: snapshot.current_id,
            credentials,
        }
    }

    /// 设置凭据禁用状态
    pub fn set_disabled(&self, id: u64, disabled: bool) -> Result<(), AdminServiceError> {
        // 先获取当前凭据 ID，用于判断是否需要切换
        let snapshot = self.token_manager.snapshot();
        let current_id = snapshot.current_id;

        self.token_manager
            .set_disabled(id, disabled)
            .map_err(|e| self.classify_error(e, id))?;

        // 只有禁用的是当前凭据时才尝试切换到下一个
        if disabled && id == current_id {
            let _ = self.token_manager.switch_to_next();
        }
        Ok(())
    }

    /// 设置凭据优先级
    pub fn set_priority(&self, id: u64, priority: u32) -> Result<(), AdminServiceError> {
        self.token_manager
            .set_priority(id, priority)
            .map_err(|e| self.classify_error(e, id))
    }

    /// 重置失败计数并重新启用
    pub fn reset_and_enable(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .reset_and_enable(id)
            .map_err(|e| self.classify_error(e, id))
    }

    /// 获取凭据余额（带缓存）
    pub async fn get_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        // 先查缓存
        {
            let cache = self.balance_cache.lock();
            if let Some(cached) = cache.get(&id) {
                let now = Utc::now().timestamp() as f64;
                if (now - cached.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    tracing::debug!("凭据 #{} 余额命中缓存", id);
                    return Ok(cached.data.clone());
                }
            }
        }

        // 缓存未命中或已过期，从上游获取
        let balance = self.fetch_balance(id).await?;

        // 更新缓存
        {
            let mut cache = self.balance_cache.lock();
            cache.insert(
                id,
                CachedBalance {
                    cached_at: Utc::now().timestamp() as f64,
                    data: balance.clone(),
                },
            );
        }
        self.save_balance_cache();

        Ok(balance)
    }

    /// 从上游获取余额（无缓存）
    async fn fetch_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        let usage = self
            .token_manager
            .get_usage_limits_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;

        let current_usage = usage.current_usage();
        let usage_limit = usage.usage_limit();
        let remaining = (usage_limit - current_usage).max(0.0);
        let usage_percentage = if usage_limit > 0.0 {
            (current_usage / usage_limit * 100.0).min(100.0)
        } else {
            0.0
        };

        Ok(BalanceResponse {
            id,
            subscription_title: usage.subscription_title().map(|s| s.to_string()),
            current_usage,
            usage_limit,
            remaining,
            usage_percentage,
            next_reset_at: usage.next_date_reset,
        })
    }

    /// 添加新凭据
    pub async fn add_credential(
        &self,
        req: AddCredentialRequest,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        self.ingest_from_request(req).await
    }

    /// 导入入口：默认 onConflict=upsert（当请求未指定时）
    pub async fn import_credential(
        &self,
        mut req: AddCredentialRequest,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        if req.on_conflict.is_none() {
            req.on_conflict = Some("upsert".to_string());
        }
        self.ingest_from_request(req).await
    }

    async fn ingest_from_request(
        &self,
        req: AddCredentialRequest,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        if let Some(ref name) = req.endpoint {
            if !self.known_endpoints.contains(name) {
                let mut known: Vec<&str> =
                    self.known_endpoints.iter().map(|s| s.as_str()).collect();
                known.sort();
                return Err(AdminServiceError::InvalidCredential(format!(
                    "未知端点 \"{}\"，已注册端点: {:?}",
                    name, known
                )));
            }
        }

        let on_conflict =
            crate::kiro::token_manager::OnConflict::parse(req.on_conflict.as_deref());
        let opts = crate::kiro::token_manager::IngestOptions { on_conflict };

        let new_cred = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: req.refresh_token,
            profile_arn: req.profile_arn,
            expires_at: None,
            auth_method: Some(req.auth_method),
            provider: req.provider,
            client_id: req.client_id,
            client_secret: req.client_secret,
            priority: req.priority,
            region: req.region,
            auth_region: req.auth_region,
            api_region: req.api_region,
            machine_id: req.machine_id,
            email: req.email,
            user_id: req.user_id,
            nickname: req.nickname,
            start_url: req.start_url,
            subscription_title: None,
            proxy_url: req.proxy_url,
            proxy_username: req.proxy_username,
            proxy_password: req.proxy_password,
            disabled: false,
            kiro_api_key: req.kiro_api_key,
            endpoint: req.endpoint,
        };

        let result = self
            .token_manager
            .ingest_credential(new_cred, opts)
            .await
            .map_err(|e| self.classify_add_error(e))?;

        if let Err(e) = self.token_manager.get_usage_limits_for(result.id).await {
            tracing::warn!("添加凭据后获取订阅等级失败（不影响凭据添加）: {}", e);
        }

        Ok(AddCredentialResponse {
            success: true,
            message: format!(
                "凭据{}成功，ID: {}",
                match result.action {
                    crate::kiro::token_manager::IngestAction::Created => "添加",
                    crate::kiro::token_manager::IngestAction::Updated => "更新",
                },
                result.id
            ),
            credential_id: result.id,
            email: result.email,
            action: Some(result.action.as_str().to_string()),
            user_id: result.user_id,
        })
    }

    /// 批量导入凭据
    pub async fn import_credentials_batch(
        &self,
        req: crate::admin::types::BatchImportRequest,
    ) -> Result<crate::admin::types::BatchImportResponse, AdminServiceError> {
        use crate::admin::types::{
            BatchImportItemResult, BatchImportResponse, BatchImportSummary,
        };

        let opts = req.options.as_ref();
        let default_conflict = opts
            .and_then(|o| o.on_conflict.clone())
            .unwrap_or_else(|| "upsert".to_string());
        let stop_on_error = opts.and_then(|o| o.stop_on_error).unwrap_or(false);
        let fetch_balance = opts.and_then(|o| o.fetch_balance).unwrap_or(true);
        let concurrency = opts.and_then(|o| o.concurrency).unwrap_or(1).clamp(1, 4);
        // 串行以保证确定性并降低上游限流风险（concurrency 预留）
        let _ = concurrency;

        let mut results = Vec::with_capacity(req.items.len());
        let mut created = 0usize;
        let mut updated = 0usize;
        let mut duplicate = 0usize;
        let mut failed = 0usize;

        for (index, mut item) in req.items.into_iter().enumerate() {
            if item.on_conflict.is_none() {
                item.on_conflict = Some(default_conflict.clone());
            }

            match self.ingest_from_request(item).await {
                Ok(resp) => {
                    let status = resp.action.clone().unwrap_or_else(|| "created".to_string());
                    match status.as_str() {
                        "updated" => updated += 1,
                        _ => created += 1,
                    }

                    let mut balance = None;
                    if fetch_balance {
                        if let Ok(b) = self.get_balance(resp.credential_id).await {
                            balance = Some(b);
                        }
                    }

                    let mut warning = None;
                    let snapshot = self.token_manager.snapshot();
                    if let Some(entry) = snapshot.entries.iter().find(|e| e.id == resp.credential_id) {
                        if !entry.has_profile_arn && !entry.auth_method.as_deref().map(|m| m.eq_ignore_ascii_case("api_key")).unwrap_or(false) {
                            warning = Some(
                                "余额可用，但 profileArn 未解析；对话可能仍 403".to_string(),
                            );
                            // 非致命：状态仍为 created/updated，警告字段区分（UI 可标 verified_warn）
                        }
                    }

                    results.push(BatchImportItemResult {
                        index,
                        status,
                        credential_id: Some(resp.credential_id),
                        email: resp.email,
                        user_id: resp.user_id,
                        error: None,
                        balance,
                        warning,
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    let is_dup = msg.contains("凭据已存在") || msg.contains("重复");
                    if is_dup {
                        duplicate += 1;
                        results.push(BatchImportItemResult {
                            index,
                            status: "duplicate".to_string(),
                            credential_id: None,
                            email: None,
                            user_id: None,
                            error: Some(msg),
                            balance: None,
                            warning: None,
                        });
                    } else {
                        failed += 1;
                        results.push(BatchImportItemResult {
                            index,
                            status: "failed".to_string(),
                            credential_id: None,
                            email: None,
                            user_id: None,
                            error: Some(msg),
                            balance: None,
                            warning: None,
                        });
                    }
                    if stop_on_error {
                        break;
                    }
                }
            }
        }

        Ok(BatchImportResponse {
            success: failed == 0,
            summary: BatchImportSummary {
                created,
                updated,
                duplicate,
                failed,
            },
            results,
        })
    }


    /// 删除凭据
    pub fn delete_credential(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .delete_credential(id)
            .map_err(|e| self.classify_delete_error(e, id))?;

        // 清理已删除凭据的余额缓存
        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();

        Ok(())
    }

    /// 获取负载均衡模式
    pub fn get_load_balancing_mode(&self) -> LoadBalancingModeResponse {
        LoadBalancingModeResponse {
            mode: self.token_manager.get_load_balancing_mode(),
        }
    }

    /// 设置负载均衡模式
    pub fn set_load_balancing_mode(
        &self,
        req: SetLoadBalancingModeRequest,
    ) -> Result<LoadBalancingModeResponse, AdminServiceError> {
        // 验证模式值
        if req.mode != "priority" && req.mode != "balanced" {
            return Err(AdminServiceError::InvalidCredential(
                "mode 必须是 'priority' 或 'balanced'".to_string(),
            ));
        }

        self.token_manager
            .set_load_balancing_mode(req.mode.clone())
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        Ok(LoadBalancingModeResponse { mode: req.mode })
    }

    /// 强制刷新指定凭据的 Token
    pub async fn force_refresh_token(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .force_refresh_token_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))
    }

    // ============ 余额缓存持久化 ============

    fn load_balance_cache_from(cache_path: &Option<PathBuf>) -> HashMap<u64, CachedBalance> {
        let path = match cache_path {
            Some(p) => p,
            None => return HashMap::new(),
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        // 文件中使用字符串 key 以兼容 JSON 格式
        let map: HashMap<String, CachedBalance> = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("解析余额缓存失败，将忽略: {}", e);
                return HashMap::new();
            }
        };

        let now = Utc::now().timestamp() as f64;
        map.into_iter()
            .filter_map(|(k, v)| {
                let id = k.parse::<u64>().ok()?;
                // 丢弃超过 TTL 的条目
                if (now - v.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    Some((id, v))
                } else {
                    None
                }
            })
            .collect()
    }

    fn save_balance_cache(&self) {
        let path = match &self.cache_path {
            Some(p) => p,
            None => return,
        };

        // 持有锁期间完成序列化和写入，防止并发损坏
        let cache = self.balance_cache.lock();
        let map: HashMap<String, &CachedBalance> =
            cache.iter().map(|(k, v)| (k.to_string(), v)).collect();

        match serde_json::to_string_pretty(&map) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    tracing::warn!("保存余额缓存失败: {}", e);
                }
            }
            Err(e) => tracing::warn!("序列化余额缓存失败: {}", e),
        }
    }

    // ============ 错误分类 ============

    /// 分类简单操作错误（set_disabled, set_priority, reset_and_enable）
    fn classify_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类余额查询错误（可能涉及上游 API 调用）
    fn classify_balance_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();

        // 1. 凭据不存在
        if msg.contains("不存在") {
            return AdminServiceError::NotFound { id };
        }

        // 2. API Key 凭据不支持刷新：客户端请求错误，映射为 400
        if msg.contains("API Key 凭据不支持刷新") {
            return AdminServiceError::InvalidCredential(msg);
        }

        // 3. 上游服务错误特征：HTTP 响应错误或网络错误
        let is_upstream_error =
            // HTTP 响应错误（来自 refresh_*_token 的错误消息）
            msg.contains("凭证已过期或无效") ||
            msg.contains("权限不足") ||
            msg.contains("已被限流") ||
            msg.contains("服务器错误") ||
            msg.contains("Token 刷新失败") ||
            msg.contains("暂时不可用") ||
            // 网络错误（reqwest 错误）
            msg.contains("error trying to connect") ||
            msg.contains("connection") ||
            msg.contains("timeout") ||
            msg.contains("timed out");

        if is_upstream_error {
            AdminServiceError::UpstreamError(msg)
        } else {
            // 4. 默认归类为内部错误（本地验证失败、配置错误等）
            // 包括：缺少 refreshToken、refreshToken 已被截断、无法生成 machineId 等
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类添加凭据错误

    /// 将在线授权 token 走统一 ingest
    async fn ingest_online_tokens(
        &self,
        tokens: crate::kiro::online_auth::CompletedTokens,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        use chrono::{Duration as ChronoDuration, Utc};
        let expires_at = (Utc::now() + ChronoDuration::seconds(tokens.expires_in as i64)).to_rfc3339();
        let mut req = AddCredentialRequest {
            refresh_token: Some(tokens.refresh_token),
            auth_method: tokens.auth_method,
            provider: Some(tokens.provider),
            profile_arn: None,
            client_id: Some(tokens.client_id),
            client_secret: Some(tokens.client_secret),
            priority: 0,
            region: Some(tokens.region.clone()),
            auth_region: Some(tokens.region),
            api_region: None,
            machine_id: None,
            email: None,
            user_id: None,
            nickname: None,
            start_url: tokens.start_url,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            kiro_api_key: None,
            endpoint: None,
            on_conflict: Some("upsert".into()),
        };
        // Prefer import path defaults
        if req.on_conflict.is_none() {
            req.on_conflict = Some("upsert".into());
        }
        let resp = self.ingest_from_request(req).await?;
        // access_token already applied via refresh inside ingest for OAuth
        let _ = expires_at;
        Ok(resp)
    }

    pub async fn start_builder_id_login(
        &self,
        region: Option<String>,
    ) -> Result<crate::kiro::online_auth::BuilderIdStartResponse, AdminServiceError> {
        let proxy = self.token_manager.global_proxy();
        crate::kiro::online_auth::start_builder_id(region, proxy, self.token_manager.config())
            .await
            .map_err(|e| AdminServiceError::UpstreamError(e.to_string()))
    }

    pub async fn poll_builder_id_login(
        &self,
        session_id: String,
    ) -> Result<serde_json::Value, AdminServiceError> {
        let proxy = self.token_manager.global_proxy();
        let result = crate::kiro::online_auth::poll_builder_id(
            &session_id,
            proxy,
            self.token_manager.config(),
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("expired") {
                AdminServiceError::InvalidCredential(msg)
            } else {
                AdminServiceError::UpstreamError(msg)
            }
        })?;

        match result {
            Err(pending) => Ok(serde_json::to_value(pending).unwrap()),
            Ok(tokens) => {
                let resp = self.ingest_online_tokens(tokens).await?;
                Ok(serde_json::json!({
                    "success": true,
                    "completed": true,
                    "credentialId": resp.credential_id,
                    "email": resp.email,
                    "userId": resp.user_id,
                    "action": resp.action,
                }))
            }
        }
    }

    pub async fn start_iam_sso_login(
        &self,
        start_url: String,
        region: Option<String>,
    ) -> Result<crate::kiro::online_auth::IamSsoStartResponse, AdminServiceError> {
        if start_url.trim().is_empty() {
            return Err(AdminServiceError::InvalidCredential(
                "startUrl is required".into(),
            ));
        }
        let proxy = self.token_manager.global_proxy();
        crate::kiro::online_auth::start_iam_sso(
            &start_url,
            region,
            proxy,
            self.token_manager.config(),
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("startUrl") {
                AdminServiceError::InvalidCredential(msg)
            } else {
                AdminServiceError::UpstreamError(msg)
            }
        })
    }

    pub async fn complete_iam_sso_login(
        &self,
        session_id: String,
        callback_url: String,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        let proxy = self.token_manager.global_proxy();
        let tokens = crate::kiro::online_auth::complete_iam_sso(
            &session_id,
            &callback_url,
            proxy,
            self.token_manager.config(),
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("expired") {
                AdminServiceError::InvalidCredential(msg)
            } else if msg.contains("startUrl") || msg.contains("无效") || msg.contains("状态") {
                AdminServiceError::InvalidCredential(msg)
            } else {
                AdminServiceError::UpstreamError(msg)
            }
        })?;
        self.ingest_online_tokens(tokens).await
    }

    pub async fn import_sso_tokens(
        &self,
        bearer_token: String,
        region: Option<String>,
    ) -> Result<crate::admin::types::SsoTokenImportResponse, AdminServiceError> {
        use crate::admin::types::{SsoTokenAccountResult, SsoTokenImportResponse};

        let lines: Vec<&str> = bearer_token
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            return Err(AdminServiceError::InvalidCredential(
                "bearerToken is required".into(),
            ));
        }

        let proxy = self.token_manager.global_proxy();
        let mut accounts = Vec::new();
        let mut errors = Vec::new();

        for line in lines {
            match crate::kiro::online_auth::import_sso_token(
                line,
                region.clone(),
                proxy,
                self.token_manager.config(),
            )
            .await
            {
                Ok(tokens) => match self.ingest_online_tokens(tokens).await {
                    Ok(resp) => accounts.push(SsoTokenAccountResult {
                        credential_id: resp.credential_id,
                        email: resp.email,
                        user_id: resp.user_id,
                    }),
                    Err(e) => errors.push(e.to_string()),
                },
                Err(e) => errors.push(e.to_string()),
            }
        }

        if accounts.is_empty() {
            return Err(AdminServiceError::UpstreamError(
                if errors.is_empty() {
                    "SSO token import failed".into()
                } else {
                    errors.join("; ")
                },
            ));
        }

        Ok(SsoTokenImportResponse {
            success: true,
            accounts,
            errors,
        })
    }

        fn classify_add_error(&self, e: anyhow::Error) -> AdminServiceError {
        let msg = e.to_string();

        // 凭据验证失败（refreshToken 无效、格式错误等）
        let is_invalid_credential = msg.contains("缺少 refreshToken")
            || msg.contains("refreshToken 为空")
            || msg.contains("refreshToken 已被截断")
            || msg.contains("凭据已存在")
            || msg.contains("refreshToken 重复")
            || msg.contains("kiroApiKey 重复")
            || msg.contains("缺少 kiroApiKey")
            || msg.contains("kiroApiKey 为空")
            || msg.contains("凭证已过期或无效")
            || msg.contains("权限不足")
            || msg.contains("已被限流");

        if is_invalid_credential {
            AdminServiceError::InvalidCredential(msg)
        } else if msg.contains("error trying to connect")
            || msg.contains("connection")
            || msg.contains("timeout")
        {
            AdminServiceError::UpstreamError(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类删除凭据错误
    fn classify_delete_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else if msg.contains("只能删除已禁用的凭据") || msg.contains("请先禁用凭据") {
            AdminServiceError::InvalidCredential(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }
}
