//! Engine voor de officiële Gemini CLI (`gemini -p … -o stream-json`). Studio praat nooit zelf met Google en raakt geen inloggegevens
//! aan; Studio gebruikt de CLI als subproces. (Google verbiedt het hergebruik van de inlog van de CLI door andere software; dat doen
//! we niet.) Gecontroleerd tegen Gemini CLI 0.71.1 (`--help` en de bron van de CLI): `-p`, `-o stream-json`, `-m`, `--approval-mode`
//! (default | auto_edit | yolo | plan), `--skip-trust`, en de omgevingsvariabele `GEMINI_CLI_SYSTEM_SETTINGS_PATH` voor een eigen
//! instellingenbestand met de MCP-server. Zie DECISIONS.md.

use super::registry::detect_limit;
use super::runner::{failure_code, run, Spawn};
use super::{AiEngine, ChatEvent, RunHandle, TokenUsage, TurnOutcome, TurnRequest};
use crate::error::{AppError, AppResult};
use crate::proc;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct GeminiEngine {
    program: Option<PathBuf>,
}

impl GeminiEngine {
    pub fn new() -> Self {
        Self { program: None }
    }
    #[cfg(test)]
    pub fn with_program(p: PathBuf) -> Self {
        Self { program: Some(p) }
    }
    fn program(&self) -> AppResult<PathBuf> {
        self.program.clone().or_else(|| proc::find("gemini")).ok_or_else(|| AppError::ToolMissing("gemini".into()))
    }
}

impl Default for GeminiEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Instellingen voor alleen deze run: de lokale Studio-server als MCP-server (adres en token uit het MCP-bestand van de run).
/// Staat in de eigen configuratiemap van Studio (alleen leesbaar voor de gebruiker), nooit in het project.
pub fn settings_json(mcp_config: &Path) -> Option<Value> {
    let (url, token) = super::codex::studio_mcp(mcp_config)?;
    Some(json!({ "mcpServers": { super::claude::MCP_SERVER: { "httpUrl": url, "headers": { "Authorization": format!("Bearer {token}") }, "trust": true } } }))
}

pub fn build_args(req: &TurnRequest) -> Vec<String> {
    let mut a: Vec<String> = vec!["-o".into(), "stream-json".into(), "--skip-trust".into(), "--approval-mode".into(), if req.plan_only { "plan".into() } else { "auto_edit".into() }];
    if let Some(m) = req.model.as_ref().filter(|m| !m.starts_with('-') && !m.trim().is_empty()) {
        a.extend(["-m".into(), m.clone()]);
    }
    // De eigenlijke opdracht gaat via stdin; `-p` zet de CLI in headless-modus en voegt deze zin eraan toe.
    a.extend(["-p".into(), "Do the task described above.".into()]);
    a
}

fn tool_kind(name: &str) -> &'static str {
    match name {
        "read_file" | "read_many_files" | "list_directory" | "glob" | "grep_search" => "read",
        "write_file" | "replace" => "edit",
        "run_shell_command" => "command",
        "web_fetch" | "google_web_search" => "network",
        n if n.starts_with("mcp_") || n.contains("studio") => "studio",
        _ => "other",
    }
}

#[derive(Default)]
pub struct GeminiParser {
    text: String,
    msg: u32,
}

fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

impl GeminiParser {
    fn flush(&mut self) -> Vec<ChatEvent> {
        if self.text.trim().is_empty() {
            self.text.clear();
            return vec![];
        }
        let t = std::mem::take(&mut self.text);
        vec![ChatEvent::Text { message_id: format!("gemini-{}", self.msg), text: t }]
    }

