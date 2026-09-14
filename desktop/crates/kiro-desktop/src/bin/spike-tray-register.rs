//! Spike（change 6 任务 2.5）：tray-icon 0.25 ksni 的真实注册与降级
//!
//! 无窗口纯 D-Bus 验证：`TrayIcon::new` 在有 / 无
//! `org.kde.StatusNotifierWatcher` 时的行为。
//!
//! 运行（真实注册）：
//! `dbus-run-session -- bash -c 'python3 <fake watcher> & sleep 1; cargo run --release --bin spike-tray-register'`
//! 期望输出 `REGISTER OK`。
//!
//! 运行（无宿主降级）：
//! `dbus-run-session -- cargo run --release --bin spike-tray-register`
//! 期望输出 `DEGRADED: ...`。
//!
//! 本 bin 仅用于取证，不属于应用交付物（同 `spike-zero-window`）。

use tray_icon::{
    Icon, TrayIconBuilder,
    menu::Menu,
};

fn main() {
    // 与主程序同款：22x22 绿点
    let mut rgba = vec![0u8; 22 * 22 * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&[76, 175, 80, 255]);
    }
    let icon = Icon::from_rgba(rgba, 22, 22).expect("图标构建失败");

    match TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("kiro-rs spike")
        .with_menu(Box::new(Menu::new()))
        .build()
    {
        Ok(_tray) => {
            println!("REGISTER OK");
            // 短暂驻留，让 watcher 侧的注册回调与信号落地后再退出
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Err(e) => println!("DEGRADED: {}", e),
    }
}
