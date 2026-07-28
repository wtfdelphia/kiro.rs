//! OpenAI 兼容协议模块
//!
//! 与 `crate::anthropic` 平级、互不侵入。请求侧翻译成内存 `MessagesRequest`
//! 后复用 Anthropic 侧的转换核与上游调用；响应侧平行实现 OpenAI 输出状态机。
//!
//! 设计见 `docs/multi-protocol-api-design.md` §6 与决策 D1/D8/D9/D12。

pub mod converter;
pub mod error;
pub mod handlers;
pub mod responses;
pub mod responses_stream;
pub mod responses_types;
pub mod stream;
pub mod types;
pub mod websearch;

use axum::{Router, extract::DefaultBodyLimit, middleware, routing::post};

use crate::anthropic::{AppState, MAX_BODY_SIZE, auth_middleware, cors_layer};

/// 创建 OpenAI 兼容路由
///
/// 注意：`Router::merge` 只合并路由表，**不传播已应用的 layer**，
/// 所以 auth / cors / body limit 必须在这里各自挂齐。
/// 漏 auth 会让端点裸奔；漏 body limit 会退回 axum 默认 2MB 导致带图请求 413。
pub fn create_openai_routes(state: AppState) -> Router {
    let v1_routes = Router::new()
        .route("/chat/completions", post(handlers::post_chat_completions))
        .route("/responses", post(handlers::post_responses))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .nest("/v1", v1_routes)
        .layer(cors_layer())
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    const PATH: &str = "/v1/chat/completions";
    const RESPONSES_PATH: &str = "/v1/responses";

    fn router(require_api_key: bool, key: &str) -> Router {
        create_openai_routes(AppState::new(key, true).with_auth_runtime(require_api_key, key))
    }

    async fn post_to(app: Router, path: &str, api_key: Option<&str>, body: String) -> u16 {
        let mut builder = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json");
        if let Some(k) = api_key {
            builder = builder.header("x-api-key", k);
        }
        app.oneshot(builder.body(Body::from(body)).expect("构造请求失败"))
            .await
            .expect("路由调用失败")
            .status()
            .as_u16()
    }

    async fn post(app: Router, api_key: Option<&str>, body: String) -> u16 {
        post_to(app, PATH, api_key, body).await
    }

    fn minimal_body() -> String {
        r#"{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"hi"}]}"#.to_string()
    }

    #[tokio::test]
    async fn test_auth_required_no_key_rejected() {
        // 漏挂 auth_middleware 时本测试转红（merge 不传播 layer）
        let status = post(router(true, "secret"), None, minimal_body()).await;
        assert_eq!(status, 401, "requireApiKey 开启且无 key 时必须 401");
    }

    #[tokio::test]
    async fn test_auth_required_wrong_key_rejected() {
        let status = post(router(true, "secret"), Some("wrong"), minimal_body()).await;
        assert_eq!(status, 401);
    }

    #[tokio::test]
    async fn test_auth_required_correct_key_passes_auth() {
        // provider 未配置，通过鉴权后应是 503 而非 401
        let status = post(router(true, "secret"), Some("secret"), minimal_body()).await;
        assert_eq!(status, 503, "鉴权通过后应因无 provider 返回 503，实际 {}", status);
    }

    #[tokio::test]
    async fn test_auth_disabled_passes_without_key() {
        let status = post(router(false, "secret"), None, minimal_body()).await;
        assert_ne!(status, 401, "requireApiKey 关闭时不应因鉴权被拒");
        assert_eq!(status, 503);
    }

    #[tokio::test]
    async fn test_body_over_default_limit_not_rejected() {
        // 漏挂 DefaultBodyLimit 时退回 axum 默认 2MB，本测试转红
        let big = "x".repeat(4 * 1024 * 1024);
        let body = format!(
            r#"{{"model":"claude-sonnet-4.5","messages":[{{"role":"user","content":"{}"}}]}}"#,
            big
        );
        let status = post(router(false, "k"), None, body).await;
        assert_ne!(status, 413, "4MB 请求不应被默认 2MB 限制拦截");
    }

    #[tokio::test]
    async fn test_invalid_json_returns_openai_error_shape() {
        let app = router(false, "k");
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(PATH)
                    .header("content-type", "application/json")
                    .body(Body::from("{not json"))
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败");
        assert_eq!(resp.status().as_u16(), 400);

        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .expect("读取响应体失败");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("响应非 JSON");
        assert_eq!(json["error"]["type"], "invalid_request_error");
        assert!(json["error"]["message"].is_string());
    }

    fn responses_body() -> String {
        r#"{"model":"claude-sonnet-4.5","input":"hi"}"#.to_string()
    }

    #[tokio::test]
    async fn test_responses_auth_required() {
        let status = post_to(router(true, "secret"), RESPONSES_PATH, None, responses_body()).await;
        assert_eq!(status, 401, "Responses 端点必须受同一 auth layer 保护");
    }

    #[tokio::test]
    async fn test_responses_correct_key_passes_auth() {
        let status =
            post_to(router(true, "secret"), RESPONSES_PATH, Some("secret"), responses_body()).await;
        assert_eq!(status, 503, "鉴权通过后应因无 provider 返回 503");
    }

    #[tokio::test]
    async fn test_responses_body_over_default_limit_not_rejected() {
        let big = "x".repeat(4 * 1024 * 1024);
        let body = format!(r#"{{"model":"claude-sonnet-4.5","input":"{}"}}"#, big);
        let status = post_to(router(false, "k"), RESPONSES_PATH, None, body).await;
        assert_ne!(status, 413);
    }

    #[tokio::test]
    async fn test_responses_invalid_json_openai_shape() {
        let resp = router(false, "k")
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(RESPONSES_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from("{oops"))
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败");
        assert_eq!(resp.status().as_u16(), 400);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["type"], "invalid_request_error");
    }

    #[tokio::test]
    async fn test_responses_previous_response_id_rejected_over_http() {
        let resp = router(false, "k")
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(RESPONSES_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"claude-sonnet-4.5","input":"hi","previous_response_id":"resp_1"}"#,
                    ))
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败");
        assert_eq!(resp.status().as_u16(), 400, "有状态续接必须明确报错，不得静默降级");
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            json["error"]["message"].as_str().unwrap().contains("previous_response_id"),
            "错误信息应点明该字段"
        );
    }

    #[tokio::test]
    async fn test_responses_get_method_not_allowed() {
        let status = router(false, "k")
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(RESPONSES_PATH)
                    .body(Body::empty())
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败")
            .status()
            .as_u16();
        assert_eq!(status, 405);
    }

    #[tokio::test]
    async fn test_responses_retrieve_not_mounted() {
        // GET /v1/responses/{id} 仍为 planned
        let status = router(false, "k")
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/responses/resp_123")
                    .body(Body::empty())
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败")
            .status()
            .as_u16();
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn test_get_method_not_allowed() {
        let app = router(false, "k");
        let status = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(PATH)
                    .body(Body::empty())
                    .expect("构造请求失败"),
            )
            .await
            .expect("路由调用失败")
            .status()
            .as_u16();
        assert_eq!(status, 405);
    }
}
