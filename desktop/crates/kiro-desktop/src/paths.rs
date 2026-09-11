//! 数据目录解析（设计文档 §9.1 / §9.3）
//!
//! 桌面端数据目录统一为 `<data_dir>/kiro-rs/`：
//! Linux `$XDG_DATA_HOME`（缺省 `~/.local/share`）、macOS
//! `~/Library/Application Support`、Windows `%APPDATA%`。
//! 环境变量 `KIRO_RS_DATA_DIR` 可整体覆盖（测试与调试用）。

use std::ffi::OsString;
use std::path::PathBuf;

/// 解析桌面数据目录（读取真实环境变量）
pub fn data_dir() -> PathBuf {
    resolve(
        std::env::var_os("KIRO_RS_DATA_DIR"),
        std::env::var_os("HOME"),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("APPDATA"),
    )
}

/// 纯函数解析，便于单测
fn resolve(
    override_dir: Option<OsString>,
    home: Option<OsString>,
    xdg_data_home: Option<OsString>,
    appdata: Option<OsString>,
) -> PathBuf {
    if let Some(dir) = override_dir
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }

    let home = home.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

    #[cfg(target_os = "macos")]
    {
        let _ = (xdg_data_home, appdata);
        return home.join("Library").join("Application Support").join("kiro-rs");
    }

    #[cfg(target_os = "windows")]
    {
        let _ = xdg_data_home;
        let base = appdata
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"));
        return base.join("kiro-rs");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = appdata;
        let base = xdg_data_home
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"));
        base.join("kiro-rs")
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn env_override_wins() {
        let got = resolve(
            Some("/tmp/kiro-data".into()),
            Some("/home/u".into()),
            None,
            None,
        );
        assert_eq!(got, std::path::PathBuf::from("/tmp/kiro-data"));
    }

    #[test]
    fn empty_override_falls_through() {
        let got = resolve(
            Some("".into()),
            Some("/home/u".into()),
            Some("/xdg".into()),
            None,
        );
        // Linux 分支：XDG_DATA_HOME 生效
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(got, std::path::PathBuf::from("/xdg/kiro-rs"));
        #[cfg(target_os = "macos")]
        assert_eq!(got, std::path::PathBuf::from("/home/u/Library/Application Support/kiro-rs"));
    }

    #[test]
    fn xdg_default() {
        let got = resolve(None, Some("/home/u".into()), None, None);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(got, std::path::PathBuf::from("/home/u/.local/share/kiro-rs"));
    }
}
