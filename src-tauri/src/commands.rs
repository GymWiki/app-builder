//! Tauri-commands: dunne laag tussen de UI en de modules.

use crate::chat::{ChatCtx, ChatManager};
use crate::db::{Db, Project};
use crate::permissions::{Decision, Permissions};
use std::sync::Arc;
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
    pub db: Arc<Db>,
    pub pty: PtyManager,
    pub chat: Arc<ChatManager>,
    pub chat_ctx: Arc<ChatCtx>,
    pub perms: Arc<Permissions>,
    pub preview: Arc<crate::preview::PreviewManager>,
    pub data_dir: PathBuf,
    /// Projecten waarvoor nu een lange taak draait (test, build, upload): één tegelijk.
    pub jobs: std::sync::Mutex<std::collections::HashSet<String>>,
}

const SETTING_KEYS: [&str; 4] = ["projects_dir", "setup_completed", "org", "dev_mode"];
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
pub fn project_open(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<Project> {
    state.db.project_touch_opened(&id)?;
    let p = state.db.project_get(&id)?;
    // Het profiel en de plugin horen er altijd te zijn; dit is goedkoop als alles al klaar is.
    spawn_profile(&app, &p.id, PathBuf::from(&p.path), false);
    Ok(p)
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
    .inspect(|p| {
        log::info!("project created: {} ({})", p.name, p.template);
        spawn_profile(&app, &p.id, PathBuf::from(&p.path), false);
    })
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

// ───────────── chat ─────────────

#[tauri::command]
pub fn chat_send(
    state: State<'_, AppState>,
    project_id: String,
    text: String,
    context: Option<String>,
    language: String,
) -> AppResult<()> {
    state.db.project_get(&project_id)?;
    state.chat.send(state.chat_ctx.clone(), project_id, text, context, language)
}

#[tauri::command]
pub fn chat_cancel(state: State<'_, AppState>, project_id: String) {
    state.perms.deny_all(&project_id);
    state.chat.cancel(&project_id);
}

#[tauri::command]
pub fn chat_running(state: State<'_, AppState>, project_id: String) -> bool {
    state.chat.is_running(&project_id)
}

/// Opgeslagen gebeurtenissen van het huidige gesprek (JSON per gebeurtenis).
#[tauri::command]
pub fn chat_history(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<serde_json::Value>> {
    Ok(state
        .db
        .messages_active(&project_id)?
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect())
}

/// Nieuw gesprek: het oude blijft bewaard, maar het nieuwe begint zonder context.
#[tauri::command]
pub fn chat_new_session(state: State<'_, AppState>, project_id: String) -> AppResult<()> {
    if state.chat.is_running(&project_id) {
        return Err(AppError::Invalid("Claude is still working".into()));
    }
    state.db.project_get(&project_id)?;
    state.db.session_create(&project_id, "claude-code")?;
    Ok(())
}

#[tauri::command]
pub fn permission_respond(state: State<'_, AppState>, id: String, decision: Decision) -> bool {
    state.perms.respond(&state.db, &id, decision)
}

#[tauri::command]
pub fn project_screens(state: State<'_, AppState>, id: String) -> AppResult<Vec<crate::studio_tools::Screen>> {
    let p = state.db.project_get(&id)?;
    crate::studio_tools::list_screens(std::path::Path::new(&p.path))
}

/// Ruwe `claude`-terminal in de projectmap ("geavanceerd"-tab).
#[tauri::command]
pub fn pty_open_claude(app: AppHandle, state: State<'_, AppState>, project_id: String) -> AppResult<String> {
    let p = state.db.project_get(&project_id)?;
    let claude = proc::find("claude").ok_or_else(|| AppError::ToolMissing("claude".into()))?;
    state.pty.spawn(&app, &claude, &[], Some(std::path::Path::new(&p.path)))
}

// ───────────── preview ─────────────

#[tauri::command]
pub async fn preview_start(app: AppHandle, state: State<'_, AppState>, project_id: String, target: String) -> AppResult<()> {
    let flutter = proc::find("flutter").ok_or_else(|| AppError::ToolMissing("flutter".into()))?;
    let project = state.db.project_get(&project_id)?;
    let dir = PathBuf::from(&project.path);
    if !crate::preview::project_dir_exists(&dir) {
        return Err(AppError::NotFound(format!("Flutter project at {}", dir.display())));
    }
    let handle = app.clone();
    blocking(move || {
        let st = handle.state::<AppState>();
        st.preview.start(&flutter, &project_id, &dir, &target)
    })
    .await
}

#[tauri::command]
pub async fn preview_stop(state: State<'_, AppState>) -> AppResult<()> {
    let p = state.preview.clone();
    blocking(move || {
        p.stop();
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn preview_state(state: State<'_, AppState>) -> crate::preview::PreviewState {
    state.preview.state()
}

#[tauri::command]
pub fn preview_log(state: State<'_, AppState>) -> Vec<String> {
    state.preview.log()
}

#[tauri::command]
pub fn preview_reload(state: State<'_, AppState>) -> AppResult<()> {
    state.preview.reload()
}

#[tauri::command]
pub async fn preview_devices() -> AppResult<Vec<crate::preview::Device>> {
    let flutter = proc::find("flutter").ok_or_else(|| AppError::ToolMissing("flutter".into()))?;
    blocking(move || crate::preview::list_devices(&flutter)).await
}

// ───────────── hulpjes voor fase 4 en 5 ─────────────

fn project_and_dir(state: &AppState, id: &str) -> AppResult<(Project, PathBuf)> {
    let p = state.db.project_get(id)?;
    let dir = PathBuf::from(&p.path);
    Ok((p, dir))
}

struct JobGuard<'a> {
    state: &'a AppState,
    id: String,
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.state.jobs.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.id);
    }
}

fn start_job<'a>(state: &'a AppState, id: &str) -> AppResult<JobGuard<'a>> {
    if !state.jobs.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string()) {
        return Err(AppError::Invalid("another task is still running for this project".into()));
    }
    Ok(JobGuard { state, id: id.to_string() })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobLine {
    project_id: String,
    job: String,
    line: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobState {
    project_id: String,
    job: String,
    /// running | done | failed
    status: String,
}

fn job_state(app: &AppHandle, id: &str, job: &str, status: &str) {
    let _ = app.emit("job-state", JobState { project_id: id.into(), job: job.into(), status: status.into() });
}

// ───────────── thema, schermen, instellingen ─────────────

#[tauri::command]
pub fn theme_get(state: State<'_, AppState>, project_id: String) -> AppResult<serde_json::Value> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::studio_tools::get_theme(&dir)
}

#[tauri::command]
pub fn theme_set(state: State<'_, AppState>, project_id: String, seed_color: String) -> AppResult<serde_json::Value> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::studio_tools::set_theme(&dir, &seed_color)
}

#[tauri::command]
pub fn screens_graph(state: State<'_, AppState>, project_id: String) -> AppResult<crate::screens::ScreenGraph> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::screens::screen_graph(&dir)
}

#[tauri::command]
pub fn platform_get(state: State<'_, AppState>, project_id: String) -> AppResult<crate::platform::PlatformSettings> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    Ok(crate::platform::read_settings(&dir))
}

#[tauri::command]
pub fn platform_set_name(state: State<'_, AppState>, project_id: String, name: String) -> AppResult<crate::platform::Applied> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let r = crate::platform::set_app_name(&dir, &name)?;
    state.db.project_rename(&project_id, name.trim())?;
    Ok(r)
}

#[tauri::command]
pub fn platform_set_bundle(state: State<'_, AppState>, project_id: String, bundle_id: String) -> AppResult<crate::platform::Applied> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let r = crate::platform::set_bundle_id(&dir, &bundle_id)?;
    state.db.project_set_bundle(&project_id, &bundle_id)?;
    Ok(r)
}

#[tauri::command]
pub fn platform_set_min_os(state: State<'_, AppState>, project_id: String, android: Option<u32>, ios: Option<String>) -> AppResult<crate::platform::Applied> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::platform::set_min_os(&dir, android, ios.as_deref())
}

#[tauri::command]
pub fn platform_set_permission(state: State<'_, AppState>, project_id: String, key: String, enabled: bool, ios_reason: String) -> AppResult<crate::platform::Applied> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::platform::set_permission(&dir, &key, enabled, &ios_reason)
}

/// Versie expliciet zetten (`1.2.3` + buildnummer) in pubspec.yaml en in Studio.
#[tauri::command]
pub fn version_set(state: State<'_, AppState>, project_id: String, version: String, build: u32) -> AppResult<()> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || p.parse::<u32>().is_err()) || build == 0 {
        return Err(AppError::Invalid("version must look like 1.2.3 with a build number of 1 or more".into()));
    }
    let path = dir.join("pubspec.yaml");
    let text = std::fs::read_to_string(&path)?;
    let new_line = format!("version: {version}+{build}");
    let mut found = false;
    let mut out: Vec<String> = text.lines().map(|l| if l.starts_with("version:") { found = true; new_line.clone() } else { l.to_string() }).collect();
    if !found {
        return Err(AppError::Invalid("pubspec.yaml has no version line".into()));
    }
    if text.ends_with('\n') {
        out.push(String::new());
    }
    std::fs::write(&path, out.join("\n"))?;
    state.db.project_set_version(&project_id, &version, build as i64)
}

#[tauri::command]
pub fn assets_generate_icon(state: State<'_, AppState>, project_id: String, source_path: String) -> AppResult<crate::assets::IconReport> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::assets::generate_icons(&dir, std::path::Path::new(&source_path))
}

// ───────────── versies ─────────────

#[tauri::command]
pub fn snapshots_list(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<crate::db::Snapshot>> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let mut list = state.db.snapshots_list(&project_id)?;
    // Nog niet vastgelegde wijzigingen laten we de UI apart weten via het aantal.
    let _ = crate::gitops::changed_files(&dir);
    list.truncate(200);
    Ok(list)
}

#[tauri::command]
pub fn snapshot_create(state: State<'_, AppState>, project_id: String, label: String) -> AppResult<serde_json::Value> {
    let (p, _) = project_and_dir(&state, &project_id)?;
    crate::studio_tools::create_snapshot(&state.db, &p, &label, "manual")
}

/// Teruggaan naar een eerdere versie (de UI vraagt eerst bevestiging). Niets gaat verloren: het wordt een nieuwe snapshot.
#[tauri::command]
pub fn snapshot_restore(state: State<'_, AppState>, project_id: String, commit: String) -> AppResult<serde_json::Value> {
    if state.chat.is_running(&project_id) || state.preview.state().project_id.as_deref() == Some(&project_id) && state.preview.state().status == "reloading" {
        return Err(AppError::Invalid("wait until Claude has finished".into()));
    }
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let label = state
        .db
        .snapshots_list(&project_id)?
        .into_iter()
        .find(|s| s.commit == commit)
        .map(|s| format!("Restored: {}", s.label))
        .unwrap_or_else(|| "Restored an earlier version".into());
    let new = crate::gitops::restore(&dir, &commit, &label)?;
    if let Some(c) = &new {
        let v = state.db.project_get(&project_id)?.version;
        state.db.snapshot_insert(&project_id, c, &label, "manual", &v)?;
        // Versie uit pubspec weer overnemen
        if let Some((a, b, c2, n)) = std::fs::read_to_string(dir.join("pubspec.yaml")).ok().and_then(|t| crate::studio_tools::parse_pubspec_version(&t)) {
            let _ = state.db.project_set_version(&project_id, &format!("{a}.{b}.{c2}"), n as i64);
        }
    }
    state.db.project_touch_updated(&p.id)?;
    Ok(serde_json::json!({ "restored": new.is_some(), "commit": new }))
}

/// Concept voor een changelog: de labels van alle momentopnames sinds de laatste publicatie.
#[tauri::command]
pub fn changelog_draft(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<String>> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let meta = crate::meta::load(&dir)?;
    let mut lines = crate::gitops::log_since(&dir, meta.last_published_commit.as_deref()).or_else(|_| crate::gitops::log_since(&dir, None))?;
    lines.retain(|l| !l.starts_with("Before restoring") && !l.starts_with("Start:"));
    Ok(lines)
}

// ───────────── vermelding, geheimen, checklist ─────────────

#[tauri::command]
pub fn meta_get(state: State<'_, AppState>, project_id: String) -> AppResult<crate::meta::Meta> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::meta::load(&dir)
}

#[tauri::command]
pub fn meta_save(state: State<'_, AppState>, project_id: String, meta: crate::meta::Meta) -> AppResult<()> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    // Wat uit meta is afgeleid (laatste publicatie) mag de UI niet zelf overschrijven.
    let current = crate::meta::load(&dir)?;
    let mut meta = meta;
    meta.last_published_commit = current.last_published_commit;
    meta.last_published_version = current.last_published_version;
    crate::meta::save(&dir, &meta)
}

/// Geheimen: alleen zetten, controleren of ze bestaan en verwijderen. De waarde komt nooit terug.
#[tauri::command]
pub fn secret_set(state: State<'_, AppState>, project_id: String, name: String, value: String) -> AppResult<()> {
    state.db.project_get(&project_id)?;
    if name == "play_service_account_json" && serde_json::from_str::<serde_json::Value>(&value).ok().and_then(|v| v.get("client_email").cloned()).is_none() {
        return Err(AppError::Invalid("this does not look like a Google service account key (JSON with client_email)".into()));
    }
    if value.trim().is_empty() {
        return Err(AppError::Invalid("the value is empty".into()));
    }
    crate::keychain::set(&project_id, &name, value.trim())
}

#[tauri::command]
pub fn secret_has(project_id: String, name: String) -> AppResult<bool> {
    crate::keychain::has(&project_id, &name)
}

#[tauri::command]
pub fn secret_delete(project_id: String, name: String) -> AppResult<()> {
    crate::keychain::delete(&project_id, &name)
}

#[tauri::command]
pub fn data_setup_prompt(state: State<'_, AppState>, project_id: String) -> AppResult<String> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    Ok(crate::integrations::setup_prompt(&crate::meta::load(&dir)?))
}

fn count_screenshots(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir.join("store/screenshots")).map(|rd| rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "png" || x == "jpg")).count()).unwrap_or(0)
}

