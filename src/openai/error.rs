//! OpenAI error shape
//!
//! 与 Anthropic 端点的 error shape 严格分家：Anthropic 侧用
//! `crate::anthropic::types::ErrorResponse`，本模块只服务 OpenAI 端点。

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// OpenAI 错误信封
#[derive(Debug, Serialize)]
pub struct OpenAiErrorBody {
    pub error: OpenAiErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct OpenAiErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: &'static str,
    pub code: Option<String>,
}

/// 端点错误
#[derive(Debug)]
pub enum OpenAiError {
    /// 400 invalid_request_error
    InvalidRequest(String),
    /// 502 api_error（上游调用失败）
    Upstream(String),
    /// 503 server_error（provider 未配置 / 无可用凭据）
    Unavailable(String),
    /// 500 server_error（内部错误）
    Internal(String),
}

impl OpenAiError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request_error",
            Self::Upstream(_) => "api_error",
            Self::Unavailable(_) | Self::Internal(_) => "server_error",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRequest(m)
            | Self::Upstream(m)
            | Self::Unavailable(m)
            | Self::Internal(m) => m,
        }
    }

    pub fn body(&self) -> OpenAiErrorBody {
        OpenAiErrorBody {
            error: OpenAiErrorDetail {
                message: self.message().to_string(),
                error_type: self.error_type(),
                code: None,
            },
        }
    }
}

impl IntoResponse for OpenAiError {
    fn into_response(self) -> Response {
        (self.status(), Json(self.body())).into_response()
    }
}

impl From<crate::anthropic::ConversionError> for OpenAiError {
    fn from(e: crate::anthropic::ConversionError) -> Self {
        use crate::anthropic::ConversionError as CE;
        match e {
            // 沿用现有错误文案，不为 OpenAI 端点放宽 resolve 策略
            CE::UnsupportedModel(model) => Self::InvalidRequest(format!("模型不支持: {}", model)),
            CE::EmptyMessages => Self::InvalidRequest("消息列表为空".to_string()),
        }
    }
}

/// 把 provider 错误映射为 OpenAI 方言
///
/// 平行于 `crate::anthropic::handlers::map_provider_error`（它产出 Anthropic shape）。
pub fn map_provider_error(err: anyhow::Error) -> OpenAiError {
    let msg = err.to_string();
    // 无可用凭据属于服务端暂时不可用，其余按上游错误处理
    if msg.contains("没有可用") || msg.contains("无可用") || msg.contains("凭据") {
        OpenAiError::Unavailable(format!("上游 API 调用失败: {}", msg))
    } else {
        OpenAiError::Upstream(format!("上游 API 调用失败: {}", msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_and_type_mapping() {
        let cases = [
            (
                OpenAiError::InvalidRequest("x".into()),
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
            ),
            (
                OpenAiError::Upstream("x".into()),
                StatusCode::BAD_GATEWAY,
                "api_error",
            ),
            (
                OpenAiError::Unavailable("x".into()),
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
            ),
            (
                OpenAiError::Internal("x".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
            ),
        ];
        for (err, status, ty) in cases {
            assert_eq!(err.status(), status);
            assert_eq!(err.error_type(), ty);
        }
    }

    #[test]
    fn test_error_shape_is_openai_not_anthropic() {
        let json = serde_json::to_value(OpenAiError::InvalidRequest("bad".into()).body()).unwrap();
        // OpenAI: {"error":{"message":..,"type":..,"code":..}}
        assert!(json.get("error").is_some());
        assert_eq!(json["error"]["message"], "bad");
        assert_eq!(json["error"]["type"], "invalid_request_error");
        // Anthropic shape 的顶层 "type":"error" 不应出现在顶层
        assert!(json.get("type").is_none());
    }

    #[test]
    fn test_conversion_error_mapping() {
        let e: OpenAiError = crate::anthropic::ConversionError::UnsupportedModel("zzz".into()).into();
        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        assert!(e.message().contains("zzz"));
        // 不使用「凭据无效」前缀（对齐既有 model-resolution 约定）
        assert!(!e.message().starts_with("凭据无效"));

        let e2: OpenAiError = crate::anthropic::ConversionError::EmptyMessages.into();
        assert_eq!(e2.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_no_secret_leak_in_body() {
        let err = OpenAiError::Upstream("token=abc refreshToken=xyz".into());
        let json = serde_json::to_string(&err.body()).unwrap();
        // 错误文案由调用方保证不含密钥；此处断言结构不额外附加字段
        assert!(!json.to_lowercase().contains("\"apikey\""));
        assert!(!json.to_lowercase().contains("\"accesstoken\""));
    }

    #[test]
    fn test_provider_error_credential_maps_to_unavailable() {
        let err = map_provider_error(anyhow::anyhow!("没有可用的凭据"));
        assert_eq!(err.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.error_type(), "server_error");
    }

    #[test]
    fn test_provider_error_other_maps_to_upstream() {
        let err = map_provider_error(anyhow::anyhow!("connection reset"));
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.error_type(), "api_error");
    }
}
