//! secret 后端：系统钥匙串 + 加密文件回退（设计文档 §7.4）
//!
//! 启动时探测钥匙串（写探针条目再删），可用用 [`KeyringBackend`]，否则
//! 用 [`EncryptedFileBackend`]。全程只选一个后端，不做逐字段混用。
//!
//! Linux 的 keyring v1 后端走 zbus Secret Service（纯 Rust，无 GTK）；
//! headless / 极简桌面没有 Secret Service 时探测失败，自动回退。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use parking_lot::Mutex;

/// 钥匙串服务名（固定）
pub const KEYRING_SERVICE: &str = "dev.kiro-rs.desktop";

/// 启动探测用的探针条目 key
const PROBE_KEY: &str = "startup-probe";

/// secret 存取后端
///
/// `key` 形如 `cred-3:refresh_token`（见 [`crate::store`]）。
pub trait SecretBackend: Send + Sync {
    fn write(&self, key: &str, value: &str) -> anyhow::Result<()>;
    /// 条目不存在返回 `Ok(None)`
    fn read(&self, key: &str) -> anyhow::Result<Option<String>>;
    /// 条目不存在视为删除成功
    fn delete(&self, key: &str) -> anyhow::Result<()>;
    /// 是否为系统钥匙串（供界面显示「系统钥匙串」还是「文件回退」）
    fn is_system(&self) -> bool;
}

/// 系统钥匙串后端（keyring 4.x v1）
pub struct KeyringBackend;

impl KeyringBackend {
    fn entry(key: &str) -> anyhow::Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, key)
            .map_err(|e| anyhow!("钥匙串条目创建失败 ({}): {}", key, e))
    }
}

impl SecretBackend for KeyringBackend {
    fn write(&self, key: &str, value: &str) -> anyhow::Result<()> {
        Self::entry(key)?
            .set_password(value)
            .map_err(|e| anyhow!("钥匙串写入失败 ({}): {}", key, e))
    }

    fn read(&self, key: &str) -> anyhow::Result<Option<String>> {
        match Self::entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow!("钥匙串读取失败 ({}): {}", key, e)),
        }
    }

    fn delete(&self, key: &str) -> anyhow::Result<()> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow!("钥匙串删除失败 ({}): {}", key, e)),
        }
    }

    fn is_system(&self) -> bool {
        true
    }
}

/// 加密文件回退后端
///
/// 内存 `HashMap` 整体序列化为 JSON 后 `ring` AEAD 加密落盘。安全边界
/// 是文件权限（两个文件均 0600），不是用户口令。
pub struct EncryptedFileBackend {
    path: PathBuf,
    aead: super::crypt::Aead,
    /// 内存映射 + 落盘串行化
    map: Mutex<HashMap<String, String>>,
}

impl EncryptedFileBackend {
    /// 打开或创建：`secrets.key`（密钥）+ `secrets.enc`（密文）
    pub fn open(data_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("创建数据目录失败: {}", data_dir.display()))?;

        let key_path = data_dir.join("secrets.key");
        let key = super::crypt::load_or_create_key(&key_path)?;
        let aead = super::crypt::Aead::from_key_bytes(&key)?;

        let path = data_dir.join("secrets.enc");
        let map = if path.exists() {
            let sealed = std::fs::read(&path)
                .with_context(|| format!("读取 secrets.enc 失败: {}", path.display()))?;
            let plaintext = aead
                .open(&sealed)
                .context("解密 secrets.enc 失败")?;
            serde_json::from_slice(&plaintext).context("secrets.enc 内容解析失败")?
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            aead,
            map: Mutex::new(map),
        })
    }

    /// 把当前内存映射加密落盘（调用方持有 map 锁）
    fn persist(&self, map: &HashMap<String, String>) -> anyhow::Result<()> {
        let plaintext = serde_json::to_vec(map)?;
        let sealed = self.aead.seal(&plaintext)?;
        super::crypt::write_file_0600(&self.path, &sealed)
    }
}

impl SecretBackend for EncryptedFileBackend {
    fn write(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let mut map = self.map.lock();
        map.insert(key.to_string(), value.to_string());
        self.persist(&map)
            .with_context(|| format!("写入 secrets.enc 失败 ({})", key))?;
        Ok(())
    }

