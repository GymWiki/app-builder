mod commands;
mod db;
mod error;
mod install;
mod logging;
mod projects;
mod proc;
mod pty;
mod toolchain;

use tauri::Manager;
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
            let db = db::Db::open(&dir.join("studio.db"))?;
            log::info!("Studio {} started on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
            app.manage(commands::AppState { db, pty: pty::PtyManager::default() });
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Studio");
}
