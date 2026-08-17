//! Admin `GET /api/admin/public-api` 的响应 DTO 与示例生成
//!
//! 约束：永不回传完整客户端密钥；示例中的密钥位置一律用占位符。

use serde::Serialize;

use super::catalog::{PublicEndpoint, catalog, families, family_label};

/// 示例中的密钥占位符（禁止填入真实 client key 或 admin key）
pub const API_KEY_PLACEHOLDER: &str = "API_KEY";

/// 服务概要
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub listen_host: String,
    pub port: u16,
    pub require_api_key: bool,
    /// 掩码形式，永不含完整值
    pub api_key_mask: Option<String>,
    pub has_api_key: bool,
    pub auth_headers: Vec<String>,
    /// 未配置公开 Base URL 时为 null，前端回落 window.location.origin
    pub suggested_base_url: Option<String>,
}

/// 单个端点的展示形态
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDto {
    pub id: String,
    pub method: String,
    pub path: String,
    pub aliases: Vec<String>,
    pub auth: String,
    pub status: String,
    pub stream: bool,
    pub summary: String,
    pub client_hints: Vec<String>,
    pub examples: EndpointExamples,
}

/// 接入示例
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointExamples {
    pub curl: String,
}

/// 协议族分组
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyGroup {
    pub family: String,
    pub label: String,
    pub endpoints: Vec<EndpointDto>,
}

/// `GET /api/admin/public-api` 响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiResponse {
    pub server: ServerSummary,
    pub families: Vec<FamilyGroup>,
}

/// 构建响应
///
/// `api_key_mask` 由调用方用既有掩码规则生成（本模块不接触明文 key）。
pub fn build_response(
    listen_host: impl Into<String>,
    port: u16,
    require_api_key: bool,
    api_key_mask: Option<String>,
    suggested_base_url: Option<String>,
) -> PublicApiResponse {
    let example_base = suggested_base_url
        .clone()
        .unwrap_or_else(|| format!("http://localhost:{}", port));

    let groups = families()
        .into_iter()
        .map(|family| FamilyGroup {
            family: family.to_string(),
            label: family_label(family).to_string(),
            endpoints: catalog()
                .iter()
                .filter(|e| e.family == family)
                .map(|e| endpoint_dto(e, &example_base))
                .collect(),
        })
        .collect();

    PublicApiResponse {
        server: ServerSummary {
            listen_host: listen_host.into(),
            port,
            require_api_key,
            has_api_key: api_key_mask.is_some(),
            api_key_mask,
            auth_headers: vec![
                "x-api-key".to_string(),
                "Authorization: Bearer".to_string(),
            ],
            suggested_base_url,
        },
        families: groups,
    }
}

fn endpoint_dto(e: &PublicEndpoint, base_url: &str) -> EndpointDto {
    EndpointDto {
        id: e.id.to_string(),
        method: e.method.to_string(),
        path: e.path.to_string(),
        aliases: e.aliases.iter().map(|s| s.to_string()).collect(),
        auth: e.auth.as_str().to_string(),
        status: e.status.as_str().to_string(),
        stream: e.stream,
        summary: e.summary.to_string(),
        client_hints: e.client_hints.iter().map(|s| s.to_string()).collect(),
        examples: EndpointExamples {
            curl: build_curl(e, base_url),
        },
    }
}