    pub fn feed(&mut self, line: &str) -> Vec<ChatEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return vec![] };
        match v["type"].as_str().unwrap_or("") {
            "init" => vec![ChatEvent::Init { session_id: v["session_id"].as_str().unwrap_or("").into(), model: v["model"].as_str().map(String::from), mcp: vec![] }],
            "message" if v["role"] == "assistant" => {
                let c = v["content"].as_str().unwrap_or("");
                self.text.push_str(c);
                vec![ChatEvent::TextDelta { message_id: format!("gemini-{}", self.msg), text: c.to_string() }]
            }
            "tool_use" => {
                let mut out = self.flush();
                self.msg += 1;
                let name = v["tool_name"].as_str().unwrap_or("").to_string();
                let p = &v["parameters"];
                let path = p["file_path"].as_str().or(p["path"].as_str()).or(p["dir_path"].as_str()).map(String::from);
                let detail = p["command"].as_str().map(String::from);
                let summary = path.clone().or_else(|| detail.clone()).or_else(|| p["url"].as_str().map(String::from)).unwrap_or_else(|| name.clone());
                out.push(ChatEvent::ToolUse { id: v["tool_id"].as_str().unwrap_or("").into(), name: name.clone(), kind: tool_kind(&name).into(), summary: trunc(&summary, 120), path, detail });
                out
            }
            "tool_result" => vec![ChatEvent::ToolResult { tool_use_id: v["tool_id"].as_str().unwrap_or("").into(), ok: v["status"] == "success", text: trunc(v["output"].as_str().or(v["error"]["message"].as_str()).unwrap_or(""), 600) }],
            "error" => {
                let m = v["message"].as_str().unwrap_or("").to_string();
                // Een limiet komt hierna nog als resultaat: eenmaal melden.
                if m.is_empty() || detect_limit(&m).is_some() {
                    vec![]
                } else {
                    vec![ChatEvent::Error { code: failure_code(&m).into(), message: m }]
                }
            }
            "result" => {
                let mut out = self.flush();
                let s = &v["stats"];
                let usage = TokenUsage { input: s["input_tokens"].as_u64().unwrap_or(0), output: s["output_tokens"].as_u64().unwrap_or(0), cache_read: s["cached"].as_u64().unwrap_or(0), cache_write: 0 };
                let ok = v["status"] == "success";
                let msg = v["error"]["message"].as_str().unwrap_or("").to_string();
                out.push(ChatEvent::Done {
                    ok,
                    code: (!ok).then(|| failure_code(&msg).into()),
                    text: if ok { String::new() } else { msg },
                    duration_ms: s["duration_ms"].as_u64(),
                    cost_usd: None,
                    turns: Some(1),
                    tokens: s["total_tokens"].as_u64().filter(|t| *t > 0),
                    usage: Some(usage),
                });
                out
            }
            _ => vec![],
        }
    }
}

