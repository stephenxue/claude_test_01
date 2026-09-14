//! Thin HTTP client for the local Ollama server: embeddings + streaming chat.

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::process::OLLAMA_BASE_URL;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

pub async fn embed(model: &str, text: &str) -> Result<Vec<f32>> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{OLLAMA_BASE_URL}/api/embeddings"))
        .json(&EmbeddingRequest { model, prompt: text })
        .send()
        .await
        .context("调用 Ollama 向量化接口失败")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("向量化请求失败 ({status}): {body}"));
    }

    let parsed: EmbeddingResponse = resp.json().await.context("解析向量化响应失败")?;
    Ok(parsed.embedding)
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct ChatStreamChunk {
    message: Option<ChatMessageChunk>,
    done: bool,
}

#[derive(Deserialize)]
struct ChatMessageChunk {
    content: String,
}

/// Streams the chat completion, emitting a Tauri event named `event_name`
/// for every incremental token, and returns the full assembled reply.
pub async fn chat_stream(
    app: &AppHandle,
    model: &str,
    messages: &[ChatMessage],
    event_name: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{OLLAMA_BASE_URL}/api/chat"))
        .json(&ChatRequest {
            model,
            messages,
            stream: true,
        })
        .send()
        .await
        .context("调用 Ollama 聊天接口失败")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("聊天请求失败 ({status}): {body}"));
    }

    let mut full_reply = String::new();
    let mut buffer = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("聊天数据流中断")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Ollama streams newline-delimited JSON objects.
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer.drain(..=pos);
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<ChatStreamChunk>(&line) {
                Ok(parsed) => {
                    if let Some(msg) = parsed.message {
                        if !msg.content.is_empty() {
                            full_reply.push_str(&msg.content);
                            let _ = app.emit(event_name, &msg.content);
                        }
                    }
                    if parsed.done {
                        return Ok(full_reply);
                    }
                }
                Err(_) => continue, // ignore malformed / keep-alive lines
            }
        }
    }

    Ok(full_reply)
}

#[derive(Serialize)]
struct PullRequest<'a> {
    model: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct PullProgressChunk {
    status: String,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct ModelPullProgress {
    pub model: String,
    pub status: String,
    pub percent: f32,
}

/// Pulls a model, emitting `model-pull-progress` events with byte-level
/// download progress reported by the Ollama daemon itself.
pub async fn pull_model(app: &AppHandle, model: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{OLLAMA_BASE_URL}/api/pull"))
        .json(&PullRequest { model, stream: true })
        .send()
        .await
        .context("调用 Ollama 拉取模型接口失败")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("拉取模型失败 ({status}): {body}"));
    }

    let mut buffer = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("拉取模型数据流中断")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer.drain(..=pos);
            if line.is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<PullProgressChunk>(&line) {
                let percent = match (parsed.completed, parsed.total) {
                    (Some(c), Some(t)) if t > 0 => (c as f32 / t as f32) * 100.0,
                    _ => 0.0,
                };
                let _ = app.emit(
                    "model-pull-progress",
                    ModelPullProgress {
                        model: model.to_string(),
                        status: parsed.status,
                        percent,
                    },
                );
            }
        }
    }

    Ok(())
}
