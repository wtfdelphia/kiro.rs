//! Admin API 错误类型定义

use std::fmt;

use axum::http::StatusCode;

use super::types::AdminErrorResponse;

/// Admin 服务错误类型
#[derive(Debug)]
pub enum AdminServiceError {
    /// 凭据不存在
    NotFound { id: u64 },

    /// 上游服务调用失败（网络、API 错误等）
    UpstreamError(String),

    /// 内部状态错误
    InternalError(String),

    /// 凭据无效（验证失败）
    InvalidCredential(String),
}

impl fmt::Display for AdminServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdminServiceError::NotFound { id } => {
                write!(f, "凭据不存在: {}", id)
            }
            AdminServiceError::UpstreamError(msg) => write!(f, "上游服务错误: {}", msg),
            AdminServiceError::InternalError(msg) => write!(f, "内部错误: {}", msg),
            AdminServiceError::InvalidCredential(msg) => write!(f, "凭据无效: {}", msg),
        }
    }
}

impl std::error::Error for AdminServiceError {}

impl AdminServiceError {
    /// 获取对应的 HTTP 状态码
    pub fn status_code(&self) -> StatusCode {
        match self {
            AdminServiceError::NotFound { .. } => StatusCode::NOT_FOUND,
            AdminServiceError::UpstreamError(_) => StatusCode::BAD_GATEWAY,
            AdminServiceError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AdminServiceError::InvalidCredential(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// 转换为 API 错误响应
    pub fn into_response(self) -> AdminErrorResponse {
        match &self {
            AdminServiceError::NotFound { .. } => AdminErrorResponse::not_found(self.to_string()),
            AdminServiceError::UpstreamError(_) => AdminErrorResponse::api_error(self.to_string()),
            AdminServiceError::InternalError(_) => {
                AdminErrorResponse::internal_error(self.to_string())
            }
            AdminServiceError::InvalidCredential(_) => {
                AdminErrorResponse::invalid_request(self.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn status_codes_map_correctly() {
        assert_eq!(
            AdminServiceError::NotFound { id: 1 }.status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AdminServiceError::InvalidCredential("x".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AdminServiceError::UpstreamError("x".into()).status_code(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            AdminServiceError::InternalError("x".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn invalid_credential_response_has_no_secret_fields() {
        let resp = AdminServiceError::InvalidCredential(
            "model unmapped: gpt-4".into(),
        )
        .into_response();
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.to_lowercase().contains("refreshtoken"));
        assert!(!json.to_lowercase().contains("accesstoken"));
        assert!(json.contains("gpt-4"));
    }

    #[test]
    fn upstream_error_response_has_no_secret_fields() {
        let resp = AdminServiceError::UpstreamError(
            "upstream generate failed: 403 denied".into(),
        )
        .into_response();
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.to_lowercase().contains("refreshtoken"));
        assert!(!json.to_lowercase().contains("accesstoken"));
        assert_eq!(
            AdminServiceError::UpstreamError("x".into()).status_code(),
            StatusCode::BAD_GATEWAY
        );
    }

}

