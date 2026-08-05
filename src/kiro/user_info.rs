//! 用户信息拉取（对齐 Kiro-Go GetUserInfo）
//!
//! 通过 getUsageLimits?isEmailRequired=true 获取 email / userId。
//! 失败 best-effort，不阻断凭据入库。

use anyhow::{bail, Result};
use std::sync::RwLock;

use crate::http_client::{build_client, ProxyConfig};
use crate::kiro::model::usage_limits::UsageLimitsResponse;
use crate::model::config::Config;

/// 测试可替换的 URL 构造器（与 Kiro-Go userInfoURL 一致）
static USER_INFO_URL: RwLock<fn() -> String> = RwLock::new(default_user_info_url);

fn default_user_info_url() -> String {
    "https://q.us-east-1.amazonaws.com/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST&isEmailRequired=true"
        .to_string()
}

/// 仅测试用：替换 GetUserInfo endpoint
#[cfg(test)]
#[allow(dead_code)]
pub fn set_user_info_url_for_test(f: fn() -> String) {
    *USER_INFO_URL.write().unwrap() = f;
}

#[cfg(test)]
#[allow(dead_code)]
pub fn reset_user_info_url_for_test() {
    *USER_INFO_URL.write().unwrap() = default_user_info_url;
}

fn current_user_info_url() -> String {
    let f = *USER_INFO_URL.read().unwrap();
    f()
}

/// 拉取用户 email 与 userId
pub async fn get_user_info(
    access_token: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<(Option<String>, Option<String>)> {
    let url = current_user_info_url();
    let client = build_client(proxy, 30, config.tls_backend)?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Accept", "application/json")
        .header("User-Agent", "aws-sdk-js/1.0.18 KiroAPIProxy")
        .header("x-amz-user-agent", "aws-sdk-js/1.0.18 KiroAPIProxy")
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("GetUserInfo failed: {} {}", status, body);
    }

    let data: UsageLimitsResponse = response.json().await?;
    let email = data
        .user_info
        .as_ref()
        .and_then(|u| u.email.clone())
        .filter(|s| !s.is_empty());
    let user_id = data
        .user_info
        .as_ref()
        .and_then(|u| u.user_id.clone())
        .filter(|s| !s.is_empty());
    Ok((email, user_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_url_contains_email_required() {
        let url = default_user_info_url();
        assert!(url.contains("isEmailRequired=true"));
        assert!(url.contains("getUsageLimits"));
    }
}