/// 生成 curl 示例；密钥位置固定为占位符
fn build_curl(e: &PublicEndpoint, base_url: &str) -> String {
    let url = format!("{}{}", base_url.trim_end_matches('/'), e.path);
    if e.method == "GET" {
        // Responses WebSocket ingress：示例用 wscat（transport 为 upgrade websocket）
        if e.path == "/v1/responses" {
            let ws_url = url.replacen("http://", "ws://", 1).replacen("https://", "wss://", 1);
            return format!(
                "wscat -c {} \\\n  -H \"x-api-key: {}\"",
                ws_url, API_KEY_PLACEHOLDER
            );
        }
        return format!(
            "curl {} \\\n  -H \"x-api-key: {}\"",
            url, API_KEY_PLACEHOLDER
        );
    }

    let body = match e.family {
        "openai-chat" => {
            r#"{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"hi"}]}"#
        }
        "openai-responses" => r#"{"model":"claude-sonnet-4.5","input":"hi"}"#,
        _ if e.path.ends_with("/count_tokens") => {
            r#"{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"hi"}]}"#
        }
        _ => {
            r#"{"model":"claude-sonnet-4.5","max_tokens":1024,"messages":[{"role":"user","content":"hi"}]}"#
        }
    };

    format!(
        "curl -X POST {} \\\n  -H \"x-api-key: {}\" \\\n  -H \"content-type: application/json\" \\\n  -d '{}'",
        url, API_KEY_PLACEHOLDER, body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PublicApiResponse {
        build_response("0.0.0.0", 8080, true, Some("sk-a***bcde".to_string()), None)
    }

    #[test]
    fn test_suggested_base_url_null_when_unconfigured() {
        assert!(sample().server.suggested_base_url.is_none());
    }

    #[test]
    fn test_all_endpoints_present_and_grouped() {
        let resp = sample();
        let total: usize = resp.families.iter().map(|f| f.endpoints.len()).sum();
        assert_eq!(total, catalog().len());
        assert_eq!(resp.families.len(), families().len());
    }

    #[test]
    fn test_examples_use_placeholder_only() {
        let resp = sample();
        for family in &resp.families {
            for e in &family.endpoints {
                assert!(
                    e.examples.curl.contains(API_KEY_PLACEHOLDER),
                    "{} 的 curl 示例缺少密钥占位符",
                    e.id
                );
            }
        }
    }

    #[test]
    fn test_serialized_response_has_no_full_key() {
        // 明文 key 绝不进入 DTO：这里传入掩码，断言序列化结果只含掩码
        let json = serde_json::to_string(&sample()).expect("序列化失败");
        assert!(json.contains("sk-a***bcde"));
        assert!(
            !json.contains("sk-abcdefghijklmnop"),
            "响应中不得出现完整 key"
        );
        assert!(json.contains("API_KEY"), "示例应使用占位符");
    }

    #[test]
    fn test_example_base_falls_back_to_localhost_port() {
        let resp = build_response("0.0.0.0", 9999, false, None, None);
        let models = resp
            .families
            .iter()
            .flat_map(|f| &f.endpoints)
            .find(|e| e.path == "/v1/models")
            .expect("缺少 /v1/models");
        assert!(models.examples.curl.contains("http://localhost:9999/v1/models"));
    }

    #[test]
    fn test_suggested_base_url_used_in_examples() {
        let resp = build_response(
            "0.0.0.0",
            8080,
            true,
            None,
            Some("https://proxy.example.com/".to_string()),
        );
        let msg = resp
            .families
            .iter()
            .flat_map(|f| &f.endpoints)
            .find(|e| e.path == "/v1/messages")
            .expect("缺少 /v1/messages");
        assert!(
            msg.examples
                .curl
                .contains("https://proxy.example.com/v1/messages"),
            "curl 未使用 suggestedBaseUrl: {}",
            msg.examples.curl
        );
    }

    #[test]
    fn test_has_api_key_follows_mask() {
        assert!(!build_response("0.0.0.0", 8080, true, None, None).server.has_api_key);
        assert!(sample().server.has_api_key);
    }

    #[test]
    fn test_status_serialized_lowercase() {
        let resp = sample();
        let all: Vec<_> = resp.families.iter().flat_map(|f| &f.endpoints).collect();

        let chat = all
            .iter()
            .find(|e| e.path == "/v1/chat/completions")
            .expect("缺少 /v1/chat/completions");
        assert_eq!(chat.status, "live");

        let responses = all
            .iter()
            .find(|e| e.path == "/v1/responses")
            .expect("缺少 /v1/responses");
        assert_eq!(responses.status, "live");

        let retrieve = all
            .iter()
            .find(|e| e.path == "/v1/responses/{id}")
            .expect("缺少 /v1/responses/{id}");
        assert_eq!(retrieve.status, "planned");
    }
}
