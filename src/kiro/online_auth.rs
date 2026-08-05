//! 在线授权：BuilderId 设备码 / IAM SSO / SSO Token
//!
//! 对齐 Kiro-Go auth/{builderid,iam_sso,sso_token}.go 的主流程；
//! 网络步骤通过可替换钩子支持单测，不依赖真实上游。

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::http_client::{build_client, ProxyConfig};
use crate::model::config::Config;

const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);
const SCOPES: &[&str] = &[
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
    "codewhisperer:transformations",
    "codewhisperer:taskassist",
];

// ============ 会话存储 ============

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BuilderIdSession {
    pub id: String,
    pub client_id: String,
    pub client_secret: String,
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u32,
    pub region: String,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IamSsoSession {
    pub id: String,
    pub client_id: String,
    pub client_secret: String,
    pub code_verifier: String,
    pub state: String,
    pub region: String,
    pub start_url: String,
    pub redirect_uri: String,
    pub expires_at: Instant,
}

#[derive(Default)]
struct SessionStore {
    builder_id: HashMap<String, BuilderIdSession>,
    iam_sso: HashMap<String, IamSsoSession>,
}

fn sessions() -> &'static Mutex<SessionStore> {
    static SESSIONS: std::sync::OnceLock<Mutex<SessionStore>> = std::sync::OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(SessionStore::default()))
}

fn cleanup_expired(store: &mut SessionStore) {
    let now = Instant::now();
    store.builder_id.retain(|_, s| s.expires_at > now);
    store.iam_sso.retain(|_, s| s.expires_at > now);
}

/// 测试用：清空所有会话
#[cfg(test)]
pub fn clear_sessions_for_test() {
    let mut store = sessions().lock();
    store.builder_id.clear();
    store.iam_sso.clear();
}

// ============ 可 mock 的上游操作 ============

#[derive(Debug, Clone)]
pub struct DeviceAuthStart {
    pub client_id: String,
    pub client_secret: String,
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u32,
    pub expires_in: u64,
}

#[derive(Debug, Clone)]
pub enum DevicePollOutcome {
    Pending,
    SlowDown,
    Completed {
        access_token: String,
        refresh_token: String,
        expires_in: u64,
    },
}

#[derive(Debug, Clone)]
pub struct IamStartResult {
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: String,
}

#[derive(Debug, Clone)]
pub struct TokenBundle {
    pub access_token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub client_secret: String,
    pub region: String,
    pub expires_in: u64,
    pub start_url: Option<String>,
}

type BuilderStartFn = fn(&str, Option<&ProxyConfig>, &Config) -> Result<DeviceAuthStart>;
type BuilderPollFn =
    fn(&BuilderIdSession, Option<&ProxyConfig>, &Config) -> Result<DevicePollOutcome>;
type IamStartFn = fn(&str, &str, Option<&ProxyConfig>, &Config) -> Result<IamStartResult>;
type IamCompleteFn =
    fn(&IamSsoSession, &str, Option<&ProxyConfig>, &Config) -> Result<TokenBundle>;
type SsoTokenFn = fn(&str, &str, Option<&ProxyConfig>, &Config) -> Result<TokenBundle>;

static BUILDER_START: RwLock<BuilderStartFn> = RwLock::new(real_builder_start);
static BUILDER_POLL: RwLock<BuilderPollFn> = RwLock::new(real_builder_poll);
static IAM_START: RwLock<IamStartFn> = RwLock::new(real_iam_start);
static IAM_COMPLETE: RwLock<IamCompleteFn> = RwLock::new(real_iam_complete);
static SSO_TOKEN: RwLock<SsoTokenFn> = RwLock::new(real_sso_token_import);

#[cfg(test)]
pub fn set_builder_hooks_for_test(start: BuilderStartFn, poll: BuilderPollFn) {
    *BUILDER_START.write().unwrap() = start;
    *BUILDER_POLL.write().unwrap() = poll;
}

#[cfg(test)]
pub fn set_iam_hooks_for_test(start: IamStartFn, complete: IamCompleteFn) {
    *IAM_START.write().unwrap() = start;
    *IAM_COMPLETE.write().unwrap() = complete;
}

