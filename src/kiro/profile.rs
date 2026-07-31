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

/// Outcome of the ListAvailableProfiles stage, decoupled from HTTP details.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ListOutcome {
    /// Trusted (non-placeholder) ARN obtained.
    Resolved(String),
    /// Upstream returned a known placeholder ARN.
    Placeholder,
    /// Upstream returned an empty profile list.
    Empty,
    /// Request failed.
    Failed,
}

/// What the caller should do after the list stage. Keeps the decision testable offline.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolveAction {
    /// Use and persist this ARN.
    Use(String),
    /// Credential type does not support profile ARN.
    Unsupported,
    /// Proceed without profileArn.
    SoftUnavailable,
    /// Force refresh may yield an ARN (Kiro-owned refresh endpoint).
    ForceRefresh,
    /// No path left; bail with list context.
    Fail,
}

/// Decide what to do given a credential and the list stage outcome.
///
/// Pure: no I/O, no token manager. Cache hits and unsupported credential types are
/// short-circuited before this point by `resolve_profile_arn`.
///
/// A refresh MUST NOT be issued when it cannot possibly return a profileArn: AWS SSO
/// OIDC (`refresh_routes_to_idc`) is a plain OAuth2 token endpoint whose response carries
/// no profileArn, so refreshing for that purpose is a guaranteed-useless round trip on
/// every request. Only the Kiro-owned refresh endpoint may return one.
fn decide_profile_action(credentials: &KiroCredentials, list: ListOutcome) -> ResolveAction {
    if credentials.is_api_key_credential() {
        return ResolveAction::Unsupported;
    }

    if let ListOutcome::Resolved(arn) = list {
        return ResolveAction::Use(arn);
    }

    // Refresh cannot produce an ARN for these; proceed without one instead.
    if crate::kiro::token_manager::refresh_routes_to_idc(credentials) {
        return ResolveAction::SoftUnavailable;
    }

    if credentials.refresh_token.is_some() {
        return ResolveAction::ForceRefresh;
    }

    ResolveAction::Fail
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

    let (list, list_err) =
        match list_available_profiles_with_retry(credentials, &config, token, proxy.as_ref()).await {
            Ok(arn) if !arn.is_empty() && !is_known_placeholder_profile_arn(&arn) => {
                (ListOutcome::Resolved(arn), None)
            }
            Ok(arn) if !arn.is_empty() => {
                // Upstream somehow returned a known placeholder — do not trust/persist.
                (
                    ListOutcome::Placeholder,
                    Some(anyhow!("list returned placeholder profileArn")),
                )
            }
            Ok(_) => (ListOutcome::Empty, Some(anyhow!("empty profile list"))),
            Err(e) => (ListOutcome::Failed, Some(e)),
        };

    match decide_profile_action(credentials, list) {
        ResolveAction::Use(arn) => {
            let _ = token_manager.set_profile_arn(credential_id, Some(arn.clone()), provider);
            Ok(arn)
        }
        ResolveAction::Unsupported => Err(anyhow!(ProfileArnUnsupported)),
        ResolveAction::SoftUnavailable => Err(anyhow!(ProfileArnUnavailable)),
        ResolveAction::ForceRefresh => {
            match token_manager.force_refresh_token_for(credential_id).await {
                Ok(()) => {
                    if let Some(arn) = token_manager.profile_arn_of(credential_id) {
                        if !is_known_placeholder_profile_arn(&arn) {
                            return Ok(arn);
                        }
                        let _ = token_manager.clear_profile_arn(credential_id);
                    }
                    // refresh succeeded but no trusted profileArn — proceed without
                    Err(anyhow!(ProfileArnUnavailable))
                }
                Err(refresh_err) => {
                    if let Some(le) = list_err {
                        bail!("no available Kiro profile (list: {}; refresh: {})", le, refresh_err);
                    }
                    bail!("no available Kiro profile (refresh: {})", refresh_err);
                }
            }
        }
        ResolveAction::Fail => {
            if let Some(le) = list_err {
                bail!(
                    "no available Kiro profile (list: {}; no refreshToken to fall back on)",
                    le
                );
            }
            bail!("no available Kiro profile")
        }
    }
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

    /// IdC credential: authMethod=idc with clientId/clientSecret.
    fn idc_cred() -> KiroCredentials {
        let mut c = KiroCredentials::default();
        c.auth_method = Some("idc".to_string());
        c.client_id = Some("cid".to_string());
        c.client_secret = Some("sec".to_string());
        c.refresh_token = Some("rt".to_string());
        c
    }

    /// Social credential: refresh goes to the Kiro-owned endpoint.
    fn social_cred() -> KiroCredentials {
        let mut c = KiroCredentials::default();
        c.auth_method = Some("social".to_string());
        c.refresh_token = Some("rt".to_string());
        c
    }

    const MISSED: [ListOutcome; 3] = [
        ListOutcome::Failed,
        ListOutcome::Empty,
        ListOutcome::Placeholder,
    ];

    #[test]
    fn test_decide_idc_never_force_refreshes() {
        let c = idc_cred();
        for list in MISSED {
            assert_eq!(
                decide_profile_action(&c, list.clone()),
                ResolveAction::SoftUnavailable,
                "IdC must proceed without ARN, not force refresh (list: {:?})",
                list
            );
        }
    }

    #[test]
    fn test_decide_social_still_force_refreshes() {
        let c = social_cred();
        for list in MISSED {
            assert_eq!(
                decide_profile_action(&c, list.clone()),
                ResolveAction::ForceRefresh,
                "Social refresh may return an ARN; must not regress (list: {:?})",
                list
            );
        }
    }

    /// Contradictory but importable: explicit provider=BuilderId with authMethod=social.
    /// Refresh routes to the Kiro endpoint, so it must NOT be soft-released.
    #[test]
    fn test_decide_builder_provider_with_social_auth_force_refreshes() {
        let mut c = social_cred();
        c.provider = Some("BuilderId".to_string());
        for list in MISSED {
            assert_eq!(
                decide_profile_action(&c, list.clone()),
                ResolveAction::ForceRefresh,
                "provider=BuilderId must not override actual refresh routing (list: {:?})",
                list
            );
        }
    }

    /// Missing authMethod + clientId/clientSecret is inferred as idc, so refresh routes
    /// to OIDC and must be soft-released too.
    #[test]
    fn test_decide_missing_auth_method_with_client_creds_is_soft() {
        let mut c = idc_cred();
        c.auth_method = None;
        for list in MISSED {
            assert_eq!(
                decide_profile_action(&c, list.clone()),
                ResolveAction::SoftUnavailable,
                "inferred idc must be soft-released (list: {:?})",
                list
            );
        }
    }

    #[test]
    fn test_decide_resolved_arn_is_used() {
        let arn = "arn:aws:codewhisperer:us-east-1:1:profile/REAL".to_string();
        for c in [idc_cred(), social_cred()] {
            assert_eq!(
                decide_profile_action(&c, ListOutcome::Resolved(arn.clone())),
                ResolveAction::Use(arn.clone())
            );
        }
    }

    #[test]
    fn test_decide_api_key_unsupported() {
        let mut c = KiroCredentials::default();
        c.kiro_api_key = Some("ksk_x".to_string());
        c.auth_method = Some("api_key".to_string());
        for list in MISSED {
            assert_eq!(
                decide_profile_action(&c, list),
                ResolveAction::Unsupported
            );
        }
    }

    #[test]
    fn test_decide_social_without_refresh_token_fails() {
        let mut c = social_cred();
        c.refresh_token = None;
        for list in MISSED {
            assert_eq!(decide_profile_action(&c, list), ResolveAction::Fail);
        }
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

    // ============ external_idp 回归（逻辑未改，仅锁定行为）============

    fn external_cred() -> KiroCredentials {
        let mut c = KiroCredentials::default();
        c.refresh_token = Some("x".repeat(150));
        c.auth_method = Some("external_idp".to_string());
        c.client_id = Some("ms-cid".to_string());
        c.token_endpoint =
            Some("https://login.microsoftonline.com/t/oauth2/v2.0/token".to_string());
        c
    }

    #[test]
    fn test_supports_profiles_external_idp() {
        // supports_profiles 的 external 分支此前无测试覆盖
        let c = external_cred();
        assert!(
            supports_profiles(&c),
            "external_idp 应支持 profile ARN 解析"
        );

        // 由 provider 触发的等价路径
        let mut by_provider = KiroCredentials::default();
        by_provider.refresh_token = Some("x".repeat(150));
        by_provider.provider = Some("ExternalIdp".to_string());
        assert!(supports_profiles(&by_provider));
    }

    #[test]
    fn test_external_real_profile_arn_is_trusted() {
        // external 账号的 profileArn 是真实值而非占位，必须原样保留
        let mut c = external_cred();
        let real = "arn:aws:codewhisperer:us-east-1:000000000000:profile/FAKEEXTERNAL";
        c.profile_arn = Some(real.to_string());
        assert_eq!(trusted_profile_arn(&c), Some(real));

        // 占位值仍不可信
        c.profile_arn = Some(BUILDER_ID_PROFILE_ARN.to_string());
        assert!(trusted_profile_arn(&c).is_none());
    }

    #[test]
    fn test_external_uses_arn_from_list() {
        // 缓存命中由 resolve_profile_arn 在调用 decide 之前处理（trusted_profile_arn），
        // decide 只负责 list 阶段之后的决策。此处验证 list 得到可信 ARN 时直接采用。
        let c = external_cred();
        let real = "arn:aws:codewhisperer:us-east-1:000000000000:profile/FAKEEXTERNAL";
        assert_eq!(
            decide_profile_action(&c, ListOutcome::Resolved(real.to_string())),
            ResolveAction::Use(real.to_string())
        );
    }

    #[test]
    fn test_external_without_arn_currently_force_refreshes() {
        // 已知遗留（非期望终态）：Microsoft token 端点不返回 profileArn，
        // 逻辑上 external 应与 IdC 一样软放行；但 refresh_routes_to_idc 对
        // external 返回 false，故此处判定为 ForceRefresh——一次注定无收益的往返。
        // 修它需重新定义该谓词语义，会牵连本 capability 的既有 spec 场景。
        // 本测试锁定当前行为，使将来的修复必须显式更新它。
        let c = external_cred();
        assert_eq!(
            decide_profile_action(&c, ListOutcome::Failed),
            ResolveAction::ForceRefresh,
            "记录现状：external 目前落在强刷分支"
        );
    }

    #[test]
    fn test_external_api_key_still_unsupported() {
        let mut c = external_cred();
        c.kiro_api_key = Some("ksk_x".to_string());
        assert!(
            !supports_profiles(&c),
            "API Key 凭据不支持 profile，优先于 external 判定"
        );
    }
}
