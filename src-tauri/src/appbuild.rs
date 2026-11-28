//! Bouwpijplijn van een nieuwe app (fase 14), na het deterministische scaffolden uit de blueprint:
//! 1. kern (één run, sterk model) → `studio/core-summary.md`, 2. pagina's (fan-out, **meerdere tegelijk**), gerichte herpoging per pagina,
//! 3. integratie (`flutter analyze` + gericht herstel), 4. momentopname. Annuleren laat het project consistent achter; er wordt nooit een
//! half project geleverd zonder dat de gebruiker weet welke pagina's nog niet klaar zijn. Elke run gebruikt een eigen werkgebied (scope-router).

use crate::blueprint::{Blueprint, Page};
use crate::chat::{ChatCtx, ChatManager, StepResult, StepSpec, StepTier};
use crate::engine::RunHandle;
use crate::scaffold::PLACEHOLDER;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const DEFAULT_PARALLEL: usize = 4;
pub const MAX_PARALLEL: usize = 8;

/// Aantal pagina's dat tegelijk gebouwd wordt (instelling `build_parallel`, standaard 4, tussen 1 en 8).
pub fn parallel_from(setting: Option<&str>) -> usize {
    setting.and_then(|s| s.trim().parse::<usize>().ok()).map(|n| n.clamp(1, MAX_PARALLEL)).unwrap_or(DEFAULT_PARALLEL)
}

