//! JSON 导入导出（设计文档 §7.5）
//!
//! `credentials.json` / `config.json` 在桌面端降级为交换格式：
//! - 导入：首启检测到 JSON 文件，读入 SQLite，成功后源文件改名 `.bak`
//! - 导出：格式与现有 `credentials.example.*.json` / `config.example.json`
//!   兼容，可直接给 CLI 用。secret 明文写入导出文件（JSON 格式的本征属性，
//!   界面确认对话框在 change 5）。

use std::path::Path;

use anyhow::Context;
use kiro_rs::kiro::model::credentials::CredentialsConfig;
use kiro_rs::storage::{ConfigStore, CredentialStore};

use super::SqliteStore;

/// 导入结果：凭据条数 / 是否导入了配置 / 备份路径
#[derive(Debug, Default)]
pub struct ImportReport {
    pub credential_count: usize,
    pub config_imported: bool,
    pub backups: Vec<std::path::PathBuf>,
}

/// 检测并导入：`<data_dir>` 与 cwd 下的 `config.json` / `credentials.json`。
///
/// 凭据文件存在时经 `CredentialsConfig::load_detailed` 解析（含导入工具
/// 容器格式的适配），走正常 `persist` 路径落库（secret 拆分进后端）。
/// 配置走 `Config::load` + `save`。单文件导入失败只告警，不阻塞其余文件。
pub fn detect_and_import(data_dir: &Path, store: &SqliteStore) -> ImportReport {
    let cwd = std::env::current_dir().unwrap_or_default();
    detect_and_import_in(data_dir, store, &[data_dir.to_path_buf(), cwd])
}

/// 可注入探测目录的导入实现（测试绕开 cwd）
pub fn detect_and_import_in(
    data_dir: &Path,
    store: &SqliteStore,
    dirs: &[std::path::PathBuf],
) -> ImportReport {
    let _ = data_dir;
    let mut report = ImportReport::default();

    for dir in dirs {
        let cred_path = dir.join("credentials.json");
        if cred_path.exists() && report.credential_count == 0 {
            match import_credentials(store, &cred_path) {
                Ok(n) => {
                    if let Ok(backup) = backup_file(&cred_path) {
                        report.backups.push(backup);
                    }
                    report.credential_count = n;
                }
                Err(e) => tracing::warn!("凭据文件导入失败 {:?}: {:#}", cred_path, e),
            }
        }

        let config_path = dir.join("config.json");
        if config_path.exists() && !report.config_imported {
            match import_config(store, &config_path) {
                Ok(()) => {
                    if let Ok(backup) = backup_file(&config_path) {
                        report.backups.push(backup);
                    }
                    report.config_imported = true;
                }
                Err(e) => tracing::warn!("配置文件导入失败 {:?}: {:#}", config_path, e),
            }
        }
    }

    if report.credential_count > 0 || report.config_imported {
        tracing::info!(
            "JSON 导入完成：凭据 {} 条，配置 {}，备份 {:?}",
            report.credential_count,
            if report.config_imported { "已导入" } else { "未导入" },
            report.backups
        );
    }
    report
}

/// 导入单个凭据 JSON 文件（单对象或数组或导入工具容器格式）
pub fn import_credentials(store: &SqliteStore, path: &Path) -> anyhow::Result<usize> {
    let loaded = CredentialsConfig::load_detailed(path)
        .with_context(|| format!("解析凭据文件失败: {}", path.display()))?;
    let creds = loaded.config.into_sorted_credentials();
    let n = creds.len();
    store
        .persist(&creds)
        .context("凭据写入 SQLite 失败")?;
    Ok(n)
}

/// 导入单个配置文件
pub fn import_config(store: &SqliteStore, path: &Path) -> anyhow::Result<()> {
    let config = kiro_rs::model::config::Config::load(path)
        .with_context(|| format!("解析配置文件失败: {}", path.display()))?;
    store.save(&config).context("配置写入 SQLite 失败")
}

/// 导出凭据为 JSON 数组（多凭据格式，与 `credentials.example.multiple.json`
/// 同构；单条时也用数组，CLI 侧两种都认）
// 入口按钮在 change 5 设置面板接上；本期只有测试引用，
// 故在非测试编译下是死代码
#[allow(dead_code)]
pub fn export_credentials(store: &SqliteStore, path: &Path) -> anyhow::Result<()> {
    let loaded = CredentialStore::load(store).context("从 SQLite 读取凭据失败")?;
    let creds = loaded.config.into_sorted_credentials();
    let json = serde_json::to_string_pretty(&creds).context("凭据序列化失败")?;
    std::fs::write(path, json)
        .with_context(|| format!("导出凭据文件失败: {}", path.display()))
}

/// 导出配置为 JSON 对象（与 `config.example.json` 同构）
#[allow(dead_code)]
pub fn export_config(store: &SqliteStore, path: &Path) -> anyhow::Result<()> {
    let config = ConfigStore::load(store).context("从 SQLite 读取配置失败")?;
    let json = serde_json::to_string_pretty(&config).context("配置序列化失败")?;
    std::fs::write(path, json)
        .with_context(|| format!("导出配置文件失败: {}", path.display()))
}

/// 源文件改名 `.bak`（沿用现有迁移的备份习惯）
fn backup_file(path: &Path) -> anyhow::Result<std::path::PathBuf> {
    let backup = path.with_extension(format!(
        "{}.bak",
        path.extension().and_then(|e| e.to_str()).unwrap_or("json")
    ));
    std::fs::rename(path, &backup)
        .with_context(|| format!("备份源文件失败: {}", path.display()))?;
    Ok(backup)
}
