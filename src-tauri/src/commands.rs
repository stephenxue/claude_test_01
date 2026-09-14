//! All `#[tauri::command]` handlers invoked from the React frontend.

use crate::config::{self, AppConfig, ConfigState};
use crate::docs::{chunker, scanner};
use crate::hardware;
use crate::ollama::{client, install, models, process};
use crate::vectordb::{ChunkRecord, VectorDb};
use crate::AppState;
use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

fn app_error(e: anyhow::Error) -> String {
    format!("{e:#}")
}

// ---------------------------------------------------------------------
// Ollama install / run status
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct OllamaStatusDto {
    pub installed: bool,
    pub running: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[tauri::command]
pub async fn check_ollama_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OllamaStatusDto, String> {
    let path = install::detect_ollama(&app);
    let running = process::is_running().await;
    let version = path.as_deref().and_then(install::ollama_version);

    if let Some(p) = &path {
        let mut cfg = state.config.0.lock().unwrap();
        cfg.ollama_installed = true;
        cfg.ollama_path = Some(p.to_string_lossy().to_string());
        cfg.ollama_version = version.clone();
        let _ = config::save_config(&app, &cfg);
    }

    Ok(OllamaStatusDto {
        installed: path.is_some(),
        running,
        path: path.map(|p| p.to_string_lossy().to_string()),
        version,
    })
}

#[tauri::command]
pub async fn install_ollama(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OllamaStatusDto, String> {
    let path = install::install_ollama(&app).await.map_err(app_error)?;
    process::start_ollama_server(&state.ollama_process, &path).map_err(app_error)?;
    let ready = process::wait_until_ready(std::time::Duration::from_secs(20)).await;

    let version = install::ollama_version(&path);
    {
        let mut cfg = state.config.0.lock().unwrap();
        cfg.ollama_installed = true;
        cfg.ollama_path = Some(path.to_string_lossy().to_string());
        cfg.ollama_version = version.clone();
        let _ = config::save_config(&app, &cfg);
    }

    Ok(OllamaStatusDto {
        installed: true,
        running: ready,
        path: Some(path.to_string_lossy().to_string()),
        version,
    })
}

#[tauri::command]
pub async fn start_ollama(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OllamaStatusDto, String> {
    let path = install::detect_ollama(&app).ok_or("未检测到 Ollama，请先安装".to_string())?;
    process::start_ollama_server(&state.ollama_process, &path).map_err(app_error)?;
    let ready = process::wait_until_ready(std::time::Duration::from_secs(20)).await;
    let version = install::ollama_version(&path);

    Ok(OllamaStatusDto {
        installed: true,
        running: ready,
        path: Some(path.to_string_lossy().to_string()),
        version,
    })
}

// ---------------------------------------------------------------------
// Model install status
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ModelsStatusDto {
    pub installed: bool,
    pub embedding_model: Option<String>,
    pub chat_model: Option<String>,
    pub recommendation: models::ModelChoice,
}

#[tauri::command]
pub async fn check_models_status(state: State<'_, AppState>) -> Result<ModelsStatusDto, String> {
    let hw = hardware::detect_hardware();
    let recommendation = models::select_models(&hw);
    let cfg = state.config.0.lock().unwrap().clone();

    Ok(ModelsStatusDto {
        installed: cfg.models_installed,
        embedding_model: cfg.embedding_model_tag,
        chat_model: cfg.chat_model_tag,
        recommendation,
    })
}

#[tauri::command]
pub async fn install_models(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ModelsStatusDto, String> {
    let hw = hardware::detect_hardware();
    let choice = models::select_models(&hw);

    client::pull_model(&app, &choice.embedding.tag)
        .await
        .map_err(app_error)?;
    client::pull_model(&app, &choice.chat.tag)
        .await
        .map_err(app_error)?;

    {
        let mut cfg = state.config.0.lock().unwrap();
        cfg.embedding_model_tag = Some(choice.embedding.tag.clone());
        cfg.chat_model_tag = Some(choice.chat.tag.clone());
        cfg.embedding_dim = Some(choice.embedding_dim);
        cfg.models_installed = true;
        let _ = config::save_config(&app, &cfg);
    }

    Ok(ModelsStatusDto {
        installed: true,
        embedding_model: Some(choice.embedding.tag.clone()),
        chat_model: Some(choice.chat.tag.clone()),
        recommendation: choice,
    })
}

// ---------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.0.lock().unwrap().clone())
}

/// Flips the "自动检查更新" switch (see `UpdateDialog.tsx`'s hourly timer,
/// which reads this back via `get_config`) and persists it immediately so
/// the choice survives an app restart.
#[tauri::command]
pub async fn set_auto_update_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = state.config.0.lock().unwrap();
    cfg.auto_update_enabled = enabled;
    config::save_config(&app, &cfg).map_err(app_error)?;
    Ok(())
}

// ---------------------------------------------------------------------
// Misc file utilities
// ---------------------------------------------------------------------

/// Writes `content` to `path`, overwriting it if it already exists. Used by
/// the chat "下载对话" button: the frontend gets a save path from a native
/// save dialog (`@tauri-apps/plugin-dialog`'s `save()`), then hands the path
/// and Markdown text here. We do this via a plain command rather than the
/// `@tauri-apps/plugin-fs` JS API because the WKWebView on macOS does not
/// reliably fire browser-style `<a download>` saves, and a bespoke command
/// avoids adding + scoping a whole extra fs-permission surface for one write.
#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("保存文件失败: {e}"))
}

/// Manual "导入更新包" fallback for users who can't reach the GitHub-hosted
/// auto-updater (see `manual_update.rs`). Runs the (blocking, shells out to
/// `hdiutil`/`tar`/`cp`) install on a blocking thread so it doesn't stall the
/// async runtime, then - on success - never returns, since the process
/// relaunches itself into the new version.
#[tauri::command]
pub async fn install_update_from_file(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::manual_update::install_from_file(Path::new(&path))
    })
    .await
    .map_err(|e| format!("更新任务异常终止: {e}"))?
    .map_err(|e| format!("{e:#}"))
}

