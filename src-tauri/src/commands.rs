//! Tauri-commands: dunne laag tussen de UI en de modules.

use crate::db::{Db, Project};
use crate::error::{AppError, AppResult};
use crate::install::{self, InstallPlan};
use crate::projects::{self, CreateRequest, Tools};
use crate::pty::PtyManager;
use crate::toolchain::{self, FlutterCheck, ToolStatus};
use crate::{logging, proc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub db: Db,
    pub pty: PtyManager,
}

const SETTING_KEYS: [&str; 3] = ["projects_dir", "setup_completed", "org"];
const DEFAULT_ORG: &str = "com.example";

fn default_projects_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .document_dir()
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Studio")
}

fn projects_dir(app: &AppHandle, state: &AppState) -> AppResult<PathBuf> {
    Ok(state
        .db
        .setting_get("projects_dir")?
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_projects_dir(app)))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::CommandFailed(format!("background task failed: {e}")))?
}

// ───────────── app ─────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
    projects_dir: String,
    log_dir: Option<String>,
    min_versions: MinVersions,
}

#[derive(Serialize)]
pub struct MinVersions {
    git: &'static str,
    flutter: &'static str,
    claude: &'static str,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        projects_dir: projects_dir(&app, &state)?.display().to_string(),
        log_dir: app.path().app_log_dir().ok().map(|p| p.display().to_string()),
        min_versions: MinVersions {
            git: toolchain::MIN_GIT,
            flutter: toolchain::MIN_FLUTTER,
            claude: toolchain::MIN_CLAUDE,
        },
    })
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> AppResult<std::collections::HashMap<String, String>> {
    Ok(state.db.settings_all()?.into_iter().filter(|(k, _)| SETTING_KEYS.contains(&k.as_str())).collect())
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, key: String, value: String) -> AppResult<()> {
    if !SETTING_KEYS.contains(&key.as_str()) {
        return Err(AppError::Invalid(format!("unknown setting {key}")));
    }
    if key == "org" && !projects::valid_org(&value) {
        return Err(AppError::Invalid("organisation id must look like com.example".into()));
    }
    state.db.setting_set(&key, &value)
}

/// Logregels vanuit de frontend. Altijd geredigeerd (geheimen nooit in logs).
#[tauri::command]
pub fn log_event(level: String, message: String) {
    let msg: String = logging::redact(&message).chars().take(4000).collect();
    match level.as_str() {
        "error" => log::error!(target: "ui", "{msg}"),
        "warn" => log::warn!(target: "ui", "{msg}"),
        "debug" => log::debug!(target: "ui", "{msg}"),
        _ => log::info!(target: "ui", "{msg}"),
    }
}

// ───────────── toolchain ─────────────

#[tauri::command]
pub async fn check_git() -> AppResult<ToolStatus> {
    blocking(|| Ok(toolchain::check_git())).await
}

#[tauri::command]
pub async fn check_claude() -> AppResult<ToolStatus> {
    blocking(|| Ok(toolchain::check_claude())).await
}

#[tauri::command]
pub async fn check_flutter() -> AppResult<FlutterCheck> {
    blocking(|| Ok(toolchain::check_flutter())).await
}

/// Alleen een boolean: is de gebruiker ingelogd? Tokens worden nooit gelezen.
#[tauri::command]
pub async fn claude_login_status() -> AppResult<bool> {
    blocking(toolchain::claude_logged_in).await
}

#[tauri::command]
pub fn install_plan(component: String) -> Option<InstallPlan> {
    install::install_plan(&component, std::env::consts::OS)
}

// ───────────── terminal ─────────────

#[tauri::command]
pub fn pty_open_claude_login(app: AppHandle, state: State<'_, AppState>) -> AppResult<String> {
    let claude = proc::find("claude").ok_or_else(|| AppError::ToolMissing("claude".into()))?;
    state.pty.spawn(&app, &claude, &["auth".into(), "login".into()], None)
}

/// Draait een vooraf vastgelegd installatiecommando. De UI vraagt eerst bevestiging.
#[tauri::command]
pub fn pty_open_install(app: AppHandle, state: State<'_, AppState>, component: String) -> AppResult<String> {
    let plan = install::install_plan(&component, std::env::consts::OS)
        .ok_or_else(|| AppError::Invalid(format!("no guided install for {component}")))?;
    let program = proc::find(&plan.program).ok_or_else(|| AppError::ToolMissing(plan.program.clone()))?;
    state.pty.spawn(&app, &program, &plan.args, None)
}

/// Uitvoer die een sessie al heeft geproduceerd (voor een terminal die later verschijnt).
#[tauri::command]
pub fn pty_backlog(state: State<'_, AppState>, id: String) -> crate::pty::Backlog {
    state.pty.backlog(&id)
}

#[tauri::command]
pub fn pty_write(state: State<'_, AppState>, id: String, data: String) -> AppResult<()> {
    state.pty.write(&id, &data)
}

#[tauri::command]
pub fn pty_resize(state: State<'_, AppState>, id: String, cols: u16, rows: u16) -> AppResult<()> {
    state.pty.resize(&id, cols, rows)
}

#[tauri::command]
pub fn pty_kill(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.pty.kill(&id)
}

// ───────────── projecten ─────────────

#[tauri::command]
pub fn projects_list(state: State<'_, AppState>, query: Option<String>) -> AppResult<Vec<Project>> {
    state.db.projects_list(query.as_deref())
}

#[tauri::command]
pub fn project_get(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    state.db.project_get(&id)
}

#[tauri::command]
pub fn project_open(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    state.db.project_touch_opened(&id)?;
    state.db.project_get(&id)
}

#[tauri::command]
pub fn project_rename(state: State<'_, AppState>, id: String, name: String) -> AppResult<Project> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::Invalid("project name must be 1-60 characters".into()));
    }
    state.db.project_rename(&id, name)
}

#[derive(Deserialize)]
pub struct CreateArgs {
    name: String,
    template: String,
    #[serde(default)]
    description: String,
}

#[derive(Clone, Serialize)]
struct Progress {
    step: String,
}

#[tauri::command]
pub async fn project_create(app: AppHandle, state: State<'_, AppState>, args: CreateArgs) -> AppResult<Project> {
    let flutter = proc::find("flutter").ok_or_else(|| AppError::ToolMissing("flutter".into()))?;
    let git = proc::find("git").ok_or_else(|| AppError::ToolMissing("git".into()))?;
    let dir = projects_dir(&app, &state)?;
    let org = state.db.setting_get("org")?.unwrap_or_else(|| DEFAULT_ORG.into());
    let req = CreateRequest { name: args.name, template: args.template, description: args.description, org };

    let app2 = app.clone();
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        projects::create_project(&state.db, &Tools { flutter, git }, &dir, &req, &|step| {
            let _ = app2.emit("project-create-progress", Progress { step: step.into() });
        })
    })
    .await
    .inspect(|p| log::info!("project created: {} ({})", p.name, p.template))
    .inspect_err(|e| log::error!("project creation failed: {e}"))
}

/// Verwijdert het project uit Studio; de map alleen als `delete_files` expliciet aan staat
/// (de UI vraagt daarvoor een aparte bevestiging).
#[tauri::command]
pub fn project_delete(app: AppHandle, state: State<'_, AppState>, id: String, delete_files: bool) -> AppResult<()> {
    let project = state.db.project_get(&id)?;
    if delete_files {
        projects::delete_project_files(&projects_dir(&app, &state)?, &PathBuf::from(&project.path))?;
    }
    state.db.project_delete_record(&id)?;
    log::info!("project removed: {} (files deleted: {delete_files})", project.name);
    Ok(())
}
