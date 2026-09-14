//! 开机自启入口管理（change 6，设计文档 §6.4 / design D5）
//!
//! 三平台纯文件 / 注册表操作，无需提权：
//! - Linux：`$XDG_CONFIG_HOME`（缺省 `~/.config`）`/autostart/dev.kiro-rs.desktop`
//! - macOS：`~/Library/LaunchAgents/dev.kiro-rs.desktop.plist`（`RunAtLoad`）
//! - Windows：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 的 `kiro-rs` 键
//!
//! 内容生成抽纯函数（只依赖路径与程序名），单测覆盖字段。写删失败只
//! 报错误不 panic；调用方（设置视图）在入口操作成功后才落库
//! `launch_at_login`。

use std::path::PathBuf;

/// desktop entry / plist / 注册表键共用的身份标识
pub const APP_ID: &str = "dev.kiro-rs";

/// 设置「开机自启」：写入口（幂等覆盖）；`false` 则删入口
pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
    if enabled {
        write_entry()
    } else {
        remove_entry()
    }
}

/// 入口路径（Linux / macOS 为文件；Windows 无文件路径，返回 `None`）
fn entry_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|h| h.join("Library/LaunchAgents/dev.kiro-rs.desktop.plist"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home_dir().map(|h| h.join(".config")))?;
        Some(config.join("autostart").join(format!("{}.desktop", APP_ID)))
    }
    #[cfg(target_os = "windows")]
    {
        None
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Linux desktop entry 内容（纯函数，单测覆盖）
///
/// `Terminal=false`：桌面端是 GUI；`X-GNOME-Autostart-enabled=true`
/// 兼容 GNOME 的自启开关读取。
pub fn desktop_entry_content(exec_path: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=kiro-rs\n\
         Comment=kiro-rs 桌面端（代理服务器常驻托盘）\n\
         Exec={}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        quote_exec_arg(exec_path)
    )
}

/// desktop entry `Exec` 参数引用：含空格 / 制表符时整体加双引号，
/// 内部双引号、反斜杠、美元符按规范转义
fn quote_exec_arg(arg: &str) -> String {
    if arg.is_empty() || arg.contains([' ', '\t']) {
        let escaped: String = arg
            .chars()
            .flat_map(|c| match c {
                '"' => vec!['\\', '"'],
                '\\' => vec!['\\', '\\'],
                '$' => vec!['\\', '$'],
                '`' => vec!['\\', '`'],
                _ => vec![c],
            })
            .collect();
        format!("\"{}\"", escaped)
    } else {
        arg.to_string()
    }
}

/// macOS LaunchAgent plist 内容（纯函数，单测覆盖）
#[cfg(any(target_os = "macos", test))]
pub fn macos_plist_content(exec_path: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{}</string>\n\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>KeepAlive</key>\n\
         \t<false/>\n\
         </dict>\n\
         </plist>\n",
        APP_ID,
        xml_escape(exec_path)
    )
}

/// plist `<string>` 内容的 XML 转义（路径含 & / < / > 时避免产出非法文档）
#[cfg(any(target_os = "macos", test))]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Windows `Run` 键的值：整体加双引号。
///
/// 该值按 CreateProcess 命令行规则解析，遇到空格即截断可执行文件名：
/// `C:\Program Files\...` 裸写会在 `C:\Program` 处失败，开机自启静默
/// 失效。路径含内嵌双引号的场景极罕见，引号包裹即可。
#[cfg(any(target_os = "windows", test))]
fn windows_run_value(exec_path: &str) -> String {
    format!("\"{}\"", exec_path)
}

/// 当前可执行文件绝对路径（入口内容的 `Exec` / `ProgramArguments`）
fn current_exe() -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;
    Ok(exe.to_string_lossy().into_owned())
}