fn gather_facts(state: &AppState, p: &Project, dir: &std::path::Path) -> crate::checklist::Facts {
    let last = state.db.setting_get(&format!("testrun:{}", p.id)).ok().flatten().unwrap_or_default();
    let current = crate::gitops::fingerprint(dir).unwrap_or_default();
    let android_id = crate::platform::read_settings(dir).android_application_id.unwrap_or_else(|| p.bundle_id.clone());
    let pubspec = std::fs::read_to_string(dir.join("pubspec.yaml")).unwrap_or_default();
    crate::checklist::Facts {
        bundle_id: android_id,
        version: p.version.clone(),
        has_icon: dir.join("store/icon-512.png").is_file(),
        screenshots: count_screenshots(dir),
        privacy_docs: dir.join("legal/privacy-policy.md").is_file() && dir.join("legal/terms.md").is_file(),
        changelog: std::fs::read_to_string(dir.join("CHANGELOG.md")).map(|t| t.trim().len() > 20).unwrap_or(false),
        keystore: crate::signing::keystore_path(&state.data_dir, &p.id).is_file(),
        signing_configured: crate::signing::gradle_configured(dir),
        test_passed_fingerprint: last.strip_prefix("passed:").map(str::to_string),
        test_ever_ran: !last.is_empty(),
        test_failed_last: last.starts_with("failed:"),
        current_fingerprint: current,
        native_features: crate::testrun::native_features(&pubspec),
    }
}