// ---------------------------------------------------------------------
// Source folder + indexing
// ---------------------------------------------------------------------

fn vector_db_dir(app: &AppHandle, source: &Path) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("vectordb");
    let slug: String = source
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    Ok(base.join(slug))
}

async fn ensure_vector_db(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let (source, dim) = {
        let cfg = state.config.0.lock().unwrap();
        let source = cfg
            .source_folder
            .clone()
            .ok_or("尚未选择 source 文件夹".to_string())?;
        let dim = cfg.embedding_dim.ok_or("尚未安装向量模型".to_string())?;
        (source, dim)
    };

    let mut guard = state.vector_db.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    let db_path = vector_db_dir(app, Path::new(&source))?;
    let db = VectorDb::open(&db_path, dim).await.map_err(app_error)?;
    *guard = Some(db);
    Ok(())
}

#[tauri::command]
pub async fn select_source_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let source = PathBuf::from(&path);
    scanner::ensure_layout(&source).map_err(app_error)?;

    {
        let mut cfg = state.config.0.lock().unwrap();
        cfg.source_folder = Some(path.clone());
        let _ = config::save_config(&app, &cfg);
    }
    // Force re-open of the vector DB against the (possibly new) folder.
    *state.vector_db.lock().await = None;

    Ok(())
}

/// Max number of embedding HTTP calls in flight at once. Pairs with
/// `OLLAMA_NUM_PARALLEL` set when we start our own `ollama serve` (see
/// ollama::process::start_ollama_server) so the server can actually work on
/// more than one of these at a time.
const EMBEDDING_CONCURRENCY: usize = 4;