#[derive(Default)]
pub struct BuildHandle {
    cancelled: AtomicBool,
    runs: Mutex<Vec<Arc<RunHandle>>>,
}
impl BuildHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        for r in self.runs.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            r.cancel();
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    fn new_run(&self) -> Arc<RunHandle> {
        let h = Arc::new(RunHandle::default());
        let mut g = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        g.push(h.clone());
        if self.is_cancelled() {
            h.cancel();
        }
        h
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageStatus {
    pub id: String,
    pub name: String,
    /// waiting | building | done | failed | attention
    pub status: String,
    pub attempts: u32,
    pub seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// scaffold | core | pages | integrate | done
    pub phase: String,
    pub core_done: bool,
    pub pages: Vec<PageStatus>,
    pub done: usize,
    pub total: usize,
    pub building: Vec<String>,
    pub eta_seconds: Option<u64>,
    pub parallel: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhaseStat {
    pub name: String,
    pub seconds: u64,
    pub ai_calls: u32,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub phases: Vec<PhaseStat>,
    pub total_seconds: u64,
    pub parallel: usize,
    pub pages_total: usize,
    pub pages_first_try: usize,
    pub pages_done: usize,
    pub pages_failed: Vec<String>,
    pub pages_attention: Vec<String>,
    pub core_ok: bool,
    pub analyze_ok: Option<bool>,
    pub limited: bool,
    pub cancelled: bool,
    pub models: Vec<String>,
}

// ───────────── prompts ─────────────

fn style_rules(bp: &Blueprint) -> String {
    format!("Style rules: colors only from Theme.of(context).colorScheme, spacing/radius from AppSpacing/AppRadius (lib/core/theme/tokens.dart), no hard-coded colors, fonts or magic numbers. Look: {} ({} look, {} font). Texts for the user are in the language \"{}\".", bp.style.card, bp.style.kit, bp.style.font, bp.app.language)
}

pub fn core_prompt(bp: &Blueprint) -> String {
    let part = serde_json::json!({ "app": bp.app, "models": bp.models, "features": bp.features, "integrations": bp.integrations, "pages": bp.pages.iter().map(|p| serde_json::json!({"id": p.id, "name": p.name, "models": p.models, "features": p.features})).collect::<Vec<_>>() });
    format!(
        "Build the CORE of a new Flutter app from this blueprint. Do not ask questions; choose sensible defaults.\nBlueprint: {part}\n\n\
         The scaffold already exists: models in lib/core/models/, example data in lib/core/data/sample_data.dart, routes in lib/core/navigation/, theme in lib/core/theme/. Pages are placeholders that other workers fill in at the same time, so you must NOT edit lib/features/.\n\
         Do this:\n\
         1. For every model a repository with Riverpod (Notifier, no code generation) in lib/core/data/, persisted on the phone (use `flutter pub add` for a small package such as shared_preferences if needed). When kStudioPreview (lib/core/preview_mode.dart) is true, return the example data and do not persist.\n\
         2. Shared widgets that two or more pages need in lib/core/widgets/ (reuse the existing ones first).\n\
         3. Wrap the app in ProviderScope if that is not so yet (lib/main.dart).\n\
         4. Write studio/core-summary.md (max 40 lines): one line per provider/repository/widget with its import path and how to use it. The page workers only read this file, so make it complete and exact.\n\
         5. Run flutter analyze and fix what you broke.\n{}",
        style_rules(bp)
    )
}

pub fn page_prompt(bp: &Blueprint, page: &Page, core_summary: &str) -> String {
    let models: Vec<serde_json::Value> = page.models.iter().filter_map(|id| bp.models.iter().find(|m| &m.id == id)).map(|m| serde_json::json!({"id": m.id, "fields": m.fields.iter().map(|f| format!("{}:{}", f.name, f.kind)).collect::<Vec<_>>()})).collect();
    let features: Vec<&str> = page.features.iter().filter_map(|id| bp.features.iter().find(|f| &f.id == id)).map(|f| f.title.as_str()).collect();
    let links: Vec<String> = page.links.iter().filter_map(|id| bp.pages.iter().find(|p| &p.id == id)).map(|p| format!("{} → context.go('{}')", p.name, p.route)).collect();
    format!(
        "Build ONE page of a new Flutter app: lib/features/{id}/{id}_screen.dart (class {cls}). Do not ask questions.\nApp: {app} — {purpose}\nPage: {name} (route {route}) — {ppurpose}\nKey parts: {parts}\nModels: {models}\nFeatures: {features}\nLinks to other pages: {links}\n\n\
         Replace the placeholder (remove the `// studio:placeholder` line). The core is already built; this is what it offers:\n{core}\n\n\
         Rules: only edit files inside lib/features/{id}/ (create widgets there; do not touch other pages or lib/core/). Show the example data when kStudioPreview is true so the page is useful at once. Run flutter analyze and fix what you broke.\n{style}",
        id = page.id,
        cls = format!("{}Screen", crate::scaffold::pascal(&page.id)),
        app = bp.app.name,
        purpose = bp.app.purpose,
        name = page.name,
        route = page.route,
        ppurpose = page.purpose,
        parts = page.components.join("; "),
        models = serde_json::to_string(&models).unwrap_or_default(),
        features = features.join("; "),
        links = if links.is_empty() { "-".into() } else { links.join("; ") },
        core = if core_summary.trim().is_empty() { "(no summary available; read lib/core/ as needed)".into() } else { core_summary.chars().take(6000).collect::<String>() },
        style = style_rules(bp),
    )
}

/// Is de pagina echt gebouwd? (De markering van het scaffold-bestand moet weg zijn.)
pub fn page_built(dir: &Path, id: &str) -> bool {
    std::fs::read_to_string(dir.join(format!("lib/features/{id}/{id}_screen.dart"))).is_ok_and(|t| !t.contains(PLACEHOLDER) && t.len() > 200)
}

fn tier_for(p: &Page) -> StepTier {
    if p.components.len() <= 2 && p.models.is_empty() { StepTier::Light } else { StepTier::Normal }
}

// ───────────── de pijplijn ─────────────

struct Shared {
    pages: Vec<PageStatus>,
    core_done: bool,
    phase: String,
    stats: Vec<PhaseStat>,
    limited: bool,
    models: Vec<String>,
    durations: Vec<u64>,
}

pub fn run(mgr: &ChatManager, ctx: &ChatCtx, project_id: &str, bp: &Blueprint, parallel: usize, language: &str, handle: &BuildHandle, only: Option<&[String]>, emit: &(dyn Fn(Progress) + Sync)) -> Report {
    let started = Instant::now();
    let project = ctx.db.project_get(project_id).ok();
    let dir: PathBuf = project.as_ref().map(|p| PathBuf::from(&p.path)).unwrap_or_default();
    let parallel = parallel.clamp(1, MAX_PARALLEL);
    let step_no = AtomicU32::new(1);
    let sh = Mutex::new(Shared {
        pages: bp.pages.iter().map(|p| PageStatus { id: p.id.clone(), name: p.name.clone(), status: "waiting".into(), attempts: 0, seconds: 0 }).collect(),
        core_done: false,
        phase: "scaffold".into(),
        stats: vec![],
        limited: false,
        models: vec![],
        durations: vec![],
    });
    let snapshot = |label: &str| {
        if let Some(p) = &project {
            let _ = crate::studio_tools::create_snapshot(&ctx.db, p, label, "auto");
        }
    };
    let progress = || {
        let g = sh.lock().unwrap_or_else(|e| e.into_inner());
        let done = g.pages.iter().filter(|p| p.status == "done").count();
        let total = g.pages.len();
        let remaining = total.saturating_sub(done);
        let avg = if g.durations.is_empty() { 90 } else { g.durations.iter().sum::<u64>() / g.durations.len() as u64 };
        let waves = remaining.div_ceil(parallel.max(1)) as u64;
        let eta = (g.phase == "pages" || g.phase == "core").then(|| waves * avg + if g.core_done { 0 } else { 120 });
        emit(Progress { phase: g.phase.clone(), core_done: g.core_done, pages: g.pages.clone(), done, total, building: g.pages.iter().filter(|p| p.status == "building").map(|p| p.id.clone()).collect(), eta_seconds: eta, parallel });
    };
    let record = |name: &str, secs: u64, calls: u32, tokens: u64| {
        let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
        match g.stats.iter_mut().find(|s| s.name == name) {
            Some(s) => {
                s.seconds += secs;
                s.ai_calls += calls;
                s.tokens += tokens;
            }
            None => g.stats.push(PhaseStat { name: name.into(), seconds: secs, ai_calls: calls, tokens }),
        }
    };
    let step = |spec: StepSpec| -> StepResult {
        let n = step_no.fetch_add(1, Ordering::SeqCst);
        let h = handle.new_run();
        let r = mgr.run_build_step(ctx, project_id, n, &spec, language, &h);
        let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
        if r.limit {
            g.limited = true;
        }
        if let Some(m) = &r.model {
            if !g.models.contains(m) {
                g.models.push(m.clone());
            }
        }
        r
    };
    let set = |id: &str, status: &str, add_attempt: bool, secs: Option<u64>| {
        let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = g.pages.iter_mut().find(|p| p.id == id) {
            p.status = status.into();
            if add_attempt {
                p.attempts += 1;
            }
            if let Some(s) = secs {
                p.seconds += s;
            }
        }
        if status == "done" {
            if let Some(s) = secs {
                g.durations.push(s.max(1));
            }
        }
    };
    let base = crate::gitops::head(&dir).ok();
    snapshot("Scaffold of your new app");
    let base = crate::gitops::head(&dir).ok().or(base);

    // ── 1. kern (bij het opnieuw maken van losse pagina's staat de kern er al) ──
    sh.lock().unwrap_or_else(|e| e.into_inner()).phase = "core".into();
    if only.is_some() {
        let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
        g.core_done = true;
        for p in g.pages.iter_mut() {
            if !only.unwrap_or(&[]).contains(&p.id) {
                p.status = "done".into();
            }
        }
    }
    progress();
    let core = if only.is_some() { StepResult { ok: true, text: String::new(), tokens: 0, usage: Default::default(), seconds: 0, limit: false, model: None } } else { step(StepSpec {
        label: "core".into(),
        prompt: core_prompt(bp),
        tier: StepTier::Strong,
        allowed_prefixes: vec!["lib/core/".into(), "assets/".into()],
        allowed_files: vec!["pubspec.yaml".into(), "pubspec.lock".into(), "lib/main.dart".into(), "lib/app.dart".into(), "studio/core-summary.md".into()],
    }) };
    if only.is_none() {
        record("core", core.seconds, 1, core.tokens);
    }
    let core_ok = core.ok && !handle.is_cancelled();
    if core_ok && only.is_none() {
        sh.lock().unwrap_or_else(|e| e.into_inner()).core_done = true;
        snapshot("Core of your app");
    }
    progress();

    // ── 2. pagina's, meerdere tegelijk ──
    let core_summary = std::fs::read_to_string(dir.join("studio/core-summary.md")).unwrap_or_default();
    let mut first_try = 0usize;
    if core_ok && !handle.is_cancelled() {
        sh.lock().unwrap_or_else(|e| e.into_inner()).phase = "pages".into();
        progress();
        let pass = |ids: Vec<String>, workers: usize, retry: bool| {
            let queue: Mutex<VecDeque<String>> = Mutex::new(ids.into());
            std::thread::scope(|s| {
                for _ in 0..workers.max(1) {
                    s.spawn(|| loop {
                        if handle.is_cancelled() {
                            break;
                        }
                        let Some(id) = queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front() else { break };
                        let Some(page) = bp.pages.iter().find(|p| p.id == id) else { continue };
                        set(&id, "building", true, None);
                        progress();
                        let r = step(StepSpec {
                            label: format!("page {id}"),
                            prompt: page_prompt(bp, page, &core_summary),
                            tier: tier_for(page),
                            allowed_prefixes: vec![format!("lib/features/{id}/")],
                            allowed_files: vec![],
                        });
                        record(if retry { "pages-retry" } else { "pages" }, r.seconds, 1, r.tokens);
                        let ok = r.ok && page_built(&dir, &id);
                        set(&id, if ok { "done" } else { "failed" }, false, Some(r.seconds));
                        progress();
                    });
                }
            });
        };
        pass(bp.pages.iter().filter(|p| only.is_none_or(|o| o.contains(&p.id))).map(|p| p.id.clone()).collect(), parallel, false);
        first_try = sh.lock().unwrap_or_else(|e| e.into_inner()).pages.iter().filter(|p| p.status == "done").count();
        // één gerichte herpoging per mislukte pagina; zit het abonnement aan zijn limiet, dan één tegelijk
        let failed: Vec<String> = sh.lock().unwrap_or_else(|e| e.into_inner()).pages.iter().filter(|p| p.status == "failed").map(|p| p.id.clone()).collect();
        if !failed.is_empty() && !handle.is_cancelled() {
            let limited = sh.lock().unwrap_or_else(|e| e.into_inner()).limited;
            pass(failed, if limited { 1 } else { parallel }, true);
        }
    }

    // ── 3. integratie ──
    let mut analyze_ok = None;
    if core_ok && !handle.is_cancelled() {
        sh.lock().unwrap_or_else(|e| e.into_inner()).phase = "integrate".into();
        progress();
        let t0 = Instant::now();
        let flutter = ctx.flutter.clone().or_else(|| crate::proc::find("flutter")).filter(|_| ctx.checks);
        if let Some(flutter) = flutter {
            let check = || crate::quality::run_checks(&dir, &flutter, crate::scope::Level::Project, &["lib/main.dart".to_string()]);
            let mut issues = check();
            if !issues.is_empty() {
                // gerichte reparatie: per eigenaar (pagina of kern) één taak, meerdere tegelijk
                let mut owners: std::collections::BTreeMap<String, Vec<crate::quality::Issue>> = Default::default();
                for i in &issues {
                    let owner = i.file.strip_prefix("lib/features/").and_then(|r| r.split('/').next()).map(String::from).unwrap_or_else(|| "core".into());
                    owners.entry(owner).or_default().push(i.clone());
                }
                let queue: Mutex<VecDeque<(String, Vec<crate::quality::Issue>)>> = Mutex::new(owners.into_iter().collect());
                std::thread::scope(|s| {
                    for _ in 0..parallel {
                        s.spawn(|| loop {
                            if handle.is_cancelled() {
                                break;
                            }
                            let Some((owner, list)) = queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front() else { break };
                            let files: Vec<String> = list.iter().map(|i| i.file.clone()).collect();
                            let prefix = if owner == "core" { "lib/core/".to_string() } else { format!("lib/features/{owner}/") };
                            let r = step(StepSpec { label: format!("repair {owner}"), prompt: crate::quality::repair_prompt(&list, 1), tier: StepTier::Normal, allowed_prefixes: vec![prefix], allowed_files: files });
                            record("integrate", r.seconds, 1, r.tokens);
                        });
                    }
                });
                issues = check();
            }
            analyze_ok = Some(issues.is_empty());
            if !issues.is_empty() {
                let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
                for i in &issues {
                    if let Some(id) = i.file.strip_prefix("lib/features/").and_then(|r| r.split('/').next()) {
                        if let Some(p) = g.pages.iter_mut().find(|p| p.id == id && p.status == "done") {
                            p.status = "attention".into();
                        }
                    }
                }
            }
        }
        record("integrate", t0.elapsed().as_secs(), 0, 0);
        progress();
    }

    // ── afronden of annuleren: altijd een consistente staat ──
    let cancelled = handle.is_cancelled();
    if cancelled {
        if let Some(base) = &base {
            // wat niet af is gaat terug naar het scaffold; wat klaar is blijft staan
            let g = sh.lock().unwrap_or_else(|e| e.into_inner());
            let unfinished: Vec<String> = g.pages.iter().filter(|p| p.status != "done").map(|p| format!("lib/features/{}/", p.id)).collect();
            let core_done = g.core_done;
            drop(g);
            let changed = crate::gitops::changed_since(&dir, base).unwrap_or_default();
            let revert: Vec<String> = changed.into_iter().filter(|f| unfinished.iter().any(|u| f.starts_with(u)) || (!core_done && (f.starts_with("lib/core/") || f == "pubspec.yaml" || f == "pubspec.lock"))).collect();
            let _ = crate::gitops::restore_files(&dir, base, &revert);
        }
    }
    let g = sh.lock().unwrap_or_else(|e| e.into_inner());
    let report = Report {
        phases: g.stats.clone(),
        total_seconds: started.elapsed().as_secs(),
        parallel,
        pages_total: g.pages.len(),
        pages_first_try: first_try,
        pages_done: g.pages.iter().filter(|p| p.status == "done").count(),
        pages_failed: g.pages.iter().filter(|p| p.status == "failed" || p.status == "waiting").map(|p| p.id.clone()).collect(),
        pages_attention: g.pages.iter().filter(|p| p.status == "attention").map(|p| p.id.clone()).collect(),
        core_ok,
        analyze_ok,
        limited: g.limited,
        cancelled,
        models: g.models.clone(),
    };
    drop(g);
    sh.lock().unwrap_or_else(|e| e.into_inner()).phase = "done".into();
    progress();
    let _ = std::fs::create_dir_all(dir.join("studio"));
    let _ = std::fs::write(dir.join("studio/build-report.json"), serde_json::to_string_pretty(&report).unwrap_or_default() + "\n");
    snapshot(if cancelled { "Build stopped" } else { "First version of your app" });
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_number_of_parallel_pages_is_bounded_and_defaults_to_four() {
        assert_eq!(parallel_from(None), 4);
        assert_eq!(parallel_from(Some("6")), 6);
        assert_eq!(parallel_from(Some("99")), MAX_PARALLEL);
        assert_eq!(parallel_from(Some("0")), 1);
        assert_eq!(parallel_from(Some("x")), 4);
    }

    #[test]
    fn prompts_carry_the_slice_of_the_blueprint_and_the_boundaries() {
        let b = crate::blueprint::sample();
        let c = core_prompt(&b);
        assert!(c.contains("studio/core-summary.md") && c.contains("must NOT edit lib/features/") && !c.contains("ranking list"));
        let p = page_prompt(&b, &b.pages[1], "playersProvider: lib/core/data/player_repository.dart");
        assert!(p.contains("lib/features/players/players_screen.dart") && p.contains("only edit files inside lib/features/players/") && p.contains("playersProvider") && p.contains("name:text"));
        assert!(p.contains("context.go") == false || b.pages[1].links.is_empty());
        let h = page_prompt(&b, &b.pages[0], "");
        assert!(h.contains("context.go('/players')"));
    }

    #[test]
    fn a_light_page_is_one_without_data_and_with_few_parts() {
        let b = crate::blueprint::sample();
        assert_eq!(tier_for(&b.pages[2]), StepTier::Light);
        assert_eq!(tier_for(&b.pages[1]), StepTier::Normal);
    }

    // ── met een nep-claude en een echte git-repo ──

    #[cfg(unix)]
    struct Fixture {
        root: PathBuf,
        dir: PathBuf,
        ctx: ChatCtx,
        mgr: ChatManager,
        id: String,
        bp: Blueprint,
    }

    #[cfg(unix)]
    fn fixture(name: &str, sleep: &str) -> Fixture {
        use crate::db::{Db, NewProject};
        use crate::engine::claude::ClaudeEngine;
        use crate::mcp::McpInfo;
        use std::os::unix::fs::PermissionsExt;
        let git = crate::proc::find("git").expect("git");
        let root = std::env::temp_dir().join(format!("studio-appbuild-{name}-{}", uuid::Uuid::new_v4()));
        let dir = root.join("proj");
        std::fs::create_dir_all(dir.join("lib/core/theme")).unwrap();
        std::fs::write(dir.join("pubspec.yaml"), "name: demo\ndependencies:\n  flutter:\n    sdk: flutter\n").unwrap();
        std::fs::write(dir.join("lib/core/theme/tokens.dart"), "class AppSpacing {}").unwrap();
        let mut bp = crate::blueprint::sample();
        for (id, route) in [("stats", "/stats"), ("history", "/history")] {
            bp.pages.push(Page { id: id.into(), name: id.into(), purpose: "p".into(), route: route.into(), components: vec!["a".into(), "b".into(), "c".into()], models: vec!["player".into()], features: vec![], links: vec![] });
        }
        crate::scaffold::scaffold(&dir, &bp).unwrap();
        for a in [vec!["init", "-q"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"], vec!["add", "-A"], vec!["commit", "-qm", "start"]] {
            crate::proc::run(&git, &a, Some(&dir), std::time::Duration::from_secs(10)).unwrap();
        }
        std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude_stream.jsonl"), root.join("stream.jsonl")).unwrap();
        let fake = root.join("claude");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nd=$(dirname \"$0\")\nif [ \"$1\" = \"--help\" ]; then echo '--output-format --mcp-config --resume --allowedTools --permission-mode --strict-mcp-config --include-partial-messages (--permission-prompt-tool)'; exit 0; fi\nif [ \"$1\" = \"auth\" ]; then echo '{{}}'; exit 0; fi\n\
                 prompt=$(cat)\necho \"start $(date +%s%N)\" >> \"$d/log.txt\"\nsleep {sleep}\n\
                 case \"$prompt\" in\n*\"CORE of a new\"*) mkdir -p studio; echo 'playerRepositoryProvider: lib/core/data/player_repository.dart' > studio/core-summary.md;;\n\
                 *) id=$(echo \"$prompt\" | grep -o 'lib/features/[a-z_]*/' | head -1 | cut -d/ -f3)\n\
                 echo \"$prompt\" > \"$d/prompt_$id.txt\"\n\
                 if [ -f \"$d/fail_$id\" ]; then rm \"$d/fail_$id\"; echo '{{\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"result\":\"boom\"}}'; echo \"end $(date +%s%N)\" >> \"$d/log.txt\"; exit 0; fi\n\
                 f=lib/features/$id/${{id}}_screen.dart\nsed -i '/studio:placeholder/d' $f\nprintf '// built %0300d\\n' 0 >> $f;;\nesac\n\
                 echo \"end $(date +%s%N)\" >> \"$d/log.txt\"\ncat \"$d/stream.jsonl\"\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let db = std::sync::Arc::new(Db::open_in_memory().unwrap());
        let p = db.project_insert(&NewProject { name: "Demo".into(), slug: "demo".into(), path: dir.to_string_lossy().into(), template: "blank".into(), description: String::new(), bundle_id: "com.example.demo".into(), initial_commit: None, profile_version: 0 }).unwrap();
        let ctx = ChatCtx { db, mcp: McpInfo { port: 1, token: "t".into() }, config_dir: root.join("mcp"), scopes: Default::default(), checks: false, flutter: None, ablation: Default::default(), emit: std::sync::Arc::new(|_, _| {}) };
        let mgr = ChatManager::new(std::sync::Arc::new(crate::engine::claude::ClaudeEngine::with_program(fake)));
        Fixture { root, dir, ctx, mgr, id: p.id, bp }
    }

    /// (hoogste aantal tegelijk lopende runs, aantal runs) uit het logbestand van de nep-claude.
    #[cfg(unix)]
    fn overlap(root: &Path) -> (usize, usize) {
        let log = std::fs::read_to_string(root.join("log.txt")).unwrap_or_default();
        let mut ev: Vec<(u128, i32)> = log.lines().filter_map(|l| {
            let mut it = l.split_whitespace();
            let k = it.next()?;
            let t: u128 = it.next()?.parse().ok()?;
            Some((t, if k == "start" { 1 } else { -1 }))
        }).collect();
        ev.sort();
        let (mut cur, mut max) = (0i32, 0i32);
        for (_, d) in &ev {
            cur += d;
            max = max.max(cur);
        }
        (max as usize, ev.iter().filter(|e| e.1 == 1).count())
    }

    #[cfg(unix)]
    #[test]
    fn pages_are_built_in_parallel_up_to_the_limit_and_the_result_is_snapshotted() {
        let f = fixture("par", "0.6");
        let progress: Mutex<Vec<Progress>> = Mutex::new(vec![]);
        let r = run(&f.mgr, &f.ctx, &f.id, &f.bp, 3, "en", &BuildHandle::default(), None, &|p| progress.lock().unwrap().push(p));
        assert!(r.core_ok && r.pages_done == 5 && r.pages_failed.is_empty() && r.pages_first_try == 5, "{r:?}");
        let (max, runs) = overlap(&f.root);
        assert_eq!(runs, 6, "one core run and five page runs");
        assert!((2..=3).contains(&max), "pages must overlap but never exceed the limit: {max}");
        // de pagina's kregen de samenvatting van de kern mee en alleen hun eigen map als werkgebied
        let prompt = std::fs::read_to_string(f.root.join("prompt_players.txt")).unwrap();
        assert!(prompt.contains("playerRepositoryProvider") && prompt.contains("only edit files inside lib/features/players/") && prompt.contains("lib/features/players/**"));
        assert_eq!(crate::gitops::last_commit_subject(&f.dir).unwrap(), "First version of your app");
        assert!(f.dir.join("studio/build-report.json").is_file());
        let p = progress.lock().unwrap();
        assert!(p.iter().any(|x| x.building.len() >= 2) && p.last().unwrap().phase == "done" && p.last().unwrap().done == 5);
        assert!(r.phases.iter().any(|s| s.name == "core" && s.ai_calls == 1) && r.phases.iter().any(|s| s.name == "pages" && s.ai_calls == 5));
    }

    #[cfg(unix)]
    #[test]
    fn a_limit_of_one_builds_the_pages_one_after_the_other() {
        let f = fixture("seq", "0.3");
        let r = run(&f.mgr, &f.ctx, &f.id, &f.bp, 1, "en", &BuildHandle::default(), None, &|_| {});
        assert_eq!(r.pages_done, 5);
        assert_eq!(overlap(&f.root).0, 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_page_is_retried_once_and_counted_as_not_first_try() {
        let f = fixture("retry", "0.2");
        std::fs::write(f.root.join("fail_players"), "").unwrap();
        let r = run(&f.mgr, &f.ctx, &f.id, &f.bp, 3, "en", &BuildHandle::default(), None, &|_| {});
        assert_eq!((r.pages_done, r.pages_first_try, r.pages_failed.len()), (5, 4, 0), "{r:?}");
        assert!(r.phases.iter().any(|s| s.name == "pages-retry" && s.ai_calls == 1));
    }

    #[cfg(unix)]
    #[test]
    fn a_page_that_keeps_failing_is_reported_never_hidden() {
        let f = fixture("fail", "0.2");
        // faalt bij de eerste én de tweede poging
        std::fs::write(f.root.join("fail_stats"), "").unwrap();
        let fake = f.root.join("claude");
        let script = std::fs::read_to_string(&fake).unwrap().replace("rm \"$d/fail_$id\"; ", "");
        std::fs::write(&fake, script).unwrap();
        let r = run(&f.mgr, &f.ctx, &f.id, &f.bp, 3, "en", &BuildHandle::default(), None, &|_| {});
        assert_eq!((r.pages_done, r.pages_failed.clone()), (4, vec!["stats".to_string()]));
        let bp: Blueprint = serde_json::from_str(&std::fs::read_to_string(f.dir.join("studio/blueprint.json")).unwrap()).unwrap();
        assert_eq!(bp.pages.len(), 5);
        assert!(std::fs::read_to_string(f.dir.join("lib/features/stats/stats_screen.dart")).unwrap().contains("studio:placeholder"), "the unfinished page stays the scaffold, not half work");
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_stops_every_run_and_leaves_a_consistent_project() {
        let f = fixture("cancel", "4");
        let handle = BuildHandle::default();
        let r = std::thread::scope(|s| {
            let h = s.spawn(|| run(&f.mgr, &f.ctx, &f.id, &f.bp, 3, "en", &handle, None, &|_| {}));
            std::thread::sleep(std::time::Duration::from_millis(1500));
            handle.cancel();
            h.join().unwrap()
        });
        assert!(r.cancelled && r.pages_done == 0, "{r:?}");
        assert_eq!(crate::gitops::changed_files(&f.dir).unwrap(), 0, "nothing half-written is left in the working tree");
        for id in ["home", "players", "settings", "stats", "history"] {
            assert!(std::fs::read_to_string(f.dir.join(format!("lib/features/{id}/{id}_screen.dart"))).unwrap().contains("studio:placeholder"), "{id}");
        }
        assert!(r.total_seconds < 15, "cancel must not wait for the slow runs");
    }
}
