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
    /// Projecten waar nu een Aankopen-flow draait.
    pub purchases_applying: std::sync::Mutex<std::collections::HashSet<String>>,
    pub data_dir: PathBuf,
    /// Projecten waarvoor nu een lange taak draait (test, build, upload): één tegelijk.
    pub jobs: std::sync::Mutex<std::collections::HashSet<String>>,
}

const SETTING_KEYS: [&str; 7] = ["projects_dir", "setup_completed", "org", "dev_mode", "model_override", "contact_email", "build_parallel"];
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

/// Studio opnieuw starten na een update (de gebruiker koos daar zelf voor).
#[tauri::command]
pub fn app_restart(app: AppHandle) {
    app.restart();
}


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
    blocking(|| {
        crate::licenses::refresh_process_env();
        Ok(toolchain::check_git())
    })
    .await
}

#[tauri::command]
pub async fn check_claude() -> AppResult<ToolStatus> {
    blocking(|| {
        crate::licenses::refresh_process_env();
        Ok(toolchain::check_claude())
    })
    .await
}

#[tauri::command]
pub async fn check_flutter() -> AppResult<FlutterCheck> {
    blocking(|| {
        crate::licenses::refresh_process_env();
        Ok(toolchain::check_flutter())
    })
    .await
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

// ───────────── onboarding van een nieuwe app (fase 14) ─────────────

static BUILDS: std::sync::Mutex<Option<std::collections::HashMap<String, (Arc<crate::appbuild::BuildHandle>, serde_json::Value)>>> = std::sync::Mutex::new(None);

fn builds<R>(f: impl FnOnce(&mut std::collections::HashMap<String, (Arc<crate::appbuild::BuildHandle>, serde_json::Value)>) -> R) -> R {
    let mut g = BUILDS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Default::default))
}

/// Functies voorstellen uit de beschrijving (de AI), plus wat Studio daarvan niet kan met een haalbaar alternatief.
#[tauri::command]
pub async fn onboarding_features(state: State<'_, AppState>, description: String, language: String) -> AppResult<serde_json::Value> {
    let (chat, ctx) = (state.chat.clone(), state.chat_ctx.clone());
    blocking(move || {
        let caps = crate::blueprint::capabilities();
        let text = chat.ask_text(&ctx, &crate::onboarding::features_prompt(&description, &caps, &language), crate::chat::StepTier::Light, std::time::Duration::from_secs(90));
        let ai = text.as_deref().map(|t| crate::onboarding::parse_features(t, &caps)).filter(|f| !f.is_empty());
        let used_ai = ai.is_some();
        let unsupported: Vec<serde_json::Value> = crate::blueprint::unsupported_in(&description, &caps).into_iter().map(|(id, title, alt)| serde_json::json!({ "id": id, "title": title, "alternative": alt })).collect();
        Ok(serde_json::json!({ "features": ai.unwrap_or_else(|| crate::onboarding::default_features(&language)), "unsupported": unsupported, "ai": used_ai }))
    })
    .await
}

#[tauri::command]
pub fn onboarding_styles(description: String) -> Vec<crate::onboarding::StyleCard> {
    crate::onboarding::style_cards(&description)
}

/// Het plan (blueprint) uit beschrijving, functies en stijl. Eén aanroep met een sterk model, één herstelronde; zonder antwoord een eenvoudige terugval.
#[tauri::command]
pub async fn onboarding_plan(state: State<'_, AppState>, description: String, features: Vec<String>, card_id: String, seed: Option<String>, language: String) -> AppResult<serde_json::Value> {
    let (chat, ctx) = (state.chat.clone(), state.chat_ctx.clone());
    blocking(move || {
        let cards = crate::onboarding::style_cards(&description);
        let card = cards.iter().find(|c| c.id == card_id).cloned().unwrap_or_else(|| cards[0].clone());
        let mut ask = |q: &str| chat.ask_text(&ctx, q, crate::chat::StepTier::Strong, std::time::Duration::from_secs(240));
        let (mut bp, fallback, issues) = crate::onboarding::make_plan(&mut ask, &description, &features, &card, &language);
        crate::onboarding::apply_style(&mut bp, &card, seed.as_deref());
        bp.app.description = description.chars().take(4000).collect();
        // Een paar naamsuggesties (klein model); zonder antwoord blijft de gekozen naam gewoon staan.
        let names_q = format!("Suggest 3 short, friendly app names (at most 20 characters, in the language \"{language}\") for this app. Answer with ONLY a JSON array of 3 strings.\nApp: {description}");
        let names: Vec<String> = chat
            .ask_text(&ctx, &names_q, crate::chat::StepTier::Light, std::time::Duration::from_secs(60))
            .and_then(|t| crate::onboarding::extract_array(&t).and_then(|a| serde_json::from_str::<Vec<String>>(a).ok()))
            .unwrap_or_default()
            .into_iter()
            .filter(|n| !n.trim().is_empty() && n.chars().count() <= 30)
            .take(3)
            .collect();
        Ok(serde_json::json!({ "blueprint": bp, "fallback": fallback, "issues": issues, "nameSuggestions": names }))
    })
    .await
}

/// Een onderbroken onboarding hervatten zonder dataverlies: het concept staat lokaal (geen geheimen).
#[tauri::command]
pub fn onboarding_draft_get(state: State<'_, AppState>) -> AppResult<Option<String>> {
    state.db.setting_get("onboarding_draft")
}

#[tauri::command]
pub fn onboarding_draft_set(state: State<'_, AppState>, draft: Option<String>) -> AppResult<()> {
    match draft.filter(|d| !d.is_empty()) {
        Some(d) if d.len() <= 200_000 => state.db.setting_set("onboarding_draft", &d),
        Some(_) => Err(AppError::Invalid("draft is too large".into())),
        None => state.db.setting_set("onboarding_draft", ""),
    }
}

/// "Bouw mijn app": project maken, deterministisch scaffolden en daarna de AI-bouw op de achtergrond starten (meerdere pagina's tegelijk).
#[tauri::command]
pub async fn onboarding_build(app: AppHandle, state: State<'_, AppState>, blueprint: crate::blueprint::Blueprint) -> AppResult<Project> {
    let issues = crate::blueprint::validate(&blueprint, &crate::blueprint::capabilities());
    if !issues.is_empty() {
        return Err(AppError::Invalid(format!("the plan is not valid: {}", issues.join("; "))));
    }
    let flutter = proc::find("flutter").ok_or_else(|| AppError::ToolMissing("flutter".into()))?;
    let git = proc::find("git").ok_or_else(|| AppError::ToolMissing("git".into()))?;
    let dir = projects_dir(&app, &state)?;
    let org = state.db.setting_get("org")?.unwrap_or_else(|| DEFAULT_ORG.into());
    let req = CreateRequest { name: blueprint.app.name.clone(), template: "blank".into(), description: blueprint.app.purpose.clone(), org };
    let (app2, handle) = (app.clone(), app.clone());
    let bp = blueprint.clone();
    let project = blocking(move || {
        let state = handle.state::<AppState>();
        let tools = Tools { flutter: flutter.clone(), git };
        let p = projects::create_project(&state.db, &tools, &dir, &req, &|step| {
            let _ = app2.emit("project-create-progress", Progress { step: step.into() });
        })?;
        let pdir = PathBuf::from(&p.path);
        let rep = crate::scaffold::scaffold(&pdir, &bp)?;
        // Een eigen logo (optioneel): alle iconformaten uit die ene afbeelding.
        if let Some(logo) = bp.style.logo.as_deref().filter(|l| !l.is_empty()) {
            let _ = crate::assets::generate_icons(&pdir, std::path::Path::new(logo));
        }
        if rep.needs_pub_get {
            let _ = proc::run(&flutter, &["pub", "get"], Some(&pdir), std::time::Duration::from_secs(300));
        }
        // Eén keer invullen: wat de gebruiker hier beschreef gaat naar de winkelvermelding en de vragenlijst.
        let mut meta = crate::meta::load(&pdir).unwrap_or_default();
        crate::preflight::fix_listing(&mut meta, &bp.app.name, &bp.app.purpose, "");
        if meta.listing.full_description.is_empty() {
            meta.listing.full_description = crate::preflight::shorten(&bp.app.description, 4000);
        }
        meta.questionnaire.has_accounts = Some(!bp.integrations.auth.is_empty());
        meta.questionnaire.in_app_purchases = Some(bp.integrations.purchases);
        meta.data.provider = bp.integrations.storage.clone();
        meta.data.auth_methods = bp.integrations.auth.clone();
        let _ = crate::meta::save(&pdir, &meta);
        let _ = state.db.ux_record(Some(&p.id), "onboarding", "deterministic");
        Ok(p)
    })
    .await?;
    spawn_profile(&app, &project.id, PathBuf::from(&project.path), false);
    start_build(&app, &project.id, blueprint, None);
    Ok(project)
}

