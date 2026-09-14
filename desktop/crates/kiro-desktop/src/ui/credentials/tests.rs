//! 凭据视图测试（change 4 任务 5.2 / 5.3）
//!
//! 5.2：headless 表格——行数/列数/单元格与快照一致（真实 AdminService +
//! tempdir SqliteStore，无主题渲染）。5.3：写操作经 `CoreHandle::exec` 走
//! 真实服务，重查断言；删除后 SQLite 行与钥匙串条目同步消失。

use std::sync::Arc;

use kiro_rs::admin::types::CredentialsQuery;
use kiro_rs::kiro::model::credentials::KiroCredentials;
use kiro_rs::model::config::Config;

use super::table::CredentialsDelegate;
use crate::core::CoreHandle;
use crate::store::SqliteStore;
use crate::store::secrets::MemoryBackend;

fn open_store(dir: &std::path::Path) -> Arc<SqliteStore> {
    SqliteStore::open_with_backend(dir, Box::new(MemoryBackend::new())).unwrap()
}

fn social_cred(email: &str, refresh_token: &str) -> KiroCredentials {
    let mut c = KiroCredentials::default();
    c.auth_method = Some("social".to_string());
    c.email = Some(email.to_string());
    c.refresh_token = Some(refresh_token.to_string());
    c
}

/// 用 SQLite 存储装配真实 `AdminService`（与生产同一装配路径）
fn service_with_store(
    store: Arc<SqliteStore>,
    creds: Vec<KiroCredentials>,
) -> Arc<kiro_rs::admin::AdminService> {
    let tm = kiro_rs::kiro::token_manager::MultiTokenManager::with_stores(
        Config::default(),
        creds,
        None,
        Some(store.clone()),
        Some(store.clone()),
        true,
    )
    .expect("MultiTokenManager 装配失败");
    Arc::new(kiro_rs::admin::AdminService::new_with_runtime(
        Arc::new(tm),
        vec!["ide".to_string()],
        None,
        None,
    ))
}

/// 5.2 headless 表格：行数/列数/单元格与快照一致（无主题渲染）
#[gpui::test]
fn table_rows_columns_and_cells_match_snapshot(cx: &mut gpui::TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());
    let creds = vec![social_cred("a@x.com", "rt-1"), social_cred("b@x.com", "rt-2")];
    let service = service_with_store(store, creds);

    let resp = service.query_credentials(&CredentialsQuery::default());
    let rows = resp.credentials;
    assert_eq!(rows.len(), 2, "快照应有两条凭据");

    cx.skip_drawing();
    let window = cx.add_window(move |window, cx| {
        gpui_kit::component::table::TableState::new(
            CredentialsDelegate { rows, view: None },
            window,
            cx,
        )
    });

    let (headers, cells) = window
        .update(cx, |t, _, cx| t.dump(cx))
        .expect("窗口未就绪");
    assert_eq!(headers.len(), 9, "列数应为 9");
    assert_eq!(cells.len(), 2, "行数应为 2");
    // 第 1 列（账号）展示 email
    assert_eq!(cells[0][1], "a@x.com");
    assert_eq!(cells[1][1], "b@x.com");
    // 第 0 列是 id（with_stores 已分配）
    assert!(!cells[0][0].is_empty());
}

/// 5.3 写操作经 exec 走真实服务：启停 + 优先级，重查断言
#[tokio::test]
async fn exec_toggle_and_priority_reload_reflects_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());
    let creds = vec![social_cred("a@x.com", "rt-1"), social_cred("b@x.com", "rt-2")];
    let service = service_with_store(store.clone(), creds);
    let handle = CoreHandle::new(service.clone(), tokio::runtime::Handle::current());

    let q = CredentialsQuery::default();
    let first = service.query_credentials(&q).credentials.into_iter().next().unwrap();
    let id = first.id;
    let orig_priority = first.priority;

    // 禁用
    let svc = service.clone();
    handle
        .exec(async move { svc.set_disabled(id, true).map_err(|e| e.to_string()) })
        .await
        .unwrap()
        .unwrap();
    let item = service
        .query_credentials(&q)
        .credentials
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    assert!(item.disabled, "exec 禁用后应重查为禁用");

    // 优先级 +1
    let svc = service.clone();
    let next = orig_priority + 1;
    handle
        .exec(async move { svc.set_priority(id, next).map_err(|e| e.to_string()) })
        .await
        .unwrap()
        .unwrap();
    let item = service
        .query_credentials(&q)
        .credentials
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    assert_eq!(item.priority, next, "exec 调整优先级后应重查生效");
}

/// 5.3 删除：先禁用再删；删除后 SQLite 行与钥匙串条目同步消失
#[tokio::test]
async fn exec_delete_cleans_sqlite_row_and_secret() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());
    let creds = vec![social_cred("del@x.com", "rt-del")];
    let service = service_with_store(store.clone(), creds);
    let handle = CoreHandle::new(service.clone(), tokio::runtime::Handle::current());

    let q = CredentialsQuery::default();
    let before = service.query_credentials(&q);
    assert_eq!(before.total, 1);
    let id = before.credentials.first().unwrap().id;

    // 删除前置：必须先禁用（服务端约束）
    let svc = service.clone();
    handle
        .exec(async move { svc.set_disabled(id, true).map_err(|e| e.to_string()) })
        .await
        .unwrap()
        .unwrap();
    // secret 在删除前应存在
    let secret_key = format!("cred-{}:refresh_token", id);
    assert!(store.secret_present(&secret_key), "删除前钥匙串条目应存在");

    // 删除
    let svc = service.clone();
    handle
        .exec(async move { svc.delete_credential(id).map_err(|e| e.to_string()) })
        .await
        .unwrap()
        .unwrap();

    // 重查：列表为空
    let after = service.query_credentials(&q);
    assert_eq!(after.total, 0, "删除后列表应为空");
    // SQLite 行消失
    assert_eq!(store.credential_count().unwrap(), 0, "SQLite 凭据行应清空");
    // 钥匙串条目同步清理
    assert!(!store.secret_present(&secret_key), "删除后钥匙串条目应同步清理");
}

/// 审核修复 4：启用中的凭据经删除对话框单任务「先禁用再删除」路径可删
#[tokio::test]
async fn exec_delete_enabled_credential_disables_then_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());
    let creds = vec![social_cred("en@x.com", "rt-en")];
    let service = service_with_store(store.clone(), creds);
    let handle = CoreHandle::new(service.clone(), tokio::runtime::Handle::current());

    let q = CredentialsQuery::default();
    let id = service
        .query_credentials(&q)
        .credentials
        .into_iter()
        .next()
        .unwrap()
        .id;

    // 与 open_delete_confirm 的 on_ok 同构：单个 exec 内串行两步
    let svc = service.clone();
    handle
        .exec(async move {
            svc.set_disabled(id, true).map_err(|e| format!("删除前禁用失败: {}", e))?;
            svc.delete_credential(id).map_err(|e| e.to_string())
        })
        .await
        .unwrap()
        .unwrap();

    let after = service.query_credentials(&q);
    assert_eq!(after.total, 0, "启用中的凭据经先禁用再删除后应为空");
    assert_eq!(store.credential_count().unwrap(), 0, "SQLite 凭据行应清空");
}
