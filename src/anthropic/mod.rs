//! Anthropic API 兼容服务模块
//!
//! 提供与 Anthropic Claude API 兼容的 HTTP 服务端点。
//!
//! # 支持的端点
//!
//! ## 标准端点 (/v1)
//! - `GET /v1/models` - 获取可用模型列表
//! - `POST /v1/messages` - 创建消息（对话）
//! - `POST /v1/messages/count_tokens` - 计算 token 数量
//!
//! ## Claude Code 兼容端点 (/cc/v1)
//! - `POST /cc/v1/messages` - 创建消息（流式响应会等待 contextUsageEvent 后再发送 message_start，确保 input_tokens 准确）
//! - `POST /cc/v1/messages/count_tokens` - 计算 token 数量（与 /v1 相同）
//!
//! # 使用示例
//! ```rust,ignore
//! use kiro_rs::anthropic;
//!
//! let app = anthropic::create_router("your-api-key");
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
//! axum::serve(listener, app).await?;
//! ```

mod converter;
mod handlers;
mod middleware;
pub use middleware::AuthRuntime;
mod router;
mod stream;
pub mod types;
mod websearch;

pub use converter::resolve_model;
pub use router::create_router_with_provider_and_auth;

// === 供 openai 模块复用（只扩大可见性，不改实现） ===

pub(crate) use converter::{
    ConversionError, convert_request_with_policy, get_context_window_size,
};
pub(crate) use handlers::{override_thinking_from_model_name, resolution_context_from_state};
pub(crate) use middleware::{AppState, auth_middleware, cors_layer};
pub(crate) use router::MAX_BODY_SIZE;
pub(crate) use stream::extract_thinking_from_complete_text;
pub(crate) use websearch::{
    WebSearchResults, call_mcp_api, create_mcp_request, generate_search_summary,
    parse_search_results,
};