impl AiEngine for GeminiEngine {
    fn id(&self) -> &'static str {
        "gemini"
    }

    /// Modelnamen voor deze CLI staan niet in Studio (ze kunnen alleen met een inlog worden gecontroleerd): altijd het standaardmodel van de CLI.
    fn model_available(&self, _alias: &str) -> bool {
        false
    }

    fn run_turn(&self, req: &TurnRequest, run_h: &RunHandle, on_event: &mut dyn FnMut(ChatEvent)) -> AppResult<TurnOutcome> {
        let program = self.program()?;
        let mut env = vec![];
        if let Some(s) = settings_json(&req.mcp_config) {
            let f = req.mcp_config.with_extension("gemini-settings.json");
            std::fs::write(&f, s.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600))?;
            }
            env.push(("GEMINI_CLI_SYSTEM_SETTINGS_PATH".to_string(), f.to_string_lossy().into_owned()));
        }
        let prompt = format!("{}\n\n{}", super::claude::system_prompt(&req.language), req.prompt);
        let mut parser = GeminiParser::default();
        run(Spawn { program: &program, args: build_args(req), env, stdin: prompt, dir: &req.project_dir }, run_h, &mut |l| parser.feed(l), on_event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str) -> String {
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/engines").join(name)).unwrap()
    }
    fn events(name: &str) -> Vec<ChatEvent> {
        let mut p = GeminiParser::default();
        fx(name).lines().flat_map(|l| p.feed(l)).collect()
    }
    fn req() -> TurnRequest {
        TurnRequest { project_dir: PathBuf::from("/p"), prompt: "x".into(), resume: None, mcp_config: PathBuf::from("/c.json"), language: "en".into(), settings: None, model: None, plan_only: false }
    }

    #[test]
    fn a_normal_run_streams_text_tools_and_a_result() {
        let ev = events("gemini_ok.jsonl");
        assert!(matches!(&ev[0], ChatEvent::Init { session_id, model, .. } if session_id.starts_with("9c1d8a52") && model.as_deref() == Some("auto")));
        let deltas: String = ev.iter().filter_map(|e| if let ChatEvent::TextDelta { text, .. } = e { Some(text.as_str()) } else { None }).collect();
        assert_eq!(deltas, "I'll look at the screen first.Done.");
        let texts: Vec<&str> = ev.iter().filter_map(|e| if let ChatEvent::Text { text, .. } = e { Some(text.as_str()) } else { None }).collect();
        assert_eq!(texts, ["I'll look at the screen first.", "Done."], "text is closed before a tool starts and before the result");
        let tools: Vec<(&str, &str)> = ev.iter().filter_map(|e| if let ChatEvent::ToolUse { kind, summary, .. } = e { Some((kind.as_str(), summary.as_str())) } else { None }).collect();
        assert_eq!(tools, [("read", "lib/home_screen.dart"), ("edit", "lib/home_screen.dart")]);
        assert!(ev.iter().any(|e| matches!(e, ChatEvent::ToolResult { ok: false, text, .. } if text == "Edit denied")));
        match ev.last().unwrap() {
            ChatEvent::Done { ok, usage, tokens, duration_ms, .. } => {
                assert!(*ok);
                assert_eq!(*usage, Some(TokenUsage { input: 1200, output: 300, cache_read: 800, cache_write: 0 }));
                assert_eq!((*tokens, *duration_ms), (Some(1500), Some(5000)));
            }
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn a_quota_message_is_one_failed_result_with_the_code_limit() {
        let ev = events("gemini_limit.jsonl");
        assert!(!ev.iter().any(|e| matches!(e, ChatEvent::Error { .. })));
        assert!(matches!(ev.last().unwrap(), ChatEvent::Done { ok: false, code: Some(c), .. } if c == "limit"), "{ev:?}");
    }

    #[test]
    fn arguments_settings_and_tool_kinds() {
        let a = build_args(&req());
        assert_eq!(&a[..5], ["-o", "stream-json", "--skip-trust", "--approval-mode", "auto_edit"]);
        assert!(a.windows(2).any(|w| w[0] == "-p") && !a.contains(&"-m".to_string()));
        let mut r = req();
        (r.plan_only, r.model) = (true, Some("m1".into()));
        let a = build_args(&r);
        assert!(a.contains(&"plan".to_string()) && a.windows(2).any(|w| w == ["-m", "m1"]));
        assert_eq!(["read_file", "replace", "run_shell_command", "web_fetch", "mcp_studio_get_theme", "think"].map(tool_kind), ["read", "edit", "command", "network", "studio", "other"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_run_with_a_fake_gemini_uses_a_private_settings_file_outside_the_project_and_names_a_login_problem() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("studio-gemini-run-{}", uuid::Uuid::new_v4()));
        let proj = d.join("project");
        std::fs::create_dir_all(&proj).unwrap();
        let fake = d.join("gemini");
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/engines/gemini_ok.jsonl");
        std::fs::write(&fake, format!("#!/bin/sh\necho \"$@\" > \"{0}/args.txt\"\ncp \"$GEMINI_CLI_SYSTEM_SETTINGS_PATH\" \"{0}/settings-seen.json\"\ncat > \"{0}/stdin.txt\"\ncat \"{1}\"\n", d.display(), sample.display())).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cfg = d.join("cfg.json");
        std::fs::write(&cfg, r#"{"mcpServers":{"studio":{"url":"http://127.0.0.1:9/mcp/p","headers":{"Authorization":"Bearer tok123"}}}}"#).unwrap();
        let mut r = req();
        (r.project_dir, r.mcp_config, r.prompt) = (proj.clone(), cfg, "make it bigger".into());
        let mut got = vec![];
        GeminiEngine::with_program(fake.clone()).run_turn(&r, &RunHandle::default(), &mut |e| got.push(e)).unwrap();
        assert!(matches!(got.last().unwrap(), ChatEvent::Done { ok: true, .. }));
        let seen: Value = serde_json::from_str(&std::fs::read_to_string(d.join("settings-seen.json")).unwrap()).unwrap();
        assert_eq!(seen["mcpServers"]["studio"]["httpUrl"], "http://127.0.0.1:9/mcp/p");
        assert_eq!(std::fs::read_dir(&proj).unwrap().count(), 0, "nothing (and no token) is written into the project");
        assert!(!std::fs::read_to_string(d.join("args.txt")).unwrap().contains("tok123"));

        std::fs::write(&fake, "#!/bin/sh\ncat > /dev/null\ncat \"$(dirname \"$0\")/nli.txt\" >&2\nexit 41\n").unwrap();
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/engines/gemini_not_logged_in.txt"), d.join("nli.txt")).unwrap();
        let mut got = vec![];
        GeminiEngine::with_program(fake).run_turn(&r, &RunHandle::default(), &mut |e| got.push(e)).unwrap();
        assert!(matches!(got.last().unwrap(), ChatEvent::Error { code, .. } if code == "not_logged_in"), "{got:?}");
        std::fs::remove_dir_all(d).ok();
    }
}
