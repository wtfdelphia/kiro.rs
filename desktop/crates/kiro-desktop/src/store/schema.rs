//! SQLite schema 与幂等迁移（设计文档 §7.3）
//!
//! 七表：`schema_version` / `credentials` / `secrets` / `config` / `stats` /
//! `balance_cache` / `model_catalog`。本期真正读写的只有 `credentials` /
//! `secrets` / `config` / `model_catalog`；`stats` 与 `balance_cache` 只建
//! 不用（运行时仍走 `cache_dir()` 派生的文件路径，收编留给后续 change）。
//!
//! change 6（desktop-tray-resident）：v2 新增 `preferences` 表，承载
//! 托盘常驻相关的三个行为开关（见 `store::mod` 的 `get_preference`）。

use rusqlite::Connection;

/// 当前 schema 版本。后续迁移在此递增并追加 DDL。
pub const SCHEMA_VERSION: i64 = 2;

/// 开库即执行的 PRAGMA：WAL + busy_timeout + NORMAL 同步 + 外键
pub fn apply_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// 幂等迁移：按 `schema_version` 逐版本执行，重复开库不报错不丢数据
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
    )?;

    let current: i64 = conn
        .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(V1_DDL)?;
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
    }

    if current < 2 {
        conn.execute_batch(V2_DDL)?;
        conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])?;
    }

    Ok(())
}

/// v1：七表 DDL
///
/// `credentials.fields` 存非敏感字段的整体 JSON（字段 20+ 且持续增长，
/// 逐列映射维护成本高于收益）；secret 字段不落此列，经钥匙串存
/// （`secrets` 表只存钥匙串条目引用）。
///
/// `secrets` 复合主键 `(credential_id, field)`：每凭据最多 4 个 secret
/// 字段（refresh_token / client_secret / kiro_api_key / proxy_password）。
const V1_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS credentials (
    id            INTEGER PRIMARY KEY,
    auth_method   TEXT NOT NULL,
    profile       TEXT,
    priority      INTEGER NOT NULL DEFAULT 100,
    disabled      INTEGER NOT NULL DEFAULT 0,
    endpoint      TEXT,
    api_region    TEXT,
    auth_region   TEXT,
    fields        TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS secrets (
    credential_id INTEGER NOT NULL REFERENCES credentials(id) ON DELETE CASCADE,
    field         TEXT NOT NULL,
    keyring_ref   TEXT NOT NULL,
    PRIMARY KEY (credential_id, field)
);

CREATE TABLE IF NOT EXISTS config (
    id  INTEGER PRIMARY KEY CHECK (id = 1),
    doc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS stats (
    credential_id   INTEGER PRIMARY KEY REFERENCES credentials(id) ON DELETE CASCADE,
    success_count   INTEGER NOT NULL DEFAULT 0,
    failure_count   INTEGER NOT NULL DEFAULT 0,
    last_used_at    TEXT
);

CREATE TABLE IF NOT EXISTS balance_cache (
    credential_id   INTEGER PRIMARY KEY REFERENCES credentials(id) ON DELETE CASCADE,
    snapshot        TEXT NOT NULL,
    fetched_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS model_catalog (
    credential_id   INTEGER NOT NULL REFERENCES credentials(id) ON DELETE CASCADE,
    model_id        TEXT NOT NULL,
    info            TEXT NOT NULL,
    refreshed_at    INTEGER NOT NULL,
    PRIMARY KEY (credential_id, model_id)
);
"#;

/// v2：行为偏好表（change 6 托盘常驻）
///
/// 键值对，值为字符串化的布尔（"1" / "0"）。缺省项不写库，读取时由
/// 调用方兜底（见 `get_preference`）。
const V2_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS preferences (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        apply_pragmas(&conn).unwrap();
        conn
    }

    #[test]
    fn migrate_creates_all_tables() {
        let conn = in_memory();
        migrate(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN (
                    'schema_version','credentials','secrets','config',
                    'stats','balance_cache','model_catalog','preferences')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 8);

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = in_memory();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();

        // 重复迁移不产生重复版本号行
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn migrate_preserves_data_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiro.db");

        {
            let conn = Connection::open(&path).unwrap();
            apply_pragmas(&conn).unwrap();
            migrate(&conn).unwrap();
            conn.execute(
                "INSERT INTO config (id, doc) VALUES (1, '{\"port\":9001}')",
                [],
            )
            .unwrap();
        }

        // 重开：迁移幂等，数据仍在
        let conn = Connection::open(&path).unwrap();
        apply_pragmas(&conn).unwrap();
        migrate(&conn).unwrap();
        let doc: String = conn
            .query_row("SELECT doc FROM config WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(doc, "{\"port\":9001}");
    }

    /// v1 → v2：旧库重开只补 `preferences`，已有数据不动
    #[test]
    fn migrate_v1_to_v2_keeps_data() {
        let conn = in_memory();
        // 手工造一个 v1 库：schema_version 表 + V1_DDL，版本号写 1
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
        )
        .unwrap();
        conn.execute_batch(V1_DDL).unwrap();
        conn.execute("INSERT INTO schema_version (version) VALUES (1)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO config (id, doc) VALUES (1, '{\"port\":9001}')",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let doc: String = conn
            .query_row("SELECT doc FROM config WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(doc, "{\"port\":9001}");
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }
}
