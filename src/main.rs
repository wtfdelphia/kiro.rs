mod admin;
mod admin_ui;
mod anthropic;
mod common;
mod http_client;
mod kiro;
mod model;
mod openai;
mod public_api;
pub mod token;

use std::collections::HashMap;
use std::sync::Arc;

use clap::Parser;
use kiro::endpoint::{IdeEndpoint, KiroEndpoint};
use kiro::model::credentials::{CredentialsConfig, KiroCredentials};
use kiro::provider::KiroProvider;
use kiro::token_manager::MultiTokenManager;
use model::arg::Args;
use model::config::Config;

#[tokio::main]
async fn main() {
    // 解析命令行参数
    let args = Args::parse();

    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    tracing::info!("kiro-rs v{}", env!("CARGO_PKG_VERSION"));

    // 加载配置
    let config_path = args
        .config
        .unwrap_or_else(|| Config::default_config_path().to_string());
    let config = Config::load(&config_path).unwrap_or_else(|e| {
        tracing::error!("加载配置失败: {}", e);
        std::process::exit(1);
    });

    // 加载凭证（支持单对象或数组格式）
    let credentials_path = args
        .credentials
        .unwrap_or_else(|| KiroCredentials::default_credentials_path().to_string());
    let loaded = CredentialsConfig::load_detailed(&credentials_path).unwrap_or_else(|e| {
        tracing::error!("加载凭证失败: {}", e);
        std::process::exit(1);
    });

    // 导入工具容器格式（wrapper / 旧版嵌套）：备份后规范化写回为原生格式。
    // 迁移失败不阻止启动——凭据已在内存中正确解析，下次启动会再试一次。
    if loaded.needs_migration {
        match CredentialsConfig::migrate_to_native(&credentials_path, &loaded.config) {
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
    tracing::debug!("主凭证: {:?}", first_credentials);

    // 获取 API Key（requireApiKey=true 且为空时 fail-closed，由中间件处理；启动允许空 key）
    let api_key = config.api_key.clone().unwrap_or_default();
    if config.require_api_key && api_key.trim().is_empty() {
        tracing::warn!("requireApiKey=true 但未配置 apiKey：客户端请求将一律 401（fail-closed）");
    }

    // 构建代理配置
    let proxy_config = config.proxy_url.as_ref().map(|url| {
        let mut proxy = http_client::ProxyConfig::new(url);
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
        tracing::error!("默认端点 \"{}\" 未注册", config.default_endpoint);
        std::process::exit(1);
    }

    // 校验所有凭据声明的端点都已注册
    for cred in &credentials_list {
        let name = cred
            .endpoint
            .as_deref()
            .unwrap_or(&config.default_endpoint);
        if !endpoints.contains_key(name) {
            tracing::error!(
                "凭据 id={:?} 指定了未知端点 \"{}\"（已注册: {:?}）",
                cred.id,
                name,
                endpoints.keys().collect::<Vec<_>>()
            );
            std::process::exit(1);
        }
    }

    let endpoint_names: Vec<String> = endpoints.keys().cloned().collect();

    // 创建 MultiTokenManager 和 KiroProvider
    let token_manager = MultiTokenManager::new(
        config.clone(),
        credentials_list,
        proxy_config.clone(),
        Some(credentials_path.into()),
        is_multiple_format,
    )
    .unwrap_or_else(|e| {
        tracing::error!("创建 Token 管理器失败: {}", e);
        std::process::exit(1);
    });
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
        proxy: proxy_config,
        tls_backend: config.tls_backend,
    });

    // 构建 Anthropic API 路由（profile_arn 由 provider 层根据实际凭据动态注入）
    let (anthropic_app, app_state) = anthropic::create_router_with_provider_and_auth(
        &api_key,
        Some(kiro_provider.clone()),
        config.extract_thinking,
        config.require_api_key,
    );

    // 合并 OpenAI 兼容路由（复用同一 app_state）
    // 注意：merge 不传播 layer，auth/cors/body-limit 由 create_openai_routes 自带
    let anthropic_app = anthropic_app.merge(openai::create_openai_routes(app_state.clone()));

    // 构建 Admin API 路由（如果配置了非空的 admin_api_key）
    // 安全检查：空字符串被视为未配置，防止空 key 绕过认证
    let admin_key_valid = config
        .admin_api_key
        .as_ref()
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);

    let app = if let Some(admin_key) = &config.admin_api_key {
        if admin_key.trim().is_empty() {
            tracing::warn!("admin_api_key 配置为空，Admin API 未启用");
            anthropic_app
        } else {
            let admin_service = admin::AdminService::new_with_runtime(
                token_manager.clone(),
                endpoint_names.clone(),
                Some(app_state.auth.clone()),
                Some(kiro_provider.clone()),
            );
            let admin_state = admin::AdminState::new(admin_key, admin_service);
            let admin_app = admin::create_admin_router(admin_state);

            // 创建 Admin UI 路由
            let admin_ui_app = admin_ui::create_admin_ui_router();

            tracing::info!("Admin API 已启用");
            tracing::info!("Admin UI 已启用: /admin");
            anthropic_app
                .nest("/api/admin", admin_app)
                .nest("/admin", admin_ui_app)
        }
    } else {
        anthropic_app
    };

    // 启动服务器
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("启动 Anthropic API 端点: {}", addr);
    if api_key.is_empty() {
        tracing::info!("API Key: <empty>");
    } else {
        tracing::info!("API Key: {}***", &api_key[..(api_key.len() / 2).max(1)]);
    }
    tracing::info!("requireApiKey: {}", config.require_api_key);
    // 对外 API 清单来自 public_api catalog（单一事实源），勿在此手写第二份
    tracing::info!("可用 API:");
    for endpoint in public_api::live_endpoints() {
        tracing::info!("  {:<4} {}", endpoint.method, endpoint.path);
    }
    if admin_key_valid {
        // Admin API 不属于 Public Client API，catalog 不覆盖，此处保持手写
        tracing::info!("Admin API:");
        tracing::info!("  GET  /api/admin/credentials");
        tracing::info!("  POST /api/admin/credentials/:index/disabled");
        tracing::info!("  POST /api/admin/credentials/:index/priority");
        tracing::info!("  POST /api/admin/credentials/:index/reset");
        tracing::info!("  GET  /api/admin/credentials/:index/balance");
        tracing::info!("  POST /api/admin/credentials/models/refresh");
        tracing::info!("  POST /api/admin/credentials/:id/models/refresh");
        tracing::info!("  GET  /api/admin/credentials/:id/models");
        tracing::info!("  POST /api/admin/credentials/:id/test");
        tracing::info!("  GET  /api/admin/models/catalog");
        tracing::info!("  GET/PUT /api/admin/settings/proxy");
        tracing::info!("  GET/PUT /api/admin/settings/endpoint");
        tracing::info!("  GET/PUT /api/admin/settings/auth");
        tracing::info!("Admin UI:");
        tracing::info!("  GET  /admin");
    }

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
