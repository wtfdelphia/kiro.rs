//! external_idp（Microsoft Entra ID / Azure AD）token endpoint 校验与 OAuth2 刷新
//!
//! 导入文件是外部输入。若 endpoint 未严格校验，导入功能等于任意 SSRF 入口，
//! 且泄露的是 refresh token。因此本模块的校验器是安全边界，不是便利函数。

use url::{Host, Url};

/// 允许的 Microsoft 登录域（公有云 / 美国政府云 / 中国云）
///
/// 硬编码而非可配置：可配置的白名单等于可绕过的白名单——攻击面会从导入文件
/// 扩大到配置文件，而配置文件同样可能来自不可信来源（Docker 挂载 / CI 注入）。
const ALLOWED_HOSTS: &[&str] = &[
    "login.microsoftonline.com",
    "login.microsoftonline.us",
    "login.partner.microsoftonline.cn",
    "login.chinacloudapi.cn",
];

/// endpoint 被拒原因
///
/// 变体不携带原始 URL：含 userinfo 的输入其密码片段不得进入错误信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointRejected {
    /// 不是合法 URL
    NotAUrl,
    /// scheme 不是 https
    NotHttps,
    /// 含 userinfo（`https://host@evil.example` 形态）
    HasUserinfo,
    /// host 是 IP 字面量而非域名
    IpLiteral,
    /// host 是 localhost 或其子域
    Loopback,
    /// host 不在白名单内
    HostNotAllowed,
    /// 缺少 host
    NoHost,
}

impl std::fmt::Display for EndpointRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::NotAUrl => "token endpoint 不是合法 URL",
            Self::NotHttps => "token endpoint 必须使用 https",
            Self::HasUserinfo => "token endpoint 不得包含 userinfo",
            Self::IpLiteral => "token endpoint 不得使用 IP 地址",
            Self::Loopback => "token endpoint 不得指向本机",
            Self::HostNotAllowed => "token endpoint 的域名不在允许的 Microsoft 登录域内",
            Self::NoHost => "token endpoint 缺少域名",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for EndpointRejected {}

/// 校验 external_idp 的 token endpoint
///
/// 依据 URL 解析器给出的 host 判断，不做字符串前缀/后缀匹配——后者会被
/// `https://evil.example\.login.microsoftonline.com` 这类反斜杠归一化绕过。
pub fn validate_token_endpoint(raw: &str) -> Result<Url, EndpointRejected> {
    let parsed = Url::parse(raw.trim()).map_err(|_| EndpointRejected::NotAUrl)?;

    if parsed.scheme() != "https" {
        return Err(EndpointRejected::NotHttps);
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(EndpointRejected::HasUserinfo);
    }

    // 显式区分域名与 IP 字面量。KAM 只靠白名单隐式挡住 IP，意图不可读也不可断言。
    let domain = match parsed.host() {
        Some(Host::Domain(d)) => d.to_ascii_lowercase(),
        Some(Host::Ipv4(_)) | Some(Host::Ipv6(_)) => return Err(EndpointRejected::IpLiteral),
        None => return Err(EndpointRejected::NoHost),
    };

    if domain == "localhost" || domain.ends_with(".localhost") {
        return Err(EndpointRejected::Loopback);
    }

    let allowed = ALLOWED_HOSTS.iter().any(|suffix| {
        domain == *suffix || domain.ends_with(&format!(".{suffix}"))
    });
    if !allowed {
        return Err(EndpointRejected::HostNotAllowed);
    }

    Ok(parsed)
}

/// 由 issuerUrl 按 Microsoft v2.0 惯例派生 token endpoint
///
/// 派生结果再次走同一校验器：派生不绕过校验。
pub fn derive_token_endpoint_from_issuer(issuer: &str) -> Result<Url, EndpointRejected> {
    let trimmed = issuer.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(EndpointRejected::NotAUrl);
    }
    validate_token_endpoint(&format!("{trimmed}/oauth2/v2.0/token"))
}

/// 解析 external 凭据的有效 token endpoint：优先 tokenEndpoint，其次由 issuerUrl 派生
pub fn resolve_token_endpoint(
    token_endpoint: Option<&str>,
    issuer_url: Option<&str>,
) -> Result<Url, EndpointRejected> {
    if let Some(raw) = token_endpoint.map(str::trim).filter(|s| !s.is_empty()) {
        return validate_token_endpoint(raw);
    }
    if let Some(raw) = issuer_url.map(str::trim).filter(|s| !s.is_empty()) {
        return derive_token_endpoint_from_issuer(raw);
    }
    Err(EndpointRejected::NotAUrl)
}

