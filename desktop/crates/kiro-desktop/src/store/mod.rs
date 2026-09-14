//! SQLite 存储后端（设计文档 §7）
//!
//! `SqliteStore` 同时实现 `kiro_rs::storage::{CredentialStore, ConfigStore}`：
//! 凭据与配置落 SQLite，secret 字段经 [`secrets::SecretBackend`] 走系统
//! 钥匙串（不可用时回退加密文件），SQLite 只存钥匙串条目引用。
//!
//! `stats` / `balance_cache` 两表本期只建不用：运行时统计与余额缓存仍走
//! `cache_dir()` 派生的文件路径，收编留给后续 change。

pub mod crypt;
pub mod json_io;
pub mod schema;
pub mod secrets;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use kiro_rs::kiro::model::available_models::UpstreamModelInfo;
use kiro_rs::kiro::model::credentials::{CredentialsConfig, KiroCredentials, LoadedCredentials};
use kiro_rs::model::config::Config;
use kiro_rs::storage::{ConfigStore, CredentialStore};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};

/// 行为偏好（change 6：托盘常驻 / 开机自启 / 自动启动服务器）
pub mod prefs {
    /// 关窗时最小化常驻（关则走两阶段退出）
    pub const CLOSE_TO_TRAY: &str = "close_to_tray";
    /// 开机自启（联动平台入口，见 `autostart`）
    pub const LAUNCH_AT_LOGIN: &str = "launch_at_login";
    /// 装配成功后自动启动内嵌服务器
    pub const AUTO_START_SERVER: &str = "auto_start_server";

    /// 缺省值（不写库，读时兜底）
    pub fn default(key: &str) -> bool {
        match key {
            CLOSE_TO_TRAY => true,
            AUTO_START_SERVER => true,
            LAUNCH_AT_LOGIN => false,
            _ => false,
        }
    }
}

/// SQLite 存储：单写连接 + secret 后端
pub struct SqliteStore {
    data_dir: PathBuf,
    /// 单写连接：所有读写共用，事务保持短小（设计文档 §7.3）
    conn: Mutex<Connection>,
    secrets: Box<dyn secrets::SecretBackend>,
}

impl SqliteStore {
    /// 打开或创建 `<data_dir>/kiro.db`，探测并选择 secret 后端。
    ///
    /// 打开失败（库损坏等）时备份损坏文件并重建空库，再失败才报错
    /// （§5.4 启动期处置：凭据需重新导入，界面明示由 change 5 接）。
    pub fn open(data_dir: &Path) -> anyhow::Result<Arc<Self>> {
        let db_path = data_dir.join("kiro.db");
        match Self::open_inner(data_dir, &db_path) {
            Ok(store) => Ok(store),
            Err(e) => {
                let backup = data_dir.join(format!(
                    "kiro.db.corrupt-{}",
                    chrono::Utc::now().timestamp()
                ));
                tracing::warn!(
                    "SQLite 打开失败（{}），备份 {:?} → {:?} 后重建空库",
                    e,
                    db_path,
                    backup
                );
                move_corrupt_artifacts(&db_path, &backup);
                Self::open_inner(data_dir, &db_path)
            }
        }
    }

