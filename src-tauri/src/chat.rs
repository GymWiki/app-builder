//! Eén chatbeurt van begin tot eind: sessie, opslag, MCP-config, engine, automatische momentopname.

use crate::db::{Db, RunRecord};
use crate::gitops;
use crate::index::CodeIndex;
use crate::modelsel::{self, EngineConfig, Purpose};
use crate::proc;
use crate::quality;
use crate::scope::{self, RunScope, ScopeRegistry};
use crate::engine::{AiEngine, ChatEvent, RunHandle, TurnRequest};
use crate::error::{AppError, AppResult};
use crate::mcp::{config_json_with, McpInfo};
use crate::studio_tools;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub struct ChatManager {
    engine: Arc<dyn AiEngine>,
    /// Alle bekende engines (Claude Code, Codex, Gemini) op id; per opdracht wordt er één gekozen.
    engines: HashMap<&'static str, Arc<dyn AiEngine>>,
    /// Alleen voor tests: welke engines als "klaar" gelden (anders wordt het echt gecontroleerd).
    ready_override: Mutex<Option<Vec<String>>>,
    running: Mutex<HashMap<String, Arc<RunHandle>>>,
    /// Modelkeuze (fase 11): tiers, klassen en gewichten uit `engines/<engine>.json`.
    config: Mutex<Arc<EngineConfig>>,
}

/// Wat per beurt verschilt: model, alleen-plannen en of de sessie van het scherm wordt hervat.
#[derive(Clone, Default)]
struct TurnOpts {
    model: Option<String>,
    plan_only: bool,
    fresh: bool,
}

/// Voor de evaluatieset: onderdelen van fase 6 t/m 8 uitzetten om hun effect los van de modelkeuze te kunnen meten.
#[derive(Clone, Copy, Default)]
pub struct Ablation {
    /// Geen werkgebied: het hele project, zonder scope-regel in de prompt en zonder bewaking.
    pub no_router: bool,
    /// Geen projectprofiel: geen Dart-MCP en geen schermindex.
    pub no_profile: bool,
}

pub struct ChatCtx {
    pub db: Arc<Db>,
    pub mcp: McpInfo,
    pub config_dir: PathBuf,
    pub emit: Emit,
    pub scopes: Arc<ScopeRegistry>,
    /// Controle na afloop aan/uit (tests) en een vaste Flutter (tests); anders wordt `flutter` in PATH gezocht.
    pub checks: bool,
    pub flutter: Option<PathBuf>,
    pub ablation: Ablation,
}

fn kind_of(e: &ChatEvent) -> &'static str {
    match e {
        ChatEvent::User { .. } => "user",
        ChatEvent::Init { .. } => "init",
        ChatEvent::TextDelta { .. } => "text_delta",
        ChatEvent::Text { .. } => "text",
        ChatEvent::ToolUse { .. } => "tool_use",
        ChatEvent::ToolResult { .. } => "tool_result",
        ChatEvent::Snapshot { .. } => "snapshot",
        ChatEvent::Status { .. } => "status",
        ChatEvent::Done { .. } => "done",
        ChatEvent::Error { .. } => "error",
    }
}

fn error_code(e: &AppError) -> &'static str {
    match e {
        AppError::ToolMissing(_) => "tool_missing",
        AppError::CommandFailed(m) if m.contains("outdated") => "outdated",
        _ => "failed",
    }
}

static DART: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Extra MCP-servers voor deze beurt: de eigen backend van de gebruiker (Supabase/Firebase) en de Dart/Flutter-MCP
/// (`dart mcp-server`, alleen met Dart >= 3.9). Valt de Dart-MCP weg, dan werkt alles nog via Studio zelf.
pub fn mcp_extras(project_id: &str, dir: &std::path::Path) -> Value {
    let mut extra = crate::integrations::extra_servers(project_id, dir);
    let dart = DART.get_or_init(crate::profile::dart_version);
    if dart.as_deref().is_some_and(crate::profile::dart_mcp_supported) {
        extra["dart"] = crate::profile::dart_mcp_server();
    }
    extra
}

impl ChatManager {
    pub fn new(engine: Arc<dyn AiEngine>) -> Self {
        let mut engines: HashMap<&'static str, Arc<dyn AiEngine>> = HashMap::new();
        engines.insert("codex", Arc::new(crate::engine::codex::CodexEngine::new()));
        engines.insert("gemini", Arc::new(crate::engine::gemini::GeminiEngine::new()));
        engines.insert(engine.id(), engine.clone());
        Self { engine, engines, ready_override: Mutex::new(None), running: Mutex::new(HashMap::new()), config: Mutex::new(Arc::new(EngineConfig::embedded())) }
    }

