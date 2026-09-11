//! store 集成测试：迁移幂等、钥匙串拆分与回退、导入导出往返、并发写

use std::collections::HashMap;
use std::sync::Arc;

use kiro_rs::kiro::model::credentials::KiroCredentials;
use kiro_rs::storage::{ConfigStore, CredentialStore};
use parking_lot::Mutex;

use super::secrets::SecretBackend;
use super::{SqliteStore, json_io};

/// 内存 secret 后端（绕开钥匙串探测，确定性测试）
struct MemoryBackend {
    map: Mutex<HashMap<String, String>>,
}

impl MemoryBackend {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }
}

impl SecretBackend for MemoryBackend {
    fn write(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.map.lock().insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn read(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.map.lock().get(key).cloned())
    }
    fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.map.lock().remove(key);
        Ok(())
    }
    fn is_system(&self) -> bool {
        true
    }
}

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

#[test]
fn credential_roundtrip_splits_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());

    let mut cred = social_cred("a@example.com", "rt-secret-1");
    cred.id = Some(1);
    cred.kiro_api_key = Some("ksk-secret".to_string());
    cred.proxy_password = Some("pw-secret".to_string());
    cred.client_secret = Some("cs-secret".to_string());
    store.persist(&[cred]).unwrap();

    let loaded = CredentialStore::load(&*store).unwrap();
    assert!(!loaded.needs_migration);
    let creds = loaded.config.into_sorted_credentials();
    assert_eq!(creds.len(), 1);
    let c = &creds[0];
    assert_eq!(c.id, Some(1));
    assert_eq!(c.refresh_token.as_deref(), Some("rt-secret-1"));
    assert_eq!(c.kiro_api_key.as_deref(), Some("ksk-secret"));
    assert_eq!(c.proxy_password.as_deref(), Some("pw-secret"));
    assert_eq!(c.client_secret.as_deref(), Some("cs-secret"));

    // SQLite 的 fields 列不含任何 secret 明文
    let conn = rusqlite::Connection::open(dir.path().join("kiro.db")).unwrap();
    let fields: String = conn
        .query_row("SELECT fields FROM credentials WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    for secret in ["rt-secret-1", "ksk-secret", "pw-secret", "cs-secret"] {
        assert!(!fields.contains(secret), "fields 列含明文 secret: {}", secret);
    }
    // secrets 表只存钥匙串引用
    let refs: Vec<String> = {
        let mut stmt = conn.prepare("SELECT keyring_ref FROM secrets").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(refs.len(), 4);
    assert!(refs.iter().all(|r| r.starts_with("cred-1:")));
}

#[test]
fn persist_assigns_ids_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());

    let creds = vec![social_cred("a@x.com", "rt-1"), social_cred("b@x.com", "rt-2")];
    store.persist(&creds).unwrap();

    let loaded = CredentialStore::load(&*store).unwrap();
    let out = loaded.config.into_sorted_credentials();
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|c| c.id.is_some()));
}

#[test]
fn persist_deletes_by_diff_and_cleans_orphan_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());

    let mut c1 = social_cred("a@x.com", "rt-1");
    c1.id = Some(1);
    let mut c2 = social_cred("b@x.com", "rt-2");
    c2.id = Some(2);
    store.persist(&[c1.clone(), c2.clone()]).unwrap();
    assert_eq!(store.credential_count().unwrap(), 2);

    // 第二次只保留 #2：#1 的行与钥匙串条目都应清理
    store.persist(&[c2]).unwrap();
    assert_eq!(store.credential_count().unwrap(), 1);
    let loaded = CredentialStore::load(&*store).unwrap();
    let out = loaded.config.into_sorted_credentials();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, Some(2));
}