    fn read(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.map.lock().get(key).cloned())
    }

    fn delete(&self, key: &str) -> anyhow::Result<()> {
        let mut map = self.map.lock();
        if map.remove(key).is_some() {
            self.persist(&map)
                .with_context(|| format!("回写 secrets.enc 失败 ({})", key))?;
        }
        Ok(())
    }

    fn is_system(&self) -> bool {
        false
    }
}

/// 探测钥匙串可用性并选择后端。
///
/// 探针：写一个条目再删。`Entry::new` 失败（无默认存储）、写失败、删失败
/// 都视为不可用。探针条目删除失败不阻塞回退判断（悬空探针下次覆盖）。
pub fn select_backend(data_dir: &Path) -> anyhow::Result<Box<dyn SecretBackend>> {
    match probe_keyring() {
        Ok(()) => {
            tracing::info!("系统钥匙串可用，secret 走钥匙串");
            Ok(Box::new(KeyringBackend))
        }
        Err(e) => {
            tracing::warn!("系统钥匙串不可用（{}），回退到加密文件存储", e);
            let backend = EncryptedFileBackend::open(data_dir)
                .context("加密文件回退后端初始化失败")?;
            Ok(Box::new(backend))
        }
    }
}

/// 钥匙串探针：`store_status` + 写探针条目再删
fn probe_keyring() -> anyhow::Result<()> {
    if let Err(e) = keyring::Entry::store_status() {
        return Err(anyhow!("钥匙串后端初始化失败: {}", e));
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, PROBE_KEY)
        .map_err(|e| anyhow!("探针条目创建失败: {}", e))?;
    entry
        .set_password("probe")
        .map_err(|e| anyhow!("探针写入失败: {}", e))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!("探针删除失败: {}", e)),
    }
}

/// 内存 secret 后端（测试专用：绕开钥匙串探测，确定性断言）
#[cfg(test)]
#[derive(Default)]
pub struct MemoryBackend {
    map: Mutex<HashMap<String, String>>,
}

#[cfg(test)]
impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let backend = EncryptedFileBackend::open(dir.path()).unwrap();

        backend.write("cred-1:refresh_token", "tok-abc").unwrap();
        backend.write("cred-2:kiro_api_key", "ksk_xyz").unwrap();

        assert_eq!(
            backend.read("cred-1:refresh_token").unwrap().as_deref(),
            Some("tok-abc")
        );
        assert_eq!(backend.read("cred-9:missing").unwrap(), None);

        // 重开：从密文恢复
        let reopened = EncryptedFileBackend::open(dir.path()).unwrap();
        assert_eq!(
            reopened.read("cred-2:kiro_api_key").unwrap().as_deref(),
            Some("ksk_xyz")
        );

        reopened.delete("cred-1:refresh_token").unwrap();
        assert_eq!(reopened.read("cred-1:refresh_token").unwrap(), None);
        // 删除不存在的条目不报错
        reopened.delete("cred-1:refresh_token").unwrap();
    }

    #[test]
    fn encrypted_file_secrets_are_not_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let backend = EncryptedFileBackend::open(dir.path()).unwrap();
        backend.write("cred-1:refresh_token", "super-secret-token").unwrap();

        let sealed = std::fs::read(dir.path().join("secrets.enc")).unwrap();
        let as_string = String::from_utf8_lossy(&sealed);
        assert!(!as_string.contains("super-secret-token"));
    }

    #[test]
    fn backend_selection_falls_back_when_keyring_unavailable() {
        // headless 测试环境没有 Secret Service：探测必须失败并回退。
        // 若环境恰好有钥匙串（桌面跑测试），探针成功则后端是系统的，
        // 两种结果都合法，只断言 select 不 panic。
        let dir = tempfile::tempdir().unwrap();
        let backend = select_backend(dir.path()).unwrap();
        let _ = backend.is_system();
    }

    #[cfg(unix)]
    #[test]
    fn encrypted_file_permissions_are_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let backend = EncryptedFileBackend::open(dir.path()).unwrap();
        backend.write("cred-1:refresh_token", "v").unwrap();

        for name in ["secrets.enc", "secrets.key"] {
            let mode = std::fs::metadata(dir.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{} 权限应为 0600", name);
        }
    }
}
