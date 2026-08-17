use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TlsBackend {
    Rustls,
    NativeTls,
}

impl Default for TlsBackend {
    fn default() -> Self {
        Self::Rustls
    }
}

/// KNA 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_region")]
    pub region: String,

    /// Auth Region（用于 Token 刷新），未配置时回退到 region
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_region: Option<String>,

    /// API Region（用于 API 请求），未配置时回退到 region
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_region: Option<String>,

    #[serde(default = "default_kiro_version")]
    pub kiro_version: String,

    #[serde(default)]
    pub machine_id: Option<String>,

    #[serde(default)]
    pub api_key: Option<String>,

    #[serde(default = "default_system_version")]
    pub system_version: String,

    #[serde(default = "default_node_version")]
    pub node_version: String,

    #[serde(default = "default_tls_backend")]
    pub tls_backend: TlsBackend,

    /// 外部 count_tokens API 地址（可选）
    #[serde(default)]
    pub count_tokens_api_url: Option<String>,

    /// count_tokens API 密钥（可选）
    #[serde(default)]
    pub count_tokens_api_key: Option<String>,

    /// count_tokens API 认证类型（可选，"x-api-key" 或 "bearer"，默认 "x-api-key"）
    #[serde(default = "default_count_tokens_auth_type")]
    pub count_tokens_auth_type: String,

    /// HTTP 代理地址（可选）
    /// 支持格式: http://host:port, https://host:port, socks5://host:port
    #[serde(default)]
    pub proxy_url: Option<String>,

    /// 代理认证用户名（可选）
    #[serde(default)]
    pub proxy_username: Option<String>,

    /// 代理认证密码（可选）
    #[serde(default)]
    pub proxy_password: Option<String>,

    /// Admin API 密钥（可选，启用 Admin API 功能）
    #[serde(default)]
    pub admin_api_key: Option<String>,

    /// 是否要求客户端 API Key（默认 true，兼容现网）
    #[serde(default = "default_require_api_key")]
    pub require_api_key: bool,

    /// 负载均衡模式（"priority" 或 "balanced"）
    #[serde(default = "default_load_balancing_mode")]
    pub load_balancing_mode: String,

    /// 是否开启非流式响应的 thinking 块提取（默认 true）
    ///
    /// 启用后，非流式响应中的 `<thinking>...</thinking>` 标签会被解析为
    /// 独立的 `{"type": "thinking", ...}` 内容块,与流式响应行为一致。
    #[serde(default = "default_extract_thinking")]
    pub extract_thinking: bool,

    /// 默认端点名称（凭据未显式指定 endpoint 时使用，默认 "ide"）
    #[serde(default = "default_endpoint")]
    pub default_endpoint: String,

    /// 端点特定的配置
    ///
    /// 键为端点名（如 "ide" / "cli"），值为该端点自由定义的参数对象。
    /// 未在此表出现的端点沿用实现内置默认值。
    #[serde(default)]
    pub endpoints: HashMap<String, serde_json::Value>,

    /// 模型解析策略（别名 / auto / catalog 透传）
    #[serde(default)]
    pub model_resolution: ModelResolutionConfig,

    /// 是否启用 web_search 代执行（仅 `/v1/responses` 端点，默认 true）
    ///
    /// 该端点的 web_search 工具判定较宽（含 `web_search_20250305` 等形状），
    /// 关闭后此类工具走正常 tools 路径交给模型自行决定。
    #[serde(default = "default_web_search_emulation")]
    pub web_search_emulation: bool,

    /// WebSocket ingress（`GET /v1/responses`）运行时设置（默认启用，可热加载）
    #[serde(default)]
    pub websocket: WsSettings,

    /// 配置文件路径（运行时元数据，不写入 JSON）
    #[serde(skip)]
    config_path: Option<PathBuf>,
}

/// 模型解析配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelResolutionConfig {
    /// auto 映射到的默认聊天模型
    #[serde(default = "default_chat_model")]
    pub default_chat_model: String,

    /// 是否允许 catalog 命中的上游 id 透传
    #[serde(default = "default_allow_catalog_passthrough")]
    pub allow_catalog_passthrough: bool,

    /// 是否在 /v1/models 额外暴露兼容别名（gpt-4o 等）
    #[serde(default)]
    pub expose_compat_aliases_in_models: bool,

    /// 可选自定义兼容别名（覆盖内置表同名项）
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub compat_aliases: HashMap<String, String>,
}

impl Default for ModelResolutionConfig {
    fn default() -> Self {
        Self {
            default_chat_model: default_chat_model(),
            allow_catalog_passthrough: default_allow_catalog_passthrough(),
            expose_compat_aliases_in_models: false,
            compat_aliases: HashMap::new(),
        }
    }
}

/// WebSocket 传输模式
///
/// - `http_bridge`：客户端保持 WS，每个 turn 翻译为一次上游 HTTP/SSE 调用
///   （当前唯一实现的模式）
/// - `passthrough`：WS→WS 帧中继，**预留未实现**；选中时握手前返回 501
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WsTransportMode {
    HttpBridge,
    Passthrough,
}