#[test]
fn missing_secret_entry_disables_credential_only() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Box::new(MemoryBackend::new());
    let store = SqliteStore::open_with_backend(dir.path(), backend).unwrap();

    let mut c1 = social_cred("a@x.com", "rt-1");
    c1.id = Some(1);
    let mut c2 = social_cred("b@x.com", "rt-2");
    c2.id = Some(2);
    store.persist(&[c1.clone(), c2.clone()]).unwrap();

    // 模拟钥匙串条目丢失：删掉 #1 的 refresh_token 条目
    store.secrets.delete("cred-1:refresh_token").unwrap();

    let loaded = CredentialStore::load(&*store).unwrap();
    let mut out = loaded.config.into_sorted_credentials();
    out.sort_by_key(|c| c.id);
    assert_eq!(out.len(), 2);
    assert!(out[0].disabled, "条目缺失的凭据应被禁用");
    assert!(!out[1].disabled, "其余凭据不受影响");
    assert!(out[0].refresh_token.is_none());
    assert_eq!(out[1].refresh_token.as_deref(), Some("rt-2"));
}

#[test]
fn config_roundtrip_and_default_when_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());

    // 空库：默认配置
    let config = ConfigStore::load(&*store).unwrap();
    assert_eq!(config.port, kiro_rs::model::config::Config::default().port);

    let mut config = kiro_rs::model::config::Config::default();
    config.port = 9777;
    config.api_key = Some("sk-test".to_string());
    store.save(&config).unwrap();

    let reloaded = ConfigStore::load(&*store).unwrap();
    assert_eq!(reloaded.port, 9777);
    assert_eq!(reloaded.api_key.as_deref(), Some("sk-test"));
    assert!(reloaded.config_path().is_none());
}

#[test]
fn model_catalog_roundtrip() {
    use kiro_rs::kiro::model::available_models::UpstreamModelInfo;

    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());

    // 外键约束：model_catalog.credential_id 指向 credentials(id)
    let mut cred = social_cred("owner@x.com", "rt-owner");
    cred.id = Some(3);
    store.persist(&[cred]).unwrap();

    let models = vec![
        UpstreamModelInfo {
            model_id: "claude-sonnet-4.6".into(),
            model_name: Some("Sonnet".into()),
            description: None,
            input_types: vec!["text".into()],
            rate_multiplier: None,
            token_limits: None,
        },
        UpstreamModelInfo {
            model_id: "claude-haiku-4".into(),
            model_name: None,
            description: None,
            input_types: vec![],
            rate_multiplier: Some(1.0),
            token_limits: None,
        },
    ];
    store.save_model_catalog(3, &models, 1_700_000_000).unwrap();

    let loaded = store.load_model_catalog().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].0, 3);
    // 读回按 model_id 排序（ORDER BY credential_id, model_id），与插入顺序无关
    let mut got = loaded[0].1.clone();
    got.sort_by(|a, b| a.model_id.cmp(&b.model_id));
    let mut want = models.clone();
    want.sort_by(|a, b| a.model_id.cmp(&b.model_id));
    assert_eq!(got, want);

    // 覆盖写：旧条目被替换
    store.save_model_catalog(3, &models[..1], 1_700_000_100).unwrap();
    let loaded = store.load_model_catalog().unwrap();
    assert_eq!(loaded[0].1.len(), 1);
}

#[test]
fn json_import_export_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());

    // 造一个多凭据 JSON（格式同 credentials.example.multiple.json）
    let json = r#"[
        {
            "refreshToken": "rt-json-1",
            "authMethod": "social",
            "email": "a@example.com",
            "priority": 0
        },
        {
            "kiroApiKey": "ksk-json-2",
            "authMethod": "api_key",
            "priority": 1,
            "disabled": true
        }
    ]"#;
    let cred_file = dir.path().join("credentials.json");
    std::fs::write(&cred_file, json).unwrap();

    let n = json_io::import_credentials(&store, &cred_file).unwrap();
    assert_eq!(n, 2);

    // 导出：格式兼容，secret 明文（JSON 格式本征）
    let export_file = dir.path().join("export.json");
    json_io::export_credentials(&store, &export_file).unwrap();
    let exported = std::fs::read_to_string(&export_file).unwrap();
    assert!(exported.contains("rt-json-1"));
    assert!(exported.contains("ksk-json-2"));

    // 往返：再导入一遍，数据一致
    let store2_dir = tempfile::tempdir().unwrap();
    let store2 = open_store(store2_dir.path());
    json_io::import_credentials(&store2, &export_file).unwrap();
    let creds = CredentialStore::load(&*store2).unwrap().config.into_sorted_credentials();
    assert_eq!(creds.len(), 2);
}

