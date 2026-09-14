//! Manual "import an update package" fallback for users who can't reach
//! GitHub through the normal auto-updater (most notably: users in mainland
//! China, where GitHub is often unreachable or unreliable). The user obtains
//! a release file by any means that works for them (a colleague, a proxy, a
//! USB drive) and picks it via a file dialog in the app; this module swaps
//! it into place and relaunches - the same net effect as Tauri's built-in
//! updater, just without the network fetch. There's no cryptographic
//! signature check on this path (unlike the normal updater, which verifies
//! against `pubkey` in `tauri.conf.json`), because the user already
//! explicitly picked and trusts this specific local file - that's the whole
//! point of the manual-import escape hatch.
//!
//! Supported input formats:
//! - `.dmg` - a normal signed disk image, mounted, `.app` copied out.
//! - `.app.tar.gz` - the exact artifact `createUpdaterArtifacts: true`
//!   already produces for the *automatic* updater
//!   (`src-tauri/target/release/bundle/macos/*.app.tar.gz` after
//!   `npm run tauri build`), so one release build serves both update paths.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the currently-running `.app` bundle, e.g.
/// `/Applications/本地知识库助手.app`. Derived from the running binary's own
/// path: `<bundle>.app/Contents/MacOS/<binary>` - three `parent()`s up.
fn current_app_bundle() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("无法获取当前应用路径")?;
    let bundle = exe
        .parent() // MacOS/
        .and_then(Path::parent) // Contents/
        .and_then(Path::parent) // <name>.app
        .ok_or_else(|| anyhow!("当前应用不是标准的 macOS .app 结构，无法自动替换更新"))?;
    if bundle.extension().and_then(|e| e.to_str()) != Some("app") {
        return Err(anyhow!(
            "未能定位到 .app 应用包（得到: {}），无法自动替换更新",
            bundle.display()
        ));
    }
    Ok(bundle.to_path_buf())
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let output = cmd
        .output()
        .with_context(|| format!("执行系统命令失败: {what}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{what} 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn path_str(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| anyhow!("路径包含无法处理的字符: {}", p.display()))
}

/// Finds the single top-level `*.app` directory directly inside `dir`.
fn find_app_bundle_in(dir: &Path) -> Result<PathBuf> {
    for entry in std::fs::read_dir(dir).context("读取更新包内容失败")? {
        let entry = entry.context("读取更新包内容失败")?;
        let path = entry.path();
        if path.is_dir() && path.extension().and_then(|e| e.to_str()) == Some("app") {
            return Ok(path);
        }
    }
    Err(anyhow!("更新包内没有找到 .app 应用包，文件可能已损坏或格式不对"))
}

/// Extracts `update_file` (mounting a `.dmg`, or untarring a `.tar.gz`),
/// copies the new `.app` bundle over the currently installed one, then
/// relaunches the app. On success this function does not return - the
/// process is replaced. On failure (before anything was touched, or on a
/// clearly recoverable step) it returns a descriptive error instead.
pub fn install_from_file(update_file: &Path) -> Result<()> {
    let current_bundle = current_app_bundle()?;

    let tmp = std::env::temp_dir().join(format!("app-update-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).context("创建临时目录失败")?;

    let extension = update_file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    // `.tar.gz` reports its extension as "gz" (the "tar" part is a second,
    // separate extension component), hence matching on "gz"/"tgz" here.
    let new_bundle = match extension.as_deref() {
        Some("dmg") => {
            let mount_point = tmp.join("mount");
            std::fs::create_dir_all(&mount_point).context("创建挂载点失败")?;
            run(
                Command::new("hdiutil").args([
                    "attach",
                    "-nobrowse",
                    "-mountpoint",
                    path_str(&mount_point)?,
                    path_str(update_file)?,
                ]),
                "挂载 .dmg 镜像",
            )?;
            // Copy the .app out before detaching - we can't read from the
            // mount point anymore once it's unmounted.
            let found_and_copied = find_app_bundle_in(&mount_point).and_then(|found| {
                let dest = tmp.join("copied.app");
                run(
                    Command::new("cp").args(["-R", path_str(&found)?, path_str(&dest)?]),
                    "从镜像中复制应用包",
                )?;
                Ok(dest)
            });
            // Always try to detach, even if the copy above failed, so we
            // don't leave a stray mounted volume behind.
            let _ = run(
                Command::new("hdiutil").args(["detach", path_str(&mount_point)?, "-quiet"]),
                "卸载 .dmg 镜像",
            );
            found_and_copied?
        }
        Some("gz") | Some("tgz") => {
            run(
                Command::new("tar").args(["-xzf", path_str(update_file)?, "-C", path_str(&tmp)?]),
                "解压更新包",
            )?;
            find_app_bundle_in(&tmp)?
        }
        _ => {
            return Err(anyhow!(
                "不支持的更新包格式，请提供 .dmg 或 .app.tar.gz 文件"
            ))
        }
    };

    // Replace the installed bundle. macOS allows overwriting a running
    // process's own bundle on disk (the running process keeps using its
    // already-open file handles; only a *relaunch* picks up the new files) -
    // this is the same trick Tauri's own built-in updater, and most other
    // macOS auto-updaters (e.g. Sparkle), rely on.
    run(
        Command::new("rm").args(["-rf", path_str(&current_bundle)?]),
        "删除旧版本",
    )?;
    run(
        Command::new("cp").args(["-R", path_str(&new_bundle)?, path_str(&current_bundle)?]),
        "安装新版本",
    )?;
    let _ = std::fs::remove_dir_all(&tmp);

    // Relaunch: spawn the new binary, then exit this process - mirrors what
    // `@tauri-apps/plugin-process`'s `relaunch()` does for the normal
    // (network) updater path.
    let macos_dir = current_bundle.join("Contents/MacOS");
    let binary_name = std::fs::read_dir(&macos_dir)
        .ok()
        .and_then(|mut it| it.next())
        .and_then(|e| e.ok())
        .map(|e| e.file_name());
    match binary_name {
        Some(name) => {
            let _ = Command::new(macos_dir.join(name)).spawn();
        }
        None => {
            return Err(anyhow!(
                "新版本已安装到 {}，但未能自动重启，请手动重新打开应用",
                current_bundle.display()
            ));
        }
    }
    std::process::exit(0);
}