    fn open_inner(data_dir: &Path, db_path: &Path) -> anyhow::Result<Arc<Self>> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("创建数据目录失败: {}", data_dir.display()))?;
        let conn = Connection::open(db_path)
            .with_context(|| format!("打开数据库失败: {}", db_path.display()))?;
        schema::apply_pragmas(&conn).context("设置 PRAGMA 失败")?;
        schema::migrate(&conn).context("schema 迁移失败")?;
        let secrets = secrets::select_backend(data_dir)?;
        Ok(Arc::new(Self {
            data_dir: data_dir.to_path_buf(),
            conn: Mutex::new(conn),
            secrets,
        }))
    }

    /// 指定 secret 后端打开（测试用，绕开钥匙串探测）
    #[cfg(test)]
    pub(crate) fn open_with_backend(
        data_dir: &Path,
        backend: Box<dyn secrets::SecretBackend>,
    ) -> anyhow::Result<Arc<Self>> {
        std::fs::create_dir_all(data_dir)?;
        let conn = Connection::open(data_dir.join("kiro.db"))?;
        schema::apply_pragmas(&conn)?;
        schema::migrate(&conn)?;
        Ok(Arc::new(Self {
            data_dir: data_dir.to_path_buf(),
            conn: Mutex::new(conn),
            secrets: backend,
        }))
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// secret 后端是否为系统钥匙串（供界面显示回退状态）
    pub fn secret_backend_is_system(&self) -> bool {
        self.secrets.is_system()
    }

    /// 凭据行数（首启导入判断用）
    pub fn credential_count(&self) -> anyhow::Result<i64> {
        let conn = self.conn.lock();
        let n = conn.query_row("SELECT COUNT(*) FROM credentials", [], |r| r.get(0))?;
        Ok(n)
    }

    /// 是否已存过配置（首启导入判断用）
    pub fn has_config(&self) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM config", [], |r| r.get(0))?;
        Ok(n > 0)
    }

    /// WAL checkpoint：把 WAL 内容合并回主库文件（两阶段退出阶段 1 调用）
    ///
    /// `PRAGMA wal_checkpoint(TRUNCATE)` 完成后 `-wal` 文件被截空，
    /// 备份与外部工具只需看主库文件。失败仅记日志：checkpoint 失败不
    /// 阻塞退出（数据仍在 WAL 里，下次打开自动恢复）。
    pub fn wal_checkpoint(store: &Arc<Self>) {
        let conn = store.conn.lock();
        match conn.pragma_update(None, "wal_checkpoint", "TRUNCATE") {
            Ok(()) => tracing::info!("SQLite WAL checkpoint 完成"),
            Err(e) => tracing::warn!("SQLite WAL checkpoint 失败（不影响退出）: {}", e),
        }
    }

    /// 钥匙串条目探测（测试专用：断言删除后的同步清理）
    #[cfg(test)]
    pub(crate) fn secret_present(&self, key: &str) -> bool {
        self.secrets.read(key).ok().flatten().is_some()
    }

    // ========================================================================
    // 模型目录（§7.3：落盘，重启后 /v1/models 不退回空目录）
    // ========================================================================

    /// 覆盖写指定凭据的模型目录
    pub fn save_model_catalog(
        &self,
        credential_id: u64,
        models: &[UpstreamModelInfo],
        refreshed_at: i64,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM model_catalog WHERE credential_id = ?1",
            [credential_id as i64],
        )?;
        for m in models {
            let info = serde_json::to_string(m).context("模型信息序列化失败")?;
            tx.execute(
                "INSERT INTO model_catalog (credential_id, model_id, info, refreshed_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![credential_id as i64, m.model_id, info, refreshed_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 读全部模型目录，按凭据分组
    pub fn load_model_catalog(&self) -> anyhow::Result<Vec<(u64, Vec<UpstreamModelInfo>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT credential_id, info FROM model_catalog ORDER BY credential_id, model_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;

        let mut grouped: BTreeMap<u64, Vec<UpstreamModelInfo>> = BTreeMap::new();
        for row in rows {
            let (id, info) = row?;
            match serde_json::from_str::<UpstreamModelInfo>(&info) {
                Ok(m) => grouped.entry(id as u64).or_default().push(m),
                Err(e) => tracing::warn!("模型目录行解析失败（已跳过）: {}", e),
            }
        }
        Ok(grouped.into_iter().collect())
    }

    // ========================================================================
    // 行为偏好（change 6：schema v2 `preferences` 表）
    // ========================================================================

    /// 读行为设置：未写库返回缺省值；读库失败记日志后同样兜底缺省
    ///（偏好读取不阻断托盘 / 关窗拦截等主路径）
    pub fn get_preference(&self, key: &str) -> bool {
        let default = prefs::default(key);
        let conn = self.conn.lock();
        let row: Option<String> = conn
            .query_row(
                "SELECT value FROM preferences WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or_else(|e| {
                tracing::warn!("读取偏好失败（{}），按缺省 {} 处理: {}", e, default, key);
                None
            });
        match row.as_deref() {
            Some("1") => true,
            Some("0") => false,
            None => default,
            Some(other) => {
                tracing::warn!("偏好值非法（{}={}），按缺省 {} 处理", key, other, default);
                default
            }
        }
    }

    /// 写行为设置（upsert）。调用方负责先完成伴随动作（如开机自启的
    /// 平台入口写入）再落库
    pub fn set_preference(&self, key: &str, value: bool) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO preferences (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, if value { "1" } else { "0" }],
        )?;
        Ok(())
    }
}