impl Default for WsTransportMode {
    fn default() -> Self {
        Self::HttpBridge
    }
}

/// 未知 mode 值回落 http_bridge 并告警（防御配置笔误），不阻断启动
impl<'de> Deserialize<'de> for WsTransportMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        match raw.trim() {
            "http_bridge" => Ok(Self::HttpBridge),
            "passthrough" => Ok(Self::Passthrough),
            other => {
                tracing::warn!(
                    mode = %other,
                    "websocket.mode 无法识别，回落为 http_bridge"
                );
                Ok(Self::HttpBridge)
            }
        }
    }
}

/// WebSocket ingress 运行时设置
///
/// JSON 字段名为 camelCase（与 `Config` 一致）；全部字段支持 Admin API 热加载，
/// 语义见 `docs/websocket-support-optimization-design.md` §4.7。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsSettings {
    /// 总开关；关闭时新 upgrade 被 503 拒绝，存量会话自然终态
    #[serde(default = "default_ws_enabled")]
    pub enabled: bool,

    /// 传输模式：http_bridge（默认）/ passthrough（预留）
    #[serde(default)]
    pub mode: WsTransportMode,

    /// 全局最大并发 WS 连接数
    #[serde(default = "default_ws_max_connections")]
    pub max_connections: usize,

    /// 升级后首帧等待上限（秒）
    #[serde(default = "default_ws_first_message_timeout")]
    pub client_first_message_timeout_seconds: u64,

    /// turn 间空闲上限（秒），0 表示关闭
    #[serde(default = "default_ws_idle_timeout")]
    pub inter_turn_idle_timeout_seconds: u64,

    /// 单条 WS 消息上限（字节）
    #[serde(default = "default_ws_max_message_bytes")]
    pub max_message_bytes: usize,

    /// 单个 turn 的上游读取超时（秒）
    #[serde(default = "default_ws_upstream_read_timeout")]
    pub upstream_read_timeout_seconds: u64,
}

impl Default for WsSettings {
    fn default() -> Self {
        Self {
            enabled: default_ws_enabled(),
            mode: WsTransportMode::HttpBridge,
            max_connections: default_ws_max_connections(),
            client_first_message_timeout_seconds: default_ws_first_message_timeout(),
            inter_turn_idle_timeout_seconds: default_ws_idle_timeout(),
            max_message_bytes: default_ws_max_message_bytes(),
            upstream_read_timeout_seconds: default_ws_upstream_read_timeout(),
        }
    }
}

fn default_ws_enabled() -> bool {
    true
}

fn default_ws_max_connections() -> usize {
    64
}

fn default_ws_first_message_timeout() -> u64 {
    30
}

fn default_ws_idle_timeout() -> u64 {
    1800
}

fn default_ws_max_message_bytes() -> usize {
    32 * 1024 * 1024
}

fn default_ws_upstream_read_timeout() -> u64 {
    900
}

fn default_chat_model() -> String {
    "claude-sonnet-4.6".to_string()
}

fn default_allow_catalog_passthrough() -> bool {
    true
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_region() -> String {
    "us-east-1".to_string()
}

fn default_kiro_version() -> String {
    "0.11.107".to_string()
}

fn default_system_version() -> String {
    const SYSTEM_VERSIONS: &[&str] = &["darwin#24.6.0", "win32#10.0.22631"];
    SYSTEM_VERSIONS[fastrand::usize(..SYSTEM_VERSIONS.len())].to_string()
}

fn default_node_version() -> String {
    "22.22.0".to_string()
}

fn default_count_tokens_auth_type() -> String {
    "x-api-key".to_string()
}

fn default_tls_backend() -> TlsBackend {
    TlsBackend::Rustls
}

fn default_load_balancing_mode() -> String {
    "priority".to_string()
}

fn default_extract_thinking() -> bool {
    true
}

fn default_web_search_emulation() -> bool {
    true
}

fn default_require_api_key() -> bool {
    true
}

fn default_endpoint() -> String {
    crate::kiro::endpoint::ide::IDE_ENDPOINT_NAME.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            region: default_region(),
            auth_region: None,
            api_region: None,
            kiro_version: default_kiro_version(),
            machine_id: None,
            api_key: None,
            system_version: default_system_version(),
            node_version: default_node_version(),
            tls_backend: default_tls_backend(),
            count_tokens_api_url: None,
            count_tokens_api_key: None,
            count_tokens_auth_type: default_count_tokens_auth_type(),
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            admin_api_key: None,
            require_api_key: default_require_api_key(),
            load_balancing_mode: default_load_balancing_mode(),
            extract_thinking: default_extract_thinking(),
            default_endpoint: default_endpoint(),
            endpoints: HashMap::new(),
            model_resolution: ModelResolutionConfig::default(),
            web_search_emulation: default_web_search_emulation(),
            websocket: WsSettings::default(),
            config_path: None,
        }
    }
}

