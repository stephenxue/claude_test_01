pub mod commands;
pub mod config;
pub mod docs;
pub mod hardware;
pub mod manual_update;
pub mod ollama;
pub mod vectordb;

use config::ConfigState;
use ollama::process::OllamaProcessState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager};
use vectordb::VectorDb;

pub struct AppState {
    pub config: ConfigState,
    pub ollama_process: OllamaProcessState,
    pub vector_db: tokio::sync::Mutex<Option<VectorDb>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Default log level is Trace, which surfaces very noisy internal
        // connection-pool chatter from the reqwest/hyper crates on every
        // HTTP call to the local Ollama server (especially once embedding
        // calls run concurrently). Info keeps our own log output while
        // filtering that out.
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let initial_config = config::load_config(&handle).unwrap_or_default();

            app.manage(AppState {
                config: ConfigState(std::sync::Mutex::new(initial_config)),
                ollama_process: OllamaProcessState::default(),
                vector_db: tokio::sync::Mutex::new(None),
            });

            build_menu(&handle)?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "menu-about" => {
                let _ = app.emit("open-about", ());
            }
            "menu-update" => {
                let _ = app.emit("open-update-check", ());
            }
            _ => {}
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    let state = window.state::<AppState>();
                    ollama::process::stop_ollama_server(&state.ollama_process);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::check_ollama_status,
            commands::install_ollama,
            commands::start_ollama,
            commands::check_models_status,
            commands::install_models,
            commands::get_config,
            commands::select_source_folder,
            commands::index_documents,
            commands::list_documents,
            commands::delete_documents,
            commands::restore_documents,
            commands::chat_send,
            commands::set_auto_update_enabled,
            commands::write_text_file,
            commands::install_update_from_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    let about_item = MenuItem::with_id(app, "menu-about", "关于", true, None::<&str>)?;
    let update_item = MenuItem::with_id(app, "menu-update", "检查更新...", true, None::<&str>)?;

    let app_submenu = Submenu::with_items(
        app,
        "本地知识库助手",
        true,
        &[
            &update_item,
            &PredefinedMenuItem::separator(app)?,
            &about_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let edit_submenu = Submenu::with_items(
        app,
        "编辑",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let menu = Menu::with_items(app, &[&app_submenu, &edit_submenu])?;
    app.set_menu(menu)?;
    Ok(())
}