/// 损坏库备份：主文件 + WAL 附属一起挪走
fn move_corrupt_artifacts(db_path: &Path, backup: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let src = if suffix.is_empty() {
            db_path.to_path_buf()
        } else {
            PathBuf::from(format!("{}{}", db_path.display(), suffix))
        };
        if src.exists() {
            let dst = PathBuf::from(format!("{}{}", backup.display(), suffix));
            if let Err(e) = std::fs::rename(&src, &dst) {
                tracing::warn!("备份损坏文件失败 {:?}: {}", src, e);
            }
        }
    }
}

// ========================================================================
// CredentialStore
// ========================================================================

/// 取出凭据中非空的 secret 字段
///
/// 本期钥匙串只覆盖这四个字段（§7.4）：
/// `refresh_token` / `client_secret` / `kiro_api_key` / `proxy_password`
fn take_secrets(cred: &KiroCredentials) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(v) = &cred.refresh_token {
        out.push(("refresh_token", v.clone()));
    }
    if let Some(v) = &cred.client_secret {
        out.push(("client_secret", v.clone()));
    }
    if let Some(v) = &cred.kiro_api_key {
        out.push(("kiro_api_key", v.clone()));
    }
    if let Some(v) = &cred.proxy_password {
        out.push(("proxy_password", v.clone()));
    }
    out
}

/// 清空 secret 字段（落 `fields` 列前）
fn clear_secrets(cred: &mut KiroCredentials) {
    cred.refresh_token = None;
    cred.client_secret = None;
    cred.kiro_api_key = None;
    cred.proxy_password = None;
}

/// 按字段名回填 secret
fn set_secret(cred: &mut KiroCredentials, field: &str, value: String) {
    match field {
        "refresh_token" => cred.refresh_token = Some(value),
        "client_secret" => cred.client_secret = Some(value),
        "kiro_api_key" => cred.kiro_api_key = Some(value),
        "proxy_password" => cred.proxy_password = Some(value),
        _ => {}
    }
}

/// 钥匙串条目 key：`cred-<id>:<field>`（§7.4）
fn secret_key(credential_id: i64, field: &str) -> String {
    format!("cred-{}:{}", credential_id, field)
}

