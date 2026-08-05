//! 对外 Public API 目录
//!
//! 本模块是「客户端 -> 本代理」端点事实的单一来源：路由挂载校验、启动日志、
//! Admin 展示均从这里派生，避免多处手写清单互相漂移。
//!
//! 注意与上游端点的区分：
//! - Public Client API（本模块）：`/v1/messages`、`/v1/chat/completions` 等
//! - Upstream Kiro Endpoint（`/api/admin/settings/endpoint`）：本代理访问上游用的端点

pub mod catalog;
pub mod dto;

#[cfg(test)]
mod routes_test;

pub use catalog::live_endpoints;
pub use dto::{PublicApiResponse, build_response};

#[cfg(test)]
pub use catalog::catalog;
