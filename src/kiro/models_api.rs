//! ListAvailableModels 上游客户端

use anyhow::{bail, Context};

use crate::http_client::{build_client, ProxyConfig};
use crate::kiro::machine_id;
use crate::kiro::model::available_models::{parse_list_available_models_body, UpstreamModelInfo};
use crate::kiro::model::credentials::KiroCredentials;
use crate::model::config::Config;

const LIST_MODELS_HOST: &str = "codewhisperer.us-east-1.amazonaws.com";
const LIST_MODELS_BASE: &str =
    "https://codewhisperer.us-east-1.amazonaws.com/ListAvailableModels";

/// 构造 ListAvailableModels URL（可单测）
pub fn build_list_available_models_url(profile_arn: Option<&str>) -> String {
    let mut url = format!("{}?origin=AI_EDITOR&maxResults=50", LIST_MODELS_BASE);
    if let Some(arn) = profile_arn.map(str::trim).filter(|s| !s.is_empty()) {
        url.push_str(&format!("&profileArn={}", urlencoding::encode(arn)));
    }
    url
}

/// 拉取账号可用模型列表（GET ListAvailableModels）
///
/// 若凭据带有 profileArn 且上游返回 403 unauthorized，会自动去掉 ARN 重试一次。
/// 返回值第二项 stripped_bad_profile_arn 为 true 时，调用方可清除本地坏 ARN。
#[allow(dead_code)]
pub async fn list_available_models(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<Vec<UpstreamModelInfo>> {
    let (models, _) = list_available_models_with_meta(credentials, config, token, proxy).await?;
    Ok(models)
}

/// 同 [list_available_models]，额外返回是否因坏 profileArn 而回退成功。
pub async fn list_available_models_with_meta(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<(Vec<UpstreamModelInfo>, bool)> {
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let user_agent = format!(
        "aws-sdk-js/1.0.0 ua/2.1 os/{} lang/js md/nodejs#{} api/codewhispererruntime#1.0.0 m/N,E KiroIDE-{}-{}",
        config.system_version, config.node_version, config.kiro_version, machine_id
    );
    let amz_user_agent = format!(
        "aws-sdk-js/1.0.0 KiroIDE-{}-{}",
        config.kiro_version, machine_id
    );

    let profile = credentials
        .profile_arn
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let client = build_client(proxy, 60, config.tls_backend)?;

    let (status, body_text) =
        send_list_available_models(&client, credentials, profile, &user_agent, &amz_user_agent, token)
            .await?;

    if status.is_success() {
        return Ok((parse_list_available_models_body(&body_text)?, false));
    }

    let summary = truncate_for_error(&body_text, 500);
    let unauthorized = is_user_not_authorized_body(&body_text) || status.as_u16() == 403;

    // Known bad profileArn (fixed placeholder etc.): retry without ARN.
    if unauthorized && profile.is_some() {
        let (status2, body2) =
            send_list_available_models(&client, credentials, None, &user_agent, &amz_user_agent, token)
                .await?;
        if status2.is_success() {
            return Ok((parse_list_available_models_body(&body2)?, true));
        }
        let summary2 = truncate_for_error(&body2, 500);
        bail!("HTTP {}: {}", status2.as_u16(), summary2);
    }

    bail!("HTTP {}: {}", status.as_u16(), summary);
}

async fn send_list_available_models(
    client: &reqwest::Client,
    credentials: &KiroCredentials,
    profile_arn: Option<&str>,
    user_agent: &str,
    amz_user_agent: &str,
    token: &str,
) -> anyhow::Result<(reqwest::StatusCode, String)> {
    let url = build_list_available_models_url(profile_arn);
    let mut request = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-amz-user-agent", amz_user_agent)
        .header("user-agent", user_agent)
        .header("host", LIST_MODELS_HOST)
        .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
        .header("amz-sdk-request", "attempt=1; max=1")
        .header("Authorization", format!("Bearer {}", token))
        .header("x-amzn-codewhisperer-optout", "true")
        .header("Connection", "close");

    if credentials.is_api_key_credential() {
        request = request.header("tokentype", "API_KEY");
    }

    let response = request
        .send()
        .await
        .context("ListAvailableModels request failed")?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .context("read ListAvailableModels body")?;
    Ok((status, body_text))
}

pub fn is_user_not_authorized_body(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("user is not authorized")
        || lower.contains("not authorized to make this call")
}

fn truncate_for_error(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        return t.to_string();
    }
    let cut: String = t.chars().take(max).collect();
    format!("{}…", cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_without_profile() {
        let url = build_list_available_models_url(None);
        assert!(url.starts_with(LIST_MODELS_BASE));
        assert!(url.contains("origin=AI_EDITOR"));
        assert!(url.contains("maxResults=50"));
        assert!(!url.contains("profileArn="));
    }

    #[test]
    fn build_url_with_profile_encodes() {
        let url = build_list_available_models_url(Some(
            "arn:aws:codewhisperer:us-east-1:1:profile/ABC",
        ));
        assert!(url.contains("profileArn="));
        assert!(url.contains("profile%2FABC") || url.contains("profile/ABC"));
    }

    #[test]
    fn truncate_for_error_short() {
        assert_eq!(truncate_for_error("ok", 10), "ok");
    }

    #[test]
    fn truncate_for_error_long() {
        let s = "a".repeat(20);
        let out = truncate_for_error(&s, 5);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= 6);
    }

    #[test]
    fn detects_user_not_authorized() {
        assert!(is_user_not_authorized_body(
            r#"{"message":"User is not authorized to make this call.","reason":null}"#
        ));
        assert!(!is_user_not_authorized_body(r#"{"message":"ok"}"#));
    }
}
