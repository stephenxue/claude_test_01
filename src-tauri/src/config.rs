//! Persistent app configuration.
//!
//! Stored as `config.json` under the OS-standard per-user app config
//! directory (macOS: `~/Library/Application Support/<bundle-id>/config.json`).
//! This is the correct, writable location for a *packaged* app - unlike the
//! source project directory, it still exists (and is writable) after the app
//! is installed from a signed `.dmg`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub ollama_installed: bool,
    pub ollama_path: Option<String>,
    pub ollama_version: Option<String>,

    pub embedding_model_tag: Option<String>,
    pub chat_model_tag: Option<String>,
    pub embedding_dim: Option<i32>,
    pub models_installed: bool,

    pub source_folder: Option<String>,
    pub last_indexed_at: Option<String>,

    /// Whether the app should silently check for updates once an hour and
    /// auto-install them (still asks before restarting - see
    /// `commands::set_auto_update_enabled` / `UpdateDialog.tsx`). `#[serde(default)]`
    /// is required here (not just on the struct) so that loading an older
    /// config.json saved before this field existed doesn't fail to parse and
    /// silently reset every other saved setting back to defaults.
    #[serde(default)]
    pub auto_update_enabled: bool,
}

pub struct ConfigState(pub Mutex<AppConfig>);

pub fn config_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .context("无法获取应用配置目录")?;
    std::fs::create_dir_all(&dir).context("创建配置目录失败")?;
    Ok(dir.join("config.json"))
}

pub fn load_config(app: &AppHandle) -> Result<AppConfig> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = std::fs::read_to_string(&path).context("读取配置文件失败")?;
    let cfg: AppConfig = serde_json::from_str(&raw).unwrap_or_default();
    Ok(cfg)
}

pub fn save_config(app: &AppHandle, cfg: &AppConfig) -> Result<()> {
    let path = config_path(app)?;
    let raw = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&path, raw).context("写入配置文件失败")?;
    Ok(())
}