fn evaluate_and_store(state: &AppState, p: &Project, dir: &std::path::Path) -> AppResult<crate::checklist::Evaluation> {
    let meta = crate::meta::load(dir)?;
    let facts = gather_facts(state, p, dir);
    let e = crate::checklist::evaluate(&meta, &facts);
    for it in &e.items {
        let detail = serde_json::to_string(&it.missing).unwrap_or_default();
        state.db.checklist_update(&p.id, &it.key, &it.status, &detail, None)?;
    }
    // Status van het project volgt de poort (gepubliceerd blijft gepubliceerd).
    if p.status != "published" {
        let status = if e.can_publish { "ready" } else if state.db.setting_get(&format!("testrun:{}", p.id))?.is_some() { "testing" } else { "draft" };
        if status != p.status {
            state.db.project_set_status(&p.id, status)?;
        }
    }
    Ok(e)
}

#[tauri::command]
pub fn checklist_evaluate(state: State<'_, AppState>, project_id: String) -> AppResult<crate::checklist::Evaluation> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    evaluate_and_store(&state, &p, &dir)
}

// ───────────── winkelschermafdrukken ─────────────

#[tauri::command]
pub fn store_screenshots_list(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<String>> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let mut v: Vec<String> = std::fs::read_dir(dir.join("store/screenshots")).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".png") || n.ends_with(".jpg")).collect()).unwrap_or_default();
    v.sort();
    Ok(v)
}

