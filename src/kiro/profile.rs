//! Profile ARN resolution aligned with Kiro IDE / Kiro-Go.
//!
//! Order: trusted cache → ListAvailableProfiles → refresh fallback → persist.
//! Known fixed placeholder ARNs are never trusted or persisted (they cause upstream 403).

use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use serde::Deserialize;

use crate::http_client::{build_client, ProxyConfig};
use crate::kiro::machine_id;
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::token_manager::MultiTokenManager;
use crate::model::config::Config;

/// REST base used by Kiro-Go ListAvailableProfiles (fixed us-east-1).
const LIST_PROFILES_URL: &str =
    "https://codewhisperer.us-east-1.amazonaws.com/ListAvailableProfiles";

const SOCIAL_SIGN_IN_PROFILE_ARN: &str =
    "arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK";

const BUILDER_ID_PROFILE_ARN: &str =
    "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX";

/// Expected condition: credential type does not support profile ARN.
#[derive(Debug)]
pub struct ProfileArnUnsupported;

impl std::fmt::Display for ProfileArnUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "profile ARN not available for this account type")
    }
}

impl std::error::Error for ProfileArnUnsupported {}

/// Whether this credential type supports profile ARN resolution (IDE supportsProfiles).
pub fn supports_profiles(credentials: &KiroCredentials) -> bool {
    if credentials.is_api_key_credential() {
        return false;
    }
    let provider = credentials.provider.as_deref().unwrap_or("");
    let auth = credentials.auth_method.as_deref().unwrap_or("");
    let is_idc_provider = eq_ci(provider, "Enterprise")
        || eq_ci(provider, "Internal")
        || eq_ci(provider, "BuilderId");
    let is_external = eq_ci(auth, "external_idp") || eq_ci(provider, "ExternalIdp");
    let is_social = eq_ci(auth, "social");
    is_idc_provider || is_external || is_social || looks_like_idc(credentials)
}

fn looks_like_idc(credentials: &KiroCredentials) -> bool {
    let auth = credentials.auth_method.as_deref().unwrap_or("");
    (eq_ci(auth, "idc") || eq_ci(auth, "builder-id") || eq_ci(auth, "iam"))
        && credentials.client_id.is_some()
        && credentials.client_secret.is_some()
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Infer default provider when missing (KAM idc import → BuilderId).
pub fn infer_provider(credentials: &KiroCredentials) -> Option<String> {
    if let Some(p) = credentials
        .provider
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Some(p.to_string());
    }
    let auth = credentials.auth_method.as_deref().unwrap_or("");
    if credentials.client_id.is_some() && credentials.client_secret.is_some() {
        if eq_ci(auth, "idc")
            || eq_ci(auth, "builder-id")
            || eq_ci(auth, "iam")
            || auth.is_empty()
        {
            return Some("BuilderId".to_string());
        }
    }
    if eq_ci(auth, "social") {
        // Social may still resolve via refresh/list; no forced provider.
        return None;
    }
    None
}

/// Fixed profile ARN table (IDE short-circuit; never call ListAvailableProfiles).
#[allow(dead_code)]
pub fn get_fixed_profile_arn(provider: &str) -> Option<&'static str> {
    if eq_ci(provider, "BuilderId") {
        Some(BUILDER_ID_PROFILE_ARN)
    } else if eq_ci(provider, "Github") || eq_ci(provider, "Google") {
        Some(SOCIAL_SIGN_IN_PROFILE_ARN)
    } else {
        None
    }
}

/// Known IDE/short-circuit placeholder ARNs. Upstream often rejects these with 403.
pub fn is_known_placeholder_profile_arn(arn: &str) -> bool {
    let a = arn.trim();
    a.eq_ignore_ascii_case(BUILDER_ID_PROFILE_ARN)
        || a.eq_ignore_ascii_case(SOCIAL_SIGN_IN_PROFILE_ARN)
}

/// Cached profile ARN only if non-empty and not a known placeholder.
pub fn trusted_profile_arn(credentials: &KiroCredentials) -> Option<&str> {
    credentials
        .profile_arn
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| !is_known_placeholder_profile_arn(s))
}

