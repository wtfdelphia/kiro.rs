//! 装配复用：`bootstrap` / `build_routes`
//!
//! 把 `main.rs` 的装配链抽成两个分步函数，供 CLI 二进制与后续桌面二进制
//! 复用。两者只差「服务器是否启动」与「存储后端」（见
//! `docs/desktop-gpui-embedded-design.md` §4.3）：
//!
//! - [`bootstrap`]：装配到「核心就绪」为止——加载配置与凭据、格式迁移、
//!   端点注册与校验、`MultiTokenManager` + 预热、`KiroProvider`、
//!   count_tokens 配置。不构建路由、不 bind、不 serve。
//! - [`build_routes`]：在核心之上构建全量路由（anthropic + openai + admin +
//!   admin_ui），返回 `(Router, AppState)`。`AppState` 必须返还：serve 侧的
//!   优雅关闭需要 `ws_shutdown`，admin 装配需要 `auth` / `ws_settings` /
//!   `ws_admission`。
//!
//! 与 `main.rs` 原版装配链的两点语义差异（均为有意）：
//!
//! 1. 五处 `process::exit(1)` 改为 `Err` 返回，由调用方决定退出语义。
//! 2. 存储读写经 [`crate::storage`] trait，路径等后端细节封装在 store 内，
//!    不再作为本模块参数。

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;

use crate::admin;
use crate::admin_ui;
use crate::anthropic::{self, AppState};
use crate::http_client::ProxyConfig;
use crate::kiro::endpoint::{IdeEndpoint, KiroEndpoint};
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::provider::KiroProvider;
use crate::kiro::token_manager::MultiTokenManager;
use crate::model::config::Config;
use crate::openai;
use crate::storage::{ConfigStore, CredentialStore};
use crate::token;

/// 装配所需的存储后端组合
///
/// 桌面端传 SQLite 实现，CLI 传 JSON 文件实现。路径等后端细节封装在
/// store 内部，此处不重复暴露。
pub struct BootOptions {
    pub credential_store: Arc<dyn CredentialStore>,
    pub config_store: Arc<dyn ConfigStore>,
}

/// `bootstrap` 的产物：核心句柄与后续路由装配所需的最小上下文
pub struct Bootstrapped {
    pub config: Config,
    pub token_manager: Arc<MultiTokenManager>,
    pub kiro_provider: Arc<KiroProvider>,
    pub endpoint_names: Vec<String>,
    pub proxy_config: Option<ProxyConfig>,
    pub is_multiple_format: bool,
}