/// Maakt een winkel-schermafdruk (1080×1920) van de draaiende preview.
#[tauri::command]
pub async fn store_screenshot_capture(app: AppHandle, state: State<'_, AppState>, project_id: String) -> AppResult<String> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let st = state.preview.state();
    let url = match (&st.url, st.project_id.as_deref() == Some(project_id.as_str()), st.status.as_str()) {
        (Some(u), true, "running" | "reloading") => u.clone(),
        _ => return Err(AppError::Invalid("start the live preview first".into())),
    };
    let work = state.data_dir.join("shots");
    let handle = app.clone();
    blocking(move || {
        let _ = handle;
        let png = crate::screenshot::capture(&url, 360, 640, 3, &work)?;
        let out = dir.join("store/screenshots");
        std::fs::create_dir_all(&out)?;
        let n = std::fs::read_dir(&out)?.flatten().count() + 1;
        let name = format!("phone-{n:02}.png");
        std::fs::write(out.join(&name), png)?;
        Ok(name)
    })
    .await
}

#[tauri::command]
pub fn store_screenshot_import(state: State<'_, AppState>, project_id: String, paths: Vec<String>) -> AppResult<Vec<String>> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let out = dir.join("store/screenshots");
    std::fs::create_dir_all(&out)?;
    let mut names = vec![];
    for p in paths.iter().take(8) {
        let src = std::path::Path::new(p);
        let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !["png", "jpg", "jpeg"].contains(&ext.as_str()) || std::fs::metadata(src)?.len() > 15 * 1024 * 1024 {
            return Err(AppError::Invalid("screenshots must be PNG or JPG images under 15 MB".into()));
        }
        let n = std::fs::read_dir(&out)?.flatten().count() + 1;
        let name = format!("imported-{n:02}.{}", if ext == "png" { "png" } else { "jpg" });
        std::fs::copy(src, out.join(&name))?;
        names.push(name);
    }
    Ok(names)
}