fn start_build(app: &AppHandle, project_id: &str, bp: crate::blueprint::Blueprint, only: Option<Vec<String>>) {
    let handle = Arc::new(crate::appbuild::BuildHandle::default());
    builds(|b| b.insert(project_id.to_string(), (handle.clone(), serde_json::Value::Null)));
    let (app, id) = (app.clone(), project_id.to_string());
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let parallel = crate::appbuild::parallel_from(st.db.setting_get("build_parallel").ok().flatten().as_deref());
        let language = st.db.setting_get("language").ok().flatten().unwrap_or_else(|| bp.app.language.clone());
        let (a2, id2) = (app.clone(), id.clone());
        let emit = move |p: crate::appbuild::Progress| {
            let v = serde_json::to_value(&p).unwrap_or_default();
            builds(|b| {
                if let Some(e) = b.get_mut(&id2) {
                    e.1 = v.clone();
                }
            });
            let _ = a2.emit("build-progress", serde_json::json!({ "projectId": id2, "progress": v }));
        };
        let report = crate::appbuild::run(&st.chat, &st.chat_ctx, &id, &bp, parallel, &language, &handle, only.as_deref(), &emit);
        let _ = st.db.ux_record(Some(&id), "onboarding", "setting_done");
        let _ = app.emit("build-done", serde_json::json!({ "projectId": id, "report": report }));
        builds(|b| b.remove(&id));
    });
}

#[tauri::command]
pub fn onboarding_cancel(project_id: String) {
    if let Some(h) = builds(|b| b.get(&project_id).map(|e| e.0.clone())) {
        h.cancel();
    }
}

/// De voortgang van een lopende bouw (voor een UI die later verschijnt).
#[tauri::command]
pub fn onboarding_status(project_id: String) -> Option<serde_json::Value> {
    builds(|b| b.get(&project_id).map(|e| e.1.clone())).filter(|v| !v.is_null())
}

/// Mislukte of niet-klare pagina's apart opnieuw laten maken.
#[tauri::command]
pub fn onboarding_retry(app: AppHandle, state: State<'_, AppState>, project_id: String, pages: Vec<String>) -> AppResult<()> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    if builds(|b| b.contains_key(&project_id)) {
        return Err(AppError::Invalid("the app is still being built".into()));
    }
    let bp: crate::blueprint::Blueprint = serde_json::from_str(&std::fs::read_to_string(dir.join("studio/blueprint.json"))?)?;
    start_build(&app, &project_id, bp, Some(pages));
    Ok(())
}

// ───────────── Android-licenties en installaties zonder terminal ─────────────

static LICENSE_RUN: std::sync::Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = std::sync::Mutex::new(None);

/// Heeft de gebruiker al toestemming gegeven voor de Android SDK-voorwaarden? (Eén keer is genoeg.)
#[tauri::command]
pub fn android_licenses_status(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let consent = state.db.setting_get("android_license_consent")?.as_deref() == Some("true");
    Ok(serde_json::json!({ "consent": consent }))
}

/// Accepteert de Android-licenties: `sdkmanager --licenses` (of `flutter doctor --android-licenses` bij `alternative`) als subproces met pipes,
/// vragen worden herkend en met `y` beantwoord. **Alleen met toestemming** (`agree`, of eerder gegeven en onthouden). Voortgang via `license-progress`,
/// de uitkomst (met automatische `flutter doctor`-controle en diagnose) via `license-done`.
#[tauri::command]
pub fn android_licenses_accept(app: AppHandle, state: State<'_, AppState>, agree: bool, alternative: bool) -> AppResult<()> {
    let stored = state.db.setting_get("android_license_consent")?.as_deref() == Some("true");
    if !agree && !stored {
        return Err(AppError::Invalid("the Android SDK terms must be accepted by the user first".into()));
    }
    if agree && !stored {
        state.db.setting_set("android_license_consent", "true")?;
    }
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut g = LICENSE_RUN.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_some() {
            return Err(AppError::Invalid("licenses are already being accepted".into()));
        }
        *g = Some(cancel.clone());
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let env = crate::licenses::system_env();
        let started = std::time::Instant::now();
        let h2 = handle.clone();
        let emit_progress = move |p: crate::licenses::Progress| {
            let _ = h2.emit("license-progress", p);
        };
        let (result, problems) = match crate::licenses::prepare(env.clone(), alternative) {
            Ok(prep) => {
                let r = crate::licenses::run(&prep, &crate::licenses::RunOpts::default(), &cancel, &emit_progress);
                let problems = if r.ok || r.cancelled { vec![] } else { crate::licenses::diagnose(&env, true) };
                (Some(r), problems)
            }
            Err(p) => (None, p),
        };
        // Controle na afloop, automatisch: `flutter doctor` bevestigt dat de licenties echt geaccepteerd zijn.
        let ok = result.as_ref().is_some_and(|r| r.ok);
        let check = if ok {
            crate::licenses::refresh_process_env();
            Some(toolchain::check_flutter())
        } else {
            None
        };
        let confirmed = check.as_ref().is_some_and(|c| c.android.state == "ok");
        if let Some(state) = handle.try_state::<AppState>() {
            let _ = state.db.ux_record(None, "setup", if ok { "deterministic" } else { "click" });
        }
        let _ = handle.emit(
            "license-done",
            serde_json::json!({
                "result": result,
                "problems": problems,
                "check": check,
                "confirmed": confirmed,
                "seconds": started.elapsed().as_secs(),
                "alternative": alternative,
            }),
        );
        *LICENSE_RUN.lock().unwrap_or_else(|e| e.into_inner()) = None;
    });
    Ok(())
}