impl CredentialStore for SqliteStore {
    fn load(&self) -> anyhow::Result<LoadedCredentials> {
        let conn = self.conn.lock();

        let mut cred_stmt = conn.prepare(
            "SELECT id, fields FROM credentials ORDER BY priority ASC, id ASC",
        )?;
        let cred_rows = cred_stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;

        let mut creds: Vec<(i64, KiroCredentials)> = Vec::new();
        for row in cred_rows {
            let (id, fields) = row?;
            let mut cred: KiroCredentials = serde_json::from_str(&fields)
                .with_context(|| format!("凭据 #{} fields 解析失败", id))?;
            cred.id = Some(id as u64);
            creds.push((id, cred));
        }

        // 经钥匙串回填 secret。条目缺失/读取失败只禁用该凭据，不影响启动（§7.4）
        let mut secret_stmt = conn.prepare(
            "SELECT credential_id, field, keyring_ref FROM secrets",
        )?;
        let secret_rows = secret_stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;

        for row in secret_rows {
            let (cred_id, field, keyring_ref) = row?;
            let Some((_, cred)) = creds.iter_mut().find(|(id, _)| *id == cred_id) else {
                continue;
            };
            match self.secrets.read(&keyring_ref) {
                Ok(Some(value)) => set_secret(cred, &field, value),
                Ok(None) => {
                    tracing::warn!(
                        "凭据 #{} 的钥匙串条目缺失（{}），已禁用该凭据",
                        cred_id,
                        keyring_ref
                    );
                    cred.disabled = true;
                }
                Err(e) => {
                    tracing::warn!(
                        "凭据 #{} 的钥匙串读取失败（{}），已禁用该凭据",
                        cred_id,
                        e
                    );
                    cred.disabled = true;
                }
            }
        }

        Ok(LoadedCredentials {
            config: CredentialsConfig::Multiple(
                creds.into_iter().map(|(_, c)| c).collect(),
            ),
            needs_migration: false,
        })
    }

    fn persist(&self, credentials: &[KiroCredentials]) -> anyhow::Result<()> {
        let mut conn = self.conn.lock();

        // 无 id 的凭据先分配 id（钥匙串条目 key 依赖 id，必须先于落库确定）
        let max_id: i64 = conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM credentials", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        let mut next_id = max_id + 1;
        let creds: Vec<KiroCredentials> = credentials
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if c.id.is_none() {
                    c.id = Some(next_id as u64);
                    next_id += 1;
                }
                c
            })
            .collect();

        // 先写钥匙串（逐字段）。任何失败整体失败，SQLite 不落库（§7.4）
        for cred in &creds {
            let id = cred.id.expect("上方已补齐 id") as i64;
            for (field, value) in take_secrets(cred) {
                self.secrets
                    .write(&secret_key(id, field), &value)
                    .with_context(|| {
                        format!("钥匙串写入失败，凭据 #{} 字段 {} 未落库", id, field)
                    })?;
            }
        }

        // 记录被删除凭据的钥匙串条目，落库后尽力清理
        let new_ids: Vec<i64> = creds.iter().map(|c| c.id.unwrap() as i64).collect();
        let orphan_refs = collect_orphan_keyring_refs(&conn, &new_ids)?;

        let now = chrono::Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        if new_ids.is_empty() {
            tx.execute("DELETE FROM credentials", [])?;
        } else {
            let placeholders = vec!["?"; new_ids.len()].join(",");
            tx.execute(
                &format!("DELETE FROM credentials WHERE id NOT IN ({})", placeholders),
                rusqlite::params_from_iter(new_ids.iter()),
            )?;
        }

        for cred in &creds {
            let id = cred.id.unwrap() as i64;

            // 保留既有行的 created_at
            let created_at: Option<String> = tx
                .query_row(
                    "SELECT created_at FROM credentials WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            let created_at = created_at.unwrap_or_else(|| now.clone());

            let mut fields_cred = cred.clone();
            clear_secrets(&mut fields_cred);
            let fields = serde_json::to_string(&fields_cred).context("凭据序列化失败")?;

            tx.execute(
                "INSERT INTO credentials
                    (id, auth_method, profile, priority, disabled, endpoint,
                     api_region, auth_region, fields, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    auth_method = excluded.auth_method,
                    profile = excluded.profile,
                    priority = excluded.priority,
                    disabled = excluded.disabled,
                    endpoint = excluded.endpoint,
                    api_region = excluded.api_region,
                    auth_region = excluded.auth_region,
                    fields = excluded.fields,
                    updated_at = excluded.updated_at",
                params![
                    id,
                    cred.auth_method,
                    cred.nickname,
                    cred.priority,
                    cred.disabled,
                    cred.endpoint,
                    cred.api_region,
                    cred.auth_region,
                    fields,
                    created_at,
                    now,
                ],
            )?;

            // secrets 行：先清后插，保证与钥匙串条目一一对应
            tx.execute("DELETE FROM secrets WHERE credential_id = ?1", [id])?;
            for (field, _) in take_secrets(cred) {
                let key = secret_key(id, field);
                tx.execute(
                    "INSERT INTO secrets (credential_id, field, keyring_ref)
                     VALUES (?1, ?2, ?3)",
                    params![id, field, key],
                )?;
            }
        }
        tx.commit()?;

        // 悬空钥匙串条目清理：尽力而为，失败仅告警（条目无引用，无泄漏风险）
        for key in orphan_refs {
            if let Err(e) = self.secrets.delete(&key) {
                tracing::warn!("清理悬空钥匙串条目失败 ({}): {}", key, e);
            }
        }
        Ok(())
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.data_dir.clone())
    }
}