#[tauri::command]
pub fn store_screenshot_delete(state: State<'_, AppState>, project_id: String, name: String) -> AppResult<()> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    if name.contains(['/', '\\']) || !(name.ends_with(".png") || name.ends_with(".jpg")) {
        return Err(AppError::Unsafe("invalid file name".into()));
    }
    let _ = std::fs::remove_file(dir.join("store/screenshots").join(name));
    Ok(())
}

/// Afbeelding als data-URL, alleen uit `store/` en de testmap.
#[tauri::command]
pub fn file_data_url(state: State<'_, AppState>, project_id: String, rel: String) -> AppResult<String> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let allowed = rel.starts_with("store/") || rel.starts_with("build/studio_test/");
    if !allowed || rel.contains("..") || !(rel.ends_with(".png") || rel.ends_with(".jpg")) {
        return Err(AppError::Unsafe("this file cannot be shown".into()));
    }
    let bytes = std::fs::read(dir.join(&rel))?;
    let mime = if rel.ends_with(".png") { "image/png" } else { "image/jpeg" };
    Ok(format!("data:{mime};base64,{}", crate::screenshot::base64(&bytes)))
}

// ───────────── AI-testrun ─────────────

#[tauri::command]
pub async fn testrun_start(app: AppHandle, state: State<'_, AppState>, project_id: String) -> AppResult<crate::testrun::TestReport> {
    let flutter = proc::find("flutter").ok_or_else(|| AppError::ToolMissing("flutter".into()))?;
    let (_, dir) = project_and_dir(&state, &project_id)?;
    if state.chat.is_running(&project_id) {
        return Err(AppError::Invalid("wait until Claude has finished".into()));
    }
    let handle = app.clone();
    let pid = project_id.clone();
    blocking(move || {
        let st = handle.state::<AppState>();
        let _guard = start_job(&st, &pid)?;
        job_state(&handle, &pid, "test", "running");
        // Vingerafdruk vóór de run: de testrun is aan precies deze code gekoppeld.
        let _ = crate::testrun::ensure_crawler(&dir, &crate::testrun::package_name(&dir).unwrap_or_default());
        let fp = crate::gitops::fingerprint(&dir)?;
        let h2 = handle.clone();
        let p2 = pid.clone();
        let out = crate::testrun::run(&flutter, &dir, &mut |line| {
            let _ = h2.emit("job-log", JobLine { project_id: p2.clone(), job: "test".into(), line: line.into() });
        });
        let out = match out {
            Ok(o) => o,
            Err(e) => {
                job_state(&handle, &pid, "test", "failed");
                return Err(e);
            }
        };
        let report = out.report.ok_or_else(|| AppError::CommandFailed(format!("the test run produced no report: {}", out.log.lines().rev().take(6).collect::<Vec<_>>().join(" | "))));
        let report = match report {
            Ok(r) => r,
            Err(e) => {
                st.db.setting_set(&format!("testrun:{pid}"), &format!("failed:{fp}"))?;
                job_state(&handle, &pid, "test", "failed");
                return Err(e);
            }
        };
        // Alleen als de code tijdens de run niet is veranderd, telt een geslaagde run voor de huidige versie.
        let unchanged = crate::gitops::fingerprint(&dir)? == fp;
        let value = if report.passed() && unchanged { format!("passed:{fp}") } else { format!("failed:{fp}") };
        st.db.setting_set(&format!("testrun:{pid}"), &value)?;
        job_state(&handle, &pid, "test", if report.passed() { "done" } else { "failed" });
        Ok(report)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    errors: usize,
    warnings: usize,
    report: Option<crate::testrun::TestReport>,
    passed: bool,
    /// De code is veranderd sinds deze run: het rapport is niet meer van toepassing.
    stale: bool,
    native_features: Vec<String>,
    /// Wat deze run niet kan testen (altijd getoond).
    not_tested: Vec<&'static str>,
}

#[tauri::command]
pub fn testrun_report(state: State<'_, AppState>, project_id: String) -> AppResult<ReportView> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let last = state.db.setting_get(&format!("testrun:{project_id}"))?.unwrap_or_default();
    let current = crate::gitops::fingerprint(&dir).unwrap_or_default();
    let fp = last.split_once(':').map(|(_, f)| f.to_string()).unwrap_or_default();
    let pubspec = std::fs::read_to_string(dir.join("pubspec.yaml")).unwrap_or_default();
    let report = crate::testrun::read_report(&dir);
    Ok(ReportView {
        errors: report.as_ref().map_or(0, |r| r.error_count()),
        warnings: report.as_ref().map_or(0, |r| r.warning_count()),
        report,
        passed: last.starts_with("passed:") && fp == current,
        stale: !fp.is_empty() && fp != current,
        native_features: crate::testrun::native_features(&pubspec),
        not_tested: vec!["camera", "push", "payments", "sensors", "real_device", "performance"],
    })
}

// ───────────── signing, build, upload ─────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningStatus {
    keytool: bool,
    keystore: bool,
    keystore_path: Option<String>,
    gradle_configured: bool,
    play_key: bool,
    fastlane: bool,
}

#[tauri::command]
pub fn signing_status(state: State<'_, AppState>, project_id: String) -> AppResult<SigningStatus> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let path = crate::signing::keystore_path(&state.data_dir, &project_id);
    Ok(SigningStatus {
        keytool: crate::signing::find_keytool(None).is_some(),
        keystore: path.is_file(),
        keystore_path: path.is_file().then(|| path.display().to_string()),
        gradle_configured: crate::signing::gradle_configured(&dir),
        play_key: crate::keychain::has(&project_id, "play_service_account_json").unwrap_or(false),
        fastlane: proc::find("fastlane").is_some(),
    })
}

#[tauri::command]
pub async fn signing_create_keystore(state: State<'_, AppState>, project_id: String, owner: String, organisation: String, country: String) -> AppResult<String> {
    state.db.project_get(&project_id)?;
    let flutter = proc::find("flutter");
    let data_dir = state.data_dir.clone();
    blocking(move || {
        let keytool = crate::signing::find_keytool(flutter.as_deref()).ok_or_else(|| AppError::ToolMissing("keytool".into()))?;
        let p = crate::signing::create_keystore(&keytool, &data_dir, &project_id, &crate::signing::dname(&owner, &organisation, &country))?;
        Ok(p.display().to_string())
    })
    .await
}

#[tauri::command]
pub fn signing_prompt() -> &'static str {
    crate::signing::GRADLE_PROMPT
}