/// Soft miss: proceed without profileArn (common for BuilderId after list failure).
#[derive(Debug)]
pub struct ProfileArnUnavailable;

impl std::fmt::Display for ProfileArnUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no trusted Kiro profileArn available; proceeding without")
    }
}

impl std::error::Error for ProfileArnUnavailable {}

/// Resolve profile ARN for a credential, optionally persisting via token manager.
///
/// Does **not** persist or return known placeholder fixed ARNs. Those cause upstream
/// User is not authorized on ListAvailableModels / generate for many accounts.
pub async fn resolve_profile_arn(
    token_manager: &MultiTokenManager,
    credential_id: u64,
    credentials: &KiroCredentials,
    token: &str,
) -> anyhow::Result<String> {
    if let Some(arn) = trusted_profile_arn(credentials) {
        return Ok(arn.to_string());
    }

    // Drop known-bad placeholder from store so later requests do not keep replaying 403.
    if credentials
        .profile_arn
        .as_ref()
        .map(|s| is_known_placeholder_profile_arn(s))
        .unwrap_or(false)
    {
        let _ = token_manager.clear_profile_arn(credential_id);
    }

    if credentials.is_api_key_credential() {
        return Err(anyhow!(ProfileArnUnsupported));
    }

    let provider = infer_provider(credentials);

    // Fixed ARN table is documentation/history only — never short-circuit or persist.

    if !supports_profiles(credentials) && provider.is_none() {
        return Err(anyhow!(ProfileArnUnsupported));
    }

    let config = token_manager.config();
    let proxy = credentials.effective_proxy(token_manager.global_proxy().as_ref());

    let list_err = match list_available_profiles_with_retry(credentials, &config, token, proxy.as_ref()).await {
        Ok(arn) if !arn.is_empty() && !is_known_placeholder_profile_arn(&arn) => {
            let _ = token_manager.set_profile_arn(credential_id, Some(arn.clone()), provider.clone());
            return Ok(arn);
        }
        Ok(arn) if !arn.is_empty() => {
            // Upstream somehow returned a known placeholder — do not trust/persist.
            Some(anyhow!("list returned placeholder profileArn"))
        }
        Ok(_) => Some(anyhow!("empty profile list")),
        Err(e) => Some(e),
    };

    // Fallback: force refresh may return profileArn
    if credentials.refresh_token.is_some() {
        match token_manager.force_refresh_token_for(credential_id).await {
            Ok(()) => {
                if let Some(arn) = token_manager.profile_arn_of(credential_id) {
                    if !is_known_placeholder_profile_arn(&arn) {
                        return Ok(arn);
                    }
                    let _ = token_manager.clear_profile_arn(credential_id);
                }
                // refresh succeeded but no trusted profileArn — proceed without
                return Err(anyhow!(ProfileArnUnavailable));
            }
            Err(refresh_err) => {
                if let Some(le) = list_err {
                    bail!("no available Kiro profile (list: {}; refresh: {})", le, refresh_err);
                }
                bail!("no available Kiro profile (refresh: {})", refresh_err);
            }
        }
    }

    // List failed / empty: for IdC/BuilderId-style accounts, generate often works without ARN.
    if looks_like_idc(credentials)
        || provider
            .as_deref()
            .map(|p| eq_ci(p, "BuilderId"))
            .unwrap_or(false)
    {
        return Err(anyhow!(ProfileArnUnavailable));
    }

    if let Some(le) = list_err {
        bail!(
            "no available Kiro profile (list: {}; refresh succeeded but returned no profileArn)",
            le
        );
    }
    bail!("no available Kiro profile")
}

