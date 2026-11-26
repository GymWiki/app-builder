mod extent;
mod config;
mod derive;
mod purchases;
mod purchases_flow;
mod inspect;
mod selection;
mod chat;
mod checklist;
mod assets;
mod commands;
mod connect;
mod db;
mod engine;
mod error;
#[cfg(test)]
mod evals;
mod gitops;
mod install;
mod index;
mod integrations;
mod scope;
mod scrub;
mod vault;
mod quality;
mod keychain;
mod leak;
mod logging;
mod mcp;
mod manifest;
mod meta;
mod modelsel;
mod permissions;
mod platform;
mod profile;
mod projects;
mod preview;
mod proc;
mod publish;
mod pty;
mod screens;
mod signing;
mod screenshot;
mod studio_tools;
mod templates;
mod testrun;
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
            // De kluis: waarden in de OS-keychain, bestanden versleuteld in de app-datamap, metadata in SQLite.
            // De ingebouwde manifesten worden bij elke start gecontroleerd; een fout daarin is een programmeerfout, geen gebruikersfout.
            let ids: Vec<&str> = manifest::all().iter().map(|m| m.id.as_str()).collect();
            for m in manifest::all() {
                if let Err(e) = manifest::check(m, &ids) {
                    log::error!("integration manifest problem: {e}");
                }
            }
            vault::init(vault::Vault::new(Arc::new(vault::KeychainBackend), &dir, db.clone()));
            {
                // Op de achtergrond: oude opslag overnemen en bekende geheimen aan de scrubber geven.
                let db = db.clone();
                std::thread::spawn(move || {
                    let ids: Vec<String> = db.projects_list(None).unwrap_or_default().into_iter().map(|p| p.id).collect();
                    for id in &ids {
                        vault::global().migrate_legacy(id);
                    }
                    vault::global().load_scrubber(&ids);
                });
            }

            // Bevestigingen: een aanvraag van Claude wordt als event naar de UI gestuurd.
            let handle = app.handle().clone();
            let perms = Arc::new(permissions::Permissions::new(
                Arc::new(move |req| {
                    let _ = handle.emit("permission-request", req);
                }),
                Duration::from_secs(600),
            ));
            let handle = app.handle().clone();
            let preview = Arc::new(preview::PreviewManager::new(Arc::new(move |name, payload| {
                let _ = handle.emit(name, payload);
            })));
            let scopes: Arc<scope::ScopeRegistry> = Default::default();
            let emit_handle = app.handle().clone();
            let mcp = mcp::start(mcp::Deps {
                db: db.clone(),
                perms: perms.clone(),
                preview: preview.clone(),
                work_dir: dir.join("shots"),
                scopes: scopes.clone(),
                vault: vault::global(),
                emit: Arc::new(move |name, payload| {
                    let _ = emit_handle.emit(name, payload);
                }),
            })?;

            let handle = app.handle().clone();
            let chat_ctx = Arc::new(chat::ChatCtx {
                db: db.clone(),
                mcp,
                config_dir: dir.join("mcp"),
                scopes,
                checks: true,
                flutter: None,
                ablation: Default::default(),
                emit: Arc::new(move |name, payload| {
                    let _ = handle.emit(name, payload);
                }),
            });
            let chat = Arc::new(chat::ChatManager::new(Arc::new(engine::claude::ClaudeEngine::new())));
            // Modelnamen, klassen en gewichten: een bestand in de app-datamap gaat voor het ingebouwde (bij te werken zonder app-update).
            chat.set_config(modelsel::EngineConfig::load(&dir, "claude"));
            log::info!("Studio {} started on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
            // Bestaande projecten met een ouder profiel stil bijwerken (nooit bestanden van de gebruiker overschrijven).
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let st = handle.state::<commands::AppState>();
                for (id, path) in st.db.projects_below_profile(profile::CURRENT).unwrap_or_default() {
                    let dir = std::path::PathBuf::from(&path);
                    if dir.is_dir() {
                        commands::spawn_profile(&handle, &id, dir, false);
                    }
                }
            });
            app.manage(commands::AppState { purchases_applying: Default::default(), db, pty: pty::PtyManager::default(), chat, chat_ctx, perms, preview, data_dir: dir.clone(), jobs: Default::default() });
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
            commands::preview_start,
            commands::preview_stop,
            commands::preview_state,
            commands::preview_log,
            commands::preview_reload,
            commands::preview_devices,
            commands::theme_get,
            commands::theme_set,
            commands::theme_apply,
            commands::ux_summary,
            commands::ux_event,
            commands::screens_graph,
            commands::engines_list,
            commands::engines_set_order,
            commands::engine_clear_limit,
            commands::engine_open_login,
            commands::purchases_suggest_id,
            commands::purchases_get,
            commands::purchases_save,
            commands::purchases_approve_gating,
            commands::purchases_confirm,
            commands::purchases_set_status,
            commands::purchases_gating_prompt,
            commands::purchases_set_gating_from_answer,
            commands::purchases_apply,
            commands::preview_select,
            commands::preview_navigate,
            commands::preview_shot_save,
            commands::preview_shot_get,
            commands::preview_shots_list,
            commands::preview_viewport,
            commands::chat_set_selection,
            commands::platform_get,
            commands::platform_set_name,
            commands::platform_set_bundle,
            commands::platform_set_min_os,
            commands::platform_set_permission,
            commands::version_set,
            commands::version_info,
            commands::assets_generate_icon,
            commands::snapshots_list,
            commands::snapshot_create,
            commands::snapshot_restore,
            commands::changelog_draft,
            commands::meta_get,
            commands::meta_save,
            commands::secret_set,
            commands::secret_has,
            commands::secret_delete,
            commands::data_setup_prompt,
            commands::checklist_evaluate,
            commands::store_screenshots_list,
            commands::store_screenshot_capture,
            commands::store_screenshot_import,
            commands::store_screenshot_delete,
            commands::file_data_url,
            commands::testrun_start,
            commands::testrun_report,
            commands::signing_status,
            commands::signing_create_keystore,
            commands::signing_prompt,
            commands::publish_android,
            commands::publish_ios,
            commands::profile_info,
            commands::run_metrics,
            commands::integration_list,
            commands::integration_set_enabled,
            commands::secret_field_set_text,
            commands::secret_field_set_file,
            commands::secret_field_delete,
            commands::secret_field_reveal,
            commands::secret_test,
            commands::vault_export,
            commands::vault_import,
            commands::vault_export_file,
            commands::vault_import_file,
            commands::leaks_scan,
            commands::leak_move_to_vault,
            commands::leak_fix_prompt,
            commands::profile_apply,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Studio")
        .run(|app, event| {
            // Geen weesprocessen: de preview (flutter/dart) stoppen als Studio afsluit.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<commands::AppState>() {
                    state.preview.stop();
                }
            }
        });
}
