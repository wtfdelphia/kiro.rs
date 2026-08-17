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
    AddCredentialRequest, AddCredentialResponse, BalanceResponse, ClientIdentitySettingsResponse,
    CredentialModelsResponse, CredentialStatusItem, CredentialsStatusResponse,
    GlobalModelsCatalogResponse, LoadBalancingModeResponse, ModelCatalogItem,
    ModelsRefreshAllResponse, ModelsRefreshErrorItem, ModelsRefreshResponse,
    SetLoadBalancingModeRequest, TestCredentialRequest, TestCredentialResponse,
    UpdateClientIdentitySettingsRequest,
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
    /// 客户端鉴权热更新句柄
    client_auth: Option<Arc<parking_lot::RwLock<crate::anthropic::AuthRuntime>>>,
    /// Provider 热更新（proxy/defaultEndpoint）
    provider: Option<Arc<crate::kiro::provider::KiroProvider>>,
    /// WebSocket ingress 设置热更新句柄
    ws_settings: Option<Arc<parking_lot::RwLock<crate::model::config::WsSettings>>>,
    /// WS 准入计数器（GET 展示活跃连接数）
    ws_admission: Option<Arc<crate::openai::ws_transport::WsAdmission>>,
}

impl AdminService {
    #[cfg(test)]
    pub fn new(
        token_manager: Arc<MultiTokenManager>,
        known_endpoints: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new_with_runtime(token_manager, known_endpoints, None, None)
    }

