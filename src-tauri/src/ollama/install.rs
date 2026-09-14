//! Detect and install the Ollama CLI on macOS without requiring `sudo` or
//! moving anything into `/Applications`.
//!
//! Strategy:
//! 1. Look for an already-installed `ollama` binary (Homebrew path, PATH,
//!    or a previous install by this app).
//! 2. If missing, prefer Homebrew (`brew install ollama`) when Homebrew is
//!    present - it is the least surprising path for a technical Mac user and
//!    needs no admin prompt.
//! 3. Otherwise download the official Ollama.app zip from ollama.com,
//!    extract the embedded CLI binary, and copy it into this app's own
//!    support directory. No system directories are touched.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager};

const OLLAMA_DOWNLOAD_URL: &str = "https://ollama.com/download/Ollama-darwin.zip";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    pub stage: String,   // "detecting" | "downloading" | "extracting" | "done" | "error"
    pub percent: f32,    // 0.0 - 100.0
    pub message: String,
}

fn emit_progress(app: &AppHandle, stage: &str, percent: f32, message: &str) {
    let _ = app.emit(
        "ollama-install-progress",
        InstallProgress {
            stage: stage.to_string(),
            percent,
            message: message.to_string(),
        },
    );
}

fn candidate_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/opt/homebrew/bin/ollama"),
        PathBuf::from("/usr/local/bin/ollama"),
        PathBuf::from("/Applications/Ollama.app/Contents/Resources/ollama"),
    ];
    if let Ok(dir) = app.path().app_config_dir() {
        paths.push(dir.join("bin").join("ollama"));
    }
    paths
}

/// Returns the path to a working `ollama` binary, if one can be found.
pub fn detect_ollama(app: &AppHandle) -> Option<PathBuf> {
    // 1) explicit known install locations
    for p in candidate_paths(app) {
        if p.is_file() {
            return Some(p);
        }
    }
    // 2) PATH lookup (covers custom installs / shells with extended PATH)
    if let Ok(output) = Command::new("which").arg("ollama").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

pub fn ollama_version(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn homebrew_path() -> Option<PathBuf> {
    for p in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

pub async fn install_ollama(app: &AppHandle) -> Result<PathBuf> {
    emit_progress(app, "detecting", 0.0, "检测 Homebrew...");

    if let Some(brew) = homebrew_path() {
        emit_progress(app, "downloading", 10.0, "通过 Homebrew 安装 Ollama...");
        let status = Command::new(&brew)
            .args(["install", "ollama"])
            .status()
            .context("运行 brew install ollama 失败")?;
        if status.success() {
            if let Some(path) = detect_ollama(app) {
                emit_progress(app, "done", 100.0, "Ollama 安装完成");
                return Ok(path);
            }
        }
        emit_progress(app, "downloading", 10.0, "Homebrew 安装失败，改为直接下载...");
    }

    download_and_extract(app).await
}

async fn download_and_extract(app: &AppHandle) -> Result<PathBuf> {
    use futures_util::StreamExt;
    use std::io::Write;

    let tmp_dir = std::env::temp_dir().join(format!("ollama-install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir)?;
    let zip_path = tmp_dir.join("Ollama-darwin.zip");

    emit_progress(app, "downloading", 0.0, "正在下载 Ollama...");

    let client = reqwest::Client::new();
    let resp = client
        .get(OLLAMA_DOWNLOAD_URL)
        .send()
        .await
        .context("下载 Ollama 失败：网络请求未成功")?;
    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = std::fs::File::create(&zip_path)?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("下载数据流中断")?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            (downloaded as f32 / total as f32) * 80.0
        } else {
            0.0
        };
        emit_progress(app, "downloading", percent, "正在下载 Ollama...");
    }
    drop(file);

    emit_progress(app, "extracting", 85.0, "正在解压...");
    let unzip_status = Command::new("unzip")
        .args(["-o", zip_path.to_str().unwrap(), "-d", tmp_dir.to_str().unwrap()])
        .status()
        .context("解压 Ollama 失败")?;
    if !unzip_status.success() {
        return Err(anyhow!("解压 Ollama.app 失败"));
    }

    let embedded_binary = tmp_dir
        .join("Ollama.app")
        .join("Contents")
        .join("Resources")
        .join("ollama");
    if !embedded_binary.is_file() {
        return Err(anyhow!("下载包中未找到 ollama 可执行文件"));
    }

    let dest_dir = app
        .path()
        .app_config_dir()
        .context("无法获取应用配置目录")?
        .join("bin");
    std::fs::create_dir_all(&dest_dir)?;
    let dest_path = dest_dir.join("ollama");
    std::fs::copy(&embedded_binary, &dest_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest_path, perms)?;
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);

    emit_progress(app, "done", 100.0, "Ollama 安装完成");
    Ok(dest_path)
}