/// Breekt het accepteren af (de gebruiker koos "Stoppen").
#[tauri::command]
pub fn android_licenses_cancel() {
    if let Some(c) = LICENSE_RUN.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        c.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Voert een vast installatieplan (git, claude) uit zonder terminal en geeft de voortgang door als `install-line`/`install-done`.
/// De UI vraagt vooraf bevestiging van precies dit commando.
#[tauri::command]
pub fn install_run(app: AppHandle, component: String) -> AppResult<()> {
    let plan = install::install_plan(&component, std::env::consts::OS).ok_or_else(|| AppError::Invalid(format!("no guided install for {component}")))?;
    let env = crate::licenses::system_env();
    let path = env.get("PATH").cloned();
    let program = which::which_in(&plan.program, path.as_deref(), std::env::current_dir().unwrap_or_default()).map_err(|_| AppError::ToolMissing(plan.program.clone()))?;
    std::thread::spawn(move || {
        let h = app.clone();
        let c = component.clone();
        let (code, log) = install::run_streaming(&program, &plan.args, &env, std::time::Duration::from_secs(900), &move |l| {
            let _ = h.emit("install-line", serde_json::json!({ "component": c, "line": l }));
        });
        crate::licenses::refresh_process_env();
        // winget meldt 0 bij gelukt; "al geïnstalleerd/geen update" geeft een eigen code die we ook als gelukt zien.
        let ok = matches!(code, Some(0) | Some(-1978335189));
        let _ = app.emit("install-done", serde_json::json!({ "component": component, "ok": ok, "exit": code, "log": log }));
    });
    Ok(())
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
    spawn_preview(&app, &p);
    Ok(p)
}

/// De preview start vanzelf zodra een project wordt geopend (op de achtergrond, blokkeert niets). Er is geen aan/uit-schakelaar: lukt het
/// niet, dan staat er een melding met een oplossing in de preview-staat.
fn spawn_preview(app: &AppHandle, p: &Project) {
    let (handle, id, dir) = (app.clone(), p.id.clone(), PathBuf::from(&p.path));
    std::thread::spawn(move || {
        let st = handle.state::<AppState>();
        let cur = st.preview.state();
        // Draait of start de preview van dit project al (webversie), dan niets doen.
        if cur.project_id.as_deref() == Some(id.as_str()) && cur.target == "web" && matches!(cur.status.as_str(), "starting" | "running" | "reloading") {
            return;
        }
        if !crate::preview::project_dir_exists(&dir) {
            st.preview.set_failure(&id, "start_failed", "The project folder could not be found.");
            return;
        }
        let Some(flutter) = proc::find("flutter") else {
            st.preview.set_failure(&id, "flutter_missing", "Flutter is not installed or could not be found.");
            return;
        };
        if let Err(e) = st.preview.start_supervised(&flutter, &id, &dir, "web") {
            log::warn!("preview did not start: {e}");
        }
    });
}

/// Laat de preview naar een route gaan (de aangeklikte pagina).
#[tauri::command]
pub async fn preview_navigate(state: State<'_, AppState>, route: String) -> AppResult<()> {
    let p = state.preview.clone();
    blocking(move || p.navigate(&route)).await
}

fn shot_path(state: &AppState, project_id: &str, screen: &str) -> AppResult<PathBuf> {
    let ok = |s: &str| !s.is_empty() && s.len() <= 80 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok(screen) || !project_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::Invalid("invalid screen name".into()));
    }
    Ok(state.data_dir.join("previews").join(project_id).join(format!("{screen}.jpg")))
}

/// Bewaart de laatste weergave van een pagina (JPEG, base64) in de gegevens van Studio (niet in het project) zodat een volgende klik meteen iets toont.
#[tauri::command]
pub fn preview_shot_save(state: State<'_, AppState>, project_id: String, screen: String, data: String) -> AppResult<()> {
    use base64::Engine;
    state.db.project_get(&project_id)?;
    let path = shot_path(&state, &project_id, &screen)?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).map_err(|_| AppError::Invalid("not an image".into()))?;
    if bytes.len() > 4_000_000 || !bytes.starts_with(&[0xFF, 0xD8]) {
        return Err(AppError::Invalid("not a jpeg image".into()));
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

/// De bewaarde weergave als data-URL, of `None`.
#[tauri::command]
pub fn preview_shot_get(state: State<'_, AppState>, project_id: String, screen: String) -> AppResult<Option<String>> {
    use base64::Engine;
    let path = shot_path(&state, &project_id, &screen)?;
    Ok(std::fs::read(path).ok().map(|b| format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(b))))
}

/// Van welke pagina's er al een bewaarde weergave is.
#[tauri::command]
pub fn preview_shots_list(state: State<'_, AppState>, project_id: String) -> Vec<String> {
    if !project_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return vec![];
    }
    std::fs::read_dir(state.data_dir.join("previews").join(&project_id)).map(|rd| rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".jpg").map(String::from)).collect()).unwrap_or_default()
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
        let tools = Tools { flutter, git };
        let p = projects::create_project(&state.db, &tools, &dir, &req, &|step| {
            let _ = app2.emit("project-create-progress", Progress { step: step.into() });
        })?;
        apply_brand_defaults(&state, &p, &tools.flutter);
        Ok(p)
    })
    .await
    .inspect(|p| {
        log::info!("project created: {} ({})", p.name, p.template);
        spawn_profile(&app, &p.id, PathBuf::from(&p.path), false);
    })
    .inspect_err(|e| log::error!("project creation failed: {e}"))
}

/// Keuzes onthouden over projecten heen: de laatst gekozen merkkleur, lettertype en stijl gelden voor een nieuw project (stil, mislukt het dan blijft de standaard).
fn apply_brand_defaults(state: &AppState, p: &Project, flutter: &std::path::Path) {
    let Some(theme) = state.db.setting_get("theme_defaults").ok().flatten().and_then(|j| serde_json::from_str::<crate::config::ThemeCfg>(&j).ok()) else { return };
    let dir = PathBuf::from(&p.path);
    let mut cfg = crate::config::load(&dir);
    cfg.theme = theme;
    match crate::config::apply(&dir, &cfg) {
        Ok(rep) => {
            if rep.needs_pub_get {
                let _ = proc::run(flutter, &["pub", "get"], Some(&dir), std::time::Duration::from_secs(180));
            }
            let _ = state.db.ux_record(Some(&p.id), "new_project", "deterministic");
        }
        Err(e) => log::warn!("brand defaults not applied: {e}"),
    }
}

/// Verwijdert het project uit Studio; de map alleen als `delete_files` expliciet aan staat
/// (de UI vraagt daarvoor een aparte bevestiging).
#[tauri::command]
pub fn project_delete(app: AppHandle, state: State<'_, AppState>, id: String, delete_files: bool) -> AppResult<()> {
    let project = state.db.project_get(&id)?;
    if delete_files {
        projects::delete_project_files(&projects_dir(&app, &state)?, &PathBuf::from(&project.path))?;
    }
    crate::vault::global().delete_project(&id);
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
    purpose: Option<String>,
) -> AppResult<()> {
    state.db.project_get(&project_id)?;
    // `helper` en `release` komen alleen van de knoppen van Studio zelf; de modelkeuze gebruikt ze voor de ondergrens.
    state.chat.send(state.chat_ctx.clone(), project_id, text, context, language, crate::modelsel::Purpose::parse(purpose.as_deref()))
}

/// Het vak dat de gebruiker tekende → de widgets (uit de eigen code) die erin staan, met vaste feiten voor Claude.
#[tauri::command]
pub async fn preview_select(state: State<'_, AppState>, project_id: String, bbox: [f64; 4]) -> AppResult<serde_json::Value> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    // Terwijl Claude aan het werk is verandert de app (hot restart): dan geen klikken in de preview; de UI valt terug op vak + screenshot.
    if state.chat.is_running(&project_id) {
        return Err(AppError::Invalid("wait until Claude has finished".into()));
    }
    let screens: Vec<(String, String)> = crate::studio_tools::list_screens(&dir)?.into_iter().map(|s| (s.class, s.file)).collect();
    let bbox = [bbox[0].clamp(0.0, 100.0), bbox[1].clamp(0.0, 100.0), bbox[2].clamp(1.0, 100.0), bbox[3].clamp(1.0, 100.0)];
    let preview = state.preview.clone();
    let sel = blocking(move || preview.select_box(&dir, bbox, &screens)).await?;
    Ok(serde_json::json!({ "selection": sel, "prompt": sel.prompt() }))
}

/// Legt vast binnen welke regels Claude in de volgende opdracht moet blijven (leeg = geen vak). Wordt bij de start van die opdracht gebruikt en gewist.
#[tauri::command]
pub fn chat_set_selection(state: State<'_, AppState>, project_id: String, ranges: Vec<(String, usize, usize)>) {
    state.chat_ctx.scopes.set_selection(&project_id, ranges);
}

/// Formaat van het scherm in de preview (CSS-pixels), voor de beeldstroom en het omzetten van een vak.
#[tauri::command]
pub fn preview_viewport(state: State<'_, AppState>, width: u32, height: u32) {
    state.preview.set_viewport(width, height);
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
        st.preview.start_supervised(&flutter, &project_id, &dir, &target)
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
    let mut v = crate::studio_tools::get_theme(&dir)?;
    let cfg = crate::config::load(&dir);
    v["font"] = serde_json::json!(cfg.theme.font);
    v["kit"] = serde_json::json!(cfg.theme.kit);
    // Is er iets met de hand veranderd? (de UI zegt dat dan kort en past het stil terug)
    v["deviation"] = serde_json::json!(cfg.theme.seed.is_some() && crate::config::plan(&dir, &cfg).iter().any(|c| c.area == "theme"));
    Ok(v)
}