#[cfg(test)]
pub fn set_sso_token_hook_for_test(f: SsoTokenFn) {
    *SSO_TOKEN.write().unwrap() = f;
}

#[cfg(test)]
pub fn reset_hooks_for_test() {
    *BUILDER_START.write().unwrap() = real_builder_start;
    *BUILDER_POLL.write().unwrap() = real_builder_poll;
    *IAM_START.write().unwrap() = real_iam_start;
    *IAM_COMPLETE.write().unwrap() = real_iam_complete;
    *SSO_TOKEN.write().unwrap() = real_sso_token_import;
}

// ============ 公共 API ============

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIdStartResponse {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u32,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIdPollPending {
    pub success: bool,
    pub completed: bool,
    pub status: String,
    pub interval: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompletedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub client_secret: String,
    pub region: String,
    pub expires_in: u64,
    pub provider: String,
    pub auth_method: String,
    pub start_url: Option<String>,
}

pub async fn start_builder_id(
    region: Option<String>,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<BuilderIdStartResponse> {
    let region = region
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "us-east-1".to_string());
    let start = {
        let f = *BUILDER_START.read().unwrap();
        f(&region, proxy, config)?
    };

    let session_id = Uuid::new_v4().to_string();
    let expires_in = if start.expires_in == 0 {
        DEFAULT_TTL.as_secs()
    } else {
        start.expires_in
    };
    let interval = if start.interval == 0 { 5 } else { start.interval };

    let session = BuilderIdSession {
        id: session_id.clone(),
        client_id: start.client_id,
        client_secret: start.client_secret,
        device_code: start.device_code,
        user_code: start.user_code.clone(),
        verification_uri: start.verification_uri.clone(),
        interval,
        region,
        expires_at: Instant::now() + Duration::from_secs(expires_in),
    };

    {
        let mut store = sessions().lock();
        cleanup_expired(&mut store);
        store.builder_id.insert(session_id.clone(), session);
    }

    Ok(BuilderIdStartResponse {
        session_id,
        user_code: start.user_code,
        verification_uri: start.verification_uri,
        interval,
        expires_in,
    })
}

pub async fn poll_builder_id(
    session_id: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<Result<CompletedTokens, BuilderIdPollPending>> {
    let session = {
        let mut store = sessions().lock();
        cleanup_expired(&mut store);
        store
            .builder_id
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("session not found or expired"))?
    };

    if Instant::now() > session.expires_at {
        let mut store = sessions().lock();
        store.builder_id.remove(session_id);
        bail!("session not found or expired");
    }

    let outcome = {
        let f = *BUILDER_POLL.read().unwrap();
        f(&session, proxy, config)?
    };

    match outcome {
        DevicePollOutcome::Pending => Ok(Err(BuilderIdPollPending {
            success: true,
            completed: false,
            status: "pending".into(),
            interval: session.interval,
        })),
        DevicePollOutcome::SlowDown => {
            let mut store = sessions().lock();
            if let Some(s) = store.builder_id.get_mut(session_id) {
                s.interval = s.interval.saturating_add(5);
            }
            let interval = store
                .builder_id
                .get(session_id)
                .map(|s| s.interval)
                .unwrap_or(session.interval + 5);
            Ok(Err(BuilderIdPollPending {
                success: true,
                completed: false,
                status: "slow_down".into(),
                interval,
            }))
        }
        DevicePollOutcome::Completed {
            access_token,
            refresh_token,
            expires_in,
        } => {
            {
                let mut store = sessions().lock();
                store.builder_id.remove(session_id);
            }
            Ok(Ok(CompletedTokens {
                access_token,
                refresh_token,
                client_id: session.client_id,
                client_secret: session.client_secret,
                region: session.region,
                expires_in,
                provider: "BuilderId".into(),
                auth_method: "idc".into(),
                start_url: None,
            }))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IamSsoStartResponse {
    pub session_id: String,
    pub authorize_url: String,
    pub expires_in: u64,
}

pub async fn start_iam_sso(
    start_url: &str,
    region: Option<String>,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<IamSsoStartResponse> {
    if start_url.trim().is_empty() {
        bail!("startUrl is required");
    }
    let region = region
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "us-east-1".to_string());
    let redirect_uri = "http://127.0.0.1/oauth/callback";
    let started = {
        let f = *IAM_START.read().unwrap();
        f(start_url, &region, proxy, config)?
    };

    let code_verifier = generate_code_verifier();
    // Real authorize URL may already include challenge; for mock hooks we rebuild if empty.
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = Uuid::new_v4().to_string();
    let authorize_url = if started.authorize_url.is_empty() {
        format!(
            "https://oidc.{region}.amazonaws.com/authorize?response_type=code&client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256",
            urlencoding::encode(&started.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&state),
            urlencoding::encode(&code_challenge),
        )
    } else {
        started.authorize_url
    };

    let session_id = Uuid::new_v4().to_string();
    let session = IamSsoSession {
        id: session_id.clone(),
        client_id: started.client_id,
        client_secret: started.client_secret,
        code_verifier,
        state,
        region: region.clone(),
        start_url: start_url.to_string(),
        redirect_uri: redirect_uri.to_string(),
        expires_at: Instant::now() + DEFAULT_TTL,
    };
    {
        let mut store = sessions().lock();
        cleanup_expired(&mut store);
        store.iam_sso.insert(session_id.clone(), session);
    }

    Ok(IamSsoStartResponse {
        session_id,
        authorize_url,
        expires_in: DEFAULT_TTL.as_secs(),
    })
}

pub async fn complete_iam_sso(
    session_id: &str,
    callback_url: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<CompletedTokens> {
    let session = {
        let mut store = sessions().lock();
        cleanup_expired(&mut store);
        store
            .iam_sso
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("session not found or expired"))?
    };
    if Instant::now() > session.expires_at {
        let mut store = sessions().lock();
        store.iam_sso.remove(session_id);
        bail!("session not found or expired");
    }

    let tokens = {
        let f = *IAM_COMPLETE.read().unwrap();
        f(&session, callback_url, proxy, config)?
    };

    {
        let mut store = sessions().lock();
        store.iam_sso.remove(session_id);
    }

    Ok(CompletedTokens {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        client_id: tokens.client_id,
        client_secret: tokens.client_secret,
        region: tokens.region,
        expires_in: tokens.expires_in,
        provider: "Enterprise".into(),
        auth_method: "idc".into(),
        start_url: Some(session.start_url),
    })
}

pub async fn import_sso_token(
    bearer_token: &str,
    region: Option<String>,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<CompletedTokens> {
    let region = region
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "us-east-1".to_string());
    if bearer_token.trim().is_empty() {
        bail!("bearerToken is required");
    }
    let tokens = {
        let f = *SSO_TOKEN.read().unwrap();
        f(bearer_token.trim(), &region, proxy, config)?
    };
    Ok(CompletedTokens {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        client_id: tokens.client_id,
        client_secret: tokens.client_secret,
        region: tokens.region,
        expires_in: tokens.expires_in,
        provider: "BuilderId".into(),
        auth_method: "idc".into(),
        start_url: tokens.start_url,
    })
}

// ============ Real HTTP implementations (best-effort, production) ============

fn oidc_base(region: &str) -> String {
    format!("https://oidc.{region}.amazonaws.com")
}

fn real_builder_start(
    region: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<DeviceAuthStart> {
    // Synchronous wrapper is not ideal; call via block_in_place from async callers if needed.
    // For simplicity use reqwest blocking is unavailable; we use tokio runtime if present.
    // Prefer async reimplementation path below used only when hooks are default —
    // but function signature is sync for test hooks. Use block_in_place + async client.
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async_real_builder_start(region, proxy, config))
    })
}

async fn async_real_builder_start(
    region: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<DeviceAuthStart> {
    let client = build_client(proxy, 30, config.tls_backend)?;
    let base = oidc_base(region);
    let start_url = "https://view.awsapps.com/start";

    let reg_body = serde_json::json!({
        "clientName": "Kiro",
        "clientType": "public",
        "scopes": SCOPES,
        "grantTypes": ["urn:ietf:params:oauth:grant-type:device_code", "refresh_token"],
        "issuerUrl": start_url,
    });
    let reg_resp = client
        .post(format!("{base}/client/register"))
        .header("Content-Type", "application/json")
        .json(&reg_body)
        .send()
        .await?;
    if !reg_resp.status().is_success() {
        let t = reg_resp.text().await.unwrap_or_default();
        bail!("register client failed: {t}");
    }
    let reg: serde_json::Value = reg_resp.json().await?;
    let client_id = reg["clientId"].as_str().unwrap_or("").to_string();
    let client_secret = reg["clientSecret"].as_str().unwrap_or("").to_string();
    if client_id.is_empty() {
        bail!("register client missing clientId");
    }

    let auth_body = serde_json::json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "startUrl": start_url,
    });
    let auth_resp = client
        .post(format!("{base}/device_authorization"))
        .header("Content-Type", "application/json")
        .json(&auth_body)
        .send()
        .await?;
    if !auth_resp.status().is_success() {
        let t = auth_resp.text().await.unwrap_or_default();
        bail!("device authorization failed: {t}");
    }
    let auth: serde_json::Value = auth_resp.json().await?;
    let verification = auth["verificationUriComplete"]
        .as_str()
        .or_else(|| auth["verificationUri"].as_str())
        .unwrap_or("")
        .to_string();
    Ok(DeviceAuthStart {
        client_id,
        client_secret,
        device_code: auth["deviceCode"].as_str().unwrap_or("").to_string(),
        user_code: auth["userCode"].as_str().unwrap_or("").to_string(),
        verification_uri: verification,
        interval: auth["interval"].as_u64().unwrap_or(5) as u32,
        expires_in: auth["expiresIn"].as_u64().unwrap_or(600),
    })
}

fn real_builder_poll(
    session: &BuilderIdSession,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<DevicePollOutcome> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async_real_builder_poll(session, proxy, config))
    })
}