/// Mirrors the current indexing/restoring progress onto the app's dock icon
/// (macOS) / taskbar (Windows), via Tauri's cross-platform progress bar API
/// (`WebviewWindow::set_progress_bar`, added in Tauri 2.1), so progress is
/// visible even when the app window isn't focused or is minimized. `total ==
/// 0` clears the indicator. Best-effort: if there's no main window for some
/// reason, or the platform call fails, we just skip it silently rather than
/// letting a cosmetic feature fail a real indexing/restoring run.
fn set_dock_progress(app: &AppHandle, current: usize, total: usize) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let state = if total == 0 {
        tauri::window::ProgressBarState {
            status: Some(tauri::window::ProgressBarStatus::None),
            progress: None,
            unity_uri: None,
        }
    } else {
        let percent = ((current as f64 / total as f64) * 100.0).round().min(100.0) as u64;
        tauri::window::ProgressBarState {
            status: Some(tauri::window::ProgressBarStatus::Normal),
            progress: Some(percent),
            unity_uri: None,
        }
    };
    let _ = window.set_progress_bar(state);
}

/// Reads a file and splits it into chunks, without embedding it yet. Used to
/// find out how many chunks a whole indexing/restoring run will contain
/// *before* any (slow) embedding calls start, so the progress bar's `total`
/// is expressed in chunks - not files - from the very first event. Without
/// this, a run with one large file would report "1/1" for its entire
/// duration instead of showing real progress.
fn read_and_chunk(file: &Path) -> Result<(String, Vec<String>), String> {
    let file_name = file.file_name().unwrap().to_string_lossy().to_string();
    let content =
        std::fs::read_to_string(file).map_err(|e| format!("读取文件 {file_name} 失败: {e}"))?;
    Ok((file_name, chunker::chunk_text(&content)))
}

/// Embeds several texts with bounded concurrency (rather than one HTTP
/// round-trip at a time) - the dominant cost of vectorizing a large file is
/// otherwise almost entirely serialized network/inference latency. Order of
/// the returned vectors matches the order of `texts`. Pairs with
/// `OLLAMA_NUM_PARALLEL` set when we start our own `ollama serve` (see
/// ollama::process::start_ollama_server) so the server can actually work on
/// more than one of these at a time. `on_chunk_done` fires once per
/// completed embedding (arrival order, not input order, since results race
/// in) so callers can drive a chunk-level progress bar.
async fn embed_texts_concurrent(
    model: &str,
    texts: Vec<String>,
    on_chunk_done: &(dyn Fn() + Sync),
) -> anyhow::Result<Vec<Vec<f32>>> {
    let n = texts.len();
    let mut stream = stream::iter(texts.into_iter().enumerate())
        .map(|(i, text)| {
            let model = model.to_string();
            async move {
                let v = client::embed(&model, &text).await;
                (i, v)
            }
        })
        .buffer_unordered(EMBEDDING_CONCURRENCY);

    let mut out: Vec<Option<Vec<f32>>> = (0..n).map(|_| None).collect();
    while let Some((i, r)) = stream.next().await {
        out[i] = Some(r?);
        on_chunk_done();
    }
    Ok(out.into_iter().map(|o| o.expect("all indices filled")).collect())
}