/// 构造 OAuth2 refresh_token grant 的 form 字段
///
/// `client_secret` 与 `scope` 仅在非空时追加：公共客户端没有 secret，
/// 发空串会被 Microsoft 拒绝。
pub fn build_refresh_form(
    client_id: &str,
    refresh_token: &str,
    client_secret: Option<&str>,
    scopes: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("client_id", client_id.to_string()),
        ("refresh_token", refresh_token.to_string()),
    ];
    if let Some(scope) = scopes.map(str::trim).filter(|s| !s.is_empty()) {
        form.push(("scope", scope.to_string()));
    }
    if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
        form.push(("client_secret", secret.to_string()));
    }
    form
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_all_whitelisted_hosts() {
        for host in ALLOWED_HOSTS {
            let url = format!("https://{host}/tenant-id/oauth2/v2.0/token");
            assert!(
                validate_token_endpoint(&url).is_ok(),
                "白名单域应被接受: {url}"
            );
        }
    }

    #[test]
    fn accepts_subdomain_of_whitelisted_host() {
        for host in ALLOWED_HOSTS {
            let url = format!("https://sub.{host}/token");
            assert!(
                validate_token_endpoint(&url).is_ok(),
                "白名单子域应被接受: {url}"
            );
        }
    }

    #[test]
    fn accepts_uppercase_host() {
        assert!(
            validate_token_endpoint("https://LOGIN.MICROSOFTONLINE.COM/t/oauth2/v2.0/token")
                .is_ok()
        );
    }

    #[test]
    fn rejects_non_https() {
        assert_eq!(
            validate_token_endpoint("http://login.microsoftonline.com/token"),
            Err(EndpointRejected::NotHttps)
        );
    }

    #[test]
    fn rejects_non_url() {
        assert_eq!(
            validate_token_endpoint("not a url"),
            Err(EndpointRejected::NotAUrl)
        );
    }

    #[test]
    fn rejects_userinfo_bypass() {
        // 真实 host 是 attacker.example，白名单域只是 username
        assert_eq!(
            validate_token_endpoint("https://login.microsoftonline.com@attacker.example/token"),
            Err(EndpointRejected::HasUserinfo)
        );
    }

    #[test]
    fn rejects_userinfo_with_password() {
        assert_eq!(
            validate_token_endpoint("https://user:pass@login.microsoftonline.com/token"),
            Err(EndpointRejected::HasUserinfo)
        );
    }

    #[test]
    fn rejects_backslash_normalization_bypass() {
        // 字符串后缀匹配会误判通过；URL 解析器归一化后真实 host 是 evil.example
        let rejected = validate_token_endpoint(
            "https://evil.example\\.login.microsoftonline.com/token",
        );
        assert!(
            matches!(rejected, Err(EndpointRejected::HostNotAllowed)),
            "反斜杠绕过应被拒绝，实际: {rejected:?}"
        );
    }

    #[test]
    fn rejects_ipv4_literal() {
        assert_eq!(
            validate_token_endpoint("https://169.254.169.254/token"),
            Err(EndpointRejected::IpLiteral)
        );
    }

    #[test]
    fn rejects_ipv6_literal() {
        assert_eq!(
            validate_token_endpoint("https://[::1]/token"),
            Err(EndpointRejected::IpLiteral)
        );
    }

    #[test]
    fn rejects_localhost() {
        assert_eq!(
            validate_token_endpoint("https://localhost/token"),
            Err(EndpointRejected::Loopback)
        );
    }

    #[test]
    fn rejects_localhost_subdomain() {
        assert_eq!(
            validate_token_endpoint("https://evil.localhost/token"),
            Err(EndpointRejected::Loopback)
        );
    }

    #[test]
    fn rejects_suffix_disguise() {
        // 白名单域名出现在非后缀位置
        assert_eq!(
            validate_token_endpoint(
                "https://login.microsoftonline.com.attacker.example/token"
            ),
            Err(EndpointRejected::HostNotAllowed)
        );
    }

    #[test]
    fn rejects_prefix_disguise() {
        assert_eq!(
            validate_token_endpoint("https://evil-login.microsoftonline.com.bad.example/token"),
            Err(EndpointRejected::HostNotAllowed)
        );
    }

    #[test]
    fn rejects_lookalike_without_dot_boundary() {
        // notlogin.microsoftonline.com 不是 login.microsoftonline.com 的子域
        assert_eq!(
            validate_token_endpoint("https://xlogin.microsoftonline.com.evil.example/token"),
            Err(EndpointRejected::HostNotAllowed)
        );
    }

    #[test]
    fn error_display_never_leaks_credential_material() {
        let rejected =
            validate_token_endpoint("https://user:sup3rs3cret@attacker.example/token").unwrap_err();
        let shown = rejected.to_string();
        assert!(!shown.contains("sup3rs3cret"), "错误信息泄露了密码: {shown}");
        assert!(!shown.contains("attacker.example"), "错误信息含原始 host: {shown}");
    }

    #[test]
    fn derives_token_endpoint_from_issuer() {
        let url =
            derive_token_endpoint_from_issuer("https://login.microsoftonline.com/tenant-id")
                .expect("白名单 issuer 应派生成功");
        assert_eq!(
            url.as_str(),
            "https://login.microsoftonline.com/tenant-id/oauth2/v2.0/token"
        );
    }

    #[test]
    fn derives_ignores_trailing_slash() {
        let url =
            derive_token_endpoint_from_issuer("https://login.microsoftonline.com/tenant-id/")
                .expect("尾斜杠应被容忍");
        assert_eq!(
            url.as_str(),
            "https://login.microsoftonline.com/tenant-id/oauth2/v2.0/token"
        );
    }

    #[test]
    fn derived_endpoint_is_revalidated() {
        // 非白名单 issuer 派生出的 endpoint 同样被拒
        assert_eq!(
            derive_token_endpoint_from_issuer("https://attacker.example/tenant"),
            Err(EndpointRejected::HostNotAllowed)
        );
        assert_eq!(
            derive_token_endpoint_from_issuer("http://login.microsoftonline.com/t"),
            Err(EndpointRejected::NotHttps)
        );
    }

    #[test]
    fn resolve_prefers_token_endpoint_over_issuer() {
        let url = resolve_token_endpoint(
            Some("https://login.microsoftonline.us/t/oauth2/v2.0/token"),
            Some("https://login.microsoftonline.com/other"),
        )
        .expect("应使用显式 tokenEndpoint");
        assert_eq!(url.host_str(), Some("login.microsoftonline.us"));
    }

    #[test]
    fn resolve_falls_back_to_issuer() {
        let url = resolve_token_endpoint(
            None,
            Some("https://login.microsoftonline.com/tenant-id"),
        )
        .expect("应由 issuer 派生");
        assert!(url.as_str().ends_with("/oauth2/v2.0/token"));
    }

    #[test]
    fn resolve_treats_blank_as_absent() {
        assert_eq!(
            resolve_token_endpoint(Some("   "), None),
            Err(EndpointRejected::NotAUrl)
        );
        let url = resolve_token_endpoint(Some(""), Some("https://login.chinacloudapi.cn/t"))
            .expect("空 tokenEndpoint 应回退到 issuer");
        assert_eq!(url.host_str(), Some("login.chinacloudapi.cn"));
    }

    #[test]
    fn resolve_requires_one_of_them() {
        assert_eq!(
            resolve_token_endpoint(None, None),
            Err(EndpointRejected::NotAUrl)
        );
    }

    #[test]
    fn public_client_form_omits_client_secret() {
        let form = build_refresh_form("client-id-x", "refresh-x", None, None);
        assert!(
            !form.iter().any(|(k, _)| *k == "client_secret"),
            "公共客户端不得包含 client_secret 键"
        );
        assert!(!form.iter().any(|(k, _)| *k == "scope"));
        assert_eq!(
            form,
            vec![
                ("grant_type", "refresh_token".to_string()),
                ("client_id", "client-id-x".to_string()),
                ("refresh_token", "refresh-x".to_string()),
            ]
        );
    }

    #[test]
    fn blank_client_secret_is_omitted() {
        let form = build_refresh_form("cid", "rt", Some("   "), Some(""));
        assert!(!form.iter().any(|(k, _)| *k == "client_secret"));
        assert!(!form.iter().any(|(k, _)| *k == "scope"));
    }

    #[test]
    fn confidential_client_form_includes_secret_and_scope() {
        let form = build_refresh_form("cid", "rt", Some("sec"), Some("openid profile"));
        assert!(form.contains(&("client_secret", "sec".to_string())));
        assert!(form.contains(&("scope", "openid profile".to_string())));
        // 三个必填字段恒存在
        assert!(form.contains(&("grant_type", "refresh_token".to_string())));
        assert!(form.contains(&("client_id", "cid".to_string())));
        assert!(form.contains(&("refresh_token", "rt".to_string())));
    }
}