#[tauri::command]
pub fn theme_set(state: State<'_, AppState>, project_id: String, seed_color: String) -> AppResult<serde_json::Value> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::studio_tools::set_theme(&dir, &seed_color)
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThemePatch {
    pub seed_color: Option<String>,
    /// `Some("")` = terug naar de standaard.
    pub font: Option<String>,
    pub kit: Option<String>,
}

/// Thema-instellingen opslaan en meteen toepassen (deterministisch, geen AI): kleur, lettertype, stijl en koppeling aan de app.
/// Vóór een wijziging komt automatisch een momentopname; daarna `flutter pub get` (alleen bij een nieuw pakket) en een herstart van de preview.
#[tauri::command]
pub async fn theme_apply(state: State<'_, AppState>, project_id: String, patch: ThemePatch) -> AppResult<serde_json::Value> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let db = state.db.clone();
    let preview = state.preview.clone();
    blocking(move || {
        let mut cfg = crate::config::load(&dir);
        if let Some(s) = patch.seed_color {
            cfg.theme.seed = Some(s.to_uppercase());
        }
        if let Some(f) = patch.font {
            cfg.theme.font = Some(f).filter(|x| !x.is_empty());
        }
        if let Some(k) = patch.kit {
            cfg.theme.kit = Some(k).filter(|x| !x.is_empty() && x != "material");
        }
        crate::config::validate(&cfg)?;
        if !crate::config::plan(&dir, &cfg).is_empty() {
            let _ = crate::studio_tools::create_snapshot(&db, &p, "Before changing the theme", "auto");
        }
        let rep = crate::config::apply(&dir, &cfg)?;
        let mut pub_get_failed = false;
        if rep.needs_pub_get {
            pub_get_failed = match proc::find("flutter") {
                Some(f) => proc::run(&f, &["pub", "get"], Some(&dir), std::time::Duration::from_secs(180)).map(|o| o.status != Some(0)).unwrap_or(true),
                None => true,
            };
        }
        if let Ok(j) = serde_json::to_string(&cfg.theme) {
            let _ = db.setting_set("theme_defaults", &j);
        }
        let _ = db.ux_record(Some(&p.id), "theme", "deterministic");
        let _ = db.ux_record(Some(&p.id), "theme", "setting_done");
        // Een nieuw pakket vraagt een herstart (hot reload pakt het niet op); anders hot reload door de preview zelf.
        if rep.needs_pub_get && !pub_get_failed {
            let _ = preview.reload();
        }
        let mut v = serde_json::to_value(&rep)?;
        v["pubGetFailed"] = serde_json::json!(pub_get_failed);
        Ok(v)
    })
    .await
}

/// Data & Auth deterministisch toepassen vanuit de keuzes in de vermelding (`meta.data`): pakketten, init-code, auth-repository,
/// inlogscherm en route, zonder AI. Eigen tabellen en logica blijven een aparte, optionele AI-taak.
#[tauri::command]
pub async fn data_apply(state: State<'_, AppState>, project_id: String) -> AppResult<serde_json::Value> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let db = state.db.clone();
    let preview = state.preview.clone();
    blocking(move || {
        let meta = crate::meta::load(&dir)?;
        let mut cfg = crate::config::load(&dir);
        cfg.data = crate::data_gen::DataCfg { provider: meta.data.provider.clone(), auth: meta.data.auth_methods.clone() };
        if !crate::config::plan(&dir, &cfg).is_empty() {
            let _ = crate::studio_tools::create_snapshot(&db, &p, "Before setting up data and login", "auto");
        }
        let rep = crate::config::apply(&dir, &cfg)?;
        let mut pub_failed = false;
        if !rep.pub_add.is_empty() {
            pub_failed = match proc::find("flutter") {
                Some(f) => {
                    let mut args: Vec<&str> = vec!["pub", "add"];
                    args.extend(rep.pub_add.iter().map(String::as_str));
                    proc::run(&f, &args, Some(&dir), std::time::Duration::from_secs(240)).map(|o| o.status != Some(0)).unwrap_or(true)
                }
                None => true,
            };
        }
        for _ in 0..rep.changed.len().max(1) {
            let _ = db.ux_record(Some(&p.id), "data", "deterministic");
        }
        let _ = db.ux_record(Some(&p.id), "data", "setting_done");
        // Firebase: "account verwijderen" zit in de gegenereerde code, dus de winkelvraag hoeft niet nog eens gesteld (Supabase heeft eerst de SQL-functie nodig).
        let mut deletion_set = false;
        if cfg.data.provider == "firebase" && crate::data_gen::has_accounts(&cfg.data) && !meta.legal.account_deletion {
            let mut m2 = crate::meta::load(&dir)?;
            m2.legal.account_deletion = true;
            crate::meta::save(&dir, &m2)?;
            deletion_set = true;
        }
        if !pub_failed {
            let _ = preview.reload();
        }
        let mut v = serde_json::to_value(&rep)?;
        v["pubAddFailed"] = serde_json::json!(pub_failed);
        v["accountDeletionSet"] = serde_json::json!(deletion_set);
        Ok(v)
    })
    .await
}

/// UX-meting voor de ontwikkelaarsmodus: wat gebeurde deterministisch en wat via de AI, en de geschatte tokenbesparing.
#[tauri::command]
pub fn ux_summary(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let (rows, avg) = state.db.ux_summary()?;
    let mut flows: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>> = Default::default();
    for (flow, kind, n) in rows {
        flows.entry(flow).or_default().insert(kind, n.into());
    }
    let det_total: i64 = flows.values().map(|m| m.get("deterministic").and_then(|v| v.as_i64()).unwrap_or(0)).sum();
    let avg = if avg > 0 { avg } else { 8000 };
    Ok(serde_json::json!({ "flows": flows, "avgTokensPerAiRun": avg, "estimatedTokensSaved": det_total * avg }))
}

#[tauri::command]
pub fn ux_event(state: State<'_, AppState>, project_id: Option<String>, flow: String, kind: String) -> AppResult<()> {
    state.db.ux_record(project_id.as_deref(), &flow, &kind)
}

#[tauri::command]
pub fn screens_graph(state: State<'_, AppState>, project_id: String) -> AppResult<crate::screens::ScreenGraph> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let g = screens_graph_inner(&dir)?;
    // De blueprint blijft gelijk met de echte pagina's (een pagina die later is toegevoegd of verdwenen is).
    crate::scaffold::sync_blueprint(&dir, &g.nodes.iter().map(|n| (n.class.clone(), n.route.clone())).collect::<Vec<_>>());
    Ok(g)
}

fn screens_graph_inner(dir: &std::path::Path) -> AppResult<crate::screens::ScreenGraph> {
    let dir = dir.to_path_buf();
    crate::screens::screen_graph(&dir)
}

#[tauri::command]
pub fn platform_get(state: State<'_, AppState>, project_id: String) -> AppResult<crate::platform::PlatformSettings> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    Ok(crate::platform::read_settings(&dir))
}

/// Elke instelling die Studio zelf toepast: eerst een automatische momentopname (één klik terug), daarna meten dat het zonder AI ging.
fn applied_by_studio(state: &AppState, p: &Project, label: &str, flow: &str) {
    let _ = crate::studio_tools::create_snapshot(&state.db, p, label, "auto");
    let _ = state.db.ux_record(Some(&p.id), flow, "deterministic");
    let _ = state.db.ux_record(Some(&p.id), flow, "setting_done");
}

#[tauri::command]
pub fn platform_set_name(state: State<'_, AppState>, project_id: String, name: String) -> AppResult<crate::platform::Applied> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    applied_by_studio(&state, &p, "Before changing the app name", "settings");
    let r = crate::platform::set_app_name(&dir, &name)?;
    state.db.project_rename(&project_id, name.trim())?;
    Ok(r)
}

#[tauri::command]
pub fn platform_set_bundle(state: State<'_, AppState>, project_id: String, bundle_id: String) -> AppResult<crate::platform::Applied> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    applied_by_studio(&state, &p, "Before changing the bundle ID", "settings");
    let r = crate::platform::set_bundle_id(&dir, &bundle_id)?;
    state.db.project_set_bundle(&project_id, &bundle_id)?;
    Ok(r)
}

#[tauri::command]
pub fn platform_set_min_os(state: State<'_, AppState>, project_id: String, android: Option<u32>, ios: Option<String>) -> AppResult<crate::platform::Applied> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    applied_by_studio(&state, &p, "Before changing the minimum OS versions", "settings");
    crate::platform::set_min_os(&dir, android, ios.as_deref())
}

#[tauri::command]
pub fn platform_set_permission(state: State<'_, AppState>, project_id: String, key: String, enabled: bool, ios_reason: String) -> AppResult<crate::platform::Applied> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    applied_by_studio(&state, &p, "Before changing permissions", "settings");
    crate::platform::set_permission(&dir, &key, enabled, &ios_reason)
}

/// Versie expliciet zetten (`1.2.3` + buildnummer) in pubspec.yaml en in Studio. Het buildnummer moet hoger zijn dan dat van de laatste upload.
#[tauri::command]
pub fn version_set(state: State<'_, AppState>, project_id: String, version: String, build: u32) -> AppResult<crate::studio_tools::VersionInfo> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || p.parse::<u32>().is_err()) || build == 0 {
        return Err(AppError::Invalid("version must look like 1.2.3 with a build number of 1 or more".into()));
    }
    if let Some(last) = crate::meta::load(&dir)?.last_published_build {
        if build <= last {
            return Err(AppError::Invalid(format!("the build number must be higher than {last}, the one already uploaded")));
        }
    }
    let n: Vec<u32> = parts.iter().filter_map(|x| x.parse().ok()).collect();
    crate::studio_tools::apply_version(&state.db, &p, (n[0], n[1], n[2], build))?;
    crate::studio_tools::version_info(&state.db, &state.db.project_get(&project_id)?)
}

