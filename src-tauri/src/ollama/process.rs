//! Manage the `ollama serve` background process and check whether the
//! Ollama HTTP API is reachable (whether we started it, or it was already
//! running from a previous session / the official Ollama.app).

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::Duration;

pub const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";

pub struct OllamaProcessState {
    pub child: Mutex<Option<Child>>,
    pub started_by_us: Mutex<bool>,
}

impl Default for OllamaProcessState {
    fn default() -> Self {
        Self {
            child: Mutex::new(None),
            started_by_us: Mutex::new(false),
        }
    }
}

/// Quick check: is something already answering on the Ollama API port?
pub async fn is_running() -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    client
        .get(format!("{OLLAMA_BASE_URL}/api/tags"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub fn start_ollama_server(state: &OllamaProcessState, ollama_path: &Path) -> Result<()> {
    let mut child_guard = state.child.lock().unwrap();
    if child_guard.is_some() {
        return Ok(()); // already started by us
    }

    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Library")
        .join("Logs")
        .join("local-rag-app");
    let _ = std::fs::create_dir_all(&log_dir);
    let stdout_log = std::fs::File::create(log_dir.join("ollama-stdout.log")).ok();
    let stderr_log = std::fs::File::create(log_dir.join("ollama-stderr.log")).ok();

    let mut cmd = std::process::Command::new(ollama_path);
    cmd.arg("serve");
    // Allow the server to process a few embedding/chat requests concurrently
    // instead of one at a time - meaningfully speeds up bulk vectorization
    // when we fire off several embedding calls in parallel (see
    // commands::embed_texts_concurrent).
    cmd.env("OLLAMA_NUM_PARALLEL", "4");
    cmd.stdout(stdout_log.map(Stdio::from).unwrap_or_else(Stdio::null));
    cmd.stderr(stderr_log.map(Stdio::from).unwrap_or_else(Stdio::null));

    let child = cmd.spawn().context("启动 ollama serve 失败")?;
    *child_guard = Some(child);
    *state.started_by_us.lock().unwrap() = true;
    Ok(())
}

/// Poll the API until it responds or we give up.
pub async fn wait_until_ready(timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if is_running().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    false
}

pub fn stop_ollama_server(state: &OllamaProcessState) {
    let started_by_us = *state.started_by_us.lock().unwrap();
    if !started_by_us {
        return; // don't kill a server we didn't start
    }
    if let Some(mut child) = state.child.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
