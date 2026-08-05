//! Anthropic API 中间件

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use parking_lot::RwLock;

use crate::common::auth;
use crate::kiro::provider::KiroProvider;

use super::types::ErrorResponse;

/// 可热更新的客户端鉴权配置
#[derive(Debug, Clone)]
pub struct AuthRuntime {
    pub require_api_key: bool,
    pub api_key: String,
}

/// 应用共享状态
#[derive(Clone)]
pub struct AppState {
    /// 客户端鉴权（可热更新）
    pub auth: Arc<RwLock<AuthRuntime>>,
    /// Kiro Provider（可选，用于实际 API 调用）
    /// 内部使用 MultiTokenManager，已支持线程安全的多凭据管理
    pub kiro_provider: Option<Arc<KiroProvider>>,
    /// 是否开启非流式响应的 thinking 块提取
    pub extract_thinking: bool,
}

impl AppState {
    /// 创建新的应用状态
    pub fn new(api_key: impl Into<String>, extract_thinking: bool) -> Self {
        Self {
            auth: Arc::new(RwLock::new(AuthRuntime {
                require_api_key: true,
                api_key: api_key.into(),
            })),
            kiro_provider: None,
            extract_thinking,
        }
    }

    pub fn with_auth_runtime(mut self, require_api_key: bool, api_key: impl Into<String>) -> Self {
        self.auth = Arc::new(RwLock::new(AuthRuntime {
            require_api_key,
            api_key: api_key.into(),
        }));
        self
    }

    pub fn with_kiro_provider_arc(mut self, provider: Arc<KiroProvider>) -> Self {
        self.kiro_provider = Some(provider);
        self
    }

    #[cfg(test)]
    pub fn set_auth(&self, require_api_key: bool, api_key: Option<String>) {
        let mut auth = self.auth.write();
        auth.require_api_key = require_api_key;
        if let Some(k) = api_key {
            auth.api_key = k;
        }
    }

    #[cfg(test)]
    pub fn auth_snapshot(&self) -> AuthRuntime {
        self.auth.read().clone()
    }
}

/// API Key 认证中间件
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let auth = state.auth.read().clone();
    if !auth.require_api_key {
        return next.run(request).await;
    }
    if auth.api_key.trim().is_empty() {
        let error = ErrorResponse::authentication_error();
        return (StatusCode::UNAUTHORIZED, Json(error)).into_response();
    }
    match auth::extract_api_key(&request) {
        Some(key) if auth::constant_time_eq(&key, &auth.api_key) => next.run(request).await,
        _ => {
            let error = ErrorResponse::authentication_error();
            (StatusCode::UNAUTHORIZED, Json(error)).into_response()
        }
    }
}

/// CORS 中间件层
///
/// **安全说明**：当前配置允许所有来源（Any），这是为了支持公开 API 服务。
/// 如果需要更严格的安全控制，请根据实际需求配置具体的允许来源、方法和头信息。
///
/// # 配置说明
/// - `allow_origin(Any)`: 允许任何来源的请求
/// - `allow_methods(Any)`: 允许任何 HTTP 方法
/// - `allow_headers(Any)`: 允许任何请求头
pub fn cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};

    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_runtime_require_false_skips_key() {
        let state = AppState::new("secret", true).with_auth_runtime(false, "secret");
        let snap = state.auth_snapshot();
        assert!(!snap.require_api_key);
        assert_eq!(snap.api_key, "secret");
    }

    #[test]
    fn auth_runtime_require_true_empty_key() {
        let state = AppState::new("", true).with_auth_runtime(true, "");
        let snap = state.auth_snapshot();
        assert!(snap.require_api_key);
        assert!(snap.api_key.trim().is_empty());
    }

    #[test]
    fn set_auth_hot_update() {
        let state = AppState::new("old", true).with_auth_runtime(true, "old");
        state.set_auth(false, Some("new".into()));
        let snap = state.auth_snapshot();
        assert!(!snap.require_api_key);
        assert_eq!(snap.api_key, "new");
    }

    #[test]
    fn set_auth_keep_key_when_none() {
        let state = AppState::new("keep", true).with_auth_runtime(true, "keep");
        state.set_auth(true, None);
        assert_eq!(state.auth_snapshot().api_key, "keep");
    }
}
