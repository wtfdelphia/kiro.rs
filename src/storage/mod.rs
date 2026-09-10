//! 存储接缝
//!
//! 凭据与配置的读写统一收口到 trait，CLI（JSON 文件）与桌面端（SQLite，
//! 见 `docs/desktop-gpui-embedded-design.md` change 3）各自挂实现。
//! CLI 侧实现必须保持现有 JSON 文件行为不变。

use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::kiro::model::credentials::{CredentialsConfig, KiroCredentials, LoadedCredentials};
use crate::model::config::Config;

/// 凭据存储
pub trait CredentialStore: Send + Sync {
    /// 加载凭据配置。语义与 [`CredentialsConfig::load_detailed`] 一致：
    /// 后端源缺失或为空返回空多凭据配置，不是错误。
    fn load(&self) -> anyhow::Result<LoadedCredentials>;

    /// 回写凭据列表。是否调用由上层按格式语义决定（仅多凭据格式回写）。
    fn persist(&self, credentials: &[KiroCredentials]) -> anyhow::Result<()>;

    /// 缓存目录（统计等派生路径的锚点）。无后端源时返回 None。
    fn cache_dir(&self) -> Option<PathBuf>;

    /// 若加载结果需要格式迁移，写回原生格式并返回备份位置。
    /// 无迁移概念的实现保持默认（不操作）。
    fn migrate_to_native(&self, _config: &CredentialsConfig) -> anyhow::Result<Option<PathBuf>> {
        Ok(None)
    }
}

/// 配置存储
pub trait ConfigStore: Send + Sync {
    fn load(&self) -> anyhow::Result<Config>;
    fn save(&self, config: &Config) -> anyhow::Result<()>;
}

/// JSON 文件凭据存储（CLI 默认）
pub struct JsonCredentialStore {
    credentials_path: PathBuf,
}

impl JsonCredentialStore {
    pub fn new(credentials_path: impl Into<PathBuf>) -> Self {
        Self {
            credentials_path: credentials_path.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.credentials_path
    }
}

impl CredentialStore for JsonCredentialStore {
    fn load(&self) -> anyhow::Result<LoadedCredentials> {
        CredentialsConfig::load_detailed(&self.credentials_path)
    }

    fn persist(&self, credentials: &[KiroCredentials]) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(credentials).context("序列化凭据失败")?;
        let path = &self.credentials_path;

        // 原子写入（在 Tokio runtime 内使用 block_in_place 避免阻塞 worker）
        //
        // 与迁移路径共用同一个原子写工具：在同一个文件上同时存在原子与非原子两条
        // 写路径，比两者都不原子更糟——读者无法判断当前内容出自哪条路径。
        let write = || crate::common::atomic_file::write_atomic(path, &json);
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(write)
                .with_context(|| format!("回写凭据文件失败: {:?}", path))?;
        } else {
            write().with_context(|| format!("回写凭据文件失败: {:?}", path))?;
        }

        tracing::debug!("已回写凭据到文件: {:?}", path);
        Ok(())
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        self.credentials_path.parent().map(|d| d.to_path_buf())
    }

    fn migrate_to_native(&self, config: &CredentialsConfig) -> anyhow::Result<Option<PathBuf>> {
        let backup = CredentialsConfig::migrate_to_native(&self.credentials_path, config)?;
        Ok(Some(backup))
    }
}

/// JSON 文件配置存储（CLI 默认）
pub struct JsonConfigStore {
    config_path: PathBuf,
}

impl JsonConfigStore {
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_path.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.config_path
    }
}

impl ConfigStore for JsonConfigStore {
    /// 文件不存在时返回默认配置（与 [`Config::load`] 语义一致）
    fn load(&self) -> anyhow::Result<Config> {
        Config::load(&self.config_path)
    }

    fn save(&self, config: &Config) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(config).context("序列化配置失败")?;
        std::fs::write(&self.config_path, content)
            .with_context(|| format!("写入配置文件失败: {}", self.config_path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kiro-rs-storage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn json_credential_store_load_missing_returns_empty_multiple() {
        let dir = temp_dir();
        let store = JsonCredentialStore::new(dir.join("credentials.json"));

        let loaded = store.load().unwrap();
        assert!(loaded.config.is_multiple());
        assert!(!loaded.needs_migration);
        assert_eq!(loaded.config.into_sorted_credentials().len(), 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn json_credential_store_roundtrip() {
        let dir = temp_dir();
        let path = dir.join("credentials.json");
        let store = JsonCredentialStore::new(&path);

        // api_key 类型凭据：校验只要求 kiro_api_key 非空
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("api_key".to_string());
        cred.kiro_api_key = Some("test-key-roundtrip".to_string());
        cred.email = Some("roundtrip@example.com".to_string());
        store.persist(&[cred.clone()]).unwrap();

        let loaded = store.load().unwrap();
        let creds = loaded.config.into_sorted_credentials();
        assert_eq!(creds.len(), 1);
        // 数字 id 不在加载路径读取（normalize_record 既有行为，
        // 由 MultiTokenManager::new 重新分配），此处断言可存活字段
        assert_eq!(creds[0].id, None);
        assert_eq!(creds[0].kiro_api_key.as_deref(), Some("test-key-roundtrip"));
        assert_eq!(creds[0].email.as_deref(), Some("roundtrip@example.com"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn json_credential_store_cache_dir_is_parent() {
        let dir = temp_dir();
        let store = JsonCredentialStore::new(dir.join("credentials.json"));
        assert_eq!(store.cache_dir().unwrap(), dir);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn json_config_store_roundtrip_preserves_path_semantics() {
        let dir = temp_dir();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"host":"127.0.0.1","port":9001}"#).unwrap();

        let store = JsonConfigStore::new(&path);
        let mut config = store.load().unwrap();
        assert_eq!(config.port, 9001);

        config.port = 9002;
        store.save(&config).unwrap();

        let reloaded = store.load().unwrap();
        assert_eq!(reloaded.port, 9002);
        assert_eq!(reloaded.config_path().unwrap(), path);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn json_config_store_load_missing_returns_default_with_path() {
        let dir = temp_dir();
        let path = dir.join("no-such-config.json");
        let store = JsonConfigStore::new(&path);

        let config = store.load().unwrap();
        assert_eq!(config.config_path().unwrap(), path);
        assert_eq!(config.host, Config::default().host);

        std::fs::remove_dir_all(&dir).ok();
    }
}