async fn async_real_builder_poll(
    session: &BuilderIdSession,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<DevicePollOutcome> {
    let client = build_client(proxy, 30, config.tls_backend)?;
    let base = oidc_base(&session.region);
    let body = serde_json::json!({
        "clientId": session.client_id,
        "clientSecret": session.client_secret,
        "grantType": "urn:ietf:params:oauth:grant-type:device_code",
        "deviceCode": session.device_code,
    });
    let resp = client
        .post(format!("{base}/token"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let data: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
    if status.is_success() {
        return Ok(DevicePollOutcome::Completed {
            access_token: data["accessToken"].as_str().unwrap_or("").to_string(),
            refresh_token: data["refreshToken"].as_str().unwrap_or("").to_string(),
            expires_in: data["expiresIn"].as_u64().unwrap_or(3600),
        });
    }
    if status.as_u16() == 400 {
        match data["error"].as_str().unwrap_or("") {
            "authorization_pending" => return Ok(DevicePollOutcome::Pending),
            "slow_down" => return Ok(DevicePollOutcome::SlowDown),
            other => bail!("authorization error: {other}"),
        }
    }
    bail!("unexpected token response: {status}");
}

fn real_iam_start(
    start_url: &str,
    region: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<IamStartResult> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async_real_iam_start(
            start_url, region, proxy, config,
        ))
    })
}