    pub fn new_with_runtime(
        token_manager: Arc<MultiTokenManager>,
        known_endpoints: impl IntoIterator<Item = String>,
        client_auth: Option<Arc<parking_lot::RwLock<crate::anthropic::AuthRuntime>>>,
        provider: Option<Arc<crate::kiro::provider::KiroProvider>>,
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
            client_auth,
            provider,
            ws_settings: None,
            ws_admission: None,
        }
    }

    /// 挂接 WebSocket ingress 运行时句柄（设置热更新 + 准入计数）
    pub fn with_ws_runtime(
        mut self,
        ws_settings: Arc<parking_lot::RwLock<crate::model::config::WsSettings>>,
        ws_admission: Arc<crate::openai::ws_transport::WsAdmission>,
    ) -> Self {
        self.ws_settings = Some(ws_settings);
        self.ws_admission = Some(ws_admission);
        self
    }

    /// 获取所有凭据状态
    pub fn get_all_credentials(&self) -> CredentialsStatusResponse {
        let snapshot = self.token_manager.snapshot();
        let default_endpoint = self.token_manager.config().default_endpoint.clone();

        let mut credentials: Vec<CredentialStatusItem> = snapshot
            .entries
            .into_iter()
            .map(|entry| {
                let model_meta = self
                    .token_manager
                    .get_credential_models_cached(entry.id)
                    .ok();
                CredentialStatusItem {
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
                    model_count: model_meta
                        .as_ref()
                        .map(|m| m.models.len() as u32)
                        .unwrap_or(0),
                    models_updated_at: model_meta.as_ref().and_then(|m| m.updated_at.clone()),
                    models_last_error: model_meta.as_ref().and_then(|m| m.last_error.clone()),
                }
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

    /// 构建模型解析元数据（raw id 列表 + catalog 集合）
    fn annotate_model_ids(&self, models: &[String]) -> Vec<ModelCatalogItem> {
        use crate::anthropic::resolve_model;
        use crate::kiro::model::available_models::model_id_set;

        let policy = self.token_manager.config().model_resolution.clone();
        let mut catalog_models = self.token_manager.global_model_catalog();
        catalog_models.extend(models.iter().cloned().map(|id| {
            crate::kiro::model::available_models::UpstreamModelInfo {
                model_id: id,
                model_name: None,
                description: None,
                input_types: vec![],
                rate_multiplier: None,
                token_limits: None,
            }
        }));
        let catalog_set = model_id_set(&catalog_models);
        let catalog_ref = if catalog_set.is_empty() {
            None
        } else {
            Some(&catalog_set)
        };

        models
            .iter()
            .map(|id| match resolve_model(id, &policy, catalog_ref) {
                Ok(r) => ModelCatalogItem {
                    id: id.clone(),
                    resolvable: true,
                    resolve_to: Some(r.model_id),
                    resolve_kind: Some(r.kind.as_str().to_string()),
                    testable: true,
                },
                Err(_) => ModelCatalogItem {
                    id: id.clone(),
                    resolvable: false,
                    resolve_to: None,
                    resolve_kind: None,
                    testable: false,
                },
            })
            .collect()
    }

    /// 全局模型 catalog 摘要
    pub fn get_global_models_catalog(&self) -> GlobalModelsCatalogResponse {
        let catalog = self.token_manager.global_model_catalog();
        let mut models: Vec<String> = catalog.iter().map(|m| m.model_id.clone()).collect();
        models.sort();
        let model_items = self.annotate_model_ids(&models);
        // 取各凭据缓存 updated_at 的最新值（若有）
        let snapshot = self.token_manager.snapshot();
        let mut updated_at: Option<String> = None;
        for e in &snapshot.entries {
            if let Ok(meta) = self.token_manager.get_credential_models_cached(e.id) {
                if let Some(ts) = meta.updated_at {
                    updated_at = match updated_at {
                        Some(cur) if cur >= ts => Some(cur),
                        _ => Some(ts),
                    };
                }
            }
        }
        GlobalModelsCatalogResponse {
            success: true,
            count: models.len(),
            models,
            model_items,
            updated_at,
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
        // 重新启用时异步刷新模型
        if !disabled {
            self.token_manager.spawn_refresh_models_arc(id);
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
            .map_err(|e| self.classify_error(e, id))?;
        // 启用后异步刷新模型缓存
        self.token_manager.spawn_refresh_models_arc(id);
        Ok(())
    }

    /// 获取凭据余额（带缓存；force=true 跳过 TTL）
    pub async fn get_balance(
        &self,
        id: u64,
        force: bool,
    ) -> Result<BalanceResponse, AdminServiceError> {
        // 先查缓存（force 时跳过）
        if !force {
            let cache = self.balance_cache.lock();
            if let Some(cached) = cache.get(&id) {
                let now = Utc::now().timestamp() as f64;
                if (now - cached.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    tracing::debug!("凭据 #{} 余额命中缓存", id);
                    return Ok(cached.data.clone());
                }
            }
        } else {
            tracing::debug!("凭据 #{} 余额 force 刷新，跳过缓存", id);
        }

        // 缓存未命中、已过期或 force，从上游获取
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

        // 按认证族校验必需字段与 endpoint 合法性。
        // external_idp 的 endpoint 校验必须在此完成：不合法则不得发起任何出站请求。
        req.validate_shape()
            .map_err(AdminServiceError::InvalidCredential)?;

        let on_conflict = crate::kiro::token_manager::OnConflict::parse(req.on_conflict.as_deref());
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
            token_endpoint: req.token_endpoint,
            issuer_url: req.issuer_url,
            scopes: req.scopes,
        };

        let result = self
            .token_manager
            .ingest_credential(new_cred, opts)
            .await
            .map_err(|e| self.classify_add_error(e))?;

        if let Err(e) = self.token_manager.get_usage_limits_for(result.id).await {
            tracing::warn!("添加凭据后获取订阅等级失败（不影响凭据添加）: {}", e);
        }

        // 异步刷新模型缓存（失败不影响添加）
        self.token_manager.spawn_refresh_models_arc(result.id);

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
        use crate::admin::types::{BatchImportItemResult, BatchImportResponse, BatchImportSummary};

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
                        if let Ok(b) = self.get_balance(resp.credential_id, false).await {
                            balance = Some(b);
                        }
                    }

                    let mut warning = None;
                    let snapshot = self.token_manager.snapshot();
                    if let Some(entry) =
                        snapshot.entries.iter().find(|e| e.id == resp.credential_id)
                    {
                        if !entry.has_profile_arn
                            && !entry
                                .auth_method
                                .as_deref()
                                .map(|m| m.eq_ignore_ascii_case("api_key"))
                                .unwrap_or(false)
                        {
                            warning =
                                Some("余额可用，但 profileArn 未解析；对话可能仍 403".to_string());
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

    /// 导入 KAM 导出文件
    ///
    /// 容器判别与认证分类在服务端完成，与启动加载共用同一个 adapter，
    /// 使同一份文件在两个入口得到等价结果。
    pub async fn import_kam_document(
        &self,
        req: crate::admin::types::KamImportRequest,
    ) -> Result<crate::admin::types::KamImportResponse, AdminServiceError> {
        use crate::admin::types::{
            AddCredentialRequest, KamImportResponse, KamPreviewItem,
        };
        use crate::kiro::kam_adapter;

        let adapted = kam_adapter::adapt(&req.document)
            .map_err(|e| AdminServiceError::InvalidCredential(e.to_string()))?;

        let container = format!("{:?}", adapted.shape);

        // 逐条预检：不回传任何密钥材料，只回传「是否配置」状态
        let mut preview = Vec::with_capacity(adapted.records.len());
        let mut importable: Vec<(usize, AddCredentialRequest)> = Vec::new();

        for (index, record) in adapted.records.iter().enumerate() {
            match record {
                Ok(cred) => {
                    preview.push(KamPreviewItem {
                        index,
                        path: format!("$[{index}]"),
                        auth_method: cred.auth_method.clone(),
                        provider: cred.provider.clone(),
                        email: cred.email.clone(),
                        nickname: cred.nickname.clone(),
                        has_refresh_token: cred.refresh_token.is_some(),
                        has_client_id: cred.client_id.is_some(),
                        has_client_secret: cred.client_secret.is_some(),
                        has_token_endpoint: cred.token_endpoint.is_some(),
                        has_issuer_url: cred.issuer_url.is_some(),
                        has_scopes: cred.scopes.is_some(),
                        has_profile_arn: cred.profile_arn.is_some(),
                        disabled: cred.disabled,
                        valid: true,
                        error: None,
                    });

                    importable.push((
                        index,
                        AddCredentialRequest {
                            refresh_token: cred.refresh_token.clone(),
                            auth_method: cred
                                .auth_method
                                .clone()
                                .unwrap_or_else(|| "social".to_string()),
                            provider: cred.provider.clone(),
                            profile_arn: cred.profile_arn.clone(),
                            client_id: cred.client_id.clone(),
                            client_secret: cred.client_secret.clone(),
                            priority: cred.priority,
                            region: cred.region.clone(),
                            auth_region: cred.auth_region.clone(),
                            api_region: cred.api_region.clone(),
                            machine_id: cred.machine_id.clone(),
                            email: cred.email.clone(),
                            user_id: cred.user_id.clone(),
                            nickname: cred.nickname.clone(),
                            start_url: cred.start_url.clone(),
                            on_conflict: None,
                            proxy_url: cred.proxy_url.clone(),
                            proxy_username: cred.proxy_username.clone(),
                            proxy_password: cred.proxy_password.clone(),
                            kiro_api_key: cred.kiro_api_key.clone(),
                            endpoint: cred.endpoint.clone(),
                            token_endpoint: cred.token_endpoint.clone(),
                            issuer_url: cred.issuer_url.clone(),
                            scopes: cred.scopes.clone(),
                        },
                    ));
                }
                Err(rejected) => {
                    preview.push(KamPreviewItem {
                        index,
                        path: rejected.path.clone(),
                        auth_method: None,
                        provider: None,
                        email: None,
                        nickname: None,
                        has_refresh_token: false,
                        has_client_id: false,
                        has_client_secret: false,
                        has_token_endpoint: false,
                        has_issuer_url: false,
                        has_scopes: false,
                        has_profile_arn: false,
                        disabled: false,
                        valid: false,
                        error: Some(rejected.reason.clone()),
                    });
                }
            }
        }

        if req.dry_run {
            return Ok(KamImportResponse {
                success: !adapted.has_failures(),
                container,
                preview,
                summary: None,
                results: Vec::new(),
            });
        }

        // 走既有 batch 管道入库，保持冲突策略与逐条结果语义一致
        let batch = self
            .import_credentials_batch(crate::admin::types::BatchImportRequest {
                items: importable.iter().map(|(_, r)| r.clone()).collect(),
                options: req.options,
            })
            .await?;

        // 把 batch 的相对 index 映射回源文件 index
        let mut results = batch.results;
        for item in results.iter_mut() {
            if let Some((src_index, _)) = importable.get(item.index) {
                item.index = *src_index;
            }
        }

        let adapter_failed = adapted.records.iter().filter(|r| r.is_err()).count();
        let mut summary = batch.summary;
        summary.failed += adapter_failed;

        Ok(KamImportResponse {
            success: summary.failed == 0,
            container,
            preview,
            summary: Some(summary),
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

    /// 刷新单凭据模型缓存
    pub async fn refresh_models(
        &self,
        id: u64,
    ) -> Result<ModelsRefreshResponse, AdminServiceError> {
        let result = self
            .token_manager
            .refresh_models_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;
        Ok(ModelsRefreshResponse {
            success: true,
            credential_id: result.credential_id,
            count: result.count,
            models: result.models,
            updated_at: result.updated_at,
        })
    }

    /// 刷新全部启用凭据模型缓存
    pub async fn refresh_models_all(&self) -> ModelsRefreshAllResponse {
        let result = self.token_manager.refresh_models_all().await;
        ModelsRefreshAllResponse {
            success: true,
            refreshed: result.refreshed,
            failed: result.failed,
            global_count: result.global_count,
            errors: result
                .errors
                .into_iter()
                .map(|(credential_id, error)| ModelsRefreshErrorItem {
                    credential_id,
                    error,
                })
                .collect(),
        }
    }

    /// 查看凭据模型（缓存；live=true 时先刷新）
    pub async fn get_credential_models(
        &self,
        id: u64,
        live: bool,
    ) -> Result<CredentialModelsResponse, AdminServiceError> {
        if live {
            let _ = self.refresh_models(id).await?;
        }
        let snap = self
            .token_manager
            .get_credential_models_cached(id)
            .map_err(|e| self.classify_error(e, id))?;
        let model_items = self.annotate_model_ids(&snap.models);
        Ok(CredentialModelsResponse {
            success: true,
            models: snap.models,
            model_items,
            updated_at: snap.updated_at,
            last_error: snap.last_error,
        })
    }

    /// 对指定凭据做最小真实推理探测
    pub async fn test_credential(
        &self,
        id: u64,
        req: TestCredentialRequest,
    ) -> Result<TestCredentialResponse, AdminServiceError> {
        use crate::anthropic::resolve_model;
        use crate::kiro::model::available_models::model_id_set;
        use crate::kiro::model::requests::conversation::{
            ConversationState, CurrentMessage, UserInputMessage,
        };
        use crate::kiro::model::requests::kiro::KiroRequest;
        use std::time::Instant;

        let requested = req
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("claude-sonnet-4.6");

        let policy = self.token_manager.config().model_resolution.clone();
        // 优先凭据缓存，其次全局 catalog
        let mut catalog_models = Vec::new();
        if let Ok(snap) = self.token_manager.get_credential_models_cached(id) {
            catalog_models.extend(snap.models.into_iter().map(|id| {
                crate::kiro::model::available_models::UpstreamModelInfo {
                    model_id: id,
                    model_name: None,
                    description: None,
                    input_types: vec![],
                    rate_multiplier: None,
                    token_limits: None,
                }
            }));
        }
        let global = self.token_manager.global_model_catalog();
        catalog_models.extend(global);
        let catalog_set = model_id_set(&catalog_models);
        let catalog_ref = if catalog_set.is_empty() {
            None
        } else {
            Some(&catalog_set)
        };

        let resolved = resolve_model(requested, &policy, catalog_ref)
            .map_err(|e| AdminServiceError::ModelUnmapped(e.message()))?;

        let state = ConversationState::new(uuid::Uuid::new_v4().to_string())
            .with_agent_task_type("vibe")
            .with_chat_trigger_type("MANUAL")
            .with_current_message(CurrentMessage::new(UserInputMessage::new(
                "say ok",
                resolved.model_id.clone(),
            )));
        let kiro_req = KiroRequest {
            conversation_state: state,
            profile_arn: None,
        };
        let body = serde_json::to_string(&kiro_req)
            .map_err(|e| AdminServiceError::InternalError(format!("序列化测试请求失败: {}", e)))?;

        let started = Instant::now();
        let reply = self
            .run_minimal_generate(id, &body)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;
        let latency_ms = started.elapsed().as_millis() as u64;

        Ok(TestCredentialResponse {
            success: true,
            model: requested.to_string(),
            resolved_model: Some(resolved.model_id),
            resolve_kind: Some(resolved.kind.as_str().to_string()),
            reply: Some(reply),
            latency_ms,
        })
    }

    /// 使用指定凭据发送最小 generate（非流式解析文本）
    async fn run_minimal_generate(&self, id: u64, request_body: &str) -> anyhow::Result<String> {
        use crate::http_client::build_client;
        use crate::kiro::endpoint::ide::IdeEndpoint;
        use crate::kiro::endpoint::{KiroEndpoint, RequestContext};
        use crate::kiro::machine_id;
        use crate::kiro::model::events::Event;
        use crate::kiro::parser::decoder::EventStreamDecoder;
        use futures::StreamExt;

        let token = self.token_manager.ensure_access_token(id).await?;
        let mut credentials = self.token_manager.credentials_clone(id)?;

        if let Err(e) = crate::kiro::profile::ensure_profile_arn_for_request(
            &self.token_manager,
            id,
            &mut credentials,
            &token,
        )
        .await
        {
            tracing::warn!("test 前 profileArn 失败: {}", e);
        }

        let config = self.token_manager.config();
        let endpoint = IdeEndpoint::default();
        let proxy = credentials.effective_proxy(self.token_manager.global_proxy().as_ref());
        let client = build_client(proxy.as_ref(), 60, config.tls_backend)?;

        for attempt in 0..2 {
            let machine = machine_id::generate_from_credentials(&credentials, &config);
            let rctx = RequestContext {
                credentials: &credentials,
                token: &token,
                machine_id: &machine,
                config: &config,
            };
            let url = endpoint.api_url(&rctx);
            let body = endpoint.transform_api_body(request_body, &rctx);
            let base = client
                .post(&url)
                .body(body)
                .header("content-type", "application/json")
                .header("Connection", "close");
            let request = endpoint.decorate_api(base, &rctx);
            let response = request.send().await?;
            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                let has_profile = credentials
                    .profile_arn
                    .as_ref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if attempt == 0
                    && has_profile
                    && crate::kiro::models_api::is_user_not_authorized_body(&text)
                {
                    tracing::warn!(
                        "test 凭据 #{} generate 因 profileArn 未授权，清除后重试",
                        id
                    );
                    let _ = self.token_manager.clear_profile_arn(id);
                    credentials.profile_arn = None;
                    continue;
                }
                let summary: String = text.chars().take(400).collect();
                anyhow::bail!("上游 generate 失败: {} {}", status.as_u16(), summary);
            }

            let mut decoder = EventStreamDecoder::new();
            let mut stream = response.bytes_stream();
            let mut content = String::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                decoder.feed(&chunk)?;
                for result in decoder.decode_iter() {
                    let frame = match result {
                        Ok(f) => f,
                        Err(_) => continue,
                    };
                    if let Ok(Event::AssistantResponse(ev)) = Event::from_frame(frame) {
                        content.push_str(&ev.content);
                    }
                }
            }
            return Ok(content);
        }
        anyhow::bail!("上游 generate 失败: 重试耗尽")
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
        let expires_at =
            (Utc::now() + ChronoDuration::seconds(tokens.expires_in as i64)).to_rfc3339();
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
            token_endpoint: None,
            issuer_url: None,
            scopes: None,
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

    // ============ Runtime settings ============

    pub fn get_proxy_settings(&self) -> crate::admin::types::ProxySettingsResponse {
        let cfg = self.token_manager.config();
        crate::admin::types::ProxySettingsResponse {
            proxy_url: cfg.proxy_url.clone(),
            has_proxy_auth: cfg.proxy_username.is_some() && cfg.proxy_password.is_some(),
            proxy_username: cfg.proxy_username.clone(),
        }
    }

    pub fn update_proxy_settings(
        &self,
        req: crate::admin::types::UpdateProxySettingsRequest,
    ) -> Result<super::types::SuccessResponse, AdminServiceError> {
        use crate::http_client::ProxyConfig;

        let url = req
            .proxy_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let _proxy_validated = if let Some(u) = url {
            // 基础校验：支持 http/https/socks5
            let lower = u.to_lowercase();
            if !(lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("socks5://")
                || lower.starts_with("socks5h://"))
            {
                return Err(AdminServiceError::InvalidCredential(
                    "proxyUrl 必须是 http/https/socks5 URL".into(),
                ));
            }
            // 尝试用 reqwest 解析
            if let Err(e) = reqwest::Proxy::all(u) {
                return Err(AdminServiceError::InvalidCredential(format!(
                    "proxyUrl 无效: {}",
                    e
                )));
            }
            let mut p = ProxyConfig::new(u);
            if let (Some(user), Some(pass)) = (
                req.proxy_username
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
                req.proxy_password.as_deref().filter(|s| !s.is_empty()),
            ) {
                p = p.with_auth(user, pass);
            } else if let (Some(user), Some(pass)) = (
                // 保留旧密码：仅更新 URL/用户名时
                None::<&str>,
                None::<&str>,
            ) {
                let _ = (user, pass);
            }
            Some(p)
        } else {
            None
        };

        // 更新内存 config
        let username = req.proxy_username.clone().filter(|s| !s.trim().is_empty());
        let password = req.proxy_password.clone().filter(|s| !s.is_empty());
        let proxy_url = url.map(|s| s.to_string());

        self.token_manager
            .update_config_with(|cfg| {
                cfg.proxy_url = proxy_url.clone();
                if proxy_url.is_none() {
                    cfg.proxy_username = None;
                    cfg.proxy_password = None;
                } else {
                    if username.is_some() || password.is_some() {
                        if let Some(u) = username.clone() {
                            cfg.proxy_username = Some(u);
                        }
                        if let Some(p) = password.clone() {
                            cfg.proxy_password = Some(p);
                        }
                    }
                }
            })
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        // rebuild proxy from final config for runtime
        let cfg = self.token_manager.config();
        let runtime_proxy = cfg.proxy_url.as_ref().map(|u| {
            let mut p = ProxyConfig::new(u);
            if let (Some(user), Some(pass)) = (&cfg.proxy_username, &cfg.proxy_password) {
                p = p.with_auth(user, pass);
            }
            p
        });
        self.token_manager.set_global_proxy(runtime_proxy.clone());
        if let Some(provider) = &self.provider {
            provider.set_global_proxy(runtime_proxy);
        }

        self.token_manager
            .save_config()
            .map_err(|e| AdminServiceError::InternalError(format!("配置落盘失败: {}", e)))?;

        Ok(super::types::SuccessResponse::new("代理设置已更新"))
    }

    pub fn get_endpoint_settings(&self) -> crate::admin::types::EndpointSettingsResponse {
        let cfg = self.token_manager.config();
        let mut registered: Vec<String> = self.known_endpoints.iter().cloned().collect();
        registered.sort();
        if let Some(provider) = &self.provider {
            registered = provider.registered_endpoints();
        }
        crate::admin::types::EndpointSettingsResponse {
            default_endpoint: cfg.default_endpoint.clone(),
            registered_endpoints: registered,
        }
    }

    pub fn update_endpoint_settings(
        &self,
        req: crate::admin::types::UpdateEndpointSettingsRequest,
    ) -> Result<super::types::SuccessResponse, AdminServiceError> {
        let name = req.default_endpoint.trim().to_string();
        if name.is_empty() {
            return Err(AdminServiceError::InvalidCredential(
                "defaultEndpoint 不能为空".into(),
            ));
        }
        if !self.known_endpoints.contains(&name) {
            return Err(AdminServiceError::InvalidCredential(format!(
                "未知端点: {}（已注册: {:?}）",
                name, self.known_endpoints
            )));
        }
        if let Some(provider) = &self.provider {
            provider
                .set_default_endpoint(name.clone())
                .map_err(|e| AdminServiceError::InvalidCredential(e.to_string()))?;
        }
        self.token_manager
            .update_config_with(|cfg| {
                cfg.default_endpoint = name.clone();
            })
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;
        self.token_manager
            .save_config()
            .map_err(|e| AdminServiceError::InternalError(format!("配置落盘失败: {}", e)))?;
        Ok(super::types::SuccessResponse::new(format!(
            "默认端点已设置为 {}",
            name
        )))
    }

    fn mask_api_key(key: &str) -> Option<String> {
        let k = key.trim();
        if k.is_empty() {
            return None;
        }
        if k.len() <= 8 {
            return Some(format!("{}***", &k[..k.len().min(2)]));
        }
        Some(format!("{}***{}", &k[..4], &k[k.len() - 4..]))
    }

    pub fn get_auth_settings(&self) -> crate::admin::types::AuthSettingsResponse {
        if let Some(auth) = &self.client_auth {
            let a = auth.read();
            return crate::admin::types::AuthSettingsResponse {
                require_api_key: a.require_api_key,
                has_api_key: !a.api_key.trim().is_empty(),
                api_key_mask: Self::mask_api_key(&a.api_key),
            };
        }
        let cfg = self.token_manager.config();
        crate::admin::types::AuthSettingsResponse {
            require_api_key: cfg.require_api_key,
            has_api_key: cfg
                .api_key
                .as_ref()
                .map(|k| !k.trim().is_empty())
                .unwrap_or(false),
            api_key_mask: cfg.api_key.as_deref().and_then(Self::mask_api_key),
        }
    }

    pub fn update_auth_settings(
        &self,
        req: crate::admin::types::UpdateAuthSettingsRequest,
    ) -> Result<super::types::SuccessResponse, AdminServiceError> {
        let current = self.get_auth_settings();
        let require = req.require_api_key.unwrap_or(current.require_api_key);
        let new_key = req
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        // 更新 config
        self.token_manager
            .update_config_with(|cfg| {
                cfg.require_api_key = require;
                if let Some(k) = new_key {
                    cfg.api_key = Some(k.to_string());
                }
            })
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        // 热更新内存鉴权
        if let Some(auth) = &self.client_auth {
            let mut a = auth.write();
            a.require_api_key = require;
            if let Some(k) = new_key {
                a.api_key = k.to_string();
            }
        }

        self.token_manager
            .save_config()
            .map_err(|e| AdminServiceError::InternalError(format!("配置落盘失败: {}", e)))?;

        Ok(super::types::SuccessResponse::new("鉴权设置已更新"))
    }

    pub fn get_client_identity_settings(&self) -> ClientIdentitySettingsResponse {
        let cfg = self.token_manager.config();
        ClientIdentitySettingsResponse {
            kiro_version: cfg.kiro_version.clone(),
            system_version: cfg.system_version.clone(),
            node_version: cfg.node_version.clone(),
        }
    }

    /// web_search 代执行开关（仅影响 `/v1/responses` 端点）
    pub fn get_websearch_settings(&self) -> crate::admin::types::WebSearchSettingsResponse {
        crate::admin::types::WebSearchSettingsResponse {
            web_search_emulation: self.token_manager.config().web_search_emulation,
        }
    }

    pub fn update_websearch_settings(
        &self,
        req: crate::admin::types::UpdateWebSearchSettingsRequest,
    ) -> Result<super::types::SuccessResponse, AdminServiceError> {
        let enabled = req.web_search_emulation;
        self.token_manager
            .update_config_with(|cfg| {
                cfg.web_search_emulation = enabled;
            })
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        self.token_manager
            .save_config()
            .map_err(|e| AdminServiceError::InternalError(format!("配置落盘失败: {}", e)))?;

        tracing::info!(enabled = enabled, "web_search 代执行开关已更新");
        Ok(super::types::SuccessResponse::new(format!(
            "web_search 代执行已{}（仅影响 /v1/responses 端点）",
            if enabled { "启用" } else { "关闭" }
        )))
    }

    /// WebSocket ingress 运行时设置（含活跃连接数）
    pub fn get_ws_settings(&self) -> crate::admin::types::WsSettingsResponse {
        use crate::model::config::WsSettings;
        let (settings, active) = match (&self.ws_settings, &self.ws_admission) {
            (Some(ws), Some(adm)) => (ws.read().clone(), adm.active()),
            _ => (WsSettings::default(), 0),
        };
        crate::admin::types::WsSettingsResponse {
            enabled: settings.enabled,
            mode: Self::ws_mode_str(settings.mode).to_string(),
            max_connections: settings.max_connections,
            client_first_message_timeout_seconds: settings.client_first_message_timeout_seconds,
            inter_turn_idle_timeout_seconds: settings.inter_turn_idle_timeout_seconds,
            max_message_bytes: settings.max_message_bytes,
            upstream_read_timeout_seconds: settings.upstream_read_timeout_seconds,
            active_connections: active,
        }
    }

    fn ws_mode_str(mode: crate::model::config::WsTransportMode) -> &'static str {
        match mode {
            crate::model::config::WsTransportMode::HttpBridge => "http_bridge",
            crate::model::config::WsTransportMode::Passthrough => "passthrough",
        }
    }

    /// WebSocket 设置部分更新：未携带字段保持当前值；写内存立即生效，
    /// 随后更新中心配置并落盘；落盘失败时错误区分「已生效未落盘」。
    pub fn update_ws_settings(
        &self,
        req: crate::admin::types::UpdateWsSettingsRequest,
    ) -> Result<super::types::SuccessResponse, AdminServiceError> {
        use crate::model::config::{WsSettings, WsTransportMode};

        let Some(ws_handle) = &self.ws_settings else {
            return Err(AdminServiceError::InternalError(
                "WebSocket 运行时未挂接".to_string(),
            ));
        };
        let current = ws_handle.read().clone();

        // 未知 mode 显式 400（区别于配置加载期的静默回落）
        let mode = match req.mode.as_deref().map(str::trim) {
            None => current.mode,
            Some("http_bridge") => WsTransportMode::HttpBridge,
            Some("passthrough") => WsTransportMode::Passthrough,
            Some(other) => {
                return Err(AdminServiceError::InvalidCredential(format!(
                    "未知的 websocket.mode: {}（可选值: http_bridge / passthrough）",
                    other
                )));
            }
        };

        let merged = WsSettings {
            enabled: req.enabled.unwrap_or(current.enabled),
            mode,
            max_connections: req.max_connections.unwrap_or(current.max_connections),
            client_first_message_timeout_seconds: req
                .client_first_message_timeout_seconds
                .unwrap_or(current.client_first_message_timeout_seconds),
            inter_turn_idle_timeout_seconds: req
                .inter_turn_idle_timeout_seconds
                .unwrap_or(current.inter_turn_idle_timeout_seconds),
            max_message_bytes: req.max_message_bytes.unwrap_or(current.max_message_bytes),
            upstream_read_timeout_seconds: req
                .upstream_read_timeout_seconds
                .unwrap_or(current.upstream_read_timeout_seconds),
        };

        // 热更新留痕：只记录变更字段的旧→新（spec：Admin 可读写 WebSocket 运行时设置）
        let mut diff: Vec<String> = Vec::new();
        if current.enabled != merged.enabled {
            diff.push(format!("enabled: {} -> {}", current.enabled, merged.enabled));
        }
        if current.mode != merged.mode {
            diff.push(format!(
                "mode: {} -> {}",
                Self::ws_mode_str(current.mode),
                Self::ws_mode_str(merged.mode)
            ));
        }
        if current.max_connections != merged.max_connections {
            diff.push(format!(
                "maxConnections: {} -> {}",
                current.max_connections, merged.max_connections
            ));
        }
        if current.client_first_message_timeout_seconds
            != merged.client_first_message_timeout_seconds
        {
            diff.push(format!(
                "clientFirstMessageTimeoutSeconds: {} -> {}",
                current.client_first_message_timeout_seconds,
                merged.client_first_message_timeout_seconds
            ));
        }
        if current.inter_turn_idle_timeout_seconds != merged.inter_turn_idle_timeout_seconds {
            diff.push(format!(
                "interTurnIdleTimeoutSeconds: {} -> {}",
                current.inter_turn_idle_timeout_seconds, merged.inter_turn_idle_timeout_seconds
            ));
        }
        if current.max_message_bytes != merged.max_message_bytes {
            diff.push(format!(
                "maxMessageBytes: {} -> {}",
                current.max_message_bytes, merged.max_message_bytes
            ));
        }
        if current.upstream_read_timeout_seconds != merged.upstream_read_timeout_seconds {
            diff.push(format!(
                "upstreamReadTimeoutSeconds: {} -> {}",
                current.upstream_read_timeout_seconds, merged.upstream_read_timeout_seconds
            ));
        }
        tracing::info!(
            changes = if diff.is_empty() { "无字段变化".to_string() } else { diff.join(", ") },
            "WebSocket ingress 设置已更新"
        );

        // 先写内存句柄（新连接立即按新语义），再更新中心配置并落盘
        *ws_handle.write() = merged.clone();
        let persisted = self
            .token_manager
            .update_config_with(|cfg| cfg.websocket = merged.clone())
            .and_then(|_| self.token_manager.save_config());
        match persisted {
            Ok(()) => Ok(super::types::SuccessResponse::new(
                "WebSocket 设置已更新并落盘",
            )),
            Err(e) => Err(AdminServiceError::InternalError(format!(
                "WebSocket 设置已在内存生效但未落盘: {}",
                e
            ))),
        }
    }

    /// 对外 Public API 目录（只读）
    ///
    /// 注意：这里描述的是「客户端 -> 本代理」的端点，与 `get_endpoint_settings`
    /// 管理的「本代理 -> 上游 Kiro」端点是两个不同概念。
    pub fn get_public_api(&self) -> crate::public_api::PublicApiResponse {
        let cfg = self.token_manager.config();
        // 鉴权状态优先取热更新句柄，回落到配置（与 get_auth_settings 同源）
        let auth = self.get_auth_settings();
        crate::public_api::build_response(
            cfg.host.clone(),
            cfg.port,
            auth.require_api_key,
            auth.api_key_mask,
            // publicBaseUrl 尚未纳入配置，前端回落 window.location.origin
            None,
        )
    }

    pub fn update_client_identity_settings(
        &self,
        req: UpdateClientIdentitySettingsRequest,
    ) -> Result<super::types::SuccessResponse, AdminServiceError> {
        const MAX_LEN: usize = 64;
        fn validate_field(name: &str, value: &str) -> Result<String, AdminServiceError> {
            let t = value.trim();
            if t.is_empty() {
                return Err(AdminServiceError::InvalidCredential(format!(
                    "{} 不能为空",
                    name
                )));
            }
            if t.len() > MAX_LEN {
                return Err(AdminServiceError::InvalidCredential(format!(
                    "{} 长度不能超过 {} 字符",
                    name, MAX_LEN
                )));
            }
            Ok(t.to_string())
        }

        let kiro = req
            .kiro_version
            .as_deref()
            .map(|s| validate_field("kiroVersion", s))
            .transpose()?;
        let system = req
            .system_version
            .as_deref()
            .map(|s| validate_field("systemVersion", s))
            .transpose()?;
        let node = req
            .node_version
            .as_deref()
            .map(|s| validate_field("nodeVersion", s))
            .transpose()?;

        if kiro.is_none() && system.is_none() && node.is_none() {
            return Err(AdminServiceError::InvalidCredential(
                "至少提供 kiroVersion / systemVersion / nodeVersion 之一".into(),
            ));
        }

        self.token_manager
            .update_config_with(|cfg| {
                if let Some(v) = &kiro {
                    cfg.kiro_version = v.clone();
                }
                if let Some(v) = &system {
                    cfg.system_version = v.clone();
                }
                if let Some(v) = &node {
                    cfg.node_version = v.clone();
                }
            })
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        self.token_manager
            .save_config()
            .map_err(|e| AdminServiceError::InternalError(format!("配置落盘失败: {}", e)))?;

        Ok(super::types::SuccessResponse::new(
            "客户端标识已更新（后续上游请求使用新值）",
        ))
    }

    pub async fn start_builder_id_login(
        &self,
        region: Option<String>,
    ) -> Result<crate::kiro::online_auth::BuilderIdStartResponse, AdminServiceError> {
        let proxy = self.token_manager.global_proxy();
        let cfg = self.token_manager.config();
        crate::kiro::online_auth::start_builder_id(region, proxy.as_ref(), &cfg)
            .await
            .map_err(|e| AdminServiceError::UpstreamError(e.to_string()))
    }

    pub async fn poll_builder_id_login(
        &self,
        session_id: String,
    ) -> Result<serde_json::Value, AdminServiceError> {
        let proxy = self.token_manager.global_proxy();
        let cfg = self.token_manager.config();
        let result = crate::kiro::online_auth::poll_builder_id(&session_id, proxy.as_ref(), &cfg)
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
        let cfg = self.token_manager.config();
        crate::kiro::online_auth::start_iam_sso(&start_url, region, proxy.as_ref(), &cfg)
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
        let cfg = self.token_manager.config();
        let tokens = crate::kiro::online_auth::complete_iam_sso(
            &session_id,
            &callback_url,
            proxy.as_ref(),
            &cfg,
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
            let cfg = self.token_manager.config();
            match crate::kiro::online_auth::import_sso_token(
                line,
                region.clone(),
                proxy.as_ref(),
                &cfg,
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
            return Err(AdminServiceError::UpstreamError(if errors.is_empty() {
                "SSO token import failed".into()
            } else {
                errors.join("; ")
            }));
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
        } else if msg.contains("只能删除已禁用的凭据") || msg.contains("请先禁用凭据")
        {
            AdminServiceError::InvalidCredential(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::types::TestCredentialRequest;
    use crate::kiro::model::credentials::KiroCredentials;
    use crate::model::config::Config;

    fn manager_with_one() -> Arc<MultiTokenManager> {
        manager_with_config(Config::default())
    }

    fn manager_with_config(config: Config) -> Arc<MultiTokenManager> {
        let mut c = KiroCredentials::default();
        c.refresh_token = Some("a".repeat(150));
        Arc::new(MultiTokenManager::new(config, vec![c], None, None, false).unwrap())
    }

    fn temp_config() -> (Config, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "kiro-rs-client-identity-test-{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, "{}").unwrap();
        let cfg = Config::load(&path).unwrap();
        (cfg, path)
    }

    /// 挂接 WS 运行时的 service + 运行时句柄
    fn ws_service() -> (
        AdminService,
        Arc<parking_lot::RwLock<crate::model::config::WsSettings>>,
        Arc<crate::openai::ws_transport::WsAdmission>,
    ) {
        ws_service_with_manager(manager_with_one())
    }

    fn ws_service_with_manager(
        mgr: Arc<MultiTokenManager>,
    ) -> (
        AdminService,
        Arc<parking_lot::RwLock<crate::model::config::WsSettings>>,
        Arc<crate::openai::ws_transport::WsAdmission>,
    ) {
        let ws = Arc::new(parking_lot::RwLock::new(
            crate::model::config::WsSettings::default(),
        ));
        let adm = Arc::new(crate::openai::ws_transport::WsAdmission::new());
        let service =
            AdminService::new(mgr, Vec::<String>::new()).with_ws_runtime(ws.clone(), adm.clone());
        (service, ws, adm)
    }

    #[tokio::test]
    async fn test_credential_rejects_unmapped_model() {
        let mgr = manager_with_one();
        let service = AdminService::new(mgr, Vec::<String>::new());
        let err = service
            .test_credential(
                1,
                TestCredentialRequest {
                    model: Some("totally-unknown-model-xyz".into()),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        let msg = err.to_string();
        assert!(!msg.starts_with("凭据无效"), "msg={}", msg);
        let body = serde_json::to_string(&err.into_response()).unwrap();
        assert!(!body.contains("凭据无效"));
        assert!(!body.to_lowercase().contains("refreshtoken"));
        assert!(!body.to_lowercase().contains("accesstoken"));
    }

    #[tokio::test]
    async fn test_credential_resolves_auto_locally() {
        // auto 应在本地解析为 defaultChatModel；无上游 token 时可能在 generate 阶段失败，
        // 但不得以 unmapped 拒绝。
        let mgr = manager_with_one();
        let service = AdminService::new(mgr, Vec::<String>::new());
        let err = service
            .test_credential(
                1,
                TestCredentialRequest {
                    model: Some("auto".into()),
                },
            )
            .await
            .err();
        if let Some(e) = err {
            let msg = e.to_string();
            assert!(
                !msg.contains("无法映射") && !msg.contains("不在可用 catalog"),
                "auto should not fail as unmapped: {}",
                msg
            );
        }
    }

    #[test]
    fn client_identity_get_and_update() {
        let (cfg, path) = temp_config();
        let mgr = manager_with_config(cfg);
        let service = AdminService::new(mgr, Vec::<String>::new());
        let before = service.get_client_identity_settings();
        assert!(!before.kiro_version.is_empty());
        let resp = service
            .update_client_identity_settings(UpdateClientIdentitySettingsRequest {
                kiro_version: Some("0.12.0-test".into()),
                system_version: Some("win32#10.0.0".into()),
                node_version: Some("22.22.0".into()),
            })
            .unwrap();
        assert!(resp.success);
        let after = service.get_client_identity_settings();
        assert_eq!(after.kiro_version, "0.12.0-test");
        assert_eq!(after.system_version, "win32#10.0.0");
        assert_eq!(after.node_version, "22.22.0");
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("0.12.0-test"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn client_identity_rejects_empty() {
        let mgr = manager_with_one();
        let service = AdminService::new(mgr, Vec::<String>::new());
        let err = service
            .update_client_identity_settings(UpdateClientIdentitySettingsRequest {
                kiro_version: Some("  ".into()),
                system_version: None,
                node_version: None,
            })
            .unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn refresh_models_not_found() {
        let mgr = manager_with_one();
        let service = AdminService::new(mgr, Vec::<String>::new());
        let err = service.refresh_models(999).await.unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_credential_models_not_found() {
        let mgr = manager_with_one();
        let service = AdminService::new(mgr, Vec::<String>::new());
        let err = service.get_credential_models(999, false).await.unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn credentials_status_includes_model_count_default_zero() {
        let mgr = manager_with_one();
        let service = AdminService::new(mgr, Vec::<String>::new());
        let status = service.get_all_credentials();
        assert_eq!(status.total, 1);
        assert_eq!(status.credentials[0].model_count, 0);
        assert!(status.credentials[0].models_updated_at.is_none());
    }

    #[test]
    fn credentials_status_model_count_from_cache() {
        let mgr = manager_with_one();
        // seed model cache via public path used by tests elsewhere
        {
            use crate::kiro::model::available_models::UpstreamModelInfo;
            let info = UpstreamModelInfo {
                model_id: "claude-sonnet-4.6".into(),
                model_name: Some("Sonnet".into()),
                description: None,
                input_types: vec![],
                rate_multiplier: None,
                token_limits: None,
            };
            // use refresh path internals: write through refresh_models is async; use test helper if any
            // Directly call get after seeding via private API is hard; use token_manager test inject if exists.
            // Fallback: only assert field presence when empty path works; if inject available:
            mgr.test_seed_model_cache(1, vec![info], Some("2026-07-24T00:00:00Z".into()));
        }
        let service = AdminService::new(mgr, Vec::<String>::new());
        let status = service.get_all_credentials();
        assert_eq!(status.credentials[0].model_count, 1);
        assert_eq!(
            status.credentials[0].models_updated_at.as_deref(),
            Some("2026-07-24T00:00:00Z")
        );
    }

    #[tokio::test]
    async fn get_balance_force_bypasses_cache() {
        let mgr = manager_with_one();
        let service = AdminService::new(mgr, Vec::<String>::new());
        // seed cache manually
        {
            let mut cache = service.balance_cache.lock();
            cache.insert(
                1,
                CachedBalance {
                    cached_at: chrono::Utc::now().timestamp() as f64,
                    data: BalanceResponse {
                        id: 1,
                        subscription_title: Some("cached".into()),
                        current_usage: 1.0,
                        usage_limit: 10.0,
                        remaining: 9.0,
                        usage_percentage: 10.0,
                        next_reset_at: None,
                    },
                },
            );
        }
        // force=false hits cache
        let hit = service.get_balance(1, false).await.unwrap();
        assert_eq!(hit.subscription_title.as_deref(), Some("cached"));
        // force=true will try upstream and fail for fake cred — but must not return cached
        let force_err = service.get_balance(1, true).await;
        assert!(
            force_err.is_err(),
            "force should not return cache-only success without upstream"
        );
    }

    #[test]
    fn test_websearch_setting_defaults_to_enabled() {
        let service = AdminService::new(manager_with_one(), Vec::<String>::new());
        assert!(
            service.get_websearch_settings().web_search_emulation,
            "缺省时 web_search 代执行应为启用（兼容现网）"
        );
    }

    #[test]
    fn test_websearch_setting_toggle() {
        let (cfg, path) = temp_config();
        let service = AdminService::new(manager_with_config(cfg), Vec::<String>::new());

        service
            .update_websearch_settings(crate::admin::types::UpdateWebSearchSettingsRequest {
                web_search_emulation: false,
            })
            .expect("关闭失败");
        assert!(!service.get_websearch_settings().web_search_emulation);

        service
            .update_websearch_settings(crate::admin::types::UpdateWebSearchSettingsRequest {
                web_search_emulation: true,
            })
            .expect("开启失败");
        assert!(service.get_websearch_settings().web_search_emulation);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_websearch_response_has_no_unrelated_secrets() {
        let mut cfg = Config::default();
        cfg.api_key = Some("sk-client-secret-value".to_string());
        cfg.admin_api_key = Some("sk-admin-secret-value".to_string());
        cfg.proxy_password = Some("proxy-secret".to_string());
        let service = AdminService::new(manager_with_config(cfg), Vec::<String>::new());

        let json = serde_json::to_string(&service.get_websearch_settings()).unwrap();
        assert!(!json.contains("sk-client-secret-value"));
        assert!(!json.contains("sk-admin-secret-value"));
        assert!(!json.contains("proxy-secret"));
        // 响应应只含该开关本身
        assert_eq!(json, r#"{"webSearchEmulation":true}"#);
    }

    #[test]
    fn test_websearch_setting_persisted() {
        let (cfg, path) = temp_config();
        let service = AdminService::new(manager_with_config(cfg), Vec::<String>::new());
        service
            .update_websearch_settings(crate::admin::types::UpdateWebSearchSettingsRequest {
                web_search_emulation: false,
            })
            .expect("更新失败");

        let saved = std::fs::read_to_string(&path).expect("读取配置失败");
        assert!(
            saved.contains("webSearchEmulation"),
            "变更必须落盘: {}",
            saved
        );
        let _ = std::fs::remove_file(path);
    }

    // === WebSocket 运行时设置（任务 7.3）===

    #[test]
    fn test_ws_settings_get_returns_defaults_and_active_count() {
        let (service, _ws, adm) = ws_service();
        let resp = service.get_ws_settings();
        assert!(resp.enabled, "缺省必须启用（兼容现网）");
        assert_eq!(resp.mode, "http_bridge");
        assert_eq!(resp.max_connections, 64);
        assert_eq!(resp.active_connections, 0);

        // 活跃连接数来自准入计数器实时值
        let g1 = adm.try_acquire(64).expect("准入失败");
        let _g2 = adm.try_acquire(64).expect("准入失败");
        assert_eq!(service.get_ws_settings().active_connections, 2);
        drop(g1);
        assert_eq!(service.get_ws_settings().active_connections, 1);
    }

    #[test]
    fn test_ws_settings_partial_update_merges_and_persists() {
        let (cfg, path) = temp_config();
        let (service, ws, _adm) = ws_service_with_manager(manager_with_config(cfg));

        // 仅更新 enabled：其余字段必须保持不变
        service
            .update_ws_settings(crate::admin::types::UpdateWsSettingsRequest {
                enabled: Some(false),
                mode: None,
                max_connections: None,
                client_first_message_timeout_seconds: None,
                inter_turn_idle_timeout_seconds: None,
                max_message_bytes: None,
                upstream_read_timeout_seconds: None,
            })
            .expect("部分更新失败");

        let after = service.get_ws_settings();
        assert!(!after.enabled, "enabled 必须热生效");
        assert_eq!(after.mode, "http_bridge", "未携带字段必须保持原值");
        assert_eq!(after.max_connections, 64);
        assert!(!ws.read().enabled, "内存句柄必须已生效（新连接立即按新语义）");

        let saved = std::fs::read_to_string(&path).expect("读取配置失败");
        assert!(saved.contains("\"websocket\""), "websocket 块必须落盘: {}", saved);
        assert!(saved.contains("\"enabled\": false"), "落盘值应为更新后: {}", saved);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_ws_settings_update_mode_and_unknown_mode_rejected() {
        let (cfg, path) = temp_config();
        let (service, ws, _adm) = ws_service_with_manager(manager_with_config(cfg));
        service
            .update_ws_settings(crate::admin::types::UpdateWsSettingsRequest {
                enabled: None,
                mode: Some("passthrough".to_string()),
                max_connections: None,
                client_first_message_timeout_seconds: None,
                inter_turn_idle_timeout_seconds: None,
                max_message_bytes: None,
                upstream_read_timeout_seconds: None,
            })
            .expect("mode 更新失败");
        assert_eq!(ws.read().mode, crate::model::config::WsTransportMode::Passthrough);
        assert_eq!(service.get_ws_settings().mode, "passthrough");

        let err = service
            .update_ws_settings(crate::admin::types::UpdateWsSettingsRequest {
                enabled: None,
                mode: Some("bogus".to_string()),
                max_connections: None,
                client_first_message_timeout_seconds: None,
                inter_turn_idle_timeout_seconds: None,
                max_message_bytes: None,
                upstream_read_timeout_seconds: None,
            })
            .expect_err("未知 mode 必须被拒绝");
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(
            ws.read().mode,
            crate::model::config::WsTransportMode::Passthrough,
            "拒绝的更新不得改动内存值"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_ws_settings_applied_but_not_persisted_distinguished() {
        // manager_with_one 的 Config 无 config_path：save_config 必然失败，
        // 但内存值必须已生效，且错误文案区分「已生效未落盘」
        let (service, ws, _adm) = ws_service();
        let err = service
            .update_ws_settings(crate::admin::types::UpdateWsSettingsRequest {
                enabled: Some(false),
                mode: None,
                max_connections: Some(8),
                client_first_message_timeout_seconds: None,
                inter_turn_idle_timeout_seconds: None,
                max_message_bytes: None,
                upstream_read_timeout_seconds: None,
            })
            .expect_err("无配置路径时落盘必须失败");
        let msg = err.to_string();
        assert!(msg.contains("已在内存生效但未落盘"), "错误必须区分语义: {}", msg);
        assert!(!ws.read().enabled, "内存值必须已生效");
        assert_eq!(ws.read().max_connections, 8);
    }

    #[test]
    fn test_get_public_api_never_leaks_full_key() {
        let secret = "sk-abcdefghijklmnopqrstuvwxyz";
        let mut cfg = Config::default();
        cfg.api_key = Some(secret.to_string());
        cfg.require_api_key = true;
        let service = AdminService::new(manager_with_config(cfg), Vec::<String>::new());

        let body = serde_json::to_string(&service.get_public_api()).unwrap();
        assert!(
            !body.contains(secret),
            "public-api 响应中不得出现完整 client apiKey"
        );
        assert!(body.contains("API_KEY"), "示例应使用占位符 API_KEY");
        assert!(
            body.contains("\"apiKeyMask\""),
            "应返回掩码字段: {}",
            body
        );
    }

    #[test]
    fn test_get_public_api_reflects_catalog_and_auth() {
        let mut cfg = Config::default();
        cfg.require_api_key = false;
        cfg.port = 18080;
        let service = AdminService::new(manager_with_config(cfg), Vec::<String>::new());
        let resp = service.get_public_api();

        assert!(!resp.server.require_api_key);
        assert_eq!(resp.server.port, 18080);
        assert!(
            resp.server.suggested_base_url.is_none(),
            "未配置 publicBaseUrl 时应为 null"
        );

        let total: usize = resp.families.iter().map(|f| f.endpoints.len()).sum();
        assert_eq!(
            total,
            crate::public_api::catalog().len(),
            "响应端点数应与 catalog 一致"
        );
    }

    // ============ KAM 导入 ============

    fn kam_service() -> AdminService {
        AdminService::new(manager_with_one(), Vec::<String>::new())
    }

    fn fake_rt(tag: &str) -> String {
        format!("fake-refresh-token-{tag}-{}", "0".repeat(120))
    }

    #[tokio::test]
    async fn kam_import_dry_run_reports_per_record_results() {
        let service = kam_service();
        let doc = serde_json::json!({
            "version": "1.9.2",
            "accounts": [
                {
                    "label": "Social 号",
                    "authMethod": "social",
                    "provider": "Google",
                    "refreshToken": fake_rt("social"),
                    "email": "placeholder@example.invalid",
                    "enabled": true
                },
                {
                    "label": "external 机密",
                    "authMethod": "external_idp",
                    "provider": null,
                    "refreshToken": fake_rt("ext"),
                    "clientId": "ms-cid",
                    "clientSecret": "ms-sec",
                    "tokenEndpoint": "https://login.microsoftonline.com/t/oauth2/v2.0/token",
                    "scopes": "openid profile",
                    "enabled": false
                },
                {
                    "label": "未知类型",
                    "authMethod": "oauth2",
                    "refreshToken": fake_rt("bad")
                },
                {
                    "label": "非法 endpoint",
                    "authMethod": "external_idp",
                    "refreshToken": fake_rt("ssrf"),
                    "clientId": "cid",
                    "tokenEndpoint": "https://attacker.example/token"
                }
            ]
        });

        let resp = service
            .import_kam_document(crate::admin::types::KamImportRequest {
                document: doc,
                options: None,
                dry_run: true,
            })
            .await
            .expect("dry run 不应整体失败");

        assert_eq!(resp.container, "Wrapper");
        assert_eq!(resp.preview.len(), 4);
        assert!(!resp.success, "存在无效记录时 success 应为 false");
        assert!(resp.summary.is_none(), "dry run 不入库");
        assert!(resp.results.is_empty());

        // 记录 0：social 有效
        assert!(resp.preview[0].valid);
        assert_eq!(resp.preview[0].auth_method.as_deref(), Some("social"));
        assert_eq!(resp.preview[0].provider.as_deref(), Some("Google"));
        assert!(resp.preview[0].has_refresh_token);
        assert!(!resp.preview[0].disabled);

        // 记录 1：external 机密客户端有效，enabled:false → disabled:true
        assert!(resp.preview[1].valid);
        assert_eq!(resp.preview[1].auth_method.as_deref(), Some("external_idp"));
        assert!(
            resp.preview[1].provider.is_none(),
            "external 不得回填 provider"
        );
        assert!(resp.preview[1].has_client_secret);
        assert!(resp.preview[1].has_token_endpoint);
        assert!(resp.preview[1].has_scopes);
        assert!(resp.preview[1].disabled, "enabled:false 应映射为 disabled");
        assert_eq!(resp.preview[1].nickname.as_deref(), Some("external 机密"));

        // 记录 2：未知 authMethod 逐条失败
        assert!(!resp.preview[2].valid);
        let err2 = resp.preview[2].error.as_deref().unwrap();
        assert!(err2.contains("oauth2"));
        assert!(err2.contains("external_idp"), "应列出合法取值");

        // 记录 3：非法 endpoint 逐条失败
        assert!(!resp.preview[3].valid);
        let err3 = resp.preview[3].error.as_deref().unwrap();
        assert!(err3.contains("Microsoft 登录域"), "实际: {err3}");
        // 错误不得含 token 材料
        assert!(!err3.contains("fake-refresh-token"));
    }

    #[tokio::test]
    async fn kam_import_public_client_passes_precheck() {
        let service = kam_service();
        let doc = serde_json::json!([{
            "label": "公共客户端",
            "authMethod": "external_idp",
            "refreshToken": fake_rt("pub"),
            "clientId": "ms-public-cid",
            "clientSecret": null,
            "tokenEndpoint": "https://login.microsoftonline.com/t/oauth2/v2.0/token"
        }]);

        let resp = service
            .import_kam_document(crate::admin::types::KamImportRequest {
                document: doc,
                options: None,
                dry_run: true,
            })
            .await
            .unwrap();

        assert_eq!(resp.container, "FlatArray");
        assert!(
            resp.preview[0].valid,
            "公共客户端不得因缺 clientSecret 被拒: {:?}",
            resp.preview[0].error
        );
        assert!(!resp.preview[0].has_client_secret);
        assert!(resp.success);
    }

    #[tokio::test]
    async fn kam_import_rejects_unrecognized_container_wholesale() {
        let service = kam_service();
        let err = service
            .import_kam_document(crate::admin::types::KamImportRequest {
                document: serde_json::json!({ "version": "1.0", "data": [] }),
                options: None,
                dry_run: true,
            })
            .await
            .expect_err("未知容器应整体失败");
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        let msg = err.to_string();
        assert!(msg.contains("version"), "错误应含顶层 key 名: {msg}");
        assert!(msg.contains("无法识别"));
    }

    #[tokio::test]
    async fn kam_import_preview_never_leaks_secrets() {
        let service = kam_service();
        let doc = serde_json::json!([{
            "label": "含敏感字段",
            "authMethod": "external_idp",
            "refreshToken": fake_rt("leak-check"),
            "clientId": "ms-cid",
            "clientSecret": "super-secret-value",
            "password": "account-password-value",
            "tokenEndpoint": "https://login.microsoftonline.com/t/oauth2/v2.0/token",
            "proxyConfig": { "password": "proxy-password-value" }
        }]);

        let resp = service
            .import_kam_document(crate::admin::types::KamImportRequest {
                document: doc,
                options: None,
                dry_run: true,
            })
            .await
            .unwrap();

        let body = serde_json::to_string(&resp).unwrap();
        for secret in [
            "super-secret-value",
            "account-password-value",
            "proxy-password-value",
            "fake-refresh-token-leak-check",
        ] {
            assert!(!body.contains(secret), "响应泄露了 {secret}");
        }
        // 但应回传「是否已配置」状态
        assert!(body.contains("hasClientSecret"));
    }

    #[tokio::test]
    async fn kam_import_and_file_load_produce_equivalent_credentials() {
        // 本 change 的核心验收标准：同一份 fixture 经 Admin 导入与启动加载，
        // 产出的规范化凭据必须等价。改动前 external 账号在两条路径分别走
        // AWS OIDC 与 Kiro Social 两个错误端点。
        let doc = serde_json::json!({
            "version": "1.9.2",
            "accounts": [
                {
                    "label": "Social 号",
                    "authMethod": "social",
                    "provider": "Google",
                    "refreshToken": fake_rt("eq-social"),
                    "email": "eq-social@example.invalid",
                    "userId": "eq-u1",
                    "region": "us-east-1",
                    "enabled": true
                },
                {
                    "label": "external 机密",
                    "authMethod": "external_idp",
                    "provider": null,
                    "refreshToken": fake_rt("eq-ext"),
                    "clientId": "ms-cid",
                    "clientSecret": "ms-sec",
                    "tokenEndpoint": "https://login.microsoftonline.com/t/oauth2/v2.0/token",
                    "issuerUrl": "https://login.microsoftonline.com/t",
                    "scopes": "openid profile",
                    "profileArn": "arn:aws:codewhisperer:us-east-1:000000000000:profile/EQEXTERNAL",
                    "enabled": false
                },
                {
                    "label": "BuilderId 号",
                    "authMethod": "IdC",
                    "refreshToken": fake_rt("eq-idc"),
                    "clientId": "idc-cid",
                    "clientSecret": "idc-sec",
                    "region": "eu-west-1",
                    "enabled": true
                }
            ]
        });

        // 路径 A：Admin 导入的预检结果（dry run，不触发网络）
        let service = kam_service();
        let via_admin = service
            .import_kam_document(crate::admin::types::KamImportRequest {
                document: doc.clone(),
                options: None,
                dry_run: true,
            })
            .await
            .expect("Admin 导入预检应成功");
        assert!(via_admin.success, "全部记录应通过预检");

        // 路径 B：写入临时文件后经启动加载器解析
        let dir = std::env::temp_dir().join(format!("kiro-rs-equiv-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("credentials.json");
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        let loaded = crate::kiro::model::credentials::CredentialsConfig::load_detailed(&path)
            .expect("启动加载应成功");
        assert!(loaded.needs_migration, "wrapper 格式应标记为需迁移");
        let via_file = loaded.config.into_sorted_credentials();

        // 两条路径的记录数一致
        assert_eq!(via_admin.preview.len(), via_file.len(), "记录数应一致");
        assert_eq!(via_file.len(), 3);

        // 逐条比对：认证类型、provider、身份字段、endpoint 元数据、禁用状态
        for (i, cred) in via_file.iter().enumerate() {
            let p = &via_admin.preview[i];
            assert_eq!(
                p.auth_method, cred.auth_method,
                "第 {i} 条的 authMethod 在两条路径不一致"
            );
            assert_eq!(p.provider, cred.provider, "第 {i} 条的 provider 不一致");
            assert_eq!(p.email, cred.email, "第 {i} 条的 email 不一致");
            assert_eq!(p.nickname, cred.nickname, "第 {i} 条的 nickname 不一致");
            assert_eq!(p.disabled, cred.disabled, "第 {i} 条的 disabled 不一致");
            assert_eq!(
                p.has_token_endpoint,
                cred.token_endpoint.is_some(),
                "第 {i} 条的 tokenEndpoint 存在性不一致"
            );
            assert_eq!(
                p.has_issuer_url,
                cred.issuer_url.is_some(),
                "第 {i} 条的 issuerUrl 存在性不一致"
            );
            assert_eq!(
                p.has_scopes,
                cred.scopes.is_some(),
                "第 {i} 条的 scopes 存在性不一致"
            );
            assert_eq!(
                p.has_client_secret,
                cred.client_secret.is_some(),
                "第 {i} 条的 clientSecret 存在性不一致"
            );
            assert_eq!(
                p.has_profile_arn,
                cred.profile_arn.is_some(),
                "第 {i} 条的 profileArn 存在性不一致"
            );
        }

        // external 账号：两条路径都必须选中同一个刷新去向
        let external = via_file
            .iter()
            .find(|c| c.auth_method.as_deref() == Some("external_idp"))
            .expect("应有 external 凭据");
        assert!(
            !crate::kiro::token_manager::refresh_routes_to_idc(external),
            "external 不得走 AWS OIDC"
        );
        assert_eq!(
            crate::kiro::external_idp::resolve_token_endpoint(
                external.token_endpoint.as_deref(),
                external.issuer_url.as_deref(),
            )
            .expect("endpoint 应可解析")
            .host_str(),
            Some("login.microsoftonline.com"),
            "external 必须发往 Microsoft 端点"
        );
        assert!(external.disabled, "enabled:false 应映射为 disabled");
        assert!(external.provider.is_none(), "external 不得回填 provider");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn kam_import_legacy_nested_maps_label() {
        let service = kam_service();
        let doc = serde_json::json!({
            "label": "嵌套形态号",
            "email": "nested@example.invalid",
            "credentials": {
                "authMethod": "social",
                "provider": "Github",
                "refreshToken": fake_rt("nested")
            }
        });

        let resp = service
            .import_kam_document(crate::admin::types::KamImportRequest {
                document: doc,
                options: None,
                dry_run: true,
            })
            .await
            .unwrap();

        assert_eq!(resp.container, "LegacyNested");
        assert!(resp.preview[0].valid);
        assert_eq!(
            resp.preview[0].nickname.as_deref(),
            Some("嵌套形态号"),
            "嵌套形态的 label 也必须映射为 nickname"
        );
        assert_eq!(resp.preview[0].email.as_deref(), Some("nested@example.invalid"));
    }
}