impl Config {
    /// 获取默认配置文件路径
    pub fn default_config_path() -> &'static str {
        "config.json"
    }

    /// 获取有效的 Auth Region（用于 Token 刷新）
    /// 优先使用 auth_region，未配置时回退到 region
    pub fn effective_auth_region(&self) -> &str {
        self.auth_region.as_deref().unwrap_or(&self.region)
    }

    /// 获取有效的 API Region（用于 API 请求）
    /// 优先使用 api_region，未配置时回退到 region
    pub fn effective_api_region(&self) -> &str {
        self.api_region.as_deref().unwrap_or(&self.region)
    }

    /// 从文件加载配置
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            // 配置文件不存在，返回默认配置
            let mut config = Self::default();
            config.config_path = Some(path.to_path_buf());
            return Ok(config);
        }

        let content = fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&content)?;
        config.config_path = Some(path.to_path_buf());
        Ok(config)
    }

    /// 获取配置文件路径（如果有）
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// 将当前配置写回原始配置文件
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self
            .config_path
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("配置文件路径未知，无法保存配置"))?;

        let content = serde_json::to_string_pretty(self).context("序列化配置失败")?;
        fs::write(path, content)
            .with_context(|| format!("写入配置文件失败: {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 任务 1.2：旧 config.json（无 websocket 块）加载成功并取默认值
    #[test]
    fn old_config_without_websocket_block_loads_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"host":"0.0.0.0","port":9000}"#)
            .expect("旧配置（无 websocket 块）应可加载");
        assert!(cfg.websocket.enabled);
        assert_eq!(cfg.websocket.mode, WsTransportMode::HttpBridge);
        assert_eq!(cfg.websocket.max_connections, 64);
        assert_eq!(cfg.websocket.client_first_message_timeout_seconds, 30);
        assert_eq!(cfg.websocket.inter_turn_idle_timeout_seconds, 1800);
        assert_eq!(cfg.websocket.max_message_bytes, 32 * 1024 * 1024);
        assert_eq!(cfg.websocket.upstream_read_timeout_seconds, 900);
    }

    /// 任务 1.2：websocket 块按 camelCase 解析
    #[test]
    fn websocket_block_parses_camel_case() {
        let cfg: Config = serde_json::from_str(
            r#"{"websocket":{"enabled":false,"mode":"http_bridge","maxConnections":8,
                "clientFirstMessageTimeoutSeconds":5,"interTurnIdleTimeoutSeconds":0,
                "maxMessageBytes":1024,"upstreamReadTimeoutSeconds":60}}"#,
        )
        .expect("websocket 块应可解析");
        assert!(!cfg.websocket.enabled);
        assert_eq!(cfg.websocket.max_connections, 8);
        assert_eq!(cfg.websocket.client_first_message_timeout_seconds, 5);
        assert_eq!(cfg.websocket.inter_turn_idle_timeout_seconds, 0);
        assert_eq!(cfg.websocket.max_message_bytes, 1024);
        assert_eq!(cfg.websocket.upstream_read_timeout_seconds, 60);
    }

    /// 任务 1.3：未知 mode 值回落 http_bridge（warn 由 tracing 输出，此处断言结果）
    #[test]
    fn unknown_ws_mode_falls_back_to_http_bridge() {
        let mode: WsTransportMode =
            serde_json::from_str(r#""weird_mode""#).expect("未知 mode 不应导致解析失败");
        assert_eq!(mode, WsTransportMode::HttpBridge);

        let passthrough: WsTransportMode =
            serde_json::from_str(r#""passthrough""#).expect("passthrough 应可解析");
        assert_eq!(passthrough, WsTransportMode::Passthrough);

        let serialized = serde_json::to_string(&WsTransportMode::HttpBridge).unwrap();
        assert_eq!(serialized, r#""http_bridge""#);
    }

    /// 任务 7.5：重启恢复——落盘值重新加载为启动值（序列化→写盘→读回往返一致）
    #[test]
    fn ws_settings_roundtrip_restart_recovery() {
        let mut cfg = Config::default();
        cfg.websocket.enabled = false;
        cfg.websocket.mode = WsTransportMode::Passthrough;
        cfg.websocket.max_connections = 7;
        cfg.websocket.client_first_message_timeout_seconds = 11;
        cfg.websocket.inter_turn_idle_timeout_seconds = 22;
        cfg.websocket.max_message_bytes = 33;
        cfg.websocket.upstream_read_timeout_seconds = 44;

        let path = std::env::temp_dir().join(format!(
            "kiro-rs-ws-roundtrip-{}.json",
            uuid::Uuid::new_v4()
        ));
        cfg.config_path = Some(path.clone());
        cfg.save().expect("落盘失败");

        let reloaded = Config::load(&path).expect("重载失败");
        assert!(!reloaded.websocket.enabled, "重启后必须恢复落盘的 enabled");
        assert_eq!(reloaded.websocket.mode, WsTransportMode::Passthrough);
        assert_eq!(reloaded.websocket.max_connections, 7);
        assert_eq!(reloaded.websocket.client_first_message_timeout_seconds, 11);
        assert_eq!(reloaded.websocket.inter_turn_idle_timeout_seconds, 22);
        assert_eq!(reloaded.websocket.max_message_bytes, 33);
        assert_eq!(reloaded.websocket.upstream_read_timeout_seconds, 44);

        let _ = std::fs::remove_file(path);
    }
}