/// 收集不在新列表中的凭据对应的钥匙串条目 key
fn collect_orphan_keyring_refs(
    conn: &Connection,
    new_ids: &[i64],
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT credential_id, keyring_ref FROM secrets")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut orphans = Vec::new();
    for row in rows {
        let (id, key) = row?;
        if !new_ids.contains(&id) {
            orphans.push(key);
        }
    }
    Ok(orphans)
}

// ========================================================================
// ConfigStore
// ========================================================================

impl ConfigStore for SqliteStore {
    /// 无配置行时返回默认配置（与 `Config::load` 缺文件语义一致）。
    /// 返回的 `Config` 不带文件路径（`config_path` 为 `#[serde(skip)]`）。
    fn load(&self) -> anyhow::Result<Config> {
        let conn = self.conn.lock();
        let doc: Option<String> = conn
            .query_row("SELECT doc FROM config WHERE id = 1", [], |r| r.get(0))
            .optional()?;
        match doc {
            Some(d) => serde_json::from_str(&d).context("配置 JSON 解析失败"),
            None => Ok(Config::default()),
        }
    }

    fn save(&self, config: &Config) -> anyhow::Result<()> {
        let doc = serde_json::to_string_pretty(config).context("配置序列化失败")?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO config (id, doc) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET doc = excluded.doc",
            params![doc],
        )?;
        Ok(())
    }
}

// ========================================================================
// 模型目录回填与快照（设计文档 D7：桌面端专用，不动根仓调用链）
// ========================================================================

/// 从 SQLite 回填内存模型目录（`bootstrap` 之后调用）
pub fn restore_model_catalog(
    store: &SqliteStore,
    token_manager: &kiro_rs::kiro::token_manager::MultiTokenManager,
) {
    match store.load_model_catalog() {
        Ok(catalogs) => {
            let mut restored = 0usize;
            for (credential_id, models) in catalogs {
                if models.is_empty() {
                    continue;
                }
                token_manager.seed_model_cache(credential_id, models, None);
                restored += 1;
            }
            if restored > 0 {
                tracing::info!("已从 SQLite 回填 {} 个凭据的模型目录", restored);
            }
        }
        Err(e) => tracing::warn!("模型目录回填失败（不影响启动）: {}", e),
    }
}

/// 把内存模型目录快照落盘（后台任务周期调用）
pub fn snapshot_model_catalog(
    store: &SqliteStore,
    token_manager: &kiro_rs::kiro::token_manager::MultiTokenManager,
) {
    let ids: Vec<u64> = token_manager
        .snapshot()
        .entries
        .iter()
        .map(|e| e.id)
        .collect();
    let now = chrono::Utc::now().timestamp();
    for id in ids {
        let models = token_manager.credential_model_catalog(id);
        if models.is_empty() {
            continue;
        }
        if let Err(e) = store.save_model_catalog(id, &models, now) {
            tracing::warn!("模型目录落盘失败（凭据 #{}）: {}", id, e);
        }
    }
}