#[cfg(not(target_os = "windows"))]
fn write_entry() -> anyhow::Result<()> {
    let Some(path) = entry_path() else {
        anyhow::bail!("无法解析数据主目录（HOME 未设置？）");
    };
    let exe = current_exe()?;
    #[cfg(target_os = "macos")]
    let content = macos_plist_content(&exe);
    #[cfg(not(target_os = "macos"))]
    let content = desktop_entry_content(&exe);

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, content)?;
    tracing::info!("开机自启入口已写入: {}", path.display());
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn remove_entry() -> anyhow::Result<()> {
    let Some(path) = entry_path() else {
        return Ok(());
    };
    match std::fs::remove_file(&path) {
        Ok(()) => tracing::info!("开机自启入口已移除: {}", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn write_entry() -> anyhow::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    let exe = windows_run_value(&current_exe()?);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?.0;
    run.set_value::<_, _>("kiro-rs", &exe)?;
    tracing::info!("开机自启注册表项已写入: {}", exe);
    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_entry() -> anyhow::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu.open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_SET_VALUE)?;
    match run.delete_value("kiro-rs") {
        Ok(()) => tracing::info!("开机自启注册表项已移除"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_fields() {
        let content = desktop_entry_content("/usr/local/bin/kiro-desktop");
        assert!(content.starts_with("[Desktop Entry]"));
        assert!(content.contains("Type=Application"));
        assert!(content.contains("Exec=/usr/local/bin/kiro-desktop"));
        assert!(content.contains("Terminal=false"));
        assert!(content.contains("X-GNOME-Autostart-enabled=true"));
        assert!(!content.contains("X-GNOME-Autostart-enabled=false"));
    }

    #[test]
    fn desktop_entry_quotes_path_with_spaces() {
        let content = desktop_entry_content("/opt/my apps/kiro desktop/bin/kiro");
        assert!(content.contains("Exec=\"/opt/my apps/kiro desktop/bin/kiro\""));
        // 转义：内部双引号与美元符
        let quoted = quote_exec_arg("/a \"b\" $c");
        assert_eq!(quoted, "\"/a \\\"b\\\" \\$c\"");
        // 无空格不引
        assert_eq!(quote_exec_arg("/usr/bin/kiro"), "/usr/bin/kiro");
    }

    #[test]
    fn macos_plist_fields() {
        let content = macos_plist_content("/Applications/kiro-rs.app/Contents/MacOS/kiro-rs");
        assert!(content.contains("<string>dev.kiro-rs</string>"));
        assert!(content.contains(
            "<string>/Applications/kiro-rs.app/Contents/MacOS/kiro-rs</string>"
        ));
        assert!(content.contains("<key>RunAtLoad</key>"));
        assert!(content.contains("<true/>"));
    }

    #[test]
    fn macos_plist_escapes_special_chars() {
        let content = macos_plist_content("/Applications/A&B <test>/kiro");
        assert!(content.contains("<string>/Applications/A&amp;B &lt;test&gt;/kiro</string>"));
    }

    #[test]
    fn windows_run_value_quotes_path_with_spaces() {
        // Program Files 路径裸写会在空格处截断，引号包裹防截断
        assert_eq!(
            windows_run_value(r"C:\Program Files\kiro-rs\kiro-desktop.exe"),
            r#""C:\Program Files\kiro-rs\kiro-desktop.exe""#
        );
        assert_eq!(windows_run_value(r"C:\bin\kiro.exe"), r#""C:\bin\kiro.exe""#);
    }

    /// Linux：入口路径落在 XDG autostart，写删往返幂等
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn linux_entry_write_remove_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: 单线程测试进程内改环境变量
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", dir.path());
        }

        set_enabled(true).unwrap();
        let path = dir.path().join("autostart/dev.kiro-rs.desktop");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Exec="));

        // 重复写不报错（幂等）
        set_enabled(true).unwrap();

        set_enabled(false).unwrap();
        assert!(!path.exists());

        // 重复删不报错（幂等）
        set_enabled(false).unwrap();

        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }
}
