//! Admin UI 路由的路径覆盖契约
//!
//! 必须按 `src/main.rs` 的方式 nest 到 `/admin` 再打，直接打裸 router 测不出
//! 前缀折叠带来的空路径问题。

use axum::{Router, body::Body, http::Request};
use tower::ServiceExt;

/// 与生产一致的挂载方式：走 `mount_admin_ui`，绕过它就测不到尾斜杠那条补丁
fn app() -> Router {
    super::mount_admin_ui(Router::new())
}

async fn get(path: &str) -> (u16, String) {
    let req = Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("构造请求失败");

    let res = app().oneshot(req).await.expect("路由调用失败");
    let status = res.status().as_u16();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("读取响应体失败");

    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn test_admin_root_serves_index() {
    let (status, body) = get("/admin").await;
    assert_eq!(status, 200, "/admin 应返回首页");
    assert!(body.contains("<div id=\"root\"></div>"), "响应体不是 index.html");
}

#[tokio::test]
async fn test_admin_root_with_trailing_slash_serves_index() {
    // nest 把内层 `/` 折成 `/admin`，而 `/{*file}` 的 catch-all 至少要吃一个字符，
    // 两条都落不到 `/admin/`。浏览器地址栏补斜杠是常事，这里必须也给首页。
    let (status, body) = get("/admin/").await;
    assert_eq!(status, 200, "/admin/ 带尾斜杠也应返回首页");
    assert!(body.contains("<div id=\"root\"></div>"), "响应体不是 index.html");
}

#[tokio::test]
async fn test_spa_route_serves_index() {
    let (status, body) = get("/admin/credentials").await;
    assert_eq!(status, 200, "SPA 前端路由应回落到首页");
    assert!(body.contains("<div id=\"root\"></div>"), "响应体不是 index.html");
}

#[tokio::test]
async fn test_missing_asset_stays_404() {
    // 有扩展名的路径不能回落到首页，否则前端拿到 HTML 当 JS 解析会报语法错
    let (status, _) = get("/admin/assets/does-not-exist.js").await;
    assert_eq!(status, 404, "缺失的资源文件应返回 404");
}

#[tokio::test]
async fn test_path_traversal_rejected() {
    let (status, _) = get("/admin/../../etc/passwd").await;
    assert_ne!(status, 200, "穿越路径不能返回内容");
}
