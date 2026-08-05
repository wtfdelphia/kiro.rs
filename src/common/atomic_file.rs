//! 原子文件写入
//!
//! 直接 `fs::write` 覆写目标文件时，进程在写入中途死亡会留下截断的文件。
//! 凭据文件被截断意味着所有账号不可用，且用户无从判断原因。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// 同目录临时文件路径（同目录是 rename 原子性的前提，跨设备 rename 会失败）
fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tmp".to_string());
    let tmp_name = format!(".{}.tmp-{}", file_name, std::process::id());
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// 原子写入：写临时文件 → 替换目标
///
/// 失败时清理临时文件并保留原目标文件内容不变。
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let tmp = temp_path_for(path);

    if let Err(e) = std::fs::write(&tmp, contents) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("写入临时文件失败: {:?}", tmp));
    }

    // Windows 下 std::fs::rename 对同目录已存在的目标文件可覆盖（MoveFileEx
    // 带 MOVEFILE_REPLACE_EXISTING）。该行为由 write_atomic_replaces_existing
    // 测试实际验证，而非依赖假设。
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("原子替换失败: {:?}", path));
    }

    Ok(())
}

/// 备份文件到同目录，返回备份路径
///
/// 备份名含 UTC 时间戳，便于操作者识别与手工恢复。
pub fn backup_file(path: &Path, suffix: &str) -> Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "credentials.json".to_string());
    let backup_name = format!("{file_name}.{suffix}-{stamp}");
    let backup = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(backup_name),
        _ => PathBuf::from(backup_name),
    };

    std::fs::copy(path, &backup)
        .with_context(|| format!("备份 {:?} 到 {:?} 失败", path, backup))?;

    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kiro-rs-atomic-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_atomic_creates_new_file() {
        let dir = temp_dir();
        let target = dir.join("credentials.json");

        write_atomic(&target, "[]").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "[]");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_replaces_existing() {
        // 关键平台行为：Windows 下 rename 覆盖同目录已存在文件必须成功。
        // 这条不能靠假设，必须实际运行。
        let dir = temp_dir();
        let target = dir.join("credentials.json");
        std::fs::write(&target, "old-content").unwrap();

        write_atomic(&target, "new-content").unwrap();
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "new-content",
            "原子替换必须覆盖已存在的目标文件"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_leaves_no_temp_file_on_success() {
        let dir = temp_dir();
        let target = dir.join("credentials.json");
        write_atomic(&target, "content").unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "残留临时文件: {leftovers:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_preserves_original_on_failure() {
        let dir = temp_dir();
        let target = dir.join("credentials.json");
        std::fs::write(&target, "original").unwrap();

        // 目标路径的父目录不存在 → 临时文件写入失败
        let bad_target = dir.join("no-such-subdir").join("credentials.json");
        assert!(write_atomic(&bad_target, "x").is_err());

        // 原文件不受影响
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backup_file_copies_content() {
        let dir = temp_dir();
        let target = dir.join("credentials.json");
        std::fs::write(&target, "to-be-backed-up").unwrap();

        let backup = backup_file(&target, "kam-backup").unwrap();
        assert!(backup.exists());
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "to-be-backed-up"
        );
        // 原文件保留
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "to-be-backed-up");
        // 备份名可识别
        let name = backup.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("credentials.json.kam-backup-"), "名称: {name}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backup_file_errors_when_source_missing() {
        let dir = temp_dir();
        let missing = dir.join("nope.json");
        assert!(backup_file(&missing, "kam-backup").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