async fn list_available_profiles_with_retry(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<String> {
    const MAX_ATTEMPTS: usize = 3;
    let mut backoff = Duration::from_millis(200);
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 1..=MAX_ATTEMPTS {
        match list_available_profiles(credentials, config, token, proxy).await {
            Ok(arn) => return Ok(arn),
            Err(e) => {
                let transient = is_transient_profile_fetch_error(&e);
                last_err = Some(e);
                if !transient || attempt == MAX_ATTEMPTS {
                    break;
                }
                tokio::time::sleep(backoff).await;
                backoff *= 2;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("ListAvailableProfiles failed")))
}

fn is_transient_profile_fetch_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    if msg.contains("empty profile list") {
        return false;
    }
    if let Some(rest) = msg.strip_prefix("HTTP ") {
        return rest.starts_with('5') || rest.starts_with("429");
    }
    true
}

/// Pure parse of ListAvailableProfiles JSON body (testable without HTTP).
pub(crate) fn parse_list_available_profiles_body(body: &str) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct ProfilesResponse {
        profiles: Option<Vec<ProfileItem>>,
    }
    #[derive(Deserialize)]
    struct ProfileItem {
        arn: Option<String>,
    }

    let parsed: ProfilesResponse =
        serde_json::from_str(body).context("decode ListAvailableProfiles")?;
    for item in parsed.profiles.unwrap_or_default() {
        if let Some(arn) = item
            .arn
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return Ok(arn);
        }
    }
    bail!("empty profile list")
}

