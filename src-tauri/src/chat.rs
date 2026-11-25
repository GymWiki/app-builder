//! Eén chatbeurt van begin tot eind: sessie, opslag, MCP-config, engine, automatische momentopname.

use crate::db::Db;
use crate::engine::{AiEngine, ChatEvent, RunHandle, TurnRequest};
use crate::error::{AppError, AppResult};
use crate::mcp::{config_json_with, McpInfo};
use crate::studio_tools;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub struct ChatManager {
    engine: Arc<dyn AiEngine>,
    running: Mutex<HashMap<String, Arc<RunHandle>>>,
}

pub struct ChatCtx {
    pub db: Arc<Db>,
    pub mcp: McpInfo,
    pub config_dir: PathBuf,
    pub emit: Emit,
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

impl ChatManager {
    pub fn new(engine: Arc<dyn AiEngine>) -> Self {
        Self { engine, running: Mutex::new(HashMap::new()) }
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
    pub fn send(self: &Arc<Self>, ctx: Arc<ChatCtx>, project_id: String, text: String, context: Option<String>, language: String) -> AppResult<()> {
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
            this.run_flow(&ctx, &project_id, &text, context, &language, &handle);
            this.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&project_id);
            (ctx.emit)("chat-finished", json!({ "projectId": project_id }));
        });
        Ok(())
    }

    /// De eigenlijke beurt (synchroon; apart zodat tests hem direct kunnen aanroepen).
    pub fn run_flow(&self, ctx: &ChatCtx, project_id: &str, text: &str, context: Option<String>, language: &str, handle: &RunHandle) {
        let emit_event = |session: Option<&str>, e: &ChatEvent| {
            if let (Some(s), false) = (session, matches!(e, ChatEvent::TextDelta { .. })) {
                let _ = ctx.db.message_insert(s, kind_of(e), &serde_json::to_string(e).unwrap_or_default());
            }
            (ctx.emit)("chat-event", json!({ "projectId": project_id, "event": e }));
        };

        let setup = || -> AppResult<(crate::db::Project, crate::db::Session, PathBuf)> {
            let project = ctx.db.project_get(project_id)?;
            let session = match ctx.db.session_active(project_id)? {
                Some(s) => s,
                None => ctx.db.session_create(project_id, self.engine.id())?,
            };
            std::fs::create_dir_all(&ctx.config_dir)?;
            let cfg = ctx.config_dir.join(format!("{project_id}.json"));
            let extra = crate::integrations::extra_servers(project_id, std::path::Path::new(&project.path));
            std::fs::write(&cfg, serde_json::to_string_pretty(&config_json_with(&ctx.mcp, project_id, &extra))?)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok((project, session, cfg))
        };
        let (project, session, cfg) = match setup() {
            Ok(x) => x,
            Err(e) => return emit_event(None, &ChatEvent::Error { code: error_code(&e).into(), message: crate::logging::redact(&e.to_string()) }),
        };
        let sid = session.id.clone();

        emit_event(Some(&sid), &ChatEvent::User { text: text.to_string(), context: context.clone() });
        let _ = ctx.db.session_set_title_if_empty(&sid, &text.chars().take(60).collect::<String>());

        let prompt = match &context {
            Some(c) if !c.trim().is_empty() => format!("[Studio context: {c}]\n\n{text}"),
            _ => text.to_string(),
        };
        let req = TurnRequest {
            project_dir: PathBuf::from(&project.path),
            prompt,
            resume: session.engine_session_id.clone(),
            mcp_config: cfg,
            language: language.to_string(),
        };

        let mut last_text = String::new();
        let outcome = self.engine.run_turn(&req, handle, &mut |e| {
            if let ChatEvent::Text { text, .. } = &e {
                last_text = text.clone();
            }
            emit_event(Some(&sid), &e);
        });
        match outcome {
            Ok(o) => {
                if let Some(id) = o.engine_session_id {
                    let _ = ctx.db.session_set_engine_id(&sid, &id);
                }
            }
            Err(e) => emit_event(Some(&sid), &ChatEvent::Error { code: error_code(&e).into(), message: crate::logging::redact(&e.to_string()) }),
        }

        // De config kan een token bevatten (Supabase): na de beurt weer weghalen.
        let _ = std::fs::remove_file(&req.mcp_config);

        // Wijzigingen van Claude altijd vastleggen, zodat de gebruiker terug kan.
        let label: String = format!("Claude: {}", text.chars().take(50).collect::<String>().replace('\n', " "));
        if let Ok(v) = studio_tools::create_snapshot(&ctx.db, &project, &label, "auto") {
            if v["created"] == true {
                emit_event(Some(&sid), &ChatEvent::Snapshot { commit: v["commit"].as_str().unwrap_or_default().to_string(), label });
            }
        }
    }
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
            })
            .unwrap();

        let events: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let sink = events.clone();
        let ctx = Arc::new(ChatCtx {
            db: db.clone(),
            mcp: McpInfo { port: 4242, token: "tok".into() },
            config_dir: root.join("mcp"),
            emit: Arc::new(move |n, v| sink.lock().unwrap().push((n.to_string(), v))),
        });
        let mgr = Arc::new(ChatManager::new(Arc::new(ClaudeEngine::with_program(fake))));

        mgr.send(ctx.clone(), p.id.clone(), "Maak een knop\nmet twee regels".into(), Some("screen HomeScreen".into()), "nl".into()).unwrap();
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

        mgr.send(ctx, p.id.clone(), "nog iets".into(), None, "nl".into()).unwrap();
        while mgr.is_running(&p.id) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let args = std::fs::read_to_string(root.join("args.txt")).unwrap();
        assert!(args.lines().nth(1).unwrap().contains(&format!("--resume {}", s.engine_session_id.unwrap())));
        assert!(mgr.send(Arc::new(ChatCtx { db, mcp: McpInfo { port: 1, token: String::new() }, config_dir: root.join("x"), emit: Arc::new(|_, _| {}) }), p.id, "  ".into(), None, "en".into()).is_err());
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
            })
            .unwrap();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx {
            db,
            mcp: McpInfo { port: 1, token: "t".into() },
            config_dir: root.join("mcp"),
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
            .project_insert(&NewProject { name: "Demo".into(), slug: "demo".into(), path: proj.to_string_lossy().into(), template: "blank".into(), description: String::new(), bundle_id: "x.y".into(), initial_commit: None })
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
        let mcp = crate::mcp::start(crate::mcp::Deps { db: db.clone(), perms, preview: Arc::new(crate::preview::PreviewManager::new(Arc::new(|_, _| {}))), work_dir: root.join("shots") }).unwrap();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        let ctx = ChatCtx { db: db.clone(), mcp, config_dir: root.join("mcp"), emit: Arc::new(move |n, v| { if n == "chat-event" { sink.lock().unwrap().push(v["event"].clone()) } }) };
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
}