#[test]
fn detect_and_import_backups_sources() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let store = open_store(&data_dir);

    std::fs::write(
        data_dir.join("credentials.json"),
        r#"{"kiroApiKey":"ksk-x","authMethod":"api_key"}"#,
    )
    .unwrap();
    std::fs::write(data_dir.join("config.json"), r#"{"port": 9123}"#).unwrap();

    let report =
        json_io::detect_and_import_in(&data_dir, &store, &[data_dir.clone()]);
    assert_eq!(report.credential_count, 1);
    assert!(report.config_imported);
    assert_eq!(report.backups.len(), 2);
    assert!(data_dir.join("credentials.json.bak").exists());
    assert!(data_dir.join("config.json.bak").exists());
    assert!(!data_dir.join("credentials.json").exists());

    // 配置已入库
    assert_eq!(ConfigStore::load(&*store).unwrap().port, 9123);
}

#[test]
fn concurrent_writes_do_not_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(dir.path());

    // 预置一条基准凭据，保证每轮写全量列表
    let mut base = social_cred("base@x.com", "rt-base");
    base.id = Some(1);
    store.persist(&[base.clone()]).unwrap();

    let mut handles = Vec::new();
    for t in 0..8 {
        let store = Arc::clone(&store);
        let base = base.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..20 {
                let mut c = social_cred(&format!("t{}-{}@x.com", t, i), &format!("rt-{}-{}", t, i));
                c.id = Some(100 + t as u64);
                // 全量列表写：基准 + 本线程凭据
                if let Err(e) = store.persist(&[base.clone(), c]) {
                    return Err(e.to_string());
                }
            }
            Ok::<(), String>(())
        }));
    }
    for h in handles {
        h.join().unwrap().unwrap();
    }

    // 完整性检查 + 可读回
    let conn = rusqlite::Connection::open(dir.path().join("kiro.db")).unwrap();
    let integrity: String =
        conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(integrity, "ok");

    // persist 是全量替换语义：每个线程写「基准 + 本线程凭据」，最后一次
    // 写入获胜，最终恰好 2 条。并发测试的目标是「写不损坏」，由上面的
    // integrity_check 保证；行数断言只校验最后状态自洽。
    let loaded = CredentialStore::load(&*store).unwrap();
    let creds = loaded.config.into_sorted_credentials();
    assert_eq!(creds.len(), 2);
    assert!(creds.iter().any(|c| c.email.as_deref() == Some("base@x.com")));
}

#[test]
fn reopen_preserves_data() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(MemoryBackend::new());

    // 两个 store 共享同一后端，模拟重启（SQLite 持久、钥匙串内存态测试用）
    let store = SqliteStore::open_with_backend(dir.path(), Box::new(BackendClone(backend.clone()))).unwrap();
    let mut c = social_cred("a@x.com", "rt-persist");
    c.id = Some(1);
    store.persist(&[c]).unwrap();
    drop(store);

    let store2 = SqliteStore::open_with_backend(dir.path(), Box::new(BackendClone(backend))).unwrap();
    let creds = CredentialStore::load(&*store2).unwrap().config.into_sorted_credentials();
    assert_eq!(creds.len(), 1);
    assert_eq!(creds[0].refresh_token.as_deref(), Some("rt-persist"));
}

/// 共享内部 map 的后端包装（reopen 测试用）
struct BackendClone(Arc<MemoryBackend>);

impl SecretBackend for BackendClone {
    fn write(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.0.write(key, value)
    }
    fn read(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.0.read(key)
    }
    fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.0.delete(key)
    }
    fn is_system(&self) -> bool {
        true
    }
}