    /// Kiest de AI voor een nieuwe opdracht (volgorde in de instellingen, klaar en niet op "limiet"). Alleen aan het begin van een
    /// opdracht: een lopende opdracht wisselt nooit van AI. Staat er niets ingesteld, dan is het gewoon de standaard-engine.
    /// Staat deze AI klaar (geïnstalleerd en niet uitgelogd)? De standaard-engine geldt altijd als klaar: die meldt zelf wat er mis is.
    fn engine_ready(&self, id: &str) -> bool {
        if let Some(f) = self.ready_override.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return f.iter().any(|x| x == id);
        }
        id == self.engine.id() || crate::engine::registry::spec(id).map(|s| crate::engine::registry::detect(&s).ready()).unwrap_or(false)
    }

    fn engine_for(&self, ctx: &ChatCtx) -> Arc<dyn AiEngine> {
        use crate::engine::registry as reg;
        let order = reg::order(&ctx.db);
        if order.len() == 1 && order[0] == self.engine.id() {
            return self.engine.clone();
        }
        let now = reg::now_unix();
        let ready = |id: &str| self.engine_ready(id);
        let limited = |id: &str| reg::limited_until(&ctx.db, id, now).is_some();
        let id = reg::pick_engine(&order, &ready, &limited);
        self.engines.get(id.as_str()).cloned().unwrap_or_else(|| self.engine.clone())
    }

    /// Nieuwe modelconfiguratie (uit de app-datamap); geldt vanaf de volgende opdracht.
    pub fn set_config(&self, c: EngineConfig) {
        *self.config.lock().unwrap_or_else(|e| e.into_inner()) = Arc::new(c);
    }

    #[cfg(test)]
    pub fn with_engine(mut self, e: Arc<dyn AiEngine>, ready: &[&str]) -> Self {
        self.engines.insert(e.id(), e);
        *self.ready_override.lock().unwrap_or_else(|x| x.into_inner()) = Some(ready.iter().map(|s| s.to_string()).collect());
        self
    }

    fn config(&self) -> Arc<EngineConfig> {
        self.config.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_running(&self, project_id: &str) -> bool {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).contains_key(project_id)
    }

    pub fn cancel(&self, project_id: &str) {
        if let Some(h) = self.running.lock().unwrap_or_else(|e| e.into_inner()).get(project_id) {
            h.cancel();
        }
    }

    /// Start een beurt op de achtergrond. Per project loopt er maximaal één tegelijk.
    pub fn send(self: &Arc<Self>, ctx: Arc<ChatCtx>, project_id: String, text: String, context: Option<String>, language: String, purpose: Purpose) -> AppResult<()> {
        if text.trim().is_empty() {
            return Err(AppError::Invalid("message is empty".into()));
        }
        let handle = Arc::new(RunHandle::default());
        {
            let mut r = self.running.lock().unwrap_or_else(|e| e.into_inner());
            if r.contains_key(&project_id) {
                return Err(AppError::Invalid("Claude is still working on the previous message".into()));
            }
            r.insert(project_id.clone(), handle.clone());
        }
        let this = self.clone();
        std::thread::spawn(move || {
            this.run_flow_for(&ctx, &project_id, &text, context, &language, &handle, purpose);
            this.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&project_id);
            (ctx.emit)("chat-finished", json!({ "projectId": project_id }));
        });
        Ok(())
    }

    /// De eigenlijke beurt (synchroon; apart zodat tests hem direct kunnen aanroepen). Volgorde: werkgebied kiezen,
    /// momentopname, run, controle met gerichte reparatie, eventueel terugdraaien, metingen.
    #[cfg(test)]
    pub fn run_flow(&self, ctx: &ChatCtx, project_id: &str, text: &str, context: Option<String>, language: &str, handle: &RunHandle) {
        self.run_flow_for(ctx, project_id, text, context, language, handle, Purpose::Work)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_flow_for(&self, ctx: &ChatCtx, project_id: &str, text: &str, context: Option<String>, language: &str, handle: &RunHandle, purpose: Purpose) {
        let started = Instant::now();
        let cfg_models = self.config();
        let engine = self.engine_for(ctx);
        // Wat de gebruiker intikt of selecteert kan een geheim bevatten: dat gaat nooit naar het model (ook niet naar de
        // goedkope classificatievraag) en komt niet in de opgeslagen berichten.
        let text = &crate::scrub::scrub(text);
        let context = context.map(|c| crate::scrub::scrub(&c));
        let emit_event = |session: Option<&str>, e: &ChatEvent| {
            if let (Some(s), false) = (session, matches!(e, ChatEvent::TextDelta { .. })) {
                let _ = ctx.db.message_insert(s, kind_of(e), &serde_json::to_string(e).unwrap_or_default());
            }
            (ctx.emit)("chat-event", json!({ "projectId": project_id, "event": e }));
        };
        // Alles wat in de chat, de opslag of naar het model gaat, loopt eerst door de scrubber.
        let emit_event = |session: Option<&str>, e: &ChatEvent| {
            let mut v = serde_json::to_value(e).unwrap_or_default();
            crate::scrub::scrub_value(&mut v);
            match serde_json::from_value::<ChatEvent>(v) {
                Ok(clean) => emit_event(session, &clean),
                Err(_) => emit_event(session, e),
            }
        };

        let project = match ctx.db.project_get(project_id) {
            Ok(p) => p,
            Err(e) => return emit_event(None, &ChatEvent::Error { code: error_code(&e).into(), message: crate::logging::redact(&e.to_string()) }),
        };
        let dir = PathBuf::from(&project.path);

        // 1. Werkgebied kiezen (de gebruiker merkt hier niets van).
        let _ = ctx.db.session_for_scope(project_id, "", engine.id()); // begin van het gesprek
        let index = CodeIndex::build(&dir).unwrap_or_default();
        let current_key = format!("scope:{project_id}:screen");
        let current = ctx.db.setting_get(&current_key).ok().flatten();
        let mut ai_questions = 0i64;
        let purchases_flow = ctx.scopes.take_purchases(project_id);
        // 1a. Over aankopen of abonnementen begint Claude niet zelf aan code: daar is de pagina Aankopen voor. Studio zet een concept klaar,
        // opent de pagina en antwoordt in één vriendelijke zin; er draait geen model.
        if !purchases_flow && crate::purchases::intent(text) {
            let sid = match ctx.db.session_for_scope(project_id, "project", engine.id()) {
                Ok(s) => s.id,
                Err(e) => return emit_event(None, &ChatEvent::Error { code: error_code(&e).into(), message: crate::logging::redact(&e.to_string()) }),
            };
            emit_event(Some(&sid), &ChatEvent::User { text: text.to_string(), context: context.clone() });
            let _ = ctx.db.session_set_title_if_empty(&sid, &text.chars().take(60).collect::<String>());
            let mut pu = crate::purchases::load(&dir).unwrap_or_default();
            if pu.products.is_empty() {
                let d = crate::purchases::draft_from_text(text);
                (pu.products, pu.paywall, pu.provider, pu.paywall_description) = (d.products, d.paywall, d.provider, d.paywall_description);
                let _ = crate::purchases::save(&dir, &pu);
                let _ = ctx.db.purchases_store(project_id, &serde_json::to_string(&pu).unwrap_or_default());
            }
            (ctx.emit)("open-purchases", json!({ "projectId": project_id }));
            let reply = if language.starts_with("nl") {
                "Voor aankopen heb ik een eigen pagina; ik heb alvast een concept klaargezet."
            } else {
                "For purchases there is a dedicated page; I have already set up a draft for you."
            };
            emit_event(Some(&sid), &ChatEvent::Text { message_id: "purchases".into(), text: reply.into() });
            emit_event(Some(&sid), &ChatEvent::Done { ok: true, text: reply.into(), duration_ms: Some(0), code: None, cost_usd: None, turns: None, tokens: None, usage: None });
            return;
        }

        // Kleine hulptaken (zoals deze classificatie) altijd met het lichtste model; de naam komt uit de configuratie.
        let helper_alias = cfg_models.tier_index(&cfg_models.helper_tier).and_then(|t| cfg_models.resolve(t, &|m| engine.model_available(m)));
        let decision = if ctx.ablation.no_router || purchases_flow {
            // Meting zonder scope-router: het hele project, geen classificatievraag.
            scope::Decision { level: scope::Level::Project, screen: None, why: scope::Why::Fallback }
        } else {
            scope::decide(&index, text, context.as_deref(), current.as_deref(), &mut |q| {
                ai_questions += 1;
                engine.classify(q, helper_alias.as_deref())
            })
        };
        if let Some(s) = &decision.screen {
            let _ = ctx.db.setting_set(&current_key, s);
        }
        let mut scope_state = RunScope::new(&index, &decision);
        scope_state.purchases_ok = purchases_flow;
        // Een vak dat de gebruiker op de preview tekende: het bereik waarbinnen Claude hoort te blijven (controle achteraf).
        let selection_ranges = ctx.scopes.take_selection(project_id);
        let key = decision.screen.clone().unwrap_or_else(|| decision.level.as_str().to_string());
        let session = match ctx.db.session_for_scope(project_id, &key, engine.id()) {
            Ok(s) => s,
            Err(e) => return emit_event(None, &ChatEvent::Error { code: error_code(&e).into(), message: crate::logging::redact(&e.to_string()) }),
        };
        let sid = session.id.clone();

        // 2. Momentopname van vóór de run (daar draaien we naar terug als de controle mislukt) en een verse schermindex.
        let snap_label = format!("Before Claude: {}", text.chars().take(40).collect::<String>().replace('\n', " "));
        let pre = studio_tools::create_snapshot(&ctx.db, &project, &snap_label, "auto");
        // Geblokkeerd door de lekcontrole: dan is er geen veilig terugdraaipunt, dus geen controle met terugdraaien.
        let pre_blocked = pre.as_ref().map(|v| v["blocked"] == "leak").unwrap_or(false);
        let base = gitops::head(&dir).ok().filter(|_| !pre_blocked);
        if !ctx.ablation.no_profile {
            let _ = std::fs::create_dir_all(dir.join(".studio")).and_then(|_| std::fs::write(dir.join(".studio/screens.md"), index.render_markdown()));
        }

        // 3. MCP-config en per-run instellingen.
        let files = || -> AppResult<(PathBuf, Option<PathBuf>)> {
            std::fs::create_dir_all(&ctx.config_dir)?;
            let cfg = ctx.config_dir.join(format!("{project_id}.json"));
            let extra = if ctx.ablation.no_profile { crate::integrations::extra_servers(project_id, &dir) } else { mcp_extras(project_id, &dir) };
            std::fs::write(&cfg, serde_json::to_string_pretty(&config_json_with(&ctx.mcp, project_id, &extra))?)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o600))?;
            }
            let settings = match scope::settings_json(decision.level) {
                Some(v) => {
                    let f = ctx.config_dir.join(format!("{project_id}.settings.json"));
                    std::fs::write(&f, serde_json::to_string(&v)?)?;
                    Some(f)
                }
                None => None,
            };
            Ok((cfg, settings))
        };
        let (cfg, settings) = match files() {
            Ok(x) => x,
            Err(e) => return emit_event(None, &ChatEvent::Error { code: error_code(&e).into(), message: crate::logging::redact(&e.to_string()) }),
        };
        ctx.scopes.set(project_id, scope_state);

        emit_event(Some(&sid), &ChatEvent::User { text: text.to_string(), context: context.clone() });
        let _ = ctx.db.session_set_title_if_empty(&sid, &text.chars().take(60).collect::<String>());
        let why = serde_json::to_value(decision.why).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        emit_event(Some(&sid), &ChatEvent::Status { code: "scope".into(), detail: Some(format!("{}|{}", decision.level.as_str(), decision.screen.clone().unwrap_or_default())) });

        // 1b. Modelkeuze: moeilijkheidsklasse uit goedkope signalen, daarna het goedkoopste model dat die klasse aankan.
        let files_in_scope = match (decision.level, decision.screen.as_deref().and_then(|n| index.screen(n))) {
            (scope::Level::Small, _) => 1,
            (scope::Level::Screen, Some(e)) => e.own.len(),
            (scope::Level::Shared, e) => e.map(|e| e.own.len()).unwrap_or(0) + index.files.keys().filter(|f| f.starts_with("lib/core/")).count(),
            (scope::Level::Project, _) => index.files.len(),
            _ => index.files.len(),
        };
        let previous_failed = decision.screen.as_deref().map(|s| ctx.db.last_run_failed(project_id, s).unwrap_or(false)).unwrap_or(false);
        let has_skill = |n: &str| dir.join(".claude/skills").join(n).is_dir();
        let assessment = modelsel::assess(&cfg_models, &modelsel::AssessInput { level: decision.level.as_str(), text, files: files_in_scope, previous_failed, has_skill: &has_skill });
        let history: Vec<modelsel::HistoryRow> = ctx
            .db
            .class_history(assessment.class.as_str(), (cfg_models.downgrade.window.max(cfg_models.downgrade.per_class.values().map(|r| r.window).max().unwrap_or(0)) * 2) as i64)
            .unwrap_or_default()
            .into_iter()
            .map(|(tier, clean)| modelsel::HistoryRow { tier, clean })
            .collect();
        // Een handmatige keuze telt alleen in de ontwikkelaarsmodus.
        let dev = ctx.db.setting_get("dev_mode").ok().flatten().as_deref() == Some("true");
        let override_value = dev.then(|| ctx.db.setting_get("model_override").ok().flatten()).flatten().filter(|v| !v.is_empty() && v != "auto").filter(|_| engine.id() == crate::engine::registry::DEFAULT_ENGINE);
        let choice = modelsel::choose(&cfg_models, &assessment, purpose, &history, override_value.as_deref());
        let avail = |m: &str| engine.model_available(m);
        let model = choice.explicit_model.clone().or_else(|| cfg_models.resolve(choice.tier, &avail));
        let exec_model = model.clone();
        emit_event(Some(&sid), &ChatEvent::Status { code: "model".into(), detail: Some(format!("{}|{}|{}|{}|{}", assessment.class.as_str(), choice.tier_id, model.clone().unwrap_or_default(), choice.reason, assessment.reasons.join("; "))) });
        // Escalatie bij falen: één niveau zwaarder, maximaal één keer per taak.
        let escalated = std::cell::Cell::new(false);
        let esc_model = if cfg_models.escalation.max_per_task > 0 { cfg_models.escalate(choice.tier).and_then(|t| cfg_models.resolve(t, &avail)) } else { None };

        let scope_text = ctx.scopes.with(project_id, |s| s.describe()).unwrap_or_default();
        let prompt = match &context {
            _ if ctx.ablation.no_router => text.to_string(),
            Some(c) if !c.trim().is_empty() => format!("[Studio scope: {scope_text}]\n[Studio context: {c}]\n\n{text}"),
            _ => format!("[Studio scope: {scope_text}]\n\n{text}"),
        };
        // Overdracht: werkte een andere AI eerder aan dit project, dan krijgt deze een korte, neutrale notitie (de rest staat in STUDIO.md,
        // de schermindex en de momentopnames in het project zelf).
        let engine_last_key = format!("engine_last:{project_id}");
        let previous_engine = ctx.db.setting_get(&engine_last_key).ok().flatten().filter(|p| !p.is_empty() && p != engine.id());
        let prompt = match &previous_engine {
            Some(from) => {
                let recent = ctx.db.recent_user_texts(project_id, 4).unwrap_or_default();
                let note = crate::engine::registry::handover_note(from, engine.id(), gitops::last_commit_subject(&dir).ok().as_deref(), &recent);
                emit_event(Some(&sid), &ChatEvent::Status { code: "handover".into(), detail: Some(format!("{from}|{}", engine.id())) });
                format!("[Studio handover: {note}]\n\n{prompt}")
            }
            None => prompt,
        };
        let _ = ctx.db.setting_set(&engine_last_key, engine.id());
        let limit_hit = std::cell::Cell::new(false);

        let mut stats = TurnStats::default();
        let mut resume = session.engine_session_id.clone();
        let mut turn = |prompt: String, opts: &TurnOpts, stats: &mut TurnStats| -> (bool, String) {
            let req = TurnRequest {
                project_dir: dir.clone(),
                prompt: crate::scrub::scrub(&prompt),
                resume: if opts.fresh { None } else { resume.clone() },
                mcp_config: cfg.clone(),
                language: language.to_string(),
                settings: settings.clone(),
                model: opts.model.clone(),
                plan_only: opts.plan_only,
            };
            let mut last_ok = None;
            let mut last_text = String::new();
            let outcome = engine.run_turn(&req, handle, &mut |e| {
                if let ChatEvent::Init { mcp, .. } = &e {
                    for (name, status) in mcp.iter().filter(|(n, s)| n == "dart" && s != "connected" && s != "pending") {
                        log::warn!("MCP server {name} is {status}; continuing without it");
                    }
                }
                if let ChatEvent::Text { text, .. } = &e {
                    last_text = text.clone();
                }
                if matches!(&e, ChatEvent::Done { ok: false, code: Some(c), .. } | ChatEvent::Error { code: c, .. } if c == "limit") {
                    limit_hit.set(true);
                }
                if let ChatEvent::Done { ok, cost_usd, turns, tokens, usage, .. } = &e {
                    last_ok = Some(*ok);
                    stats.cost += cost_usd.unwrap_or(0.0);
                    stats.turns += turns.unwrap_or(0);
                    stats.tokens += tokens.unwrap_or(0);
                    if let Some(u) = usage {
                        stats.usage.add(u);
                    }
                }
                emit_event(Some(&sid), &e);
            });
            match outcome {
                Ok(o) => {
                    if let (Some(id), false) = (o.engine_session_id, opts.fresh) {
                        let _ = ctx.db.session_set_engine_id(&sid, &id);
                        resume = Some(id);
                    }
                }
                Err(e) => emit_event(Some(&sid), &ChatEvent::Error { code: error_code(&e).into(), message: crate::logging::redact(&e.to_string()) }),
            }
            (last_ok == Some(true) && !handle.is_cancelled(), last_text)
        };

        // Plannen met een sterk model en uitvoeren met een lichter model: alleen voor grote taken en alleen als de configuratie het aanzet.
        let mut planned = false;
        let mut prompt = prompt;
        if modelsel::should_plan(&cfg_models, &assessment, purpose, files_in_scope) && override_value.is_none() {
            let plan_model = cfg_models.tier_index(&cfg_models.planning.plan_tier).and_then(|t| cfg_models.resolve(t, &avail));
            let plan_prompt = format!("{prompt}\n\nDo NOT change any files yet. Write a short numbered plan (at most 10 lines): which files change and what happens in each.");
            let (plan_ok, plan) = turn(plan_prompt, &TurnOpts { model: plan_model, plan_only: true, fresh: true }, &mut stats);
            if plan_ok && !plan.trim().is_empty() {
                planned = true;
                prompt = format!("{prompt}\n\nFollow this plan (made for this task):\n{plan}");
            }
        }
        let run_model = if planned { cfg_models.tier_index(&cfg_models.planning.exec_tier).and_then(|t| cfg_models.resolve(t, &avail)) } else { exec_model.clone() };
        let main_opts = TurnOpts { model: run_model.clone(), ..Default::default() };
        let mut ok = turn(prompt, &main_opts, &mut stats).0;

        // 3b. Controle van het werkgebied achteraf. Bewerkingen via de editor worden al onderweg getoetst, maar Claude kan
        // ook een shell-commando gebruiken. Wat buiten het gebied is aangepast, draait Studio terug; Claude krijgt één
        // kans om met `breid_bereik_uit` om meer te vragen en het opnieuw te doen.
        if let Some(base) = base.as_ref().filter(|_| ok && !ctx.ablation.no_router) {
            for round in 0..2 {
                let changed = gitops::changed_since(&dir, base).unwrap_or_default();
                let out = ctx.scopes.with(project_id, |s| s.breaches(&changed)).unwrap_or_default();
                if out.is_empty() {
                    break;
                }
                let _ = gitops::restore_files(&dir, base, &out);
                emit_event(Some(&sid), &ChatEvent::Status { code: "scope_reverted".into(), detail: Some(out.len().to_string()) });
                ctx.scopes.with(project_id, |s| s.denied.extend(out.iter().cloned()));
                if round == 0 {
                    let msg = format!(
                        "Studio undid your changes to {} because they are outside the scope of this task. \
                         If the task really needs them, call breid_bereik_uit with a short reason first and then redo exactly those changes with the editor tools; otherwise leave them as they were.",
                        out.join(", ")
                    );
                    ok = turn(msg, &main_opts, &mut stats).0;
                    if !ok {
                        break;
                    }
                }
            }
        }

        // 3c. Is Claude binnen het gekozen vak gebleven? Een afwijking is geen fout maar een signaal: het wordt gelogd (alleen zichtbaar in
        // de ontwikkelaarsmodus) en het bestaande bereik-systeem blijft bepalen wat verder mag.
        if let (Some(ranges), Some(base)) = (selection_ranges.as_ref(), base.as_ref()) {
            if ok {
                let diff = gitops::diff_u0_since(&dir, base).unwrap_or_default();
                let hunks = crate::selection::parse_hunks(&diff);
                let in_diff: std::collections::HashSet<&str> = hunks.iter().map(|h| h.file.as_str()).collect();
                let new_files: Vec<String> = gitops::changed_since(&dir, base).unwrap_or_default().into_iter().filter(|f| !in_diff.contains(f.as_str()) && dir.join(f).is_file() && (f.starts_with("lib/") || f.starts_with("test/") || f.starts_with("assets/") || f == "pubspec.yaml")).collect();
                let hunks: Vec<_> = hunks.into_iter().filter(|h| h.file.starts_with("lib/") || h.file.starts_with("test/") || h.file.starts_with("assets/") || h.file == "pubspec.yaml").collect();
                let src = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
                let out = crate::selection::deviations(ranges, &hunks, &new_files, &src);
                if !out.is_empty() {
                    let detail = out.iter().take(8).map(|d| format!("{}:{}", d.file, d.line)).collect::<Vec<_>>().join(", ");
                    log::info!("selection exceeded in {project_id}: {detail}");
                    emit_event(Some(&sid), &ChatEvent::Status { code: "selection_exceeded".into(), detail: Some(detail) });
                }
            }
        }

        // 4. Controle, gerichte reparatie (max. 2x) en zo nodig terugdraaien.
        let mut gate_result = quality::GateResult { initial_issues: 0, repairs: 0, rolled_back: false, remaining: vec![] };
        let flutter = if ctx.checks { ctx.flutter.clone().or_else(|| proc::find("flutter")) } else { None };
        if let (true, Some(flutter), Some(base)) = (ok, flutter.as_ref(), base.as_ref()) {
            let changed = |dir: &std::path::Path| -> Vec<String> {
                gitops::changed_since(dir, base).unwrap_or_default().into_iter().filter(|f| !f.starts_with(".studio/")).collect()
            };
            let first = changed(&dir);
            if first.iter().any(|f| f.ends_with(".dart") || f == "pubspec.yaml") {
                emit_event(Some(&sid), &ChatEvent::Status { code: "checking".into(), detail: None });
                let mut check = || {
                    let level = ctx.scopes.with(project_id, |s| s.level).unwrap_or(decision.level);
                    // Een AI die nog niet lang in gebruik is krijgt altijd de volledige controle (analyse én tests), ook bij een kleine taak.
                    let level = if crate::engine::registry::spec(engine.id()).is_some_and(|s| !s.proven) { level.max(scope::Level::Shared) } else { level };
                    quality::run_checks(&dir, flutter, level, &changed(&dir))
                };
                let mut repair = |issues: &[quality::Issue], n: u32| {
                    emit_event(Some(&sid), &ChatEvent::Status { code: "repairing".into(), detail: Some(n.to_string()) });
                    let files: Vec<String> = issues.iter().map(|i| i.file.clone()).collect();
                    ctx.scopes.with(project_id, |s| s.allow_files(&files));
                    // Eén niveau zwaarder voor de reparaties (maximaal één opschaling per taak), vóór we de gebruiker lastigvallen.
                    let opts = match &esc_model {
                        Some(m) if !purpose_is_override(&override_value) => {
                            if !escalated.get() {
                                escalated.set(true);
                                emit_event(Some(&sid), &ChatEvent::Status { code: "model_escalated".into(), detail: Some(m.clone()) });
                            }
                            TurnOpts { model: Some(m.clone()), ..Default::default() }
                        }
                        _ => main_opts.clone(),
                    };
                    turn(quality::repair_prompt(issues, n), &opts, &mut stats).0
                };
                let mut rollback = || {
                    if let Ok(Some(c)) = gitops::restore(&dir, base, "Studio: undid the last change (it did not pass the checks)") {
                        let v = ctx.db.project_get(project_id).map(|p| p.version).unwrap_or_default();
                        let _ = ctx.db.snapshot_insert(project_id, &c, "Undid a change that did not pass the checks", "auto", &v);
                    }
                    emit_event(Some(&sid), &ChatEvent::Status { code: "rolled_back".into(), detail: None });
                };
                gate_result = quality::gate(&mut check, &mut repair, &mut rollback);
                let code = match (gate_result.rolled_back, gate_result.repairs) {
                    (true, _) => None,
                    (false, 0) => Some("checks_ok"),
                    (false, _) => Some("repaired"),
                };
                if let Some(code) = code {
                    emit_event(Some(&sid), &ChatEvent::Status { code: code.into(), detail: None });
                }
            }
        }
        if gate_result.rolled_back {
            ok = false;
        }

        // 5. Opruimen: de config kan een token bevatten (Supabase), het werkgebied geldt alleen voor deze run.
        let final_scope = ctx.scopes.take(project_id);
        let _ = std::fs::remove_file(&cfg);
        if let Some(f) = &settings {
            let _ = std::fs::remove_file(f);
        }

        // Wijzigingen van Claude altijd vastleggen, zodat de gebruiker terug kan.
        let label: String = format!("Claude: {}", text.chars().take(50).collect::<String>().replace('\n', " "));
        if !gate_result.rolled_back {
            if let Ok(v) = studio_tools::create_snapshot(&ctx.db, &project, &label, "auto") {
                if v["blocked"] == "leak" {
                    let n = v["leaks"].as_array().map(|a| a.len()).unwrap_or(0);
                    emit_event(Some(&sid), &ChatEvent::Status { code: "leak_blocked".into(), detail: Some(n.to_string()) });
                }
                if v["created"] == true {
                    emit_event(Some(&sid), &ChatEvent::Snapshot { commit: v["commit"].as_str().unwrap_or_default().to_string(), label });
                }
            }
        }
        let files_changed = base.as_ref().and_then(|b| gitops::changed_since(&dir, b).ok()).map(|c| c.len()).unwrap_or(0);
        // Koppeling met het volgende verzoek: de index bijwerken zodat de volgende run niet op oude gegevens begint.
        if let (false, Ok(idx)) = (ctx.ablation.no_profile, CodeIndex::build(&dir)) {
            let _ = std::fs::write(dir.join(".studio/screens.md"), idx.render_markdown());
        }

        // 6. Metingen (alleen lokaal).
        let (final_level, expansions, denied) = final_scope.map(|s| (s.level, s.expansions.len(), s.denied.len())).unwrap_or((decision.level, 0, 0));
        // Limiet bereikt: deze AI wacht tot de tijd om is; de volgende opdracht gebruikt de volgende in de volgorde. De lopende opdracht
        // wordt niet door een andere AI overgenomen.
        if limit_hit.get() {
            use crate::engine::registry as reg;
            let now = reg::now_unix();
            reg::mark_limited(&ctx.db, engine.id(), now);
            let order = reg::order(&ctx.db);
            let next = order.iter().find(|id| id.as_str() != engine.id() && reg::limited_until(&ctx.db, id, now).is_none() && self.engine_ready(id)).cloned().unwrap_or_default();
            emit_event(Some(&sid), &ChatEvent::Status { code: "engine_limit".into(), detail: Some(format!("{}|{next}", engine.id())) });
        }
        let _ = ctx.db.run_insert(
            project_id,
            &RunRecord {
                level: decision.level.as_str().into(),
                final_level: final_level.as_str().into(),
                screen: decision.screen.clone().unwrap_or_default(),
                why,
                expansions: expansions as i64,
                denied_edits: denied as i64,
                files_changed: files_changed as i64,
                issues_before: gate_result.initial_issues as i64,
                repairs: gate_result.repairs as i64,
                rolled_back: gate_result.rolled_back,
                ok,
                duration_ms: started.elapsed().as_millis() as i64,
                cost_usd: stats.cost,
                turns: stats.turns as i64,
                ai_questions,
                class: assessment.class.as_str().into(),
                tier: choice.tier_id.clone(),
                model: model.clone().unwrap_or_default(),
                engine: engine.id().into(),
                purpose: if override_value.is_some() { "override".into() } else { purpose.as_str().into() },
                escalated: escalated.get(),
                tokens: stats.tokens as i64,
                input_tokens: stats.usage.input as i64,
                output_tokens: stats.usage.output as i64,
                cache_read_tokens: stats.usage.cache_read as i64,
                cache_write_tokens: stats.usage.cache_write as i64,
                planned,
            },
        );
    }
}

