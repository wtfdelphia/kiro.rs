//! Admin UI 静态文件服务模块
//!
//! 使用 rust-embed 嵌入前端构建产物

mod router;
#[cfg(test)]
mod router_test;

pub use router::mount_admin_ui;
