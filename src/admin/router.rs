//! Admin API 路由配置

use axum::{
    Router, middleware,
    routing::{delete, get, post},
};

use super::{
    handlers::{
        add_credential, complete_iam_sso, delete_credential, force_refresh_token,
        get_all_credentials, get_auth_settings, get_client_identity_settings,
        get_credential_balance, get_credential_facets, get_credential_models, get_endpoint_settings,
        get_global_models_catalog, get_load_balancing_mode, get_proxy_settings, get_public_api,
        import_credential,
        import_credentials_batch, import_kam_document, import_sso_token, poll_builder_id,
        refresh_all_models,
        refresh_credential_models, reset_failure_count, set_credential_disabled,
        set_credential_priority, set_load_balancing_mode, start_builder_id, start_iam_sso,
        test_credential, update_auth_settings, update_client_identity_settings,
        get_websearch_settings, update_websearch_settings,
        get_ws_settings, update_ws_settings,
        update_endpoint_settings, update_proxy_settings,
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
        // 静态路径需在 /{id} 之前声明，避免被参数段吞掉
        .route("/credentials/facets", get(get_credential_facets))
        .route("/credentials/import", post(import_credential))
        .route("/credentials/import/batch", post(import_credentials_batch))
        .route("/credentials/import/kam", post(import_kam_document))
        .route("/credentials/{id}", delete(delete_credential))
        .route("/credentials/{id}/disabled", post(set_credential_disabled))
        .route("/credentials/{id}/priority", post(set_credential_priority))
        .route("/credentials/{id}/reset", post(reset_failure_count))
        .route("/credentials/{id}/refresh", post(force_refresh_token))
        .route("/credentials/{id}/balance", get(get_credential_balance))
        // models/refresh 全量必须在 /{id}/... 之前或使用更具体路径，避免冲突
        .route("/credentials/models/refresh", post(refresh_all_models))
        .route(
            "/credentials/{id}/models/refresh",
            post(refresh_credential_models),
        )
        .route("/credentials/{id}/models", get(get_credential_models))
        .route("/credentials/{id}/test", post(test_credential))
        .route("/models/catalog", get(get_global_models_catalog))
        // 对外 Public API 目录（只读）；勿与 /settings/endpoint（上游 Kiro 端点）混淆
        .route("/public-api", get(get_public_api))
        .route(
            "/config/load-balancing",
            get(get_load_balancing_mode).put(set_load_balancing_mode),
        )
        .route("/auth/builderid/start", post(start_builder_id))
        .route("/auth/builderid/poll", post(poll_builder_id))
        .route("/auth/iam-sso/start", post(start_iam_sso))
        .route("/auth/iam-sso/complete", post(complete_iam_sso))
        .route("/auth/sso-token", post(import_sso_token))
        .route(
            "/settings/proxy",
            get(get_proxy_settings).put(update_proxy_settings),
        )
        .route(
            "/settings/endpoint",
            get(get_endpoint_settings).put(update_endpoint_settings),
        )
        .route(
            "/settings/auth",
            get(get_auth_settings).put(update_auth_settings),
        )
        .route(
            "/settings/websearch",
            get(get_websearch_settings).put(update_websearch_settings),
        )
        .route(
            "/settings/websocket",
            get(get_ws_settings).put(update_ws_settings),
        )
        .route(
            "/settings/client-identity",
            get(get_client_identity_settings).put(update_client_identity_settings),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::service::AdminService;
    use crate::kiro::model::credentials::KiroCredentials;
    use crate::kiro::token_manager::MultiTokenManager;
    use crate::model::config::Config;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;

    const KEY: &str = "admin-test-key";

    /// 3 条凭据：id 1 为 idc，其余为 social，id 3 已禁用
    fn router() -> Router {
        let creds: Vec<KiroCredentials> = (1..=3u64)
            .map(|i| {
                let mut c = KiroCredentials::default();
                c.id = Some(i);
                c.priority = i as u32;
                c.auth_method = Some(if i == 1 { "idc" } else { "social" }.to_string());
                c.disabled = i == 3;
                c.refresh_token = Some(format!("{}{:03}", "a".repeat(147), i));
                c
            })
            .collect();
        let mgr =
            Arc::new(MultiTokenManager::new(Config::default(), creds, None, None, false).unwrap());
        let service = AdminService::new(mgr, Vec::<String>::new());
        create_admin_router(AdminState::new(KEY, service))
    }

    async fn get_json(uri: &str) -> (u16, serde_json::Value) {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("x-api-key", KEY)
                    .body(Body::empty())
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败");
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("读取响应体失败");
        let json = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("响应体不是合法 JSON")
        };
        (status, json)
    }

    #[tokio::test]
    async fn credentials_endpoint_applies_query_filters() {
        let (status, body) = get_json("/credentials?authMethod=idc&disabled=false").await;
        assert_eq!(status, 200);
        let ids: Vec<u64> = body["credentials"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, vec![1]);
        assert_eq!(body["pageInfo"]["filteredTotal"], 1);
        // total 与 available 是全量口径
        assert_eq!(body["total"], 3);
        assert_eq!(body["available"], 2);
    }

    #[tokio::test]
    async fn credentials_endpoint_clamps_out_of_range_paging() {
        let (status, body) = get_json("/credentials?page=-1&perPage=500").await;
        assert_eq!(status, 200);
        assert_eq!(body["pageInfo"]["page"], 1);
        assert_eq!(body["pageInfo"]["perPage"], 100);
    }

    #[tokio::test]
    async fn facets_route_is_reachable() {
        // 静态段被 /credentials/{id} 吞掉时本测试转红
        let (status, body) = get_json("/credentials/facets").await;
        assert_eq!(status, 200);
        let methods: Vec<&str> = body["authMethods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(methods, vec!["idc", "social"]);
    }

    #[tokio::test]
    async fn facets_route_requires_auth() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/credentials/facets")
                    .body(Body::empty())
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败");
        assert_eq!(response.status().as_u16(), 401);
    }
}