/// Huidige versie, laatst gepubliceerde versie en wat de volgende upload automatisch krijgt.
#[tauri::command]
pub fn version_info(state: State<'_, AppState>, project_id: String) -> AppResult<crate::studio_tools::VersionInfo> {
    let (p, _) = project_and_dir(&state, &project_id)?;
    crate::studio_tools::version_info(&state.db, &p)
}

#[tauri::command]
pub fn assets_generate_icon(state: State<'_, AppState>, project_id: String, source_path: String) -> AppResult<crate::assets::IconReport> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    crate::assets::generate_icons(&dir, std::path::Path::new(&source_path))
}

/// Splash-scherm (kleur + optioneel het app-icoon als logo) voor Android en iOS, zonder extra pakketten en zonder AI.
#[tauri::command]
pub fn assets_splash_apply(state: State<'_, AppState>, project_id: String, color: String, use_logo: bool) -> AppResult<crate::splash::SplashReport> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let icon = dir.join("store/icon-512.png");
    let logo = (use_logo && icon.is_file()).then_some(icon);
    applied_by_studio(&state, &p, "Before changing the splash screen", "assets");
    let mut rep = crate::splash::apply(&dir, &color, logo.as_deref())?;
    if use_logo && logo.is_none() {
        rep.warnings.push("no_icon".into());
    }
    Ok(rep)
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
    meta.last_published_build = current.last_published_build;
    crate::meta::save(&dir, &meta)?;
    // Eén keer invullen: het eerste e-mailadres wordt onthouden voor volgende projecten (alleen lokaal in de instellingen).
    let email = meta.listing.contact_email.trim();
    if !email.is_empty() && state.db.setting_get("contact_email")?.is_none_or(|e| e.is_empty()) {
        state.db.setting_set("contact_email", email)?;
    }
    Ok(())
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
        signing_configured: crate::signing::gradle_configured(dir),
        test_passed_fingerprint: last.strip_prefix("passed:").map(str::to_string),
        test_ever_ran: !last.is_empty(),
        test_failed_last: last.starts_with("failed:"),
        current_fingerprint: current,
        native_features: crate::testrun::native_features(&pubspec),
        missing_secrets: crate::vault::global().missing_secrets(&p.id),
        leaks: crate::leak::scan(dir, &crate::scrub::known_values()).len(),
        default_email: state.db.setting_get("contact_email").ok().flatten().is_some_and(|e| !e.is_empty()),
        purchases: purchases_facts(state, p, dir),
    }
}