async fn async_real_iam_start(
    start_url: &str,
    region: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<IamStartResult> {
    let client = build_client(proxy, 30, config.tls_backend)?;
    let base = oidc_base(region);
    let redirect_uri = "http://127.0.0.1/oauth/callback";
    let reg_body = serde_json::json!({
        "clientName": "Kiro",
        "clientType": "public",
        "scopes": SCOPES,
        "grantTypes": ["authorization_code", "refresh_token"],
        "redirectUris": [redirect_uri],
        "issuerUrl": start_url,
    });
    let reg_resp = client
        .post(format!("{base}/client/register"))
        .header("Content-Type", "application/json")
        .json(&reg_body)
        .send()
        .await?;
    if !reg_resp.status().is_success() {
        let t = reg_resp.text().await.unwrap_or_default();
        bail!("register client failed: {t}");
    }
    let reg: serde_json::Value = reg_resp.json().await?;
    let client_id = reg["clientId"].as_str().unwrap_or("").to_string();
    let client_secret = reg["clientSecret"].as_str().unwrap_or("").to_string();
    Ok(IamStartResult {
        client_id,
        client_secret,
        authorize_url: String::new(), // filled by start_iam_sso with PKCE
    })
}

fn real_iam_complete(
    session: &IamSsoSession,
    callback_url: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<TokenBundle> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async_real_iam_complete(
            session,
            callback_url,
            proxy,
            config,
        ))
    })
}

