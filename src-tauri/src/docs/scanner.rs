//! Manages the on-disk layout of the user's chosen source folder:
//!
//! ```text
//! <source>/
//!   *.txt, *.md   <- new files to be vectorized (drop files here + click
//!                    "开始向量化" / "重新扫描文件夹")
//!   archive/       <- files currently vectorized (the "已索引" left column)
//!   delete/        <- files removed from the index via the app's UI (the
//!                    "已删除" right column). Purely app-managed: files land
//!                    here only through the in-app delete button, and leave
//!                    only through the in-app restore button.
//! ```

use anyhow::{Context, Result};
use chrono::Local;
use std::path::{Path, PathBuf};

pub const ARCHIVE_DIR: &str = "archive";
pub const DELETE_DIR: &str = "delete";
const TEXT_EXTENSIONS: [&str; 2] = ["txt", "md"];

pub fn archive_dir(source: &Path) -> PathBuf {
    source.join(ARCHIVE_DIR)
}
pub fn delete_dir(source: &Path) -> PathBuf {
    source.join(DELETE_DIR)
}

/// Creates the managed subfolders if they do not already exist. Safe to call
/// every time a source folder is (re-)selected.
pub fn ensure_layout(source: &Path) -> Result<()> {
    for dir in [archive_dir(source), delete_dir(source)] {
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("创建目录失败: {}", dir.display()))?;
        }
    }
    Ok(())
}

fn is_text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| TEXT_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn list_text_files_in(dir: &Path) -> Result<Vec<(String, u64)>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut files: Vec<(String, u64, std::time::SystemTime)> = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("读取目录失败: {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_text_file(&path) {
            let meta = entry.metadata()?;
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((name, meta.len(), modified));
        }
    }
    files.sort_by(|a, b| b.2.cmp(&a.2));
    Ok(files.into_iter().map(|(n, size, _)| (n, size)).collect())
}

/// Direct-child text files sitting in the source root (i.e. newly dropped
/// files awaiting vectorization). Does not recurse into `archive/` or
/// `delete/`.
pub fn list_pending_ingestions(source: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(source).context("读取 source 目录失败")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_text_file(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Files currently vectorized (sitting in `archive/`) - the left/"已索引"
/// column in the UI.
pub fn list_archived_files(source: &Path) -> Result<Vec<(String, u64)>> {
    list_text_files_in(&archive_dir(source))
}

/// Files removed from the index (sitting in `delete/`) - the right/"已删除"
/// column in the UI.
pub fn list_deleted_files(source: &Path) -> Result<Vec<(String, u64)>> {
    list_text_files_in(&delete_dir(source))
}

/// Moves `file` into `dest_dir`, disambiguating the filename with a
/// timestamp suffix if a file with the same name already exists there.
pub fn move_into(file: &Path, dest_dir: &Path) -> Result<PathBuf> {
    // Defensive: recreate `dest_dir` if it's missing (e.g. the user emptied
    // and removed it by hand, or it was never created for this source
    // folder). `std::fs::rename` fails with a bare "No such file or
    // directory" otherwise, which otherwise looks like a mysterious defect
    // rather than a simple missing-folder case. `create_dir_all` is a no-op
    // if the folder already exists, so this is always safe to call.
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("创建目录失败: {}", dest_dir.display()))?;

    let file_name = file
        .file_name()
        .context("无效的文件路径")?
        .to_string_lossy()
        .to_string();
    let mut dest = dest_dir.join(&file_name);

    if dest.exists() {
        let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = file.extension().map(|e| e.to_string_lossy().to_string());
        let ts = Local::now().format("%Y%m%d-%H%M%S");
        let new_name = match ext {
            Some(ext) => format!("{stem}-{ts}.{ext}"),
            None => format!("{stem}-{ts}"),
        };
        dest = dest_dir.join(new_name);
    }

    std::fs::rename(file, &dest).with_context(|| {
        // Extra detail so a failure here is immediately diagnosable instead
        // of a bare OS error number: which side of the move was actually
        // missing at the moment of the call.
        format!(
            "移动文件失败: {} -> {} (源文件存在: {}, 目标目录存在: {})",
            file.display(),
            dest.display(),
            file.exists(),
            dest_dir.exists()
        )
    })?;
    Ok(dest)
}