async fn list_available_profiles(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<String> {
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let host = "codewhisperer.us-east-1.amazonaws.com";
    let user_agent = format!(
        "aws-sdk-js/1.0.0 ua/2.1 os/{} lang/js md/nodejs#{} api/codewhispererruntime#1.0.0 m/N,E KiroIDE-{}-{}",
        config.system_version, config.node_version, config.kiro_version, machine_id
    );
    let amz_user_agent = format!(
        "aws-sdk-js/1.0.0 KiroIDE-{}-{}",
        config.kiro_version, machine_id
    );

    let client = build_client(proxy, 60, config.tls_backend)?;
    let response = client
        .post(LIST_PROFILES_URL)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("x-amz-user-agent", &amz_user_agent)
        .header("user-agent", &user_agent)
        .header("host", host)
        .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
        .header("amz-sdk-request", "attempt=1; max=1")
        .header("Authorization", format!("Bearer {}", token))
        .header("x-amzn-codewhisperer-optout", "true")
        .header("Connection", "close")
        .body(r#"{"maxResults":10}"#)
        .send()
        .await
        .context("ListAvailableProfiles request failed")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("HTTP {}: {}", status.as_u16(), body);
    }

    let body_text = response
        .text()
        .await
        .context("read ListAvailableProfiles body")?;
    parse_list_available_profiles_body(&body_text)
}

/// Ensure credentials used for a request have profile_arn populated when possible.
/// Returns the ARN if resolved; Ok(None) if unsupported.
pub async fn ensure_profile_arn_for_request(
    token_manager: &MultiTokenManager,
    credential_id: u64,
    credentials: &mut KiroCredentials,
    token: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(arn) = trusted_profile_arn(credentials) {
        return Ok(Some(arn.to_string()));
    }

    // Clear in-memory placeholder so request path does not inject a known-bad ARN.
    if credentials
        .profile_arn
        .as_ref()
        .map(|s| is_known_placeholder_profile_arn(s))
        .unwrap_or(false)
    {
        credentials.profile_arn = None;
        let _ = token_manager.clear_profile_arn(credential_id);
    }

    match resolve_profile_arn(token_manager, credential_id, credentials, token).await {
        Ok(arn) if !is_known_placeholder_profile_arn(&arn) => {
            credentials.profile_arn = Some(arn.clone());
            if credentials.provider.is_none() {
                credentials.provider = infer_provider(credentials);
            }
            Ok(Some(arn))
        }
        Ok(_) => {
            credentials.profile_arn = None;
            Ok(None)
        }
        Err(e) if e.downcast_ref::<ProfileArnUnsupported>().is_some() => Ok(None),
        Err(e) if e.downcast_ref::<ProfileArnUnavailable>().is_some() => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_builder_id() {
        assert_eq!(
            get_fixed_profile_arn("BuilderId"),
            Some(BUILDER_ID_PROFILE_ARN)
        );
        assert_eq!(
            get_fixed_profile_arn("builderid"),
            Some(BUILDER_ID_PROFILE_ARN)
        );
    }

    #[test]
    fn test_fixed_github_google() {
        assert_eq!(
            get_fixed_profile_arn("Github"),
            Some(SOCIAL_SIGN_IN_PROFILE_ARN)
        );
        assert_eq!(
            get_fixed_profile_arn("Google"),
            Some(SOCIAL_SIGN_IN_PROFILE_ARN)
        );
    }

    #[test]
    fn test_infer_provider_idc_defaults_builder() {
        let mut c = KiroCredentials::default();
        c.auth_method = Some("idc".to_string());
        c.client_id = Some("cid".to_string());
        c.client_secret = Some("sec".to_string());
        assert_eq!(infer_provider(&c).as_deref(), Some("BuilderId"));
    }

    #[test]
    fn test_supports_profiles_social() {
        let mut c = KiroCredentials::default();
        c.auth_method = Some("social".to_string());
        assert!(supports_profiles(&c));
    }

    #[test]
    fn test_api_key_unsupported() {
        let mut c = KiroCredentials::default();
        c.kiro_api_key = Some("ksk_x".to_string());
        c.auth_method = Some("api_key".to_string());
        assert!(!supports_profiles(&c));
    }

    #[test]
    fn test_transient_classification() {
        assert!(is_transient_profile_fetch_error(&anyhow!("connection reset")));
        assert!(is_transient_profile_fetch_error(&anyhow!("HTTP 503 oops")));
        assert!(is_transient_profile_fetch_error(&anyhow!("HTTP 429 slow")));
        assert!(!is_transient_profile_fetch_error(&anyhow!("HTTP 403 denied")));
        assert!(!is_transient_profile_fetch_error(&anyhow!("empty profile list")));
    }
    #[test]
    fn test_parse_list_available_profiles_body_ok() {
        let body = r#"{"profiles":[{"arn":"arn:aws:codewhisperer:us-east-1:1:profile/ABC"}]}"#;
        let arn = parse_list_available_profiles_body(body).unwrap();
        assert_eq!(arn, "arn:aws:codewhisperer:us-east-1:1:profile/ABC");
    }

    #[test]
    fn test_parse_list_available_profiles_body_empty() {
        let body = r#"{"profiles":[]}"#;
        let err = parse_list_available_profiles_body(body).unwrap_err().to_string();
        assert!(err.contains("empty profile list"));
    }

    #[test]
    fn test_parse_list_available_profiles_body_blank_arn_skipped() {
        let body = r#"{"profiles":[{"arn":"  "},{"arn":"arn:aws:x:profile/OK"}]}"#;
        let arn = parse_list_available_profiles_body(body).unwrap();
        assert_eq!(arn, "arn:aws:x:profile/OK");
    }

    #[test]
    fn test_cache_contract_nonempty_profile_arn() {
        let mut c = KiroCredentials::default();
        c.profile_arn = Some("arn:cached".to_string());
        let hit = c
            .profile_arn
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        assert_eq!(hit, Some("arn:cached"));
    }

    #[test]
    fn test_idc_defaults_to_builder_fixed_arn() {
        let mut c = KiroCredentials::default();
        c.auth_method = Some("idc".to_string());
        c.client_id = Some("c".to_string());
        c.client_secret = Some("s".to_string());
        assert_eq!(infer_provider(&c).as_deref(), Some("BuilderId"));
        assert_eq!(get_fixed_profile_arn("BuilderId"), Some(BUILDER_ID_PROFILE_ARN));
    }

    #[test]
    fn test_placeholder_arn_not_trusted() {
        assert!(is_known_placeholder_profile_arn(BUILDER_ID_PROFILE_ARN));
        assert!(is_known_placeholder_profile_arn(SOCIAL_SIGN_IN_PROFILE_ARN));
        assert!(!is_known_placeholder_profile_arn(
            "arn:aws:codewhisperer:us-east-1:1:profile/REAL"
        ));

        let mut c = KiroCredentials::default();
        c.profile_arn = Some(BUILDER_ID_PROFILE_ARN.to_string());
        assert!(trusted_profile_arn(&c).is_none());

        c.profile_arn = Some("arn:aws:codewhisperer:us-east-1:1:profile/REAL".into());
        assert_eq!(
            trusted_profile_arn(&c),
            Some("arn:aws:codewhisperer:us-east-1:1:profile/REAL")
        );
    }
}
