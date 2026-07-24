//! Anthropic API 路由配置

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};

use std::sync::Arc;

use crate::kiro::provider::KiroProvider;

use super::{
    handlers::{count_tokens, get_models, post_messages, post_messages_cc},
    middleware::{AppState, auth_middleware, cors_layer},
};

/// 请求体最大大小限制 (50MB)
const MAX_BODY_SIZE: usize = 50 * 1024 * 1024;

/// 创建带有 KiroProvider 的 Anthropic API 路由
pub fn create_router_with_provider(
    api_key: impl Into<String>,
    kiro_provider: Option<KiroProvider>,
    extract_thinking: bool,
) -> Router {
    create_router_with_provider_and_auth(
        api_key,
        kiro_provider.map(Arc::new),
        extract_thinking,
        true,
    )
    .0
}

/// 创建路由并返回 AppState（供 Admin 热更新鉴权）
pub fn create_router_with_provider_and_auth(
    api_key: impl Into<String>,
    kiro_provider: Option<Arc<KiroProvider>>,
    extract_thinking: bool,
    require_api_key: bool,
) -> (Router, AppState) {
    let api_key = api_key.into();
    let mut state = AppState::new(&api_key, extract_thinking).with_auth_runtime(require_api_key, api_key);
    if let Some(provider) = kiro_provider {
        state = state.with_kiro_provider_arc(provider);
    }

    // 需要认证的 /v1 路由
    let v1_routes = Router::new()
        .route("/models", get(get_models))
        .route("/messages", post(post_messages))
        .route("/messages/count_tokens", post(count_tokens))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // 需要认证的 /cc/v1 路由（Claude Code 兼容端点）
    let cc_v1_routes = Router::new()
        .route("/messages", post(post_messages_cc))
        .route("/messages/count_tokens", post(count_tokens))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let app = Router::new()
        .nest("/v1", v1_routes)
        .nest("/cc/v1", cc_v1_routes)
        .layer(cors_layer())
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
        .with_state(state.clone());
    (app, state)
}
