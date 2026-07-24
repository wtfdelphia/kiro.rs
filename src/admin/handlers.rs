//! Admin API HTTP 处理器

use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};

use super::{
    middleware::AdminState,
    types::{
        AddCredentialRequest, SetDisabledRequest, SetLoadBalancingModeRequest, SetPriorityRequest,
        SuccessResponse, TestCredentialRequest,
    },
};

/// GET /api/admin/credentials
/// 获取所有凭据状态
pub async fn get_all_credentials(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_all_credentials();
    Json(response)
}

/// POST /api/admin/credentials/:id/disabled
/// 设置凭据禁用状态
pub async fn set_credential_disabled(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetDisabledRequest>,
) -> impl IntoResponse {
    match state.service.set_disabled(id, payload.disabled) {
        Ok(_) => {
            let action = if payload.disabled { "禁用" } else { "启用" };
            Json(SuccessResponse::new(format!("凭据 #{} 已{}", id, action))).into_response()
        }
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/priority
/// 设置凭据优先级
pub async fn set_credential_priority(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetPriorityRequest>,
) -> impl IntoResponse {
    match state.service.set_priority(id, payload.priority) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} 优先级已设置为 {}",
            id, payload.priority
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/reset
/// 重置失败计数并重新启用
pub async fn reset_failure_count(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.reset_and_enable(id) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} 失败计数已重置并重新启用",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/credentials/:id/balance
/// 获取指定凭据的余额（?force=true 跳过 TTL 缓存）
pub async fn get_credential_balance(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Query(query): Query<crate::admin::types::BalanceQuery>,
) -> impl IntoResponse {
    match state.service.get_balance(id, query.force).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/models/catalog
/// 全局模型 catalog 摘要
pub async fn get_global_models_catalog(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_global_models_catalog())
}

/// POST /api/admin/credentials
/// 添加新凭据
pub async fn add_credential(
    State(state): State<AdminState>,
    Json(payload): Json<AddCredentialRequest>,
) -> impl IntoResponse {
    match state.service.add_credential(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}
/// POST /api/admin/credentials/import
pub async fn import_credential(
    State(state): State<AdminState>,
    Json(payload): Json<AddCredentialRequest>,
) -> impl IntoResponse {
    match state.service.import_credential(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}
/// POST /api/admin/credentials/import/batch
pub async fn import_credentials_batch(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::BatchImportRequest>,
) -> impl IntoResponse {
    match state.service.import_credentials_batch(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}



/// DELETE /api/admin/credentials/:id
/// 删除凭据
pub async fn delete_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.delete_credential(id) {
        Ok(_) => Json(SuccessResponse::new(format!("凭据 #{} 已删除", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/refresh
/// 强制刷新凭据 Token
pub async fn force_refresh_token(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.force_refresh_token(id).await {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} Token 已强制刷新",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/config/load-balancing
/// 获取负载均衡模式
pub async fn get_load_balancing_mode(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_load_balancing_mode();
    Json(response)
}

/// PUT /api/admin/config/load-balancing
/// 设置负载均衡模式
pub async fn set_load_balancing_mode(
    State(state): State<AdminState>,
    Json(payload): Json<SetLoadBalancingModeRequest>,
) -> impl IntoResponse {
    match state.service.set_load_balancing_mode(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}


/// POST /api/admin/auth/builderid/start
pub async fn start_builder_id(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::BuilderIdStartRequest>,
) -> impl IntoResponse {
    match state.service.start_builder_id_login(payload.region).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/builderid/poll
pub async fn poll_builder_id(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::BuilderIdPollRequest>,
) -> impl IntoResponse {
    match state.service.poll_builder_id_login(payload.session_id).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/iam-sso/start
pub async fn start_iam_sso(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::IamSsoStartRequest>,
) -> impl IntoResponse {
    match state
        .service
        .start_iam_sso_login(payload.start_url, payload.region)
        .await
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/iam-sso/complete
pub async fn complete_iam_sso(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::IamSsoCompleteRequest>,
) -> impl IntoResponse {
    match state
        .service
        .complete_iam_sso_login(payload.session_id, payload.callback_url)
        .await
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/sso-token
pub async fn import_sso_token(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::SsoTokenImportRequest>,
) -> impl IntoResponse {
    match state
        .service
        .import_sso_tokens(payload.bearer_token, payload.region)
        .await
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

// ============ 模型目录 / 测试 ============

#[derive(Debug, serde::Deserialize)]
pub struct ModelsLiveQuery {
    #[serde(default)]
    pub live: Option<bool>,
}

/// POST /api/admin/credentials/{id}/models/refresh
pub async fn refresh_credential_models(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.refresh_models(id).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/models/refresh
pub async fn refresh_all_models(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.refresh_models_all().await)
}

/// GET /api/admin/credentials/{id}/models
pub async fn get_credential_models(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Query(query): Query<ModelsLiveQuery>,
) -> impl IntoResponse {
    let live = query.live.unwrap_or(false);
    match state.service.get_credential_models(id, live).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/{id}/test
pub async fn test_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    body: Option<Json<TestCredentialRequest>>,
) -> impl IntoResponse {
    let req = body.map(|j| j.0).unwrap_or(TestCredentialRequest { model: None });
    match state.service.test_credential(id, req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}


// ============ Runtime settings ============

pub async fn get_proxy_settings(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_proxy_settings())
}

pub async fn update_proxy_settings(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::UpdateProxySettingsRequest>,
) -> impl IntoResponse {
    match state.service.update_proxy_settings(payload) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

pub async fn get_endpoint_settings(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_endpoint_settings())
}

pub async fn update_endpoint_settings(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::UpdateEndpointSettingsRequest>,
) -> impl IntoResponse {
    match state.service.update_endpoint_settings(payload) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

pub async fn get_auth_settings(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_auth_settings())
}

pub async fn update_auth_settings(
    State(state): State<AdminState>,
    Json(payload): Json<crate::admin::types::UpdateAuthSettingsRequest>,
) -> impl IntoResponse {
    match state.service.update_auth_settings(payload) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}