/// Embeds already-chunked `pieces` for `file_name`, stores them, then moves
/// `file` into `archive/`. Shared by both first-time ingestion
/// (`index_documents`) and restoring a previously-deleted file
/// (`restore_documents`), since restoring means the vectors were actually
/// deleted and must be recomputed from the file's content - there's no
/// cache of the old vectors. Chunking happens in the caller (via
/// `read_and_chunk`) so a whole run's total chunk count is known up front.
async fn ingest_file(
    db: &VectorDb,
    embedding_model: &str,
    file: &Path,
    source: &Path,
    file_name: &str,
    pieces: Vec<String>,
    on_chunk_done: &(dyn Fn() + Sync),
) -> Result<usize, String> {
    // Guard against duplicate chunks if this file name was ever indexed before.
    db.delete_by_file_name(file_name).await.map_err(app_error)?;

    // Prefix with the file name so the embedding also captures
    // document-level context, per the spec.
    let embedding_inputs: Vec<String> = pieces
        .iter()
        .map(|p| format!("[{file_name}]\n{p}"))
        .collect();
    let vectors = embed_texts_concurrent(embedding_model, embedding_inputs, on_chunk_done)
        .await
        .map_err(app_error)?;

    let records: Vec<ChunkRecord> = pieces
        .into_iter()
        .zip(vectors)
        .enumerate()
        .map(|(idx, (piece, vector))| ChunkRecord {
            id: Uuid::new_v4().to_string(),
            file_name: file_name.to_string(),
            chunk_index: idx as i32,
            chunk_text: piece,
            vector,
        })
        .collect();
    let chunk_count = records.len();
    db.add_chunks(&records).await.map_err(app_error)?;

    scanner::move_into(file, &scanner::archive_dir(source)).map_err(app_error)?;
    Ok(chunk_count)
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexProgressEvent {
    pub stage: String, // "ingesting" | "done"
    pub file_name: String,
    /// Chunks embedded so far in this run (not files) - a single large file
    /// can contain hundreds of chunks, so file-level counts alone would sit
    /// stuck at "1/1" for the whole run.
    pub current: usize,
    pub total: usize,
    /// Best-effort estimate of remaining time, based on the average pace of
    /// chunks completed so far in this run. `None` until at least one chunk
    /// has finished embedding.
    pub eta_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexSummary {
    pub ingested_files: usize,
    pub chunks_added: usize,
}

#[tauri::command]
pub async fn index_documents(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IndexSummary, String> {
    ensure_vector_db(&app, &state).await?;

    let (source, embedding_model) = {
        let cfg = state.config.0.lock().unwrap();
        (
            cfg.source_folder.clone().ok_or("未选择 source 文件夹")?,
            cfg.embedding_model_tag.clone().ok_or("未安装向量模型")?,
        )
    };
    let source = PathBuf::from(source);

    let db_guard = state.vector_db.lock().await;
    let db = db_guard.as_ref().ok_or("向量数据库未初始化")?;

    let to_ingest = scanner::list_pending_ingestions(&source).map_err(app_error)?;

    // Read + chunk every pending file up front so the progress bar's total
    // is expressed in chunks, not files - a single large file can take
    // minutes to embed, so file-level progress alone would sit stuck at
    // "1/1" for the entire run.
    struct PendingFile {
        path: PathBuf,
        file_name: String,
        pieces: Vec<String>,
    }
    let mut pending_files = Vec::with_capacity(to_ingest.len());
    let mut total_chunks = 0usize;
    for path in to_ingest {
        let (file_name, pieces) = read_and_chunk(&path)?;
        total_chunks += pieces.len();
        pending_files.push(PendingFile { path, file_name, pieces });
    }
    let ingest_total = pending_files.len();

    let mut chunks_added = 0usize;
    let completed_chunks = AtomicUsize::new(0);
    let start = std::time::Instant::now();

    for pf in pending_files {
        let PendingFile { path, file_name, pieces } = pf;
        let progress_file_name = file_name.clone();
        let emit_progress = || {
            let current = completed_chunks.fetch_add(1, Ordering::Relaxed) + 1;
            let eta_seconds = if total_chunks > 0 {
                let avg = start.elapsed().as_secs_f64() / current as f64;
                Some(avg * (total_chunks.saturating_sub(current)) as f64)
            } else {
                None
            };
            let _ = app.emit(
                "index-progress",
                IndexProgressEvent {
                    stage: "ingesting".into(),
                    file_name: progress_file_name.clone(),
                    current,
                    total: total_chunks,
                    eta_seconds,
                },
            );
            set_dock_progress(&app, current, total_chunks);
        };

        chunks_added +=
            ingest_file(db, &embedding_model, &path, &source, &file_name, pieces, &emit_progress)
                .await?;
    }

    let _ = app.emit(
        "index-progress",
        IndexProgressEvent {
            stage: "done".into(),
            file_name: String::new(),
            current: total_chunks,
            total: total_chunks,
            eta_seconds: Some(0.0),
        },
    );
    set_dock_progress(&app, 0, 0); // clear the dock icon progress indicator

    {
        let mut cfg = state.config.0.lock().unwrap();
        cfg.last_indexed_at = Some(chrono::Local::now().to_rfc3339());
        let _ = config::save_config(&app, &cfg);
    }

    Ok(IndexSummary {
        ingested_files: ingest_total,
        chunks_added,
    })
}

// ---------------------------------------------------------------------
// Document list + delete/restore (the two-column "已索引 / 已删除" UI)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DocumentEntry {
    pub file_name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentListsDto {
    pub indexed: Vec<DocumentEntry>,
    pub deleted: Vec<DocumentEntry>,
}

#[tauri::command]
pub async fn list_documents(state: State<'_, AppState>) -> Result<DocumentListsDto, String> {
    let source = {
        let cfg = state.config.0.lock().unwrap();
        cfg.source_folder.clone()
    };
    let Some(source) = source else {
        return Ok(DocumentListsDto { indexed: vec![], deleted: vec![] });
    };
    let source = Path::new(&source);

    let indexed = scanner::list_archived_files(source)
        .map_err(app_error)?
        .into_iter()
        .map(|(file_name, size_bytes)| DocumentEntry { file_name, size_bytes })
        .collect();
    let deleted = scanner::list_deleted_files(source)
        .map_err(app_error)?
        .into_iter()
        .map(|(file_name, size_bytes)| DocumentEntry { file_name, size_bytes })
        .collect();

    Ok(DocumentListsDto { indexed, deleted })
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentActionProgressEvent {
    pub action: String, // "deleting" | "restoring"
    pub file_name: String,
    pub current: usize,
    pub total: usize,
}

/// Removes the selected files' vectors from the database and moves the
/// files from `archive/` into `delete/`. This is the only way files move
/// into `delete/` now - it's a purely app-managed folder.
#[tauri::command]
pub async fn delete_documents(
    app: AppHandle,
    state: State<'_, AppState>,
    file_names: Vec<String>,
) -> Result<(), String> {
    ensure_vector_db(&app, &state).await?;
    let source = {
        let cfg = state.config.0.lock().unwrap();
        cfg.source_folder.clone().ok_or("未选择 source 文件夹")?
    };
    let source = PathBuf::from(source);

    let db_guard = state.vector_db.lock().await;
    let db = db_guard.as_ref().ok_or("向量数据库未初始化")?;

    let total = file_names.len();
    for (i, name) in file_names.iter().enumerate() {
        let _ = app.emit(
            "document-action-progress",
            DocumentActionProgressEvent {
                action: "deleting".into(),
                file_name: name.clone(),
                current: i + 1,
                total,
            },
        );
        db.delete_by_file_name(name).await.map_err(app_error)?;
        let from = scanner::archive_dir(&source).join(name);
        if from.exists() {
            scanner::move_into(&from, &scanner::delete_dir(&source)).map_err(app_error)?;
        }
    }
    Ok(())
}

/// Re-vectorizes the selected files (their vectors were actually deleted,
/// so this re-runs the full chunk+embed pipeline - see `ingest_file`) and
/// moves them back from `delete/` into `archive/`.
#[tauri::command]
pub async fn restore_documents(
    app: AppHandle,
    state: State<'_, AppState>,
    file_names: Vec<String>,
) -> Result<(), String> {
    ensure_vector_db(&app, &state).await?;
    let (source, embedding_model) = {
        let cfg = state.config.0.lock().unwrap();
        (
            cfg.source_folder.clone().ok_or("未选择 source 文件夹")?,
            cfg.embedding_model_tag.clone().ok_or("未安装向量模型")?,
        )
    };
    let source = PathBuf::from(source);

    let db_guard = state.vector_db.lock().await;
    let db = db_guard.as_ref().ok_or("向量数据库未初始化")?;

    // Read + chunk every selected (still-present) file up front, same as
    // index_documents, so restore progress is reported in chunks rather
    // than sitting stuck at "1/1" while a large file re-embeds.
    struct PendingRestore {
        path: PathBuf,
        file_name: String,
        pieces: Vec<String>,
    }
    let mut pending = Vec::new();
    let mut total_chunks = 0usize;
    for name in &file_names {
        let file_path = scanner::delete_dir(&source).join(name);
        if !file_path.exists() {
            continue; // already gone - nothing to restore
        }
        let (file_name, pieces) = read_and_chunk(&file_path)?;
        total_chunks += pieces.len();
        pending.push(PendingRestore { path: file_path, file_name, pieces });
    }

    let completed_chunks = AtomicUsize::new(0);
    for item in pending {
        let PendingRestore { path, file_name, pieces } = item;
        let progress_file_name = file_name.clone();
        let emit_progress = || {
            let current = completed_chunks.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit(
                "document-action-progress",
                DocumentActionProgressEvent {
                    action: "restoring".into(),
                    file_name: progress_file_name.clone(),
                    current,
                    total: total_chunks,
                },
            );
            set_dock_progress(&app, current, total_chunks);
        };
        ingest_file(db, &embedding_model, &path, &source, &file_name, pieces, &emit_progress)
            .await?;
    }
    set_dock_progress(&app, 0, 0); // clear the dock icon progress indicator
    Ok(())
}

// ---------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatReplyDto {
    pub reply: String,
    /// Distinct source file names the answer was grounded in, in relevance
    /// order - shown under the reply bubble as citations.
    pub sources: Vec<String>,
}

const SYSTEM_PROMPT_TEMPLATE: &str = "你是一个本地知识库问答助手。请仅根据下面提供的“参考资料”回答用户问题；\
如果参考资料中没有足够信息，请如实说明你不知道，不要编造。回答使用与用户提问相同的语言。\n\n参考资料：\n{context}";

#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
    history: Vec<ChatTurn>,
) -> Result<ChatReplyDto, String> {
    let chat_model = {
        let cfg = state.config.0.lock().unwrap();
        cfg.chat_model_tag.clone().ok_or("未安装大语言模型")?
    };
    let embedding_model = {
        let cfg = state.config.0.lock().unwrap();
        cfg.embedding_model_tag.clone()
    };

    let mut sources: Vec<String> = Vec::new();

    // Retrieve relevant context, if the knowledge base has been built.
    let context = if let Some(embedding_model) = embedding_model {
        ensure_vector_db(&app, &state).await.ok(); // best-effort
        let db_guard = state.vector_db.lock().await;
        if let Some(db) = db_guard.as_ref() {
            let query_vec = client::embed(&embedding_model, &message)
                .await
                .map_err(app_error)?;
            let hits = db.search(query_vec, 5).await.map_err(app_error)?;
            if hits.is_empty() {
                "（知识库中暂无相关内容）".to_string()
            } else {
                let mut seen = HashSet::new();
                for h in &hits {
                    if seen.insert(h.file_name.clone()) {
                        sources.push(h.file_name.clone());
                    }
                }
                hits.iter()
                    .map(|h| format!("【来源: {}】\n{}", h.file_name, h.chunk_text))
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n")
            }
        } else {
            "（知识库尚未建立）".to_string()
        }
    } else {
        "（知识库尚未建立）".to_string()
    };

    let system_prompt = SYSTEM_PROMPT_TEMPLATE.replace("{context}", &context);

    let mut messages = vec![client::ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
    }];
    for turn in history {
        messages.push(client::ChatMessage {
            role: turn.role,
            content: turn.content,
        });
    }
    messages.push(client::ChatMessage {
        role: "user".to_string(),
        content: message,
    });

    let reply = client::chat_stream(&app, &chat_model, &messages, "chat-token")
        .await
        .map_err(app_error)?;
    Ok(ChatReplyDto { reply, sources })
}