/// 装配到「核心就绪」为止：不构建路由、不 bind、不 serve。
///
/// 五处原 `process::exit(1)` 路径（配置加载、凭据加载、默认端点校验、
/// 凭据端点校验、`MultiTokenManager` 构造）均改为 `Err` 返回。
///
/// 注意：内部会 `tokio::spawn` 预热任务，调用方必须处于 tokio 运行时
/// 上下文（CLI 的 `#[tokio::main]` 或桌面端经 `handle.enter()`）。
pub fn bootstrap(opts: &BootOptions) -> anyhow::Result<Bootstrapped> {
    // 加载配置（经配置存储后端）
    let config = opts.config_store.load().context("加载配置失败")?;

    // 加载凭据（支持单对象或数组格式；后端缺失/为空返回空多凭据配置）
    let loaded = opts.credential_store.load().context("加载凭据失败")?;

    // 导入工具容器格式（wrapper / 旧版嵌套）：备份后规范化写回为原生格式。
    // 迁移失败不阻止启动——凭据已在内存中正确解析，下次启动会再试一次。
    if loaded.needs_migration {
        match opts.credential_store.migrate_to_native(&loaded.config) {
            Ok(backup) => tracing::info!(
                "凭据文件已从导入工具格式迁移为原生格式，原文件备份于 {:?}",
                backup
            ),
            Err(e) => tracing::warn!(
                "凭据文件格式迁移失败（不影响本次启动，原文件未被修改）: {}",
                e
            ),
        }
    }

    let credentials_config = loaded.config;

    // 判断是否为多凭据格式（用于刷新后回写）
    let is_multiple_format = credentials_config.is_multiple();

    // 转换为按优先级排序的凭据列表
    let mut credentials_list = credentials_config.into_sorted_credentials();

    // 检查 KIRO_API_KEY 环境变量，自动创建 API Key 凭据
    if let Ok(kiro_api_key) = std::env::var("KIRO_API_KEY") {
        if kiro_api_key.is_empty() {
            tracing::warn!("KIRO_API_KEY 环境变量已设置但为空，视为未配置");
        } else {
            tracing::info!("检测到 KIRO_API_KEY 环境变量，添加 API Key 凭据（最高优先级）");
            let api_key_cred = KiroCredentials {
                kiro_api_key: Some(kiro_api_key),
                auth_method: Some("api_key".to_string()),
                priority: 0,
                ..Default::default()
            };
            credentials_list.insert(0, api_key_cred);
        }
    }

    tracing::info!("已加载 {} 个凭据配置", credentials_list.len());

    // 获取第一个凭据用于日志显示
    let first_credentials = credentials_list.first().cloned().unwrap_or_default();
    tracing::debug!("主凭据: {:?}", first_credentials);

    // API Key（requireApiKey=true 且为空时 fail-closed，由中间件处理；启动允许空 key）
    let api_key = config.api_key.clone().unwrap_or_default();
    if config.require_api_key && api_key.trim().is_empty() {
        tracing::warn!("requireApiKey=true 但未配置 apiKey：客户端请求将一律 401（fail-closed）");
    }

    // 构建代理配置
    let proxy_config = config.proxy_url.as_ref().map(|url| {
        let mut proxy = ProxyConfig::new(url);
        if let (Some(username), Some(password)) = (&config.proxy_username, &config.proxy_password) {
            proxy = proxy.with_auth(username, password);
        }
        proxy
    });

    if proxy_config.is_some() {
        tracing::info!("已配置 HTTP 代理: {}", config.proxy_url.as_ref().unwrap());
    }

    // 构建端点注册表
    let mut endpoints: HashMap<String, Arc<dyn KiroEndpoint>> = HashMap::new();
    {
        let ide = IdeEndpoint::new();
        endpoints.insert(ide.name().to_string(), Arc::new(ide));
    }

    // 校验默认端点存在
    if !endpoints.contains_key(&config.default_endpoint) {
        anyhow::bail!("默认端点 \"{}\" 未注册", config.default_endpoint);
    }

    // 校验所有凭据声明的端点都已注册
    for cred in &credentials_list {
        let name = cred
            .endpoint
            .as_deref()
            .unwrap_or(&config.default_endpoint);
        if !endpoints.contains_key(name) {
            anyhow::bail!(
                "凭据 id={:?} 指定了未知端点 \"{}\"（已注册: {:?}）",
                cred.id,
                name,
                endpoints.keys().collect::<Vec<_>>()
            );
        }
    }

    let endpoint_names: Vec<String> = endpoints.keys().cloned().collect();

    // 创建 MultiTokenManager（存储后端注入）与 KiroProvider
    let token_manager = MultiTokenManager::with_stores(
        config.clone(),
        credentials_list,
        proxy_config.clone(),
        Some(opts.credential_store.clone()),
        Some(opts.config_store.clone()),
        is_multiple_format,
    )
    .context("创建 Token 管理器失败")?;
    let token_manager = Arc::new(token_manager);
    // 后台预热模型目录（限并发 2）；失败仅 log，不阻塞启动与 /v1/models
    token_manager.spawn_warmup_models(2);
    let kiro_provider = Arc::new(KiroProvider::with_proxy(
        token_manager.clone(),
        proxy_config.clone(),
        endpoints,
        config.default_endpoint.clone(),
    ));

    // 初始化 count_tokens 配置
    token::init_config(token::CountTokensConfig {
        api_url: config.count_tokens_api_url.clone(),
        api_key: config.count_tokens_api_key.clone(),
        auth_type: config.count_tokens_auth_type.clone(),
        proxy: proxy_config.clone(),
        tls_backend: config.tls_backend,
    });

    Ok(Bootstrapped {
        config,
        token_manager,
        kiro_provider,
        endpoint_names,
        proxy_config,
        is_multiple_format,
    })
}

/// 构建全量路由：anthropic + openai + admin + admin_ui。
///
/// admin 挂载判定与现 `main.rs` 一致：`admin_api_key` 为 `None` 或空串
/// （trim 后）视为未配置，不挂 admin 路由。
pub fn build_routes(b: &Bootstrapped) -> (axum::Router, AppState) {
    let api_key = b.config.api_key.clone().unwrap_or_default();

    // 构建 Anthropic API 路由（profile_arn 由 provider 层根据实际凭据动态注入）
    let (anthropic_app, app_state) = anthropic::create_router_with_provider_and_auth(
        &api_key,
        Some(b.kiro_provider.clone()),
        b.config.extract_thinking,
        b.config.require_api_key,
    );

    // WebSocket ingress 运行时设置注入共享句柄（Router 已克隆同一 Arc，写穿即生效）
    app_state.set_ws_settings(b.config.websocket.clone());

    // 合并 OpenAI 兼容路由（复用同一 app_state）
    // 注意：merge 不传播 layer，auth/cors/body-limit 由 create_openai_routes 自带
    let anthropic_app = anthropic_app.merge(openai::create_openai_routes(app_state.clone()));

    // 构建 Admin API 路由（如果配置了非空的 admin_api_key）
    // 安全检查：空字符串被视为未配置，防止空 key 绕过认证
    let app = if let Some(admin_key) = &b.config.admin_api_key {
        if admin_key.trim().is_empty() {
            tracing::warn!("admin_api_key 配置为空，Admin API 未启用");
            anthropic_app
        } else {
            let admin_service = admin::AdminService::new_with_runtime(
                b.token_manager.clone(),
                b.endpoint_names.clone(),
                Some(app_state.auth.clone()),
                Some(b.kiro_provider.clone()),
            )
            .with_ws_runtime(app_state.ws_settings.clone(), app_state.ws_admission.clone());
            let admin_state = admin::AdminState::new(admin_key, admin_service);
            let admin_app = admin::create_admin_router(admin_state);

            tracing::info!("Admin API 已启用");
            tracing::info!("Admin UI 已启用: /admin");
            admin_ui::mount_admin_ui(anthropic_app.nest("/api/admin", admin_app))
        }
    } else {
        anthropic_app
    };

    (app, app_state)
}
