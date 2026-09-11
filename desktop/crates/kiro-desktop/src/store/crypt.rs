//! 加密文件回退的 AEAD 封装（设计文档 §7.4）
//!
//! `ring` AES-256-GCM：12 字节随机 nonce 前置 + 密文 + 16 字节 tag。
//! 密钥是首启随机生成的 32 字节，落盘为 `secrets.key`（0600）。不套
//! pbkdf2：密钥本身就在本地文件里，口令派生只是多一层无收益的仪式。
//! 这条回退的安全边界是文件权限，不是用户口令。

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, anyhow};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// 持有一个 AES-256-GCM 密钥的加密封装
pub struct Aead {
    key: LessSafeKey,
}

impl Aead {
    /// 从 32 字节原始密钥构造
    pub fn from_key_bytes(bytes: &[u8; KEY_LEN]) -> anyhow::Result<Self> {
        let unbound =
            UnboundKey::new(&AES_256_GCM, bytes).map_err(|_| anyhow!("AEAD 密钥构造失败"))?;
        Ok(Self {
            key: LessSafeKey::new(unbound),
        })
    }

    /// 加密：返回 `nonce(12) || ciphertext+tag`
    pub fn seal(&self, plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| anyhow!("随机数生成失败"))?;

        let mut buf = Vec::with_capacity(NONCE_LEN + plaintext.len() + 16);
        buf.extend_from_slice(&nonce_bytes);
        buf.extend_from_slice(plaintext);

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let tag = self
            .key
            .seal_in_place_separate_tag(nonce, Aad::empty(), &mut buf[NONCE_LEN..])
            .map_err(|_| anyhow!("AEAD 加密失败"))?;
        buf.extend_from_slice(tag.as_ref());
        Ok(buf)
    }

    /// 解密：输入 `nonce(12) || ciphertext+tag`
    pub fn open(&self, sealed: &[u8]) -> anyhow::Result<Vec<u8>> {
        if sealed.len() < NONCE_LEN + 16 {
            anyhow::bail!("密文长度非法");
        }
        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes.copy_from_slice(&sealed[..NONCE_LEN]);

        let mut buf = sealed[NONCE_LEN..].to_vec();
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let plaintext = self
            .key
            .open_in_place(nonce, Aad::empty(), &mut buf)
            .map_err(|_| anyhow!("AEAD 解密失败（密文被篡改或密钥不匹配）"))?;
        Ok(plaintext.to_vec())
    }
}

/// 生成 32 字节随机密钥
pub fn generate_key() -> anyhow::Result<[u8; KEY_LEN]> {
    let rng = SystemRandom::new();
    let mut key = [0u8; KEY_LEN];
    rng.fill(&mut key)
        .map_err(|_| anyhow!("随机数生成失败"))?;
    Ok(key)
}

/// 加载或创建密钥文件（0600）。
///
/// 文件存在则读入；不存在则随机生成并落盘。读入失败（长度不对等）直接
/// 报错，不静默重建：重建意味着已有密文全部解不开。
pub fn load_or_create_key(path: &Path) -> anyhow::Result<[u8; KEY_LEN]> {
    if path.exists() {
        let bytes =
            fs::read(path).with_context(|| format!("读取密钥文件失败: {}", path.display()))?;
        let arr: [u8; KEY_LEN] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("密钥文件长度非法: {}", path.display()))?;
        return Ok(arr);
    }

    let key = generate_key()?;
    write_file_0600(path, &key)?;
    Ok(key)
}

/// 写文件并设 0600 权限（Unix）
pub fn write_file_0600(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    let mut f = fs::File::create(path)
        .with_context(|| format!("创建文件失败: {}", path.display()))?;
    f.write_all(content)?;
    f.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("设置 0600 权限失败: {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = generate_key().unwrap();
        let aead = Aead::from_key_bytes(&key).unwrap();

        let plaintext = b"refresh_token=secret-value";
        let sealed = aead.seal(plaintext).unwrap();
        assert_ne!(&sealed[NONCE_LEN..], plaintext.as_slice());

        let opened = aead.open(&sealed).unwrap();
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = generate_key().unwrap();
        let aead = Aead::from_key_bytes(&key).unwrap();

        let mut sealed = aead.seal(b"payload").unwrap();
        let idx = sealed.len() - 1;
        sealed[idx] ^= 0xff;
        assert!(aead.open(&sealed).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let a1 = Aead::from_key_bytes(&generate_key().unwrap()).unwrap();
        let a2 = Aead::from_key_bytes(&generate_key().unwrap()).unwrap();
        let sealed = a1.seal(b"payload").unwrap();
        assert!(a2.open(&sealed).is_err());
    }

    #[test]
    fn load_or_create_key_persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");

        let k1 = load_or_create_key(&path).unwrap();
        assert!(path.exists());
        let k2 = load_or_create_key(&path).unwrap();
        assert_eq!(k1, k2);
    }

    #[cfg(unix)]
    #[test]
    fn key_file_permissions_are_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");
        load_or_create_key(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn corrupted_key_file_errors_not_silently_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");
        fs::write(&path, b"too-short").unwrap();
        assert!(load_or_create_key(&path).is_err());
    }
}