#[derive(Default)]
struct TurnStats {
    cost: f64,
    turns: u32,
    tokens: u64,
    usage: crate::engine::TokenUsage,
}

/// Bij een handmatige override schalen we niet automatisch op: de ontwikkelaar bepaalt dan zelf het model.
fn purpose_is_override(o: &Option<String>) -> bool {
    o.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewProject;
    use crate::engine::claude::ClaudeEngine;
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    #[test]
    fn full_turn_with_a_fake_claude_cli() {
        use std::os::unix::fs::PermissionsExt;
        let Some(git) = crate::proc::find("git") else { return };
        let root = std::env::temp_dir().join(format!("studio-chat-{}", uuid::Uuid::new_v4()));
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("lib")).unwrap();
        std::fs::write(proj.join("lib/main.dart"), "a").unwrap();
        for a in [vec!["init", "-q"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"], vec!["add", "-A"], vec!["commit", "-qm", "start"]] {
            crate::proc::run(&git, &a, Some(&proj), Duration::from_secs(10)).unwrap();
        }

        // Nep-claude: logt argumenten en stdin, "bewerkt" een bestand en speelt een echte opname af.
        let fake = root.join("claude");
        std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude_stream.jsonl"), root.join("stream.jsonl")).unwrap();
        std::fs::write(
            &fake,
            "#!/bin/sh\nd=$(dirname \"$0\")\n\
             if [ \"$1\" = \"--help\" ]; then echo '--output-format --mcp-config --resume --allowedTools --permission-mode --strict-mcp-config --include-partial-messages (--permission-prompt-tool)'; exit 0; fi\n\
             echo \"$@\" >> \"$d/args.txt\"\ncat >> \"$d/stdin.txt\"\n\
             echo changed >> \"$PWD/lib/main.dart\"\ncat \"$d/stream.jsonl\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let db = Arc::new(Db::open_in_memory().unwrap());
        let p = db
            .project_insert(&NewProject {
                name: "Demo".into(),
                slug: "demo".into(),
                path: proj.to_string_lossy().into(),
                template: "blank".into(),
                description: String::new(),
                bundle_id: "com.example.demo".into(),
                initial_commit: None,
                profile_version: 0,
            })
            .unwrap();

        let events: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let sink = events.clone();
        let ctx = Arc::new(ChatCtx {
            db: db.clone(),
            mcp: McpInfo { port: 4242, token: "tok".into() },
            config_dir: root.join("mcp"),
            scopes: Default::default(), checks: false, flutter: None, ablation: Default::default(),
            emit: Arc::new(move |n, v| sink.lock().unwrap().push((n.to_string(), v))),
        });
        let mgr = Arc::new(ChatManager::new(Arc::new(ClaudeEngine::with_program(fake))));

        mgr.send(ctx.clone(), p.id.clone(), "Maak een knop\nmet twee regels".into(), Some("screen HomeScreen".into()), "nl".into(), Purpose::Work).unwrap();
        // Tweede bericht terwijl de eerste nog loopt wordt geweigerd (of is al klaar).
        let deadline = Instant::now() + Duration::from_secs(20);
        while mgr.is_running(&p.id) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!mgr.is_running(&p.id));

        let ev = events.lock().unwrap().clone();
        let names: Vec<&str> = ev.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names.last(), Some(&"chat-finished"));
        let types: Vec<String> = ev.iter().filter(|(n, _)| n == "chat-event").map(|(_, v)| v["event"]["type"].as_str().unwrap().to_string()).collect();
        assert_eq!(types.first().map(String::as_str), Some("user"));
        for t in ["init", "tool_use", "tool_result", "text", "done", "snapshot"] {
            assert!(types.iter().any(|x| x == t), "missing event {t}: {types:?}");
        }

        // prompt via stdin, met context; resume ontbreekt bij de eerste beurt
        let stdin = std::fs::read_to_string(root.join("stdin.txt")).unwrap();
        assert!(stdin.contains("[Studio context: screen HomeScreen]") && stdin.contains("twee regels"));
        let args = std::fs::read_to_string(root.join("args.txt")).unwrap();
        assert!(args.contains("--mcp-config") && args.contains("mcp__studio__approve") && !args.contains("--resume"));

        // MCP-config staat buiten het project, bevat het token en wordt na de beurt verwijderd
        assert!(!root.join("mcp").join(format!("{}.json", p.id)).exists(), "config with tokens must not stay on disk");
        assert!(!proj.join(".mcp.json").exists());

        // sessie hervat + berichten bewaard + automatische momentopname
        let s = db.session_active(&p.id).unwrap().unwrap();
        assert!(s.engine_session_id.is_some());
        assert!(db.messages_active(&p.id).unwrap().len() >= 6);
        assert_eq!(crate::gitops::last_commit_subject(&proj).unwrap(), "Claude: Maak een knop met twee regels");

        mgr.send(ctx, p.id.clone(), "nog iets".into(), None, "nl".into(), Purpose::Work).unwrap();
        while mgr.is_running(&p.id) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let args = std::fs::read_to_string(root.join("args.txt")).unwrap();
        let turns: Vec<&str> = args.lines().filter(|l| l.contains("stream-json")).collect();
        assert!(turns[1].contains(&format!("--resume {}", s.engine_session_id.unwrap())), "{turns:?}");
        assert!(mgr.send(Arc::new(ChatCtx { db, mcp: McpInfo { port: 1, token: String::new() }, config_dir: root.join("x"), scopes: Default::default(), checks: false, flutter: None, ablation: Default::default(), emit: Arc::new(|_, _| {}) }), p.id, "  ".into(), None, "en".into(), Purpose::Work).is_err());
    }

    /// Een echte git-repo met het voorbeeldproject, een nep-claude die steeds iets achter home_screen.dart plakt en een
    /// nep-flutter die (niet) klaagt. Zo testen we de hele keten: werkgebied, momentopname, controle, reparatie, terugdraaien.
    #[cfg(unix)]
    fn scoped_fixture(broken: bool) -> (PathBuf, PathBuf, Arc<Db>, crate::db::Project, ChatCtx, ChatManager) {
        use std::os::unix::fs::PermissionsExt;
        let git = crate::proc::find("git").expect("git");
        let proj = crate::index::fixture("chat");
        let root = proj.clone();
        for a in [vec!["init", "-q"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"], vec!["add", "-A"], vec!["commit", "-qm", "start"]] {
            crate::proc::run(&git, &a, Some(&proj), Duration::from_secs(10)).unwrap();
        }
        let bin = std::env::temp_dir().join(format!("studio-fakebin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude_stream.jsonl"), bin.join("stream.jsonl")).unwrap();
        std::fs::write(
            bin.join("help.txt"),
            "--output-format --mcp-config --resume --allowedTools --permission-mode --strict-mcp-config --settings (--permission-prompt-tool)\n  --model <model>   Model for the current session. Use an\n                    alias for the latest model (e.g.\n                    'fable', 'opus', or 'sonnet') or a\n                    models full name.\n  -n, --name <name>  Set a display name\n",
        )
        .unwrap();
        let claude = bin.join("claude");
        std::fs::write(
            &claude,
            "#!/bin/sh\nd=$(dirname \"$0\")\n\
             if [ \"$1\" = \"--help\" ]; then cat \"$d/help.txt\"; exit 0; fi\n\
             echo \"$@\" >> \"$d/args.txt\"\ncat >> \"$d/stdin.txt\"\n\
             echo '// edit' >> \"$PWD/lib/features/home/home_screen.dart\"\ncat \"$d/stream.jsonl\"\n",
        )
        .unwrap();
        let flutter = bin.join("flutter");
        let out = if broken {
            "echo \"  error • Undefined name 'x' • lib/features/home/home_screen.dart:3:5 • undefined_identifier\"; exit 1"
        } else {
            "echo 'No issues found!'"
        };
        std::fs::write(&flutter, format!("#!/bin/sh\necho \"$@\" >> \"$(dirname \"$0\")/flutter_args.txt\"\n{out}\n")).unwrap();
        for f in [&claude, &flutter] {
            std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let db = Arc::new(Db::open_in_memory().unwrap());
        let p = db
            .project_insert(&NewProject {
                name: "Demo".into(),
                slug: "demo".into(),
                path: proj.to_string_lossy().into(),
                template: "blank".into(),
                description: String::new(),
                bundle_id: "com.example.demo".into(),
                initial_commit: None,
                profile_version: 0,
            })
            .unwrap();
        let ctx = ChatCtx {
            db: db.clone(),
            mcp: McpInfo { port: 1, token: "t".into() },
            config_dir: root.join("mcp-cfg"),
            emit: Arc::new(|_, _| {}),
            scopes: Default::default(),
            checks: true,
            flutter: Some(flutter),
            ablation: Default::default(),
        };
        (bin, proj, db, p, ctx, ChatManager::new(Arc::new(ClaudeEngine::with_program(claude))))
    }

    #[cfg(unix)]
    fn statuses(db: &Db, project_id: &str) -> Vec<String> {
        db.messages_active(project_id)
            .unwrap()
            .iter()
            .filter_map(|m| serde_json::from_str::<Value>(m).ok())
            .filter(|v| v["type"] == "status" && v["code"] != "model")
            .map(|v| format!("{}:{}", v["code"].as_str().unwrap(), v["detail"].as_str().unwrap_or("")))
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn a_small_change_is_scoped_checked_and_kept_when_the_checks_pass() {
        let (bin, proj, db, p, ctx, mgr) = scoped_fixture(false);
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        let st = statuses(&db, &p.id);
        assert_eq!(st, vec!["scope:small|HomeScreen", "checking:", "checks_ok:"], "{st:?}");
        // De prompt draagt het werkgebied; de run kreeg een eigen instellingenbestand dat daarna weer weg is.
        let stdin = std::fs::read_to_string(bin.join("stdin.txt")).unwrap();
        assert!(stdin.contains("[Studio scope: Scope: Small (one file) for HomeScreen") && stdin.contains("lib/features/home/home_screen.dart") && stdin.contains("breid_bereik_uit"));
        let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
        assert!(args.contains("--settings"));
        assert!(!ctx.config_dir.join(format!("{}.settings.json", p.id)).exists());
        // Snelle controle: alleen de gewijzigde bestanden en hun directe gebruikers.
        let fa = std::fs::read_to_string(bin.join("flutter_args.txt")).unwrap();
        assert!(fa.contains("analyze lib/core/navigation/router.dart lib/features/home/home_screen.dart"), "{fa}");
        assert!(!fa.contains("settings_screen"), "{fa}");
        assert!(std::fs::read_to_string(proj.join("lib/features/home/home_screen.dart")).unwrap().ends_with("// edit\n"));
        assert_eq!(crate::gitops::last_commit_subject(&proj).unwrap(), "Claude: make the title bigger on the home screen");
        // De schermindex is bijgewerkt en het werkgebied is opgeruimd.
        assert!(std::fs::read_to_string(proj.join(".studio/screens.md")).unwrap().contains("## HomeScreen"));
        assert!(ctx.scopes.with(&p.id, |_| ()).is_none());
        let m = db.runs_summary(&p.id, 10).unwrap();
        assert_eq!((m.runs, m.clean_first_time, m.rolled_back, m.ai_questions), (1, 1, 0, 0));
        assert_eq!(m.by_level["small"], 1);
    }

    #[cfg(unix)]
    #[test]
    fn failing_checks_trigger_two_targeted_repairs_and_then_an_automatic_rollback() {
        let (bin, proj, db, p, ctx, mgr) = scoped_fixture(true);
        let before = std::fs::read_to_string(proj.join("lib/features/home/home_screen.dart")).unwrap();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        let st = statuses(&db, &p.id);
        assert_eq!(st, vec!["scope:small|HomeScreen", "checking:", "repairing:1", "model_escalated:sonnet", "repairing:2", "rolled_back:"], "{st:?}");
        // Eén gewone beurt + precies twee reparaties, de reparaties op dezelfde sessie en met de foutregel erin.
        let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
        let turns: Vec<&str> = args.lines().filter(|l| l.contains("stream-json")).collect();
        assert_eq!(turns.len(), 3, "{turns:?}");
        assert!(turns[1].contains("--resume") && turns[2].contains("--resume"));
        let stdin = std::fs::read_to_string(bin.join("stdin.txt")).unwrap();
        assert!(stdin.contains("repair attempt 1 of 2") && stdin.contains("repair attempt 2 of 2") && stdin.contains("home_screen.dart:3 error: Undefined name 'x'"));
        // Terug naar de staat van vóór de run, zonder geschiedenis kwijt te raken.
        assert_eq!(std::fs::read_to_string(proj.join("lib/features/home/home_screen.dart")).unwrap(), before);
        assert!(crate::gitops::last_commit_subject(&proj).unwrap().starts_with("Studio: undid"));
        assert!(db.snapshots_list(&p.id).unwrap().iter().any(|s| s.label.contains("Undid a change")));
        let m = db.runs_summary(&p.id, 10).unwrap();
        assert_eq!((m.runs, m.rolled_back, m.clean_first_time), (1, 1, 0));
    }

    #[cfg(unix)]
    #[test]
    fn edits_outside_the_scope_made_with_the_shell_are_undone_and_claude_gets_one_chance() {
        use std::os::unix::fs::PermissionsExt;
        let (bin, proj, db, p, ctx, mgr) = scoped_fixture(false);
        // Deze nep-claude past ook een gedeeld bestand aan, zoals een shell-commando dat zou doen.
        let claude = bin.join("claude");
        std::fs::write(
            &claude,
            "#!/bin/sh\nd=$(dirname \"$0\")\n\
             if [ \"$1\" = \"--help\" ]; then cat \"$d/help.txt\"; exit 0; fi\n\
             echo \"$@\" >> \"$d/args.txt\"\ncat >> \"$d/stdin.txt\"\n\
             echo '// edit' >> \"$PWD/lib/features/home/home_screen.dart\"\necho '// sneaky' >> \"$PWD/lib/core/theme/app_theme.dart\"\ncat \"$d/stream.jsonl\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let core_before = std::fs::read_to_string(proj.join("lib/core/theme/app_theme.dart")).unwrap();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        let st = statuses(&db, &p.id);
        assert_eq!(st.iter().filter(|s| s.starts_with("scope_reverted:1")).count(), 2, "{st:?}");
        assert_eq!(std::fs::read_to_string(proj.join("lib/core/theme/app_theme.dart")).unwrap(), core_before, "the shared file is back to normal");
        assert!(std::fs::read_to_string(proj.join("lib/features/home/home_screen.dart")).unwrap().contains("// edit"), "the in-scope edit stays");
        let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
        assert_eq!(args.lines().filter(|l| l.contains("stream-json")).count(), 2, "one normal turn and one follow-up, no endless loop");
        let stdin = std::fs::read_to_string(bin.join("stdin.txt")).unwrap();
        assert!(stdin.contains("undid your changes to lib/core/theme/app_theme.dart") && stdin.contains("breid_bereik_uit"));
        assert_eq!(db.runs_summary(&p.id, 5).unwrap().denied_edits, 2);
    }

    /// Over aankopen praat Claude niet zelf: Studio zet een concept klaar, opent de pagina en er draait geen model.
    #[cfg(unix)]
    #[test]
    fn purchase_requests_are_sent_to_the_purchases_page_without_running_the_model() {
        let (bin, proj, db, p, ctx, mgr) = scoped_fixture(false);
        let events: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx { emit: Arc::new(move |n, v| sink.lock().unwrap().push((n.to_string(), v))), ..ctx };
        mgr.run_flow(&ctx, &p.id, "Ik wil een jaarabonnement met proefperiode voor onbeperkt exporteren", None, "nl", &RunHandle::default());
        assert!(!bin.join("args.txt").exists(), "the model was never started");
        let ev = events.lock().unwrap().clone();
        assert!(ev.iter().any(|(n, v)| n == "open-purchases" && v["projectId"] == p.id.as_str()));
        let said = ev.iter().filter_map(|(n, v)| (n == "chat-event" && v["event"]["type"] == "text").then(|| v["event"]["text"].as_str().unwrap().to_string())).next().unwrap();
        assert!(said.contains("eigen pagina") && said.contains("concept"), "{said}");
        let draft = crate::purchases::load(&proj).unwrap();
        assert_eq!((draft.products[0].id.as_str(), draft.products[0].trial_days), ("pro_yearly", Some(7)));
        assert!(db.purchases_load(&p.id).unwrap().is_some());
        // Een gewone opdracht blijft gewoon naar het model gaan.
        events.lock().unwrap().clear();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        assert!(bin.join("args.txt").exists() && !events.lock().unwrap().iter().any(|(n, _)| n == "open-purchases"));
    }

    /// Limiet bereikt: de lopende opdracht blijft bij haar AI, de volgende gebruikt de volgende in de volgorde, met een korte overdracht.
    #[cfg(unix)]
    #[test]
    fn a_usage_limit_moves_the_next_task_to_the_next_engine_with_a_handover_and_never_switches_mid_task() {
        use std::os::unix::fs::PermissionsExt;
        let (bin, proj, db, p, ctx, mgr) = scoped_fixture(false);
        // Claude meldt de limiet; Codex (nep) doet de volgende opdracht.
        std::fs::write(
            bin.join("claude"),
            "#!/bin/sh\nd=$(dirname \"$0\")\nif [ \"$1\" = \"--help\" ]; then cat \"$d/help.txt\"; exit 0; fi\necho x >> \"$d/claude_runs.txt\"\ncat > /dev/null\necho '{\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"result\":\"Claude AI usage limit reached|1795700000\",\"duration_ms\":5}'\n",
        )
        .unwrap();
        let codex = bin.join("codex");
        let sample = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/engines/codex_ok.jsonl");
        std::fs::write(&codex, format!("#!/bin/sh\ncat >> \"$(dirname \"$0\")/codex_stdin.txt\"\necho '// edit by codex' >> \"$PWD/lib/features/home/home_screen.dart\"\ncat \"{sample}\"\n")).unwrap();
        std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mgr = mgr.with_engine(Arc::new(crate::engine::codex::CodexEngine::with_program(codex)), &["claude-code", "codex"]);
        crate::engine::registry::set_order(&db, &["claude-code".into(), "codex".into()]).unwrap();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx { emit: Arc::new(move |n, v| if n == "chat-event" { sink.lock().unwrap().push(v["event"].clone()) }), ..ctx };

        // 1. eerste opdracht: Claude, de limiet wordt herkend, er wordt niet midden in de opdracht gewisseld
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        assert!(!bin.join("codex_stdin.txt").exists(), "Codex took nothing over mid-task");
        let st = statuses(&db, &p.id);
        assert!(st.iter().any(|s| s == "engine_limit:claude-code|codex"), "{st:?}");
        assert!(crate::engine::registry::limited_until(&db, "claude-code", crate::engine::registry::now_unix()).is_some());
        let first_done = events.lock().unwrap().iter().rev().find(|e| e["type"] == "done").cloned().unwrap();
        assert_eq!((first_done["ok"].as_bool(), first_done["code"].as_str()), (Some(false), Some("limit")));
        // 2. de volgende opdracht gaat naar Codex, met een overdracht en de volledige controle
        let claude_calls = std::fs::read_to_string(bin.join("claude_runs.txt")).unwrap().lines().count();
        std::fs::remove_file(bin.join("flutter_args.txt")).ok();
        mgr.run_flow(&ctx, &p.id, "now make the subtitle smaller", None, "en", &RunHandle::default());
        assert_eq!(std::fs::read_to_string(bin.join("claude_runs.txt")).unwrap().lines().count(), claude_calls, "Claude is not called at all while it is limited");
        let stdin = std::fs::read_to_string(bin.join("codex_stdin.txt")).unwrap();
        assert!(stdin.contains("Handover: this app was worked on with Claude Code before; you are Codex CLI now") && stdin.contains("STUDIO.md") && stdin.contains("make the title bigger"), "{stdin}");
        assert!(stdin.contains("now make the subtitle smaller"));
        assert!(statuses(&db, &p.id).iter().any(|s| s == "handover:claude-code|codex"));
        assert!(std::fs::read_to_string(proj.join("lib/features/home/home_screen.dart")).unwrap().contains("// edit by codex"));
        let runs = db.runs_summary(&p.id, 10).unwrap();
        assert_eq!(runs.runs, 2);
        // 3. de stand wordt per engine bijgehouden
        assert_eq!(db.setting_get(&format!("engine_last:{}", p.id)).unwrap().as_deref(), Some("codex"));
    }

    /// Een gekozen vak is een signaal, geen blokkade: een wijziging erbuiten blijft staan maar wordt gelogd; erbinnen geen melding.
    #[cfg(unix)]
    #[test]
    fn a_change_outside_the_selected_box_is_logged_but_kept() {
        use std::os::unix::fs::PermissionsExt;
        let home = "lib/features/home/home_screen.dart";
        for (range_end, expect_signal) in [(2usize, true), (100000usize, false)] {
            let (bin, proj, db, p, ctx, mgr) = scoped_fixture(false);
            let claude = bin.join("claude");
            std::fs::write(&claude, "#!/bin/sh\nd=$(dirname \"$0\")\nif [ \"$1\" = \"--help\" ]; then cat \"$d/help.txt\"; exit 0; fi\ncat >> \"$d/stdin.txt\"\necho '// edit' >> \"$PWD/lib/features/home/home_screen.dart\"\ncat \"$d/stream.jsonl\"\n").unwrap();
            std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
            ctx.scopes.set_selection(&p.id, vec![(home.to_string(), 1, range_end)]);
            mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
            let st = statuses(&db, &p.id);
            assert_eq!(st.iter().any(|s| s.starts_with("selection_exceeded:")), expect_signal, "{st:?}");
            assert!(std::fs::read_to_string(proj.join(home)).unwrap().contains("// edit"), "never reverted");
            assert!(ctx.scopes.take_selection(&p.id).is_none(), "the selection is used up by the run");
        }
    }

    /// Een geheim komt nooit in de chat, de opgeslagen berichten of de prompt voor het model, ook niet als de gebruiker
    /// het zelf in de chat plakt of de AI het (per ongeluk) uitprint.
    #[cfg(unix)]
    #[test]
    fn secrets_never_reach_the_chat_the_stored_messages_or_the_model() {
        use std::os::unix::fs::PermissionsExt;
        let secret = format!("CHATSECRET-{}", uuid::Uuid::new_v4().simple());
        crate::scrub::register("chat-test/secret", std::slice::from_ref(&secret));
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx { emit: Arc::new(move |n, v| if n == "chat-event" { sink.lock().unwrap().push(v["event"].clone()) }), ..ctx };
        let line = |t: &str| format!("{t}\n");
        let stream = [
            line(r#"{"type":"system","subtype":"init","session_id":"s1","model":"m"}"#),
            line(&format!(r#"{{"type":"assistant","message":{{"id":"m1","content":[{{"type":"text","text":"The token is {secret} ok"}}]}}}}"#)),
            line(&format!(r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"t1","content":"env dump {secret}"}}]}}}}"#)),
            line(&format!(r#"{{"type":"result","subtype":"success","is_error":false,"result":"done {secret}","duration_ms":5}}"#)),
        ]
        .concat();
        std::fs::write(bin.join("stream.jsonl"), stream).unwrap();
        let claude = bin.join("claude");
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        mgr.run_flow(&ctx, &p.id, &format!("my key is {secret}, remember it"), Some(format!("selected {secret}")), "en", &RunHandle::default());
        let emitted = serde_json::to_string(&*events.lock().unwrap()).unwrap();
        assert!(emitted.contains("••••") && !emitted.contains(&secret), "events sent to the UI: {emitted}");
        let stored = db.messages_active(&p.id).unwrap().join("\n");
        assert!(!stored.contains(&secret) && stored.contains("••••"), "stored messages");
        let stdin = std::fs::read_to_string(bin.join("stdin.txt")).unwrap();
        assert!(!stdin.contains(&secret), "the prompt for the model must not hold the secret: {stdin}");
        crate::scrub::unregister("chat-test/secret");
    }

    /// Een lek blokkeert het vastleggen na de run en laat het terugdraaien niet los op niet-vastgelegd werk.
    #[cfg(unix)]
    #[test]
    fn a_leak_found_before_a_run_disables_rollback_and_blocks_the_snapshot() {
        use std::os::unix::fs::PermissionsExt;
        let (bin, proj, db, p, ctx, mgr) = scoped_fixture(true); // nep-flutter klaagt altijd
        // Het werk van de gebruiker staat nog niet in git en bevat een geheim.
        std::fs::write(proj.join("lib/features/home/home_screen.dart"), "// user work\nconst k = 'ghp_abcdefghijklmnopqrstuvwxyz0123456789';\n").unwrap();
        let claude = bin.join("claude");
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let head = crate::gitops::head(&proj).unwrap();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        let st = statuses(&db, &p.id);
        assert!(st.iter().any(|s| s.starts_with("leak_blocked:")), "{st:?}");
        assert!(!st.iter().any(|s| s.starts_with("rolled_back") || s.starts_with("repairing")), "no rollback without a safe base: {st:?}");
        assert!(std::fs::read_to_string(proj.join("lib/features/home/home_screen.dart")).unwrap().contains("user work"), "the user's own work was not touched");
        assert_eq!(crate::gitops::head(&proj).unwrap(), head, "nothing with a secret was committed");
    }

    /// Alleen de beurten van het werk zelf (stream-json); de losse classificatie- en modelcontrole-aanroepen tellen niet mee.
    #[cfg(unix)]
    fn work_turns(bin: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(bin.join("args.txt")).unwrap_or_default().lines().filter(|l| l.contains("stream-json")).map(String::from).collect()
    }

    #[cfg(unix)]
    fn last_run(db: &Db, project_id: &str) -> crate::db::RunRecord {
        // Via de samenvatting per model/klasse; voor één run lezen we de gegroepeerde cijfers niet, maar de rij zelf.
        let s = db.runs_summary(project_id, 1).unwrap();
        assert_eq!(s.runs, 1);
        s.last.expect("the newest run")
    }

    #[cfg(unix)]
    #[test]
    fn the_model_follows_the_difficulty_class_and_comes_from_the_config_and_the_cli() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        let t = work_turns(&bin);
        assert!(t[0].contains("--model haiku"), "a trivial tweak uses the lightest model: {}", t[0]);
        let r = last_run(&db, &p.id);
        assert_eq!((r.class.as_str(), r.tier.as_str(), r.model.as_str(), r.purpose.as_str(), r.escalated), ("trivial", "light", "haiku", "work", false));
        // Een zwaardere opdracht (gedeeld niveau) krijgt het sterkste model; 'opus' staat in de helptekst, dus geen aparte controle.
        mgr.run_flow(&ctx, &p.id, "change the theme colors everywhere in the app", None, "en", &RunHandle::default());
        let t = work_turns(&bin);
        // (een run buiten het scherm van de fake-claude levert ook een vervolgbeurt op; dezelfde modelkeuze geldt daar)
        assert!(t.last().unwrap().contains("--model opus"), "{:?}", t.last());
        // De config in de app-datamap gaat voor: andere namen zonder nieuwe app.
        let mut c = crate::modelsel::EngineConfig::embedded();
        c.tiers[2].models = vec!["sonnet".into()];
        mgr.set_config(c);
        mgr.run_flow(&ctx, &p.id, "change the theme colors everywhere in the app", None, "en", &RunHandle::default());
        assert!(work_turns(&bin).last().unwrap().contains("--model sonnet"), "{:?}", work_turns(&bin).last());
    }

    #[cfg(unix)]
    #[test]
    fn helper_tasks_are_always_light_and_release_work_never_drops_below_the_floor() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        mgr.run_flow_for(&ctx, &p.id, "change the theme colors everywhere in the app and write the changelog", None, "en", &RunHandle::default(), Purpose::Helper);
        assert!(work_turns(&bin).iter().all(|t| t.contains("--model haiku")), "helper = lightest even for a big text, on every turn");
        assert_eq!(last_run_purpose(&db, &p.id), "helper");
        mgr.run_flow_for(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default(), Purpose::Release);
        assert!(work_turns(&bin).last().unwrap().contains("--model sonnet"), "a trivial release task still gets the release minimum");
    }

    #[cfg(unix)]
    fn last_run_purpose(db: &Db, project_id: &str) -> String {
        db.runs_summary(project_id, 1).unwrap().last.unwrap().purpose
    }

    #[cfg(unix)]
    #[test]
    fn a_manual_choice_only_counts_in_developer_mode_and_stays_out_of_the_statistics() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        db.setting_set("model_override", "opus").unwrap();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        assert!(work_turns(&bin)[0].contains("--model haiku"), "without developer mode the override is ignored");
        db.setting_set("dev_mode", "true").unwrap();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        assert!(work_turns(&bin)[1].contains("--model opus"));
        assert_eq!(last_run_purpose(&db, &p.id), "override");
        assert!(db.class_history("trivial", 10).unwrap().len() == 1, "the overridden run does not teach the statistics anything");
        db.setting_set("model_override", "auto").unwrap();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        assert!(work_turns(&bin)[2].contains("--model haiku"));
    }

    #[cfg(unix)]
    #[test]
    fn failing_checks_escalate_one_tier_once_and_only_the_repairs_use_the_heavier_model() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(true);
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        let t = work_turns(&bin);
        assert_eq!(t.len(), 3, "{t:?}");
        assert!(t[0].contains("--model haiku"), "the first attempt is as light as the class allows");
        assert!(t[1].contains("--model sonnet") && t[2].contains("--model sonnet"), "repairs go exactly one tier up, never two: {t:?}");
        let r = last_run(&db, &p.id);
        assert!(r.escalated && r.rolled_back, "{r:?}");
        let st = statuses_all(&db, &p.id);
        assert_eq!(st.iter().filter(|s| s.starts_with("model_escalated:")).count(), 1, "one escalation per task: {st:?}");
    }

    #[cfg(unix)]
    fn statuses_all(db: &Db, project_id: &str) -> Vec<String> {
        db.messages_active(project_id).unwrap().iter().filter_map(|m| serde_json::from_str::<Value>(m).ok()).filter(|v| v["type"] == "status").map(|v| format!("{}:{}", v["code"].as_str().unwrap(), v["detail"].as_str().unwrap_or(""))).collect()
    }

    #[cfg(unix)]
    #[test]
    fn five_clean_runs_in_a_class_make_studio_try_the_lighter_model_next_time() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        let clean = |tier: &str| crate::db::RunRecord { class: "normal".into(), tier: tier.into(), model: "x".into(), purpose: "work".into(), ok: true, ..Default::default() };
        let request = "show the total of all scores on the home screen";
        mgr.run_flow(&ctx, &p.id, request, None, "en", &RunHandle::default());
        let first = work_turns(&bin)[0].clone();
        assert!(first.contains("--model sonnet"), "a normal task starts on the middle model: {first}");
        for _ in 0..5 {
            db.run_insert(&p.id, &clean("mid")).unwrap();
        }
        mgr.run_flow(&ctx, &p.id, request, None, "en", &RunHandle::default());
        assert!(work_turns(&bin)[1].contains("--model haiku"), "after five clean runs: one tier lighter");
        // Een mislukking op het lichtere niveau brengt de klasse weer omhoog.
        db.run_insert(&p.id, &crate::db::RunRecord { ok: false, rolled_back: true, ..clean("light") }).unwrap();
        mgr.run_flow(&ctx, &p.id, request, None, "en", &RunHandle::default());
        assert!(work_turns(&bin)[2].contains("--model sonnet"), "{}", work_turns(&bin)[2]);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_previous_run_on_the_same_screen_raises_the_class() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        db.run_insert(&p.id, &crate::db::RunRecord { screen: "HomeScreen".into(), ok: false, rolled_back: true, purpose: "work".into(), ..Default::default() }).unwrap();
        mgr.run_flow(&ctx, &p.id, "make the title bigger on the home screen", None, "en", &RunHandle::default());
        assert!(work_turns(&bin)[0].contains("--model sonnet"), "a trivial tweak after a failure is treated as normal");
    }

    #[cfg(unix)]
    #[test]
    fn a_big_task_can_be_planned_by_the_strong_model_and_executed_by_a_lighter_one() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        let mut c = crate::modelsel::EngineConfig::embedded();
        c.planning.enabled = true;
        c.planning.min_files = 1;
        mgr.set_config(c);
        mgr.run_flow(&ctx, &p.id, "change the theme colors everywhere in the app", None, "en", &RunHandle::default());
        let t = work_turns(&bin);
        assert!(t[0].contains("--permission-mode plan") && t[0].contains("--model opus") && !t[0].contains("--resume"), "plan first, strong model, fresh session: {}", &t[0][..120]);
        assert!(t[1].contains("--permission-mode acceptEdits") && t[1].contains("--model sonnet"), "then execution with the lighter model");
        let stdin = std::fs::read_to_string(bin.join("stdin.txt")).unwrap();
        assert!(stdin.contains("Do NOT change any files yet") && stdin.contains("Follow this plan"), "the plan goes into the execution prompt");
        assert!(db.runs_summary(&p.id, 1).unwrap().last.unwrap().planned);
        // Uit (standaard): geen plan-beurt.
        let (bin2, _p2, _d2, p2, ctx2, mgr2) = scoped_fixture(false);
        mgr2.run_flow(&ctx2, &p2.id, "change the theme colors everywhere in the app", None, "en", &RunHandle::default());
        assert!(!work_turns(&bin2)[0].contains("--permission-mode plan"));
    }

    #[cfg(unix)]
    #[test]
    fn each_screen_keeps_its_own_conversation_and_new_chat_resets_all() {
        let (bin, _proj, db, p, ctx, mgr) = scoped_fixture(false);
        let run = |t: &str| mgr.run_flow(&ctx, &p.id, t, None, "en", &RunHandle::default());
        run("make the title bigger on the home screen");
        run("change the text on the settings screen");
        run("change the label on the home screen again");
        let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
        let turns: Vec<&str> = args.lines().filter(|l| l.contains("stream-json")).collect();
        assert!(!turns[0].contains("--resume"));
        assert!(!turns[1].contains("--resume"), "settings is a different screen: fresh context");
        assert!(turns[2].contains("--resume"), "home continues its own conversation");
        assert!(db.messages_active(&p.id).unwrap().len() > 12, "the feed shows all screens together");
        db.session_create(&p.id, "claude-code").unwrap(); // "nieuw gesprek"
        assert!(db.messages_active(&p.id).unwrap().is_empty());
        run("change the text on the home screen");
        let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
        assert!(!args.lines().rfind(|l| l.contains("stream-json")).unwrap().contains("--resume"));
    }

    #[test]
    fn missing_cli_becomes_a_friendly_error_event() {
        let root = std::env::temp_dir().join(format!("studio-chat-miss-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Arc::new(Db::open_in_memory().unwrap());
        let p = db
            .project_insert(&NewProject {
                name: "D".into(),
                slug: "d".into(),
                path: root.to_string_lossy().into(),
                template: "blank".into(),
                description: String::new(),
                bundle_id: "x.y".into(),
                initial_commit: None,
                profile_version: 0,
            })
            .unwrap();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx {
            db,
            mcp: McpInfo { port: 1, token: "t".into() },
            config_dir: root.join("mcp"),
            scopes: Default::default(), checks: false, flutter: None, ablation: Default::default(),
            emit: Arc::new(move |n, v| {
                if n == "chat-event" {
                    sink.lock().unwrap().push(v["event"].clone())
                }
            }),
        };
        let mgr = ChatManager::new(Arc::new(ClaudeEngine::with_program(root.join("does-not-exist"))));
        mgr.run_flow(&ctx, &p.id, "hoi", None, "nl", &RunHandle::default());
        let ev = events.lock().unwrap();
        assert!(ev.iter().any(|e| e["type"] == "error" && e["code"] == "failed"), "{ev:?}");
    }

    /// Handmatige rooktest tegen de echte `claude` CLI (kost een paar tokens van je eigen account):
    /// `cargo test real_claude_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_claude_smoke() {
        use crate::permissions::{Decision, Permissions};
        let git = crate::proc::find("git").unwrap();
        let root = std::env::temp_dir().join(format!("studio-real-{}", uuid::Uuid::new_v4()));
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("lib")).unwrap();
        std::fs::write(proj.join("lib/main.dart"), "class HomeScreen extends StatelessWidget {}\n").unwrap();
        std::fs::write(proj.join("STUDIO.md"), "# Demo\n").unwrap();
        for a in [vec!["init", "-q"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"], vec!["add", "-A"], vec!["commit", "-qm", "start"]] {
            crate::proc::run(&git, &a, Some(&proj), Duration::from_secs(10)).unwrap();
        }
        let db = Arc::new(Db::open_in_memory().unwrap());
        let p = db
            .project_insert(&NewProject { name: "Demo".into(), slug: "demo".into(), path: proj.to_string_lossy().into(), template: "blank".into(), description: String::new(), bundle_id: "x.y".into(), initial_commit: None, profile_version: 0 })
            .unwrap();

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let tx = Mutex::new(tx);
        let perms = Arc::new(Permissions::new(Arc::new(move |r| { let _ = tx.lock().unwrap().send(r.id.clone()); }), Duration::from_secs(120)));
        let asked = Arc::new(Mutex::new(Vec::<String>::new()));
        {
            let (perms, db, asked) = (perms.clone(), db.clone(), asked.clone());
            std::thread::spawn(move || {
                for id in rx {
                    asked.lock().unwrap().push(id.clone());
                    perms.respond(&db, &id, Decision::AllowOnce);
                }
            });
        }
        let mcp = crate::mcp::start(crate::mcp::Deps { db: db.clone(), perms, preview: Arc::new(crate::preview::PreviewManager::new(Arc::new(|_, _| {}))), work_dir: root.join("shots"), scopes: Default::default(), vault: crate::vault::global(), emit: Arc::new(|_, _| {}) }).unwrap();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx { db: db.clone(), mcp, config_dir: root.join("mcp"), scopes: Default::default(), checks: false, flutter: None, ablation: Default::default(), emit: Arc::new(move |n, v| { if n == "chat-event" { sink.lock().unwrap().push(v["event"].clone()) } }) };
        let mgr = ChatManager::new(Arc::new(ClaudeEngine::new()));
        let prompt = "Do exactly these steps and nothing else: 1) call the studio MCP tool project_status, 2) call the studio MCP tool list_screens, 3) fetch https://example.com with the WebFetch tool, 4) reply with the single word done.";
        mgr.run_flow(&ctx, &p.id, prompt, None, "en", &RunHandle::default());
        for e in events.lock().unwrap().iter() {
            println!("{}", serde_json::to_string(e).unwrap().chars().take(220).collect::<String>());
        }
        println!("permission prompts: {:?}", asked.lock().unwrap());
        let ev = events.lock().unwrap();
        assert!(ev.iter().any(|e| e["type"] == "tool_use" && e["kind"] == "studio"), "studio tools were not called");
        assert!(ev.iter().any(|e| e["type"] == "done" && e["ok"] == true), "turn did not finish ok");
    }

    /// Echte Flutter + echte `claude`: `STUDIO_TEST_PLUGIN_SOURCE=<pad of owner/repo> cargo test real_profile_end_to_end -- --ignored --nocapture`
    /// De ene goedkope AI-vraag van de niveau-keuze, met de echte CLI (haiku, zonder gereedschap).
    #[test]
    #[ignore]
    fn real_classify() {
        let idx = CodeIndex::build(&crate::index::fixture("cls")).unwrap();
        let started = Instant::now();
        let answer = ClaudeEngine::new().classify(&scope::ai_question(&idx, "the settings page feels cramped, give it more air"), Some("haiku")).expect("an answer");
        println!("answer after {:?}: {answer}", started.elapsed());
        let (level, screen) = scope::parse_ai(&answer, &idx).expect("a parseable answer");
        println!("level {level:?} screen {screen:?}");
        assert_eq!(screen.as_deref(), Some("SettingsScreen"));
    }

    /// Bewijst met de echte `claude` dat het werkgebied wordt afgedwongen: een bewerking buiten het gebied wordt via de
    /// permissie-tool geweigerd en `breid_bereik_uit` verbreedt het gebied.
    #[test]
    #[ignore]
    fn real_scope_enforcement() {
        use crate::permissions::Permissions;
        use crate::projects::{create_project, CreateRequest, Tools};
        let flutter = crate::proc::find("flutter").expect("flutter on PATH");
        let git = crate::proc::find("git").unwrap();
        let root = std::env::temp_dir().join(format!("studio-scope-{}", uuid::Uuid::new_v4()));
        let db = Arc::new(Db::open_in_memory().unwrap());
        let req = CreateRequest { name: "Friday Scores".into(), template: "scoreboard".into(), description: "Scores".into(), org: "com.example".into() };
        let p = create_project(&db, &Tools { flutter: flutter.clone(), git }, &root.join("projects"), &req, &|_| {}).unwrap();
        let dir = PathBuf::from(&p.path);
        let idx = CodeIndex::build(&dir).unwrap();
        let screen = idx.screens.iter().find(|s| s.feature_dir.is_some()).expect("a feature screen").clone();
        println!("screen {} own={:?} shared={:?}", screen.class, screen.own, screen.shared);
        let target = screen.shared.iter().find(|f| f.starts_with("lib/core/")).cloned().expect("a shared core file").clone();

        let perms = Arc::new(Permissions::new(Arc::new(|_| {}), Duration::from_secs(5)));
        let scopes: Arc<ScopeRegistry> = Default::default();
        let mcp = crate::mcp::start(crate::mcp::Deps { db: db.clone(), perms, preview: Arc::new(crate::preview::PreviewManager::new(Arc::new(|_, _| {}))), work_dir: root.join("shots"), scopes: scopes.clone(), vault: crate::vault::global(), emit: Arc::new(|_, _| {}) }).unwrap();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx { db: db.clone(), mcp, config_dir: root.join("mcp"), scopes, checks: false, flutter: None, ablation: Default::default(), emit: Arc::new(move |n, v| { if n == "chat-event" { sink.lock().unwrap().push(v["event"].clone()) } }) };
        let mgr = ChatManager::new(Arc::new(ClaudeEngine::new()));
        let text = format!("Use the Edit tool (never a shell command) to add the comment line `// scope-test` as the last line of {} and ALSO the comment line `// core-test` as the last line of {target}. If one of them is refused, follow the instructions in the refusal.", screen.file);
        let context = format!("The user selected the screen \"{}\" ({}).", screen.class, screen.file);
        mgr.run_flow(&ctx, &p.id, &text, Some(context), "en", &RunHandle::default());
        for e in events.lock().unwrap().iter() {
            match e["type"].as_str().unwrap_or("") {
                "tool_use" => println!("tool_use {} {}", e["name"], e["summary"]),
                "tool_result" => println!("tool_result ok={} {}", e["ok"], e["text"].as_str().unwrap_or("").chars().take(160).collect::<String>()),
                "status" => println!("status {} {}", e["code"], e["detail"]),
                _ => {}
            }
        }
        let m = db.runs_summary(&p.id, 5).unwrap();
        println!("metrics {m:?}");
        let own = std::fs::read_to_string(dir.join(&screen.file)).unwrap();
        let core = std::fs::read_to_string(dir.join(&target)).unwrap();
        println!("own edited: {} core edited: {}", own.contains("scope-test"), core.contains("core-test"));
        assert!(own.contains("scope-test"), "edits inside the scope work");
        assert!(m.denied_edits >= 1, "the edit outside the scope must have been refused first");
        assert_eq!(core.contains("core-test"), m.expanded == 1, "the core file changes only after the scope was widened");
    }

    /// Bewijst dat het projectprofiel in de praktijk werkt (skills, hook, deny-regels, Dart-MCP, structuur).
    #[test]
    #[ignore]
    fn real_profile_end_to_end() {
        use crate::permissions::Permissions;
        use crate::projects::{create_project, CreateRequest, Tools};
        let flutter = crate::proc::find("flutter").expect("flutter on PATH");
        let git = crate::proc::find("git").unwrap();
        let root = std::env::temp_dir().join(format!("studio-e2e-{}", uuid::Uuid::new_v4()));
        let db = Arc::new(Db::open_in_memory().unwrap());
        let req = CreateRequest { name: "Friday Scores".into(), template: "scoreboard".into(), description: "Scores for game night".into(), org: "com.example".into() };
        let p = create_project(&db, &Tools { flutter: flutter.clone(), git }, &root.join("projects"), &req, &|_| {}).unwrap();
        let dir = std::path::PathBuf::from(&p.path);

        // officiële plugin (project-scope) via de claude CLI
        let source = std::env::var("STUDIO_TEST_PLUGIN_SOURCE").unwrap_or_else(|_| crate::profile::PLUGIN_SOURCE.into());
        let claude = crate::proc::find("claude").unwrap();
        let outcome = crate::profile::ensure_plugin(&claude, &dir, &source);
        println!("plugin: {outcome:?}");
        assert!(matches!(outcome, crate::profile::PluginOutcome::Installed | crate::profile::PluginOutcome::AlreadyInstalled));
        assert!(crate::profile::plugin_enabled(&dir));
        // het profiel overleeft de plugin-installatie (settings.json is samengevoegd, niet vervangen)
        let facts = crate::profile::ProjectFacts { name: &p.name, structure: "feature-first" };
        crate::profile::apply(&dir, &facts).unwrap();
        let settings = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
        assert!(settings.contains("enabledPlugins") && settings.contains("Read(build/**)") && settings.contains("studio_format.dart"));

        std::fs::create_dir_all(dir.join("build")).unwrap();
        std::fs::write(dir.join("build/secret.txt"), "TOPSECRET-BUILD-OUTPUT").unwrap();

        let perms = Arc::new(Permissions::new(Arc::new(|_| {}), Duration::from_secs(5)));
        let mcp = crate::mcp::start(crate::mcp::Deps { db: db.clone(), perms, preview: Arc::new(crate::preview::PreviewManager::new(Arc::new(|_, _| {}))), work_dir: root.join("shots"), scopes: Default::default(), vault: crate::vault::global(), emit: Arc::new(|_, _| {}) }).unwrap();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx { db: db.clone(), mcp, config_dir: root.join("mcp"), scopes: Default::default(), checks: false, flutter: None, ablation: Default::default(), emit: Arc::new(move |n, v| { if n == "chat-event" { sink.lock().unwrap().push(v["event"].clone()) } }) };
        let mgr = ChatManager::new(Arc::new(ClaudeEngine::new()));
        let run = |text: &str| {
            events.lock().unwrap().clear();
            mgr.run_flow(&ctx, &p.id, text, None, "en", &RunHandle::default());
            events.lock().unwrap().clone()
        };

        // 1. de hook formatteert wat Claude schrijft
        let ev = run("Create the file lib/core/ugly.dart with exactly this content (keep the odd spacing, do not format it yourself): `class Ugly{const Ugly();int   f( int a )=>a+1;}`. Nothing else.");
        let init = ev.iter().find(|e| e["type"] == "init").expect("init event");
        println!("init mcp: {}", init["mcp"]);
        assert!(init["mcp"].as_array().unwrap().iter().any(|m| m[0] == "dart" && m[1] == "connected"), "Dart MCP must be connected: {}", init["mcp"]);
        let ugly = std::fs::read_to_string(dir.join("lib/core/ugly.dart")).expect("ugly.dart written");
        println!("ugly.dart after the hook:\n{ugly}");
        assert!(ugly.contains("int f(int a) => a + 1;") || ugly.contains("int f(int a) =>"), "the format hook must have formatted the file: {ugly}");

        // 2. deny-regel: build/ is niet leesbaar
        let ev = run("Read the file build/secret.txt and tell me exactly what it says.");
        let text: String = ev.iter().filter(|e| e["type"] == "text").map(|e| e["text"].as_str().unwrap_or("").to_string()).collect();
        let tool_texts: String = ev.iter().filter(|e| e["type"] == "tool_result").map(|e| e["text"].as_str().unwrap_or("").to_string()).collect();
        println!("deny test -> tool results: {tool_texts}\nassistant: {text}");
        assert!(!tool_texts.contains("TOPSECRET-BUILD-OUTPUT") && !text.contains("TOPSECRET-BUILD-OUTPUT"), "build/ must not be readable");

        // 3. een echte schermwijziging volgt de structuur
        let ev = run("Add a Settings screen with a switch labelled 'Sound' and make it reachable from the scoreboard screen's app bar with a settings icon button.");
        let done_ok = ev.iter().any(|e| e["type"] == "done" && e["ok"] == true);
        for e in &ev {
            println!("{}", serde_json::to_string(e).unwrap().chars().take(160).collect::<String>());
        }
        assert!(done_ok, "turn should finish ok");
        let settings_dir = dir.join("lib/features/settings");
        assert!(settings_dir.is_dir(), "a new feature folder, not files in lib/ root");
        let router = std::fs::read_to_string(dir.join("lib/core/navigation/app_router.dart")).unwrap();
        assert!(router.contains("Settings"), "route registered in app_router.dart");
        assert!(settings_dir.join("CLAUDE.md").is_file() || std::fs::read_dir(&settings_dir).unwrap().count() > 0);
        let screen_src: String = std::fs::read_dir(&settings_dir).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "dart")).map(|e| std::fs::read_to_string(e.path()).unwrap()).collect();
        assert!(!screen_src.contains("Colors.") && !screen_src.contains("Color(0x"), "no hard-coded colors in the new screen");
        let lib_root: Vec<String> = std::fs::read_dir(dir.join("lib")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        println!("lib root: {lib_root:?}");
        assert!(lib_root.iter().all(|n| ["main.dart", "app.dart", "core", "features"].contains(&n.as_str())), "nothing loose in lib/: {lib_root:?}");
        let analyze = crate::proc::run(&flutter, &["analyze"], Some(&dir), Duration::from_secs(300)).unwrap();
        println!("analyze: {}", analyze.combined().lines().rev().find(|l| l.contains("issue")).unwrap_or(""));
        assert!(analyze.ok(), "{}", analyze.combined());
        let graph = crate::screens::screen_graph(&dir).unwrap();
        let sb = graph.nodes.iter().find(|n| n.class == "ScoreboardScreen").unwrap();
        assert!(sb.children.iter().any(|c| c.contains("Settings")), "navigation edge found: {:?}", graph.nodes);
    }
}