/// Bouwt het App Bundle en uploadt het als concept naar een testtrack van Google Play.
/// De UI vraagt eerst bevestiging (`confirmed`); de poort wordt hier opnieuw server-side gecontroleerd.
#[tauri::command]
pub async fn publish_android(app: AppHandle, state: State<'_, AppState>, project_id: String, track: String, confirmed: bool) -> AppResult<serde_json::Value> {
    use crate::publish::{PlayPublisher, StorePublisher, UploadRequest};
    if !confirmed {
        return Err(AppError::Invalid("publishing needs confirmation".into()));
    }
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let eval = evaluate_and_store(&state, &p, &dir)?;
    if !eval.can_publish {
        return Err(AppError::Invalid("the checklist is not complete".into()));
    }
    let flutter = proc::find("flutter").ok_or_else(|| AppError::ToolMissing("flutter".into()))?;
    let fastlane = proc::find("fastlane").ok_or_else(|| AppError::ToolMissing("fastlane".into()))?;
    let handle = app.clone();
    blocking(move || {
        let st = handle.state::<AppState>();
        let _guard = start_job(&st, &project_id)?;
        let emit_line = |job: &'static str| {
            let (h, id) = (handle.clone(), project_id.clone());
            move |line: &str| {
                let _ = h.emit("job-log", JobLine { project_id: id.clone(), job: job.into(), line: line.into() });
            }
        };
        let env = crate::signing::build_env(&project_id, &st.data_dir)?;
        let package = crate::platform::read_settings(&dir).android_application_id.unwrap_or_else(|| p.bundle_id.clone());
        let publisher = PlayPublisher;
        log::info!("publishing {} to {}", p.name, publisher.id());

        job_state(&handle, &project_id, "build", "running");
        let aab = match publisher.build(&flutter, &dir, &env, &mut emit_line("build")) {
            Ok(a) => a,
            Err(e) => {
                job_state(&handle, &project_id, "build", "failed");
                return Err(e);
            }
        };
        job_state(&handle, &project_id, "build", "done");

        job_state(&handle, &project_id, "upload", "running");
        let work = st.data_dir.join("upload");
        let req = UploadRequest { project_id: &project_id, package: &package, artifact: &aab, track: &track, work_dir: &work, fastlane: &fastlane };
        if let Err(e) = publisher.upload(&req, &mut emit_line("upload")) {
            job_state(&handle, &project_id, "upload", "failed");
            return Err(e);
        }
        job_state(&handle, &project_id, "upload", "done");

        // Vastleggen wat er is gepubliceerd.
        let mut meta = crate::meta::load(&dir)?;
        meta.last_published_version = Some(p.version.clone());
        meta.last_published_commit = crate::gitops::head(&dir).ok();
        meta.publishing.final_confirmed = false;
        crate::meta::save(&dir, &meta)?;
        let _ = crate::studio_tools::create_snapshot(&st.db, &p, &format!("Published version {} ({track} testing)", p.version), "publish");
        st.db.project_set_status(&project_id, "published")?;
        log::info!("published {} v{} to Google Play ({track})", p.name, p.version);
        Ok(serde_json::json!({ "version": p.version, "track": track }))
    })
    .await
}

