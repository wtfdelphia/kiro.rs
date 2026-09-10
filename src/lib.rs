//! kiro-rs 核心库
//!
//! lib + bin 双目标：本 lib 与 `src/main.rs` CLI 二进制。桌面二进制（后续
//! change）依赖本 lib 复用装配链（[`bootstrap`]）与存储接缝（[`storage`]）。
//! 导出面见 `docs/desktop-gpui-embedded-design.md` §3.3。

pub mod admin;
pub mod admin_ui;
pub mod anthropic;
pub mod bootstrap;
pub mod http_client;
pub mod kiro;
pub mod model;
pub mod openai;
pub mod public_api;
pub mod storage;
pub mod token;

mod common;

pub use bootstrap::{BootOptions, Bootstrapped, bootstrap, build_routes};
