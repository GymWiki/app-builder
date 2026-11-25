mod chat;
mod commands;
mod db;
mod engine;
mod error;
mod gitops;
mod install;
mod logging;
mod mcp;
mod permissions;
mod projects;
mod proc;
mod pty;
mod studio_tools;
mod toolchain;

use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: Some("studio".into()) }),
                ])
                .max_file_size(2_000_000)
                .rotation_strategy(RotationStrategy::KeepSome(3))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let db = Arc::new(db::Db::open(&dir.join("studio.db"))?);

            // Bevestigingen: een aanvraag van Claude wordt als event naar de UI gestuurd.
            let handle = app.handle().clone();
            let perms = Arc::new(permissions::Permissions::new(
                Arc::new(move |req| {
                    let _ = handle.emit("permission-request", req);
                }),
                Duration::from_secs(600),
            ));
            let mcp = mcp::start(mcp::Deps { db: db.clone(), perms: perms.clone() })?;

            let handle = app.handle().clone();
            let chat_ctx = Arc::new(chat::ChatCtx {
                db: db.clone(),
                mcp,
                config_dir: dir.join("mcp"),
                emit: Arc::new(move |name, payload| {
                    let _ = handle.emit(name, payload);
                }),
            });
            let chat = Arc::new(chat::ChatManager::new(Arc::new(engine::claude::ClaudeEngine::new())));
            log::info!("Studio {} started on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
            app.manage(commands::AppState { db, pty: pty::PtyManager::default(), chat, chat_ctx, perms });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::settings_get,
            commands::settings_set,
            commands::log_event,
            commands::check_git,
            commands::check_claude,
            commands::check_flutter,
            commands::claude_login_status,
            commands::install_plan,
            commands::pty_open_claude_login,
            commands::pty_open_install,
            commands::pty_backlog,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_kill,
            commands::projects_list,
            commands::project_get,
            commands::project_open,
            commands::project_rename,
            commands::project_create,
            commands::project_delete,
            commands::chat_send,
            commands::chat_cancel,
            commands::chat_running,
            commands::chat_history,
            commands::chat_new_session,
            commands::permission_respond,
            commands::project_screens,
            commands::pty_open_claude,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Studio");
}
