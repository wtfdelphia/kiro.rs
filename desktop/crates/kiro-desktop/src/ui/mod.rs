//! UI 骨架（设计文档 §8 的占位版）
//!
//! Root（gpui-kit 窗口级容器）→ AppView（标题栏 + sidebar + 内容区）。
//! 四个视图是带标题的占位面板，不带数据交互。

pub mod root;
pub mod views;

pub use root::AppView;
