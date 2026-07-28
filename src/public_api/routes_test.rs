//! 防漂移契约：catalog 的 status 必须与真实 Axum 路由表一致
//!
//! 两个方向都断言：
//! - `live ⊆ routes`：每个 Live 端点必须能被路由命中（401 也算命中，证明路由存在）
//! - `planned ∉ routes`：每个 Planned 端点必须命中不到（404）
//!
//! 这是 planned → live 切换的门禁：改了 status 却忘记挂载路由，测试立即红。

use axum::{Router, body::Body, http::Request};
use tower::ServiceExt;

use super::catalog::{EndpointStatus, catalog};

/// 构造与生产一致的对外路由（无 provider，不会真的调上游）
///
/// 必须与 `src/main.rs` 的挂载方式一致：Anthropic 路由 merge OpenAI 路由。
/// 少 merge 一个协议，防漂移断言就测不到那个协议的真实挂载。
fn public_router() -> Router {
    // require_api_key = true：未带 key 的请求会被鉴权中间件拦成 401，
    // 这正是我们需要的——401 证明路由存在，且不会触达 handler 逻辑。
    let (anthropic_app, state) =
        crate::anthropic::create_router_with_provider_and_auth("test-key", None, true, true);
    anthropic_app.merge(crate::openai::create_openai_routes(state))
}

async fn status_of(method: &str, path: &str) -> u16 {
    // path 中的占位段（如 /v1/responses/{id}）替换为具体值再打
    let concrete = path.replace("{id}", "resp_probe");
    let req = Request::builder()
        .method(method)
        .uri(&concrete)
        .body(Body::empty())
        .expect("构造请求失败");

    public_router()
        .oneshot(req)
        .await
        .expect("路由调用失败")
        .status()
        .as_u16()
}

#[tokio::test]
async fn test_live_endpoints_are_mounted() {
    for e in catalog() {
        if e.status != EndpointStatus::Live {
            continue;
        }
        let status = status_of(e.method, e.path).await;
        assert_ne!(
            status, 404,
            "catalog 中 {} 标记为 live，但 {} {} 在真实路由表中不存在（404）",
            e.id, e.method, e.path
        );
        assert_ne!(
            status, 405,
            "catalog 中 {} 的方法与实际挂载不符：{} {} 返回 405",
            e.id, e.method, e.path
        );
    }
}

#[tokio::test]
async fn test_planned_endpoints_are_not_mounted() {
    for e in catalog() {
        if e.status != EndpointStatus::Planned {
            continue;
        }
        let status = status_of(e.method, e.path).await;
        assert_eq!(
            status, 404,
            "catalog 中 {} 标记为 planned，但 {} {} 已被挂载（返回 {}）。\
             planned 端点不得占用路由表，也不得返回 501 占位",
            e.id, e.method, e.path, status
        );
    }
}

#[tokio::test]
async fn test_no_alias_routes_mounted() {
    // 首版不挂别名（design.md D5）；抽查 Kiro-Go 支持的几个别名形式
    for (method, path) in [
        ("POST", "/messages"),
        ("POST", "/chat/completions"),
        ("POST", "/anthropic/v1/messages"),
        ("GET", "/models"),
    ] {
        let status = status_of(method, path).await;
        assert_eq!(
            status, 404,
            "首版不应挂载路径别名，但 {} {} 返回了 {}",
            method, path, status
        );
    }
}

#[tokio::test]
async fn test_live_endpoints_require_auth() {
    // 所有 live 端点都受 auth_middleware 约束：未带 key 时返回 401 而非放行
    for e in catalog() {
        if e.status != EndpointStatus::Live {
            continue;
        }
        let status = status_of(e.method, e.path).await;
        assert_eq!(
            status, 401,
            "{} {}（{}）未带 api key 时应返回 401，实际 {}",
            e.method, e.path, e.id, status
        );
    }
}