/// Feiten over aankopen; `None` als de app geen producten heeft (dan verandert de checklist niet).
fn purchases_facts(state: &AppState, p: &Project, dir: &std::path::Path) -> Option<crate::checklist::PurchasesFacts> {
    use crate::purchases::StoreStatus as S;
    let pu = purchases_load(state, &p.id, dir).ok().filter(|pu| !pu.products.is_empty())?;
    let scan = crate::purchases_flow::scan_paywall(dir);
    Some(crate::checklist::PurchasesFacts {
        apple: pu.apple,
        google: pu.google,
        products: pu.products.len(),
        applied: pu.applied_version > 0,
        apple_created: pu.products.iter().filter(|x| x.status.apple >= S::Created).count(),
        google_created: pu.products.iter().filter(|x| x.status.google >= S::Created).count(),
        paywall_exists: scan.exists,
        restore_button: scan.restore_button,
        store_price: scan.store_price,
        hardcoded_price: scan.hardcoded_price,
        confirmations: pu.confirmations.clone(),
    })
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

/// "Los voor mij op": lost de ontbrekende punten op die Studio zelf kan (versie, winkelvelden uit de beschrijving, changelog).
/// `item` = één checklistpunt, leeg = alles wat oplosbaar is. Vooraf komt een momentopname; er is geen AI voor nodig.
#[tauri::command]
pub fn preflight_fix(state: State<'_, AppState>, project_id: String, item: Option<String>) -> AppResult<Vec<String>> {
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let e = evaluate_and_store(&state, &p, &dir)?;
    let todo: Vec<(String, String)> = e
        .items
        .iter()
        .filter(|i| item.as_deref().is_none_or(|k| k == i.key))
        .flat_map(|i| i.fixable.iter().map(|f| (i.key.clone(), f.clone())))
        .collect();
    if todo.is_empty() {
        return Ok(vec![]);
    }
    let _ = crate::studio_tools::create_snapshot(&state.db, &p, "Before fixing the checklist", "auto");
    let mut fixed = vec![];
    let mut meta = crate::meta::load(&dir)?;
    let mut meta_changed = false;
    for (key, what) in todo {
        match (key.as_str(), what.as_str()) {
            ("app_basics", "version") => {
                if crate::preflight::fix_version(&dir)? {
                    fixed.push(what);
                }
            }
            ("store_listing", "title" | "short_description" | "full_description" | "contact_email") => {
                let email = state.db.setting_get("contact_email")?.unwrap_or_default();
                for f in crate::preflight::fix_listing(&mut meta, &p.name, &p.description, &email) {
                    if !fixed.contains(&f) {
                        fixed.push(f);
                    }
                }
                meta_changed = true;
            }
            ("version_changelog", "changelog") => {
                let mut lines = crate::gitops::log_since(&dir, meta.last_published_commit.as_deref()).or_else(|_| crate::gitops::log_since(&dir, None)).unwrap_or_default();
                lines.retain(|l| !l.starts_with("Before ") && !l.starts_with("Start:"));
                if crate::preflight::fix_changelog(&dir, &p.name, &p.version, &lines)? {
                    fixed.push(what);
                }
            }
            _ => {}
        }
    }
    if meta_changed {
        crate::meta::save(&dir, &meta)?;
    }
    for _ in &fixed {
        let _ = state.db.ux_record(Some(&p.id), "publish", "deterministic");
    }
    evaluate_and_store(&state, &p, &dir)?;
    Ok(fixed)
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
    let has = crate::signing::has_keystore(&project_id);
    Ok(SigningStatus {
        keytool: crate::signing::find_keytool(None).is_some(),
        keystore: has,
        keystore_path: None,
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
        crate::signing::create_keystore(&keytool, &data_dir, &project_id, &crate::signing::dname(&owner, &organisation, &country))?;
        Ok("vault".to_string())
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
        // Automatisch de juiste versie en het juiste buildnummer vóór de build.
        crate::studio_tools::prepare_release(&st.db, &p)?;
        let p = st.db.project_get(&project_id)?;
        let env = crate::signing::build_env(&project_id)?;
        let package = crate::platform::read_settings(&dir).android_application_id.unwrap_or_else(|| p.bundle_id.clone());
        let publisher = PlayPublisher;
        log::info!("publishing {} to {}", p.name, publisher.id());

        job_state(&handle, &project_id, "build", "running");
        let aab = match publisher.build(&flutter, &dir, &env.vars, &mut emit_line("build")) {
            Ok(a) => a,
            Err(e) => {
                job_state(&handle, &project_id, "build", "failed");
                return Err(e);
            }
        };
        drop(env); // het ontsleutelde keystore-bestand is na de build niet meer nodig
        job_state(&handle, &project_id, "build", "done");

        job_state(&handle, &project_id, "upload", "running");
        let req = UploadRequest { project_id: &project_id, package: &package, artifact: &aab, track: &track, fastlane: &fastlane };
        if let Err(e) = publisher.upload(&req, &mut emit_line("upload")) {
            job_state(&handle, &project_id, "upload", "failed");
            return Err(e);
        }
        job_state(&handle, &project_id, "upload", "done");

        // Vastleggen wat er is gepubliceerd.
        let mut meta = crate::meta::load(&dir)?;
        meta.last_published_version = Some(p.version.clone());
        meta.last_published_build = Some(p.build_number as u32);
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
    let req = UploadRequest { project_id: "", package: "", artifact: work, track: "", fastlane: work };
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
        // Hygiëne: geheimbestanden en .env komen nooit in git.
        let _ = crate::leak::ensure_gitignore(&dir);
        crate::vault::global().migrate_legacy(&id);
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
pub fn run_metrics(state: State<'_, AppState>, project_id: String) -> AppResult<crate::db::RunsSummary> {
    state.db.runs_summary(&project_id, 200)
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


// ───────────── integraties en geheimen (fase 10) ─────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationOverview {
    manifests: Vec<crate::manifest::Manifest>,
    statuses: Vec<crate::vault::IntegrationStatus>,
}

#[tauri::command]
pub fn integration_list(state: State<'_, AppState>, project_id: String) -> AppResult<IntegrationOverview> {
    state.db.project_get(&project_id)?;
    Ok(IntegrationOverview { manifests: crate::manifest::all().to_vec(), statuses: crate::vault::global().statuses(&project_id)? })
}

/// Zet een integratie aan of uit. Bij uitzetten kiest de gebruiker: geheimen bewaren of verwijderen (projectwaarden; globale
/// waarden zoals een Apple-account horen ook bij andere projecten en blijven staan).
#[tauri::command]
pub fn integration_set_enabled(state: State<'_, AppState>, project_id: String, id: String, enabled: bool, delete_secrets: bool) -> AppResult<Vec<crate::vault::IntegrationStatus>> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let m = crate::manifest::get(&id).ok_or_else(|| AppError::NotFound(format!("integration {id}")))?;
    let v = crate::vault::global();
    if enabled {
        let _ = crate::leak::ensure_gitignore(&dir);
    } else if delete_secrets {
        v.delete_integration(&project_id, &m.id)?;
    }
    state.db.integration_set_enabled(&project_id, &id, enabled)?;
    v.statuses(&project_id)
}

#[tauri::command]
pub fn secret_field_set_text(state: State<'_, AppState>, project_id: String, integration: String, field: String, value: String) -> AppResult<()> {
    state.db.project_get(&project_id)?;
    crate::vault::global().set_text(&project_id, &integration, &field, &value)
}

#[derive(Serialize)]
pub struct FileStored {
    derived: std::collections::BTreeMap<String, String>,
}

#[tauri::command]
pub fn secret_field_set_file(state: State<'_, AppState>, project_id: String, integration: String, field: String, file_name: String, data_b64: String) -> AppResult<FileStored> {
    use base64::Engine;
    state.db.project_get(&project_id)?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data_b64.trim()).map_err(|_| AppError::Invalid("the file could not be read".into()))?;
    let derived = crate::vault::global().set_file(&project_id, &integration, &field, &file_name, &bytes)?;
    Ok(FileStored { derived })
}

#[tauri::command]
pub fn secret_field_delete(state: State<'_, AppState>, project_id: String, integration: String, field: String) -> AppResult<()> {
    state.db.project_get(&project_id)?;
    crate::vault::global().delete(&project_id, &integration, &field)
}

/// Toont een waarde aan de gebruiker zelf (na bevestiging in de UI). Alleen tekstvelden; bestanden worden nooit getoond.
/// Dit is bewust GEEN MCP-tool: Claude kan dit niet aanroepen.
#[tauri::command]
pub fn secret_field_reveal(state: State<'_, AppState>, project_id: String, integration: String, field: String) -> AppResult<String> {
    state.db.project_get(&project_id)?;
    let m = crate::manifest::get(&integration).ok_or_else(|| AppError::NotFound(integration.clone()))?;
    if m.field(&field).map(|f| f.kind) == Some(crate::manifest::FieldKind::File) {
        return Err(AppError::Invalid("files are never shown".into()));
    }
    crate::vault::global().get_text(&project_id, &integration, &field)?.ok_or_else(|| AppError::NotFound("value".into()))
}

#[tauri::command]
pub async fn secret_test(state: State<'_, AppState>, project_id: String, integration: String) -> AppResult<crate::connect::TestOutcome> {
    state.db.project_get(&project_id)?;
    let m = crate::manifest::get(&integration).ok_or_else(|| AppError::NotFound(integration.clone()))?;
    blocking(move || crate::connect::run_and_record(crate::vault::global(), &crate::connect::CurlHttp, &project_id, m)).await
}

/// Versleutelde back-up als base64 (de UI bewaart het bestand).
#[tauri::command]
pub async fn vault_export(state: State<'_, AppState>, project_id: String, passphrase: String) -> AppResult<String> {
    use base64::Engine;
    state.db.project_get(&project_id)?;
    blocking(move || Ok(base64::engine::general_purpose::STANDARD.encode(crate::vault::global().export(&project_id, &passphrase)?))).await
}

#[tauri::command]
pub async fn vault_import(state: State<'_, AppState>, project_id: String, passphrase: String, data_b64: String) -> AppResult<usize> {
    use base64::Engine;
    state.db.project_get(&project_id)?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data_b64.trim()).map_err(|_| AppError::Rejected { code: "backup_unreadable".into(), detail: None })?;
    blocking(move || crate::vault::global().import(&project_id, &passphrase, &bytes)).await
}

fn backup_path(path: &str) -> AppResult<PathBuf> {
    // Het pad komt uit het opslaan/openen-venster van het besturingssysteem; we staan alleen back-upbestanden toe.
    if !path.ends_with(".studiovault") {
        return Err(AppError::Invalid("a back-up file ends in .studiovault".into()));
    }
    Ok(PathBuf::from(path))
}

/// Schrijft de versleutelde back-up naar een bestand dat de gebruiker in het opslaan-venster koos.
#[tauri::command]
pub async fn vault_export_file(state: State<'_, AppState>, project_id: String, passphrase: String, path: String) -> AppResult<()> {
    state.db.project_get(&project_id)?;
    let path = backup_path(&path)?;
    blocking(move || {
        let bytes = crate::vault::global().export(&project_id, &passphrase)?;
        std::fs::write(path, bytes)?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn vault_import_file(state: State<'_, AppState>, project_id: String, passphrase: String, path: String) -> AppResult<usize> {
    state.db.project_get(&project_id)?;
    let path = backup_path(&path)?;
    blocking(move || crate::vault::global().import(&project_id, &passphrase, &std::fs::read(path)?)).await
}

#[tauri::command]
pub fn leaks_scan(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<crate::leak::Leak>> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    Ok(crate::leak::scan(&dir, &crate::scrub::known_values()))
}

/// Verplaatst een geheim bestand uit het project naar de kluis en verwijdert het uit het project (de UI vraagt bevestiging).
#[tauri::command]
pub fn leak_move_to_vault(state: State<'_, AppState>, project_id: String, file: String) -> AppResult<(String, String)> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let leaks = crate::leak::scan(&dir, &crate::scrub::known_values());
    let leak = leaks.iter().find(|l| l.file == file && l.movable).ok_or_else(|| AppError::Invalid("this file cannot be moved automatically".into()))?;
    let name = file.rsplit('/').next().unwrap_or(&file).to_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_string()).unwrap_or_default();
    let (integration, field) = match ext.as_str() {
        "p8" => ("apple", "private_key"),
        "jks" | "keystore" | "p12" | "pfx" => ("google_play", "keystore"),
        _ => ("google_play", "service_account"),
    };
    let v = crate::vault::global();
    if v.has(&project_id, integration, field) {
        return Err(AppError::Rejected { code: "vault_has_value".into(), detail: Some(format!("{integration}.{field}")) });
    }
    let bytes = std::fs::read(dir.join(&leak.file))?;
    v.set_file(&project_id, integration, field, &name, &bytes)?;
    std::fs::remove_file(dir.join(&leak.file))?;
    Ok((integration.into(), field.into()))
}

/// Opdracht aan Claude om een vastgelegd geheim uit de code te halen. Bevat bewust geen waarde.
#[tauri::command]
pub fn leak_fix_prompt(file: String, line: u32, kind: String) -> AppResult<String> {
    let what = match kind.as_str() {
        "service_role_key" => "a Supabase secret (service_role) key",
        "private_key" => "a private key",
        _ => "a secret key or password",
    };
    Ok(format!(
        "Studio found {what} written in {file}{} . Remove it from the code. If the app really needs that capability, \
         move the work to a place the app does not ship (never into the app), or read a PUBLIC value from a dart-define or the Studio vault instead. \
         Do not print the value, do not copy it anywhere else, and tell the user in plain language that the key must be replaced with a new one because it was exposed.",
        if line > 0 { format!(" (around line {line})") } else { String::new() }
    ))
}

// ───────────── aankopen en abonnementen ─────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchasesView {
    pub purchases: crate::purchases::Purchases,
    pub plan: crate::purchases::ApplyPlan,
    pub summary: Vec<(String, usize)>,
    pub missing: Vec<crate::purchases::Missing>,
    pub problems: Vec<crate::purchases::Problem>,
    pub tasks: Vec<crate::purchases_flow::StoreTask>,
    pub bundle_id: String,
    pub applying: bool,
}

fn purchases_load(state: &AppState, project_id: &str, dir: &std::path::Path) -> AppResult<crate::purchases::Purchases> {
    // Het bestand in het project is de bron; de kopie in SQLite vangt op dat het bestand ontbreekt (bv. niet meegekomen met git).
    if dir.join("studio/purchases.json").is_file() {
        return crate::purchases::load(dir);
    }
    Ok(state.db.purchases_load(project_id)?.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

fn purchases_store(state: &AppState, project_id: &str, dir: &std::path::Path, p: &crate::purchases::Purchases) -> AppResult<()> {
    crate::purchases::save(dir, p)?;
    state.db.purchases_store(project_id, &serde_json::to_string(p).unwrap_or_default())
}

fn purchases_view(state: &AppState, project_id: &str) -> AppResult<PurchasesView> {
    let (p, dir) = project_and_dir(state, project_id)?;
    let pu = purchases_load(state, project_id, &dir)?;
    let vault = crate::vault::global();
    let pid = project_id.to_string();
    let has = move |i: &str, f: &str| vault.has(&pid, i, f);
    let plan = crate::purchases::plan(&pu);
    Ok(PurchasesView {
        summary: crate::purchases_flow::describe(&plan).into_iter().map(|(k, n)| (k.to_string(), n)).collect(),
        missing: crate::purchases::missing(&pu, pu.apple, pu.google, &has),
        problems: crate::purchases::validate(&pu),
        tasks: crate::purchases_flow::store_tasks(&pu, &p.bundle_id, pu.apple, pu.google),
        bundle_id: p.bundle_id,
        applying: state.chat.is_running(project_id) && state.purchases_applying.lock().unwrap_or_else(|e| e.into_inner()).contains(project_id),
        plan,
        purchases: pu,
    })
}

#[tauri::command]
pub fn purchases_get(state: State<'_, AppState>, project_id: String) -> AppResult<PurchasesView> {
    purchases_view(&state, &project_id)
}

/// Slaat de bewerkbare delen op. Status per winkel, de toegepaste staat en de goedkeuring van het gating-plan komen nooit van de UI.
#[tauri::command]
pub fn purchases_save(state: State<'_, AppState>, project_id: String, purchases: crate::purchases::Purchases) -> AppResult<PurchasesView> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let disk = purchases_load(&state, &project_id, &dir)?;
    let mut next = purchases;
    next.applied = disk.applied.clone();
    next.applied_version = disk.applied_version;
    next.confirmations = disk.confirmations.clone();
    for pr in &mut next.products {
        match disk.products.iter().find(|o| o.id == pr.id) {
            Some(old) => pr.status = old.status.clone(),
            None => pr.status = Default::default(),
        }
        // Zodra een product in de app of een winkel staat, ligt de soort en het ID vast.
        if let Some(old) = disk.products.iter().find(|o| o.id == pr.id && o.locked()) {
            pr.kind = old.kind;
        }
    }
    next.gating.approved = disk.gating.approved && disk.gating.points == next.gating.points;
    purchases_store(&state, &project_id, &dir, &next)?;
    purchases_view(&state, &project_id)
}

/// De gebruiker heeft het gating-plan in gewone taal gezien en keurt het goed.
#[tauri::command]
pub fn purchases_approve_gating(state: State<'_, AppState>, project_id: String) -> AppResult<PurchasesView> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let mut pu = purchases_load(&state, &project_id, &dir)?;
    pu.gating.approved = true;
    purchases_store(&state, &project_id, &dir, &pu)?;
    purchases_view(&state, &project_id)
}

/// Handmatig bevestigde stap (belasting- en bankgegevens, sandbox-test enz.).
#[tauri::command]
pub fn purchases_confirm(state: State<'_, AppState>, project_id: String, key: String, value: bool) -> AppResult<PurchasesView> {
    if !["paid_apps_agreement", "tax_bank", "merchant_account", "sandbox_tested", "apple_submitted"].contains(&key.as_str()) {
        return Err(AppError::Invalid("unknown confirmation".into()));
    }
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let mut pu = purchases_load(&state, &project_id, &dir)?;
    pu.confirmations.insert(key, value);
    purchases_store(&state, &project_id, &dir, &pu)?;
    purchases_view(&state, &project_id)
}

/// Zet de status van een product in één winkel verder (aangemaakt, in review, live). Alleen vooruit en pas na "in de app".
#[tauri::command]
pub fn purchases_set_status(state: State<'_, AppState>, project_id: String, product: String, store: String, status: crate::purchases::StoreStatus) -> AppResult<PurchasesView> {
    use crate::purchases::StoreStatus as S;
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let mut pu = purchases_load(&state, &project_id, &dir)?;
    let pr = pu.products.iter_mut().find(|p| p.id == product).ok_or_else(|| AppError::NotFound(product.clone()))?;
    let slot = match store.as_str() {
        "apple" => &mut pr.status.apple,
        "google" => &mut pr.status.google,
        _ => return Err(AppError::Invalid("unknown store".into())),
    };
    if *slot < S::InApp || status <= *slot || status < S::Created {
        return Err(AppError::Invalid("a status can only move forward, after the product is in the app".into()));
    }
    *slot = status;
    // De toegepaste kopie meenemen, anders ziet "Update app" dit als een verschil.
    if let Some(a) = pu.applied.as_mut() {
        if let Some(ap) = a.products.iter_mut().find(|p| p.id == product) {
            ap.status = pr.status.clone();
        }
    }
    purchases_store(&state, &project_id, &dir, &pu)?;
    purchases_view(&state, &project_id)
}

/// De vraag voor Claude om een gating-plan te maken (alleen lezen). De UI stuurt hem via de chat en verwerkt het antwoord.
#[tauri::command]
pub fn purchases_gating_prompt(state: State<'_, AppState>, project_id: String) -> AppResult<String> {
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let pu = purchases_load(&state, &project_id, &dir)?;
    let mut ents: Vec<String> = pu.products.iter().map(|p| p.entitlement.clone()).filter(|e| !e.is_empty()).collect();
    ents.sort();
    ents.dedup();
    let screens: Vec<(String, String)> = crate::studio_tools::list_screens(&dir)?.into_iter().map(|s| (s.class, s.file)).collect();
    Ok(crate::purchases_flow::gating_prompt(&pu.paywall_description, &ents, &screens))
}

/// Verwerkt het antwoord van Claude op de gating-vraag (zet het plan klaar, nog niet goedgekeurd).
#[tauri::command]
pub fn purchases_set_gating_from_answer(state: State<'_, AppState>, project_id: String, answer: String) -> AppResult<PurchasesView> {
    let points = crate::purchases_flow::parse_gating_plan(&answer).ok_or_else(|| AppError::Invalid("no usable gating plan in the answer".into()))?;
    let (_, dir) = project_and_dir(&state, &project_id)?;
    let mut pu = purchases_load(&state, &project_id, &dir)?;
    pu.gating = crate::purchases::GatingPlan { points, approved: false };
    purchases_store(&state, &project_id, &dir, &pu)?;
    purchases_view(&state, &project_id)
}

/// "Add to app" / "Update app": controles, momentopname, code door Claude (alleen via deze flow), en daarna de toegepaste staat vastleggen.
#[tauri::command]
pub fn purchases_apply(app: AppHandle, state: State<'_, AppState>, project_id: String, language: String, confirmed: bool) -> AppResult<()> {
    if !confirmed {
        return Err(AppError::Invalid("applying purchases needs confirmation".into()));
    }
    let (p, dir) = project_and_dir(&state, &project_id)?;
    let view = purchases_view(&state, &project_id)?;
    if let Some(m) = view.missing.first() {
        return Err(AppError::Rejected { code: format!("purchases_{}", m.code), detail: Some(m.next.clone()) });
    }
    if view.plan.is_empty() && !view.plan.first_time {
        return Err(AppError::Invalid("there is nothing to update".into()));
    }
    if state.chat.is_running(&project_id) {
        return Err(AppError::Invalid("wait until Claude has finished".into()));
    }
    let pu = view.purchases.clone();
    let legal = crate::meta::load(&dir).map(|m| m.legal).unwrap_or_default();
    let prompt = crate::purchases_flow::codegen_prompt(&pu, &view.plan, &p.bundle_id, &legal.privacy_url, &legal.terms_url);
    let emit = {
        let (app, id) = (app.clone(), project_id.clone());
        move |step: &str, detail: Option<String>| {
            let _ = app.emit("purchases-progress", serde_json::json!({ "projectId": id, "step": step, "detail": detail }));
        }
    };
    emit("snapshot", None);
    let _ = crate::studio_tools::create_snapshot(&state.db, &p, "Before updating purchases", "auto");
    state.chat_ctx.scopes.allow_purchases(&project_id);
    state.purchases_applying.lock().unwrap_or_else(|e| e.into_inner()).insert(project_id.clone());
    emit("code", None);
    if let Err(e) = state.chat.send(state.chat_ctx.clone(), project_id.clone(), prompt, None, language, crate::modelsel::Purpose::Work) {
        state.purchases_applying.lock().unwrap_or_else(|e| e.into_inner()).remove(&project_id);
        let _ = state.chat_ctx.scopes.take_purchases(&project_id);
        return Err(e);
    }
    // Wachten tot de run klaar is en kijken of hij geslaagd is (de kwaliteitscontrole draait en draait zo nodig terug).
    let handle = app.clone();
    std::thread::spawn(move || {
        let st = handle.state::<AppState>();
        while st.chat.is_running(&project_id) {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let ok = st.db.last_run(&project_id).ok().flatten().is_some_and(|r| r.ok && !r.rolled_back);
        st.purchases_applying.lock().unwrap_or_else(|e| e.into_inner()).remove(&project_id);
        let done = (|| -> AppResult<()> {
            let (p, dir) = project_and_dir(&st, &project_id)?;
            let mut pu = purchases_load(&st, &project_id, &dir)?;
            crate::purchases_flow::finalize(&mut pu);
            purchases_store(&st, &project_id, &dir, &pu)?;
            let _ = crate::studio_tools::create_snapshot(&st.db, &p, "Purchases added to the app", "auto");
            Ok(())
        })();
        match (ok, done) {
            (true, Ok(())) => emit("done", None),
            (false, _) => emit("failed", Some("rolled_back".into())),
            (true, Err(e)) => emit("failed", Some(e.to_string())),
        }
    });
    Ok(())
}

/// Voorstel voor een product-ID uit naam, soort en periode (zodat ze overal op dezelfde manier gemaakt worden).
#[tauri::command]
pub fn purchases_suggest_id(name: String, kind: crate::purchases::Kind, period: Option<crate::purchases::Period>) -> String {
    crate::purchases::suggest_id(&name, kind, period)
}

// ───────────── AI-verbindingen (Claude Code, Codex, Gemini) ─────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnginesView {
    pub engines: Vec<EngineRow>,
    pub order: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRow {
    #[serde(flatten)]
    pub status: crate::engine::registry::EngineStatus,
    pub runs: i64,
    pub clean: i64,
    pub rolled_back: i64,
}

fn engines_view(state: &AppState) -> EnginesView {
    use crate::engine::registry as reg;
    let now = reg::now_unix();
    let stats = state.db.runs_by_engine().unwrap_or_default();
    let engines = reg::specs()
        .iter()
        .map(|s| {
            let mut st = reg::detect(s);
            st.limited_until = reg::limited_until(&state.db, &s.id, now);
            let (runs, clean, rolled_back) = stats.iter().find(|r| r.0 == s.id).map(|r| (r.1, r.2, r.3)).unwrap_or_default();
            EngineRow { status: st, runs, clean, rolled_back }
        })
        .collect();
    EnginesView { engines, order: reg::order(&state.db) }
}

#[tauri::command]
pub async fn engines_list(app: AppHandle) -> AppResult<EnginesView> {
    blocking(move || Ok(engines_view(&app.state::<AppState>()))).await
}

/// Volgorde van voorkeur: de eerste die klaar is en niet op "limiet" staat krijgt de volgende opdracht.
#[tauri::command]
pub async fn engines_set_order(app: AppHandle, order: Vec<String>) -> AppResult<EnginesView> {
    blocking(move || {
        let st = app.state::<AppState>();
        crate::engine::registry::set_order(&st.db, &order)?;
        Ok(engines_view(&st))
    })
    .await
}

/// "Nu opnieuw proberen": haalt de limiet-markering weg.
#[tauri::command]
pub async fn engine_clear_limit(app: AppHandle, id: String) -> AppResult<EnginesView> {
    blocking(move || {
        let st = app.state::<AppState>();
        crate::engine::registry::spec(&id).ok_or_else(|| AppError::NotFound(id.clone()))?;
        crate::engine::registry::clear_limit(&st.db, &id);
        Ok(engines_view(&st))
    })
    .await
}

/// Opent de inlog van een AI in een terminal; de gebruiker logt zelf in (Studio ziet en bewaart geen tokens).
#[tauri::command]
pub fn engine_open_login(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<String> {
    let spec = crate::engine::registry::spec(&id).ok_or_else(|| AppError::NotFound(id.clone()))?;
    let program = proc::find(&spec.binary).ok_or_else(|| AppError::ToolMissing(spec.binary.clone()))?;
    state.pty.spawn(&app, &program, &spec.login_args, None)
}