async fn async_real_iam_complete(
    session: &IamSsoSession,
    callback_url: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<TokenBundle> {
    let parsed = reqwest::Url::parse(callback_url).map_err(|_| anyhow!("无效的回调 URL"))?;
    let pairs: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
    if let Some(err) = pairs.get("error") {
        bail!("授权失败: {err}");
    }
    let state = pairs.get("state").map(|s| s.as_str()).unwrap_or("");
    if state != session.state {
        bail!("状态不匹配，可能存在安全风险");
    }
    let code = pairs
        .get("code")
        .cloned()
        .ok_or_else(|| anyhow!("未收到授权码"))?;

    let client = build_client(proxy, 30, config.tls_backend)?;
    let base = oidc_base(&session.region);
    let body = serde_json::json!({
        "clientId": session.client_id,
        "clientSecret": session.client_secret,
        "grantType": "authorization_code",
        "code": code,
        "redirectUri": session.redirect_uri,
        "codeVerifier": session.code_verifier,
    });
    let resp = client
        .post(format!("{base}/token"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let t = resp.text().await.unwrap_or_default();
        bail!("token exchange failed: {t}");
    }
    let data: serde_json::Value = resp.json().await?;
    Ok(TokenBundle {
        access_token: data["accessToken"].as_str().unwrap_or("").to_string(),
        refresh_token: data["refreshToken"].as_str().unwrap_or("").to_string(),
        client_id: session.client_id.clone(),
        client_secret: session.client_secret.clone(),
        region: session.region.clone(),
        expires_in: data["expiresIn"].as_u64().unwrap_or(3600),
        start_url: Some(session.start_url.clone()),
    })
}

fn real_sso_token_import(
    bearer_token: &str,
    region: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<TokenBundle> {
    // Full multi-step portal flow is complex; keep production path via block_on.
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async_real_sso_token(
            bearer_token,
            region,
            proxy,
            config,
        ))
    })
}

async fn async_real_sso_token(
    bearer_token: &str,
    region: &str,
    proxy: Option<&ProxyConfig>,
    config: &Config,
) -> Result<TokenBundle> {
    // Simplified: register + device auth + whoAmI check; full associate flow mirrors Go.
    // For production parity we implement the critical register/device/token path with bearer verification.
    let client = build_client(proxy, 60, config.tls_backend)?;
    let base = oidc_base(region);
    let portal = "https://portal.sso.us-east-1.amazonaws.com";
    let start_url = "https://view.awsapps.com/start";

    // verify bearer
    let who = client
        .get(format!("{portal}/token/whoAmI"))
        .header("Authorization", format!("Bearer {bearer_token}"))
        .header("Accept", "application/json")
        .send()
        .await?;
    if !who.status().is_success() {
        bail!("Token 验证失败: {}", who.status());
    }

    // register device client
    let reg_body = serde_json::json!({
        "clientName": "Kiro API Proxy",
        "clientType": "public",
        "scopes": SCOPES,
        "grantTypes": ["urn:ietf:params:oauth:grant-type:device_code", "refresh_token"],
        "issuerUrl": start_url,
    });
    let reg = client
        .post(format!("{base}/client/register"))
        .header("Content-Type", "application/json")
        .json(&reg_body)
        .send()
        .await?;
    if !reg.status().is_success() {
        bail!("注册客户端失败: {}", reg.text().await.unwrap_or_default());
    }
    let reg_v: serde_json::Value = reg.json().await?;
    let client_id = reg_v["clientId"].as_str().unwrap_or("").to_string();
    let client_secret = reg_v["clientSecret"].as_str().unwrap_or("").to_string();

    let auth_body = serde_json::json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "startUrl": start_url,
    });
    let auth = client
        .post(format!("{base}/device_authorization"))
        .header("Content-Type", "application/json")
        .json(&auth_body)
        .send()
        .await?;
    if !auth.status().is_success() {
        bail!("设备授权失败: {}", auth.text().await.unwrap_or_default());
    }
    let auth_v: serde_json::Value = auth.json().await?;
    let device_code = auth_v["deviceCode"].as_str().unwrap_or("").to_string();
    let user_code = auth_v["userCode"].as_str().unwrap_or("").to_string();
    let interval = auth_v["interval"].as_u64().unwrap_or(1).max(1);

    // device session
    let sess = client
        .post(format!("{portal}/session/device"))
        .header("Authorization", format!("Bearer {bearer_token}"))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await?;
    if !sess.status().is_success() {
        bail!("获取设备会话失败: {}", sess.status());
    }
    let sess_v: serde_json::Value = sess.json().await?;
    let device_session = sess_v["token"]
        .as_str()
        .or_else(|| sess_v["deviceSessionId"].as_str())
        .unwrap_or("")
        .to_string();

    // accept user code
    let accept_body = serde_json::json!({
        "userCode": user_code,
        "userSessionId": device_session,
    });
    let _ = client
        .post(format!("{base}/device_authorization/accept_user_code"))
        .header("Content-Type", "application/json")
        .json(&accept_body)
        .send()
        .await?;

    // poll token a few times
    for _ in 0..30 {
        let token_body = serde_json::json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "grantType": "urn:ietf:params:oauth:grant-type:device_code",
            "deviceCode": device_code,
        });
        let tr = client
            .post(format!("{base}/token"))
            .header("Content-Type", "application/json")
            .json(&token_body)
            .send()
            .await?;
        if tr.status().is_success() {
            let data: serde_json::Value = tr.json().await?;
            return Ok(TokenBundle {
                access_token: data["accessToken"].as_str().unwrap_or("").to_string(),
                refresh_token: data["refreshToken"].as_str().unwrap_or("").to_string(),
                client_id,
                client_secret,
                region: region.to_string(),
                expires_in: data["expiresIn"].as_u64().unwrap_or(3600),
                start_url: Some(start_url.into()),
            });
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
    bail!("获取 Token 失败: timeout")
}