/// iOS: de interface bestaat al; de implementatie (cloud-build op macOS) volgt later.
#[tauri::command]
pub fn publish_ios() -> AppResult<()> {
    use crate::publish::{AppStorePublisher, StorePublisher, UploadRequest};
    let p = AppStorePublisher;
    let _ = p.id();
    let work = std::path::Path::new(".");
    let req = UploadRequest { project_id: "", package: "", artifact: work, track: "", work_dir: work, fastlane: work };
    p.upload(&req, &mut |_| {})
}

// ───────────── projectprofiel ─────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileStatus {
    project_id: String,
    /// working | ready | limited
    state: String,
    /// Alleen voor de ontwikkelaarsmodus.
    reason: Option<String>,
}

fn profile_status(app: &AppHandle, id: &str, state: &str, reason: Option<String>) {
    let _ = app.emit("profile-status", ProfileStatus { project_id: id.into(), state: state.into(), reason });
}

/// Zet op de achtergrond de officiële Flutter/Dart-plugin klaar (en past het profiel toe als dat nodig is).
/// De gebruiker ziet alleen een rustige statusregel; mislukken is niet erg (Studio's eigen skills en tools werken door).
pub fn spawn_profile(app: &AppHandle, project_id: &str, dir: PathBuf, force: bool) {
    let (app, id) = (app.clone(), project_id.to_string());
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let Ok(p) = st.db.project_get(&id) else { return };
        let version = st.db.profile_version(&id).unwrap_or(0);
        let needs_files = force || version < crate::profile::CURRENT;
        let needs_plugin = !crate::profile::plugin_enabled(&dir);
        if !needs_files && !needs_plugin {
            return;
        }
        profile_status(&app, &id, "working", None);
        if needs_files {
            let facts = crate::profile::ProjectFacts { name: &p.name, structure: crate::profile::structure_of(&dir) };
            match crate::profile::apply(&dir, &facts) {
                Ok(r) => {
                    for (f, why) in &r.skipped {
                        log::info!("profile: skipped {f}: {why}");
                    }
                    let _ = st.db.set_profile_version(&id, crate::profile::CURRENT);
                }
                Err(e) => log::warn!("profile: could not apply for {}: {e}", p.name),
            }
        }
        let mut limited: Option<String> = None;
        match proc::find("dart").and_then(|_| crate::profile::dart_version()) {
            Some(v) if crate::profile::dart_mcp_supported(&v) => {}
            Some(v) => limited = Some(format!("dart {v} is older than {}", crate::profile::MIN_DART)),
            None => limited = Some("dart not found".into()),
        }
        if needs_plugin {
            let source = std::env::var("STUDIO_PLUGIN_SOURCE").unwrap_or_else(|_| crate::profile::PLUGIN_SOURCE.into());
            match proc::find("claude") {
                Some(claude) => {
                    if let crate::profile::PluginOutcome::Failed(m) = crate::profile::ensure_plugin(&claude, &dir, &source) {
                        log::warn!("profile: plugin install failed: {m}");
                        limited = Some(format!("plugin: {m}"));
                    }
                }
                None => limited = Some("claude not found".into()),
            }
        }
        profile_status(&app, &id, if limited.is_some() { "limited" } else { "ready" }, limited);
    });
}

/// Alleen voor de ontwikkelaarsmodus.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    version: u32,
    current: u32,
    plugin: bool,
    dart: Option<String>,
    dart_mcp_ok: bool,
    structure: &'static str,
}

#[tauri::command]
pub fn profile_info(state: State<'_, AppState>, project_id: String) -> AppResult<ProfileInfo> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let dart = crate::profile::dart_version();
    Ok(ProfileInfo {
        version: state.db.profile_version(&project_id)?,
        current: crate::profile::CURRENT,
        plugin: crate::profile::plugin_enabled(&dir),
        dart_mcp_ok: dart.as_deref().is_some_and(crate::profile::dart_mcp_supported),
        dart,
        structure: crate::profile::structure_of(&dir),
    })
}

#[tauri::command]
pub fn profile_apply(app: AppHandle, state: State<'_, AppState>, project_id: String) -> AppResult<()> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    spawn_profile(&app, &project_id, dir, true);
    Ok(())
}
