//! 单实例锁（设计文档 §9.2）
//!
//! 在数据目录创建 `kiro-desktop.lock` 并加独占非阻塞文件锁：
//! 拿到锁写入 PID 正常启动；拿不到说明已有实例，提示并退出。
//! 锁文件不删除（flock 随进程释放，删除有竞态）。
//! 本期只实现 unix 分支；Windows 分支在 change 7 补。

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 单实例锁句柄（持有期间锁有效，drop 即释放）
pub struct InstanceLock {
    pub path: PathBuf,
    /// flock 句柄：只持有不读取，drop 时随文件关闭释放锁
    #[allow(dead_code)]
    file: File,
}

impl InstanceLock {
    /// 尝试获取数据目录下的单实例锁
    ///
    /// - `Ok(lock)`：拿锁成功，PID 已写入
    /// - `Err(None)`：锁被其他实例持有
    /// - `Err(Some(e))`：拿锁过程出现 IO 错误（目录不存在等）
    pub fn acquire(data_dir: &Path) -> Result<InstanceLock, Option<std::io::Error>> {
        let path = data_dir.join("kiro-desktop.lock");
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&path)
            .map_err(Some)?;

        file.try_lock().map_err(|e| {
            let io: std::io::Error = e.into();
            if io.kind() == std::io::ErrorKind::WouldBlock {
                None
            } else {
                Some(io)
            }
        })?;

        // 写 PID 便于排查（锁文件保留，不删）。
        // 先截断再写：新 PID 位数可能比上次短（12345 -> 987），
        // 不截断会残留旧字节。不能在 open 时加 .truncate(true)——
        // 那会在别的进程持锁时抹掉它的 PID。拿锁成功后才截断。
        file.set_len(0).map_err(Some)?;
        let mut f = &file;
        let _ = write!(f, "{}", std::process::id());

        Ok(InstanceLock { path, file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kiro-desktop-lock-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn acquire_twice_in_same_process_fails() {
        let dir = temp_dir();
        let first = InstanceLock::acquire(&dir).expect("首次拿锁应成功");
        assert!(first.path.ends_with("kiro-desktop.lock"));

        // 同进程重复拿同一文件锁：flock 语义下失败（WouldBlock）
        let second = InstanceLock::acquire(&dir);
        assert!(second.is_err(), "重复拿锁必须失败");
        assert!(second.err().unwrap().is_none(), "应为锁被占用而非 IO 错误");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn release_allows_reacquire() {
        let dir = temp_dir();
        let first = InstanceLock::acquire(&dir).expect("首次拿锁应成功");
        drop(first);

        let second = InstanceLock::acquire(&dir);
        assert!(second.is_ok(), "释放后应能重新拿锁");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_dir_is_io_error() {
        let dir = std::env::temp_dir().join("kiro-desktop-lock-no-such-dir");
        let result = InstanceLock::acquire(&dir);
        assert!(result.is_err());
        assert!(result.err().unwrap().is_some(), "目录缺失应为 IO 错误");
    }

    #[test]
    fn pid_is_written() {
        let dir = temp_dir();
        let lock = InstanceLock::acquire(&dir).expect("拿锁应成功");
        let content = std::fs::read_to_string(&lock.path).unwrap();
        assert_eq!(content, std::process::id().to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stale_longer_pid_is_truncated() {
        let dir = temp_dir();
        let lock_path = dir.join("kiro-desktop.lock");
        // 预写一个比当前 PID 长的旧内容，模拟上次运行的残留
        std::fs::write(&lock_path, "9999999999").unwrap();

        let lock = InstanceLock::acquire(&dir).expect("拿锁应成功");
        let content = std::fs::read_to_string(&lock.path).unwrap();
        assert_eq!(
            content,
            std::process::id().to_string(),
            "写新 PID 前必须截断旧内容，不能残留旧字节"
        );
        drop(lock);
        std::fs::remove_dir_all(&dir).ok();
    }
}