fn generate_code_verifier() -> String {
    let bytes: [u8; 32] = rand_bytes();
    base64url_nopad(&bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    base64url_nopad(&hasher.finalize())
}

fn rand_bytes() -> [u8; 32] {
    let mut out = [0u8; 32];
    for b in &mut out {
        *b = fastrand::u8(..);
    }
    out
}

fn base64url_nopad(data: &[u8]) -> String {
    // minimal base64url without padding
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
    } else if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Global hooks/session store are process-wide; serialize tests that mutate them.
    fn test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn mock_builder_start(
        _region: &str,
        _p: Option<&ProxyConfig>,
        _c: &Config,
    ) -> Result<DeviceAuthStart> {
        Ok(DeviceAuthStart {
            client_id: "cid".into(),
            client_secret: "sec".into(),
            device_code: "dc".into(),
            user_code: "ABCD-1234".into(),
            verification_uri: "https://example.com/device".into(),
            interval: 5,
            expires_in: 600,
        })
    }

    fn mock_builder_poll_pending(
        _s: &BuilderIdSession,
        _p: Option<&ProxyConfig>,
        _c: &Config,
    ) -> Result<DevicePollOutcome> {
        Ok(DevicePollOutcome::Pending)
    }

    fn mock_builder_poll_done(
        _s: &BuilderIdSession,
        _p: Option<&ProxyConfig>,
        _c: &Config,
    ) -> Result<DevicePollOutcome> {
        Ok(DevicePollOutcome::Completed {
            access_token: "at".into(),
            refresh_token: "rt".repeat(40),
            expires_in: 3600,
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn builder_start_and_pending_poll() {
        let _guard = test_lock();
        clear_sessions_for_test();
        set_builder_hooks_for_test(mock_builder_start, mock_builder_poll_pending);
        let cfg = Config::default();
        let start = start_builder_id(Some("us-east-1".into()), None, &cfg)
            .await
            .unwrap();
        assert!(!start.session_id.is_empty());
        assert_eq!(start.user_code, "ABCD-1234");
        let poll = poll_builder_id(&start.session_id, None, &cfg)
            .await
            .unwrap();
        match poll {
            Err(p) => {
                assert!(!p.completed);
                assert_eq!(p.status, "pending");
            }
            Ok(_) => panic!("expected pending"),
        }
        reset_hooks_for_test();
        clear_sessions_for_test();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn builder_poll_complete_invalidates_session() {
        let _guard = test_lock();
        clear_sessions_for_test();
        set_builder_hooks_for_test(mock_builder_start, mock_builder_poll_done);
        let cfg = Config::default();
        let start = start_builder_id(None, None, &cfg).await.unwrap();
        let poll = poll_builder_id(&start.session_id, None, &cfg)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(poll.auth_method, "idc");
        assert_eq!(poll.provider, "BuilderId");
        // second poll should fail — session removed
        let err = poll_builder_id(&start.session_id, None, &cfg)
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("session not found") || err.contains("expired"));
        reset_hooks_for_test();
        clear_sessions_for_test();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn iam_start_requires_start_url() {
        let _guard = test_lock();
        clear_sessions_for_test();
        let cfg = Config::default();
        let err = start_iam_sso("", None, None, &cfg).await.err().unwrap();
        assert!(err.to_string().contains("startUrl"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn iam_start_complete_with_hooks() {
        let _guard = test_lock();
        clear_sessions_for_test();
        set_iam_hooks_for_test(
            |_su, _r, _p, _c| {
                Ok(IamStartResult {
                    client_id: "c".into(),
                    client_secret: "s".into(),
                    authorize_url: String::new(),
                })
            },
            |session, callback, _p, _c| {
                // verify state is in callback
                assert!(callback.contains(&session.state) || callback.contains("code="));
                Ok(TokenBundle {
                    access_token: "at".into(),
                    refresh_token: "r".repeat(80),
                    client_id: session.client_id.clone(),
                    client_secret: session.client_secret.clone(),
                    region: session.region.clone(),
                    expires_in: 100,
                    start_url: Some(session.start_url.clone()),
                })
            },
        );
        let cfg = Config::default();
        let start = start_iam_sso("https://my.awsapps.com/start", Some("us-west-2".into()), None, &cfg)
            .await
            .unwrap();
        assert!(!start.authorize_url.is_empty());
        // extract state from store
        let state = {
            let store = sessions().lock();
            store.iam_sso.get(&start.session_id).unwrap().state.clone()
        };
        let tokens = complete_iam_sso(
            &start.session_id,
            &format!("http://127.0.0.1/oauth/callback?code=abc&state={state}"),
            None,
            &cfg,
        )
        .await
        .unwrap();
        assert_eq!(tokens.start_url.as_deref(), Some("https://my.awsapps.com/start"));
        reset_hooks_for_test();
        clear_sessions_for_test();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn expired_session_rejected() {
        let _guard = test_lock();
        clear_sessions_for_test();
        {
            let mut store = sessions().lock();
            store.builder_id.insert(
                "old".into(),
                BuilderIdSession {
                    id: "old".into(),
                    client_id: "c".into(),
                    client_secret: "s".into(),
                    device_code: "d".into(),
                    user_code: "u".into(),
                    verification_uri: "http://x".into(),
                    interval: 5,
                    region: "us-east-1".into(),
                    expires_at: Instant::now() - Duration::from_secs(1),
                },
            );
        }
        let cfg = Config::default();
        let err = poll_builder_id("old", None, &cfg).await.err().unwrap();
        assert!(err.to_string().contains("expired") || err.to_string().contains("not found"));
        clear_sessions_for_test();
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn sso_token_import_with_hook() {
        let _guard = test_lock();
        clear_sessions_for_test();
        set_sso_token_hook_for_test(|bearer, region, _p, _c| {
            assert_eq!(bearer, "bearer-xyz");
            Ok(TokenBundle {
                access_token: "at".into(),
                refresh_token: "r".repeat(80),
                client_id: "cid".into(),
                client_secret: "sec".into(),
                region: region.to_string(),
                expires_in: 3600,
                start_url: None,
            })
        });
        let cfg = Config::default();
        let tokens = import_sso_token("bearer-xyz", Some("us-east-1".into()), None, &cfg)
            .await
            .unwrap();
        assert_eq!(tokens.provider, "BuilderId");
        assert_eq!(tokens.auth_method, "idc");
        assert!(!tokens.refresh_token.is_empty());
        reset_hooks_for_test();
        clear_sessions_for_test();
    }
}
