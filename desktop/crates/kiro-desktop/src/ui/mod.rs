//! UI 骨架（设计文档 §8 的占位版）
//!
//! Root（gpui-kit 窗口级容器）→ AppView（标题栏 + sidebar + 内容区）。
//! change 4 起凭据视图接真实数据；总览/设置/服务仍是占位面板。

pub mod credentials;
pub mod root;
pub mod server;
pub mod settings;
pub mod views;

pub use root::AppView;
