//! Admin API 路由配置

use axum::{
    Router, middleware,
    routing::{delete, get, post},
};

use super::{
    handlers::{
        add_credential, complete_iam_sso, delete_credential, force_refresh_token,
        get_all_credentials, get_credential_balance, get_credential_models, get_load_balancing_mode,
        import_credential, import_credentials_batch, import_sso_token, poll_builder_id,
        refresh_all_models, refresh_credential_models, reset_failure_count,
        set_credential_disabled, set_credential_priority, set_load_balancing_mode,
        start_builder_id, start_iam_sso, test_credential,
    },
    middleware::{AdminState, admin_auth_middleware},
};

/// 创建 Admin API 路由
pub fn create_admin_router(state: AdminState) -> Router {
    Router::new()
        .route(
            "/credentials",
            get(get_all_credentials).post(add_credential),
        )
        .route("/credentials/import", post(import_credential))
        .route("/credentials/import/batch", post(import_credentials_batch))
        .route("/credentials/{id}", delete(delete_credential))
        .route("/credentials/{id}/disabled", post(set_credential_disabled))
        .route("/credentials/{id}/priority", post(set_credential_priority))
        .route("/credentials/{id}/reset", post(reset_failure_count))
        .route("/credentials/{id}/refresh", post(force_refresh_token))
        .route("/credentials/{id}/balance", get(get_credential_balance))
        // models/refresh 全量必须在 /{id}/... 之前或使用更具体路径，避免冲突
        .route(
            "/credentials/models/refresh",
            post(refresh_all_models),
        )
        .route(
            "/credentials/{id}/models/refresh",
            post(refresh_credential_models),
        )
        .route("/credentials/{id}/models", get(get_credential_models))
        .route("/credentials/{id}/test", post(test_credential))
        .route(
            "/config/load-balancing",
            get(get_load_balancing_mode).put(set_load_balancing_mode),
        )
        .route("/auth/builderid/start", post(start_builder_id))
        .route("/auth/builderid/poll", post(poll_builder_id))
        .route("/auth/iam-sso/start", post(start_iam_sso))
        .route("/auth/iam-sso/complete", post(complete_iam_sso))
        .route("/auth/sso-token", post(import_sso_token))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state)
}
