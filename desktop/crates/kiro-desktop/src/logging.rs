//! 日志初始化（设计文档 §6.4）
//!
//! 双写：终端 + `<data_dir>/kiro-rs/kiro-desktop.log`（追加）。
//! 常驻应用没有终端可看，文件日志是排障唯一入口。
//! 本期不做轮转（上限策略放 change 7）。

use std::fs::OpenOptions;
use std::path::Path;

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// 初始化全局日志。文件打开失败时降级为仅终端输出，不阻塞启动。
pub fn init(data_dir: &Path) {
    std::fs::create_dir_all(data_dir).ok();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_dir.join("kiro-desktop.log"));

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    match file {
        Ok(f) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(std::io::stdout),
                )
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(f)
                        .with_ansi(false),
                )
                .init();
        }
        Err(e) => {
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_writer(std::io::stdout)
                .init();
            eprintln!("日志文件打开失败，仅终端输出: {}", e);
        }
    }
}
