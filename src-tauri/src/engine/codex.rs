//! Engine voor de officiële Codex CLI (`codex exec --json`). Studio praat nooit zelf met OpenAI en raakt geen tokens aan:
//! inloggen doet de gebruiker in de CLI (`codex login`).
//!
//! Gecontroleerd tegen Codex CLI 0.172.0 (`codex exec --help`, `codex mcp add --help`): `--json` (JSONL-gebeurtenissen), `-C <map>`,
//! `-s workspace-write`, `-m`, `--skip-git-repo-check`, hervatten met `codex exec resume <id>`, en MCP over HTTP via de configuratie
//! `mcp_servers.<naam>.url` + `bearer_token_env_var` (die sleutels bestaan in de CLI; `-c` overschrijft per aanroep). Zie DECISIONS.md.

use super::registry::detect_limit;
use super::runner::{failure_code, run, Spawn};
use super::{AiEngine, ChatEvent, RunHandle, TokenUsage, TurnOutcome, TurnRequest};
use crate::error::{AppError, AppResult};
use crate::proc;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const MCP_TOKEN_ENV: &str = "STUDIO_MCP_TOKEN";

pub struct CodexEngine {
    program: Option<PathBuf>,
}

impl CodexEngine {
    pub fn new() -> Self {
        Self { program: None }
    }
    #[cfg(test)]
    pub fn with_program(p: PathBuf) -> Self {
        Self { program: Some(p) }
    }
    fn program(&self) -> AppResult<PathBuf> {
        self.program.clone().or_else(|| proc::find("codex")).ok_or_else(|| AppError::ToolMissing("codex".into()))
    }
}

impl Default for CodexEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Adres en token van de lokale Studio-server uit het MCP-bestand van deze run (hetzelfde bestand dat Claude gebruikt).
pub fn studio_mcp(config: &Path) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(config).ok()?).ok()?;
    let s = &v["mcpServers"][super::claude::MCP_SERVER];
    let url = s["url"].as_str()?.to_string();
    let token = s["headers"]["Authorization"].as_str()?.trim_start_matches("Bearer ").to_string();
    Some((url, token))
}

/// De opdracht voor Codex. De CLI kent geen "voeg systeemprompt toe", dus de uitleg gaat vooraan de prompt (en staat in AGENTS.md).
pub fn build_args(req: &TurnRequest, mcp_url: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec!["exec".into(), "--json".into(), "--skip-git-repo-check".into(), "-C".into(), req.project_dir.to_string_lossy().into_owned()];
    // Bewerken binnen het project mag, netwerk en andere mappen niet; in deze modus kan Codex niets buiten de werkmap wijzigen.
    a.extend(["-s".into(), if req.plan_only { "read-only".into() } else { "workspace-write".into() }]);
    if let Some(m) = req.model.as_ref().filter(|m| !m.starts_with('-') && !m.trim().is_empty()) {
        a.extend(["-m".into(), m.clone()]);
    }
    if let Some(u) = mcp_url {
        a.extend(["-c".into(), format!("mcp_servers.{}.url=\"{}\"", super::claude::MCP_SERVER, u)]);
        a.extend(["-c".into(), format!("mcp_servers.{}.bearer_token_env_var=\"{}\"", super::claude::MCP_SERVER, MCP_TOKEN_ENV)]);
    }
    if let Some(id) = &req.resume {
        a.extend(["resume".into(), id.clone()]);
    }
    // De prompt gaat via stdin (`-`): veilig voor meerregelige tekst.
    a.push("-".into());
    a
}

/// Vertaalt de JSONL-uitvoer van `codex exec --json` naar gebeurtenissen. Houdt de laatste antwoordtekst bij voor het resultaat.
#[derive(Default)]
pub struct CodexParser {
    last_text: String,
}

fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

impl CodexParser {
    pub fn feed(&mut self, line: &str) -> Vec<ChatEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return vec![] };
        let item = &v["item"];
        let id = item["id"].as_str().unwrap_or("").to_string();
        match v["type"].as_str().unwrap_or("") {
            "thread.started" => vec![ChatEvent::Init { session_id: v["thread_id"].as_str().unwrap_or("").to_string(), model: None, mcp: vec![] }],
            "item.started" => match item["type"].as_str().unwrap_or("") {
                "command_execution" => {
                    let c = item["command"].as_str().unwrap_or("").to_string();
                    vec![ChatEvent::ToolUse { id, name: "shell".into(), kind: "command".into(), summary: trunc(&c, 120), path: None, detail: Some(c) }]
                }
                "mcp_tool_call" => {
                    let tool = format!("{}.{}", item["server"].as_str().unwrap_or(""), item["tool"].as_str().unwrap_or(""));
                    vec![ChatEvent::ToolUse { id, name: tool.clone(), kind: "studio".into(), summary: tool, path: None, detail: None }]
                }
                _ => vec![],
            },
            "item.completed" => match item["type"].as_str().unwrap_or("") {
                "agent_message" => {
                    let t = item["text"].as_str().unwrap_or("").to_string();
                    self.last_text = t.clone();
                    vec![ChatEvent::Text { message_id: id, text: t }]
                }
                "command_execution" => {
                    let ok = item["exit_code"].as_i64().map(|c| c == 0).unwrap_or(item["status"] == "completed");
                    vec![ChatEvent::ToolResult { tool_use_id: id, ok, text: trunc(item["aggregated_output"].as_str().unwrap_or(""), 600) }]
                }
                "mcp_tool_call" => vec![ChatEvent::ToolResult { tool_use_id: id, ok: item["status"] == "completed", text: String::new() }],
                "file_change" => {
                    let paths: Vec<String> = item["changes"].as_array().map(|c| c.iter().filter_map(|x| x["path"].as_str().map(String::from)).collect()).unwrap_or_default();
                    let ok = item["status"] != "failed";
                    vec![
                        ChatEvent::ToolUse { id: id.clone(), name: "apply_patch".into(), kind: "edit".into(), summary: paths.join(", "), path: paths.first().cloned(), detail: None },
                        ChatEvent::ToolResult { tool_use_id: id, ok, text: String::new() },
                    ]
                }
                _ => vec![],
            },
            "turn.completed" => {
                let u = &v["usage"];
                let usage = TokenUsage { input: u["input_tokens"].as_u64().unwrap_or(0), output: u["output_tokens"].as_u64().unwrap_or(0), cache_read: u["cached_input_tokens"].as_u64().unwrap_or(0), cache_write: 0 };
                let total = usage.input + usage.output;
                vec![ChatEvent::Done { ok: true, text: self.last_text.clone(), duration_ms: None, code: None, cost_usd: None, turns: Some(1), tokens: (total > 0).then_some(total), usage: Some(usage) }]
            }
            "turn.failed" => {
                let m = v["error"]["message"].as_str().unwrap_or("The turn failed.").to_string();
                vec![ChatEvent::Done { ok: false, code: Some(failure_code(&m).into()), text: m, duration_ms: None, cost_usd: None, turns: None, tokens: None, usage: None }]
            }
            // Fouten tijdens de beurt: "Reconnecting…" is een nieuwe poging, geen einde. Een limiet wordt wel meteen herkend.
            "error" => {
                let m = v["message"].as_str().unwrap_or("").to_string();
                if m.starts_with("Reconnecting") || m.is_empty() || detect_limit(&m).is_some() {
                    vec![]
                } else {
                    vec![ChatEvent::Error { code: failure_code(&m).into(), message: m }]
                }
            }
            _ => vec![],
        }
    }
}

impl AiEngine for CodexEngine {
    fn id(&self) -> &'static str {
        "codex"
    }

    /// Modelnamen voor deze CLI staan niet in Studio (ze kunnen alleen met een inlog worden gecontroleerd): altijd het standaardmodel van de CLI.
    fn model_available(&self, _alias: &str) -> bool {
        false
    }

    fn run_turn(&self, req: &TurnRequest, run_h: &RunHandle, on_event: &mut dyn FnMut(ChatEvent)) -> AppResult<TurnOutcome> {
        let program = self.program()?;
        let mcp = studio_mcp(&req.mcp_config);
        let args = build_args(req, mcp.as_ref().map(|m| m.0.as_str()));
        let env = mcp.map(|m| vec![(MCP_TOKEN_ENV.to_string(), m.1)]).unwrap_or_default();
        let prompt = format!("{}\n\n{}", super::claude::system_prompt(&req.language), req.prompt);
        let mut parser = CodexParser::default();
        run(Spawn { program: &program, args, env, stdin: prompt, dir: &req.project_dir }, run_h, &mut |l| parser.feed(l), on_event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str) -> String {
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/engines").join(name)).unwrap()
    }
    fn events(name: &str) -> Vec<ChatEvent> {
        let mut p = CodexParser::default();
        fx(name).lines().flat_map(|l| p.feed(l)).collect()
    }
    fn req() -> TurnRequest {
        TurnRequest { project_dir: PathBuf::from("/p"), prompt: "x".into(), resume: None, mcp_config: PathBuf::from("/c.json"), language: "en".into(), settings: None, model: None, plan_only: false }
    }

    #[test]
    fn a_normal_run_becomes_session_tools_text_and_a_result_with_raw_tokens() {
        let ev = events("codex_ok.jsonl");
        assert!(matches!(&ev[0], ChatEvent::Init { session_id, .. } if session_id == "01a217c8-b413-7c13-b19d-874cbee0b0fb"));
        let tools: Vec<(&str, &str)> = ev.iter().filter_map(|e| if let ChatEvent::ToolUse { kind, summary, .. } = e { Some((kind.as_str(), summary.as_str())) } else { None }).collect();
        assert_eq!(tools, [("command", "bash -lc 'cat lib/main.dart'"), ("edit", "lib/home_screen.dart"), ("studio", "studio.get_theme")]);
        assert!(ev.iter().any(|e| matches!(e, ChatEvent::ToolResult { ok: true, text, .. } if text.contains("void main"))));
        assert!(ev.iter().any(|e| matches!(e, ChatEvent::Text { text, .. } if text == "I made the title bigger.")));
        match ev.last().unwrap() {
            ChatEvent::Done { ok, text, usage, tokens, .. } => {
                assert!(*ok && text == "I made the title bigger.");
                assert_eq!(*usage, Some(TokenUsage { input: 24763, output: 122, cache_read: 24448, cache_write: 0 }));
                assert_eq!(*tokens, Some(24885));
            }
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn a_usage_limit_is_a_failed_result_with_the_code_limit() {
        let ev = events("codex_limit.jsonl");
        assert!(matches!(ev.last().unwrap(), ChatEvent::Done { ok: false, code: Some(c), text, .. } if c == "limit" && text.contains("3:41 PM")));
        assert!(!ev.iter().any(|e| matches!(e, ChatEvent::Error { .. })), "the limit is reported once, as the result");
    }

    #[test]
    fn reconnect_messages_and_log_lines_are_ignored() {
        let ev = events("codex_reconnect.jsonl");
        assert_eq!(ev.len(), 1, "only the session start: {ev:?}");
        let mut p = CodexParser::default();
        assert!(p.feed("2026-11-26T12:15:59Z ERROR codex_api::endpoint::responses_websocket: failed to connect").is_empty());
        assert!(matches!(p.feed("{\"type\":\"error\",\"message\":\"bad request\"}").first(), Some(ChatEvent::Error { code, .. }) if code == "failed"));
    }

    #[test]
    fn the_arguments_use_only_flags_the_cli_documents() {
        let a = build_args(&req(), Some("http://127.0.0.1:5000/mcp/p1"));
        assert_eq!(&a[..5], ["exec", "--json", "--skip-git-repo-check", "-C", "/p"]);
        assert!(a.windows(2).any(|w| w == ["-s", "workspace-write"]) && a.last().unwrap() == "-");
        assert!(a.contains(&"mcp_servers.studio.url=\"http://127.0.0.1:5000/mcp/p1\"".to_string()));
        assert!(a.contains(&"mcp_servers.studio.bearer_token_env_var=\"STUDIO_MCP_TOKEN\"".to_string()));
        assert!(!a.iter().any(|x| x.contains("Bearer") || x.contains("token=")), "the token itself is never an argument");
        let mut r = req();
        (r.plan_only, r.resume, r.model) = (true, Some("T1".into()), Some("m1".into()));
        let a = build_args(&r, None);
        assert!(a.windows(2).any(|w| w == ["-s", "read-only"]) && a.windows(2).any(|w| w == ["-m", "m1"]));
        assert!(a.windows(2).any(|w| w == ["resume", "T1"]) && !a.iter().any(|x| x.contains("mcp_servers")));
    }

    #[test]
    fn the_studio_server_address_and_token_come_from_the_run_config() {
        let d = std::env::temp_dir().join(format!("studio-codex-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("c.json");
        std::fs::write(&f, r#"{"mcpServers":{"studio":{"type":"http","url":"http://127.0.0.1:1234/mcp/abc","headers":{"Authorization":"Bearer s3cr3t"}}}}"#).unwrap();
        assert_eq!(studio_mcp(&f), Some(("http://127.0.0.1:1234/mcp/abc".into(), "s3cr3t".into())));
        assert_eq!(studio_mcp(&d.join("missing.json")), None);
        std::fs::remove_dir_all(d).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_run_with_a_fake_codex_streams_events_passes_the_token_by_environment_and_names_a_login_problem() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("studio-codex-run-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let fake = d.join("codex");
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/engines/codex_ok.jsonl");
        std::fs::write(&fake, format!("#!/bin/sh\necho \"$@\" > \"{0}/args.txt\"\necho \"token=$STUDIO_MCP_TOKEN\" > \"{0}/env.txt\"\ncat > \"{0}/stdin.txt\"\ncat \"{1}\"\n", d.display(), sample.display())).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cfg = d.join("cfg.json");
        std::fs::write(&cfg, r#"{"mcpServers":{"studio":{"url":"http://127.0.0.1:9/mcp/p","headers":{"Authorization":"Bearer tok123"}}}}"#).unwrap();
        let mut r = req();
        (r.project_dir, r.mcp_config, r.prompt) = (d.clone(), cfg, "make the title bigger".into());
        let mut got = vec![];
        let out = CodexEngine::with_program(fake.clone()).run_turn(&r, &RunHandle::default(), &mut |e| got.push(e)).unwrap();
        assert_eq!(out.engine_session_id.as_deref(), Some("01a217c8-b413-7c13-b19d-874cbee0b0fb"));
        assert!(matches!(got.last().unwrap(), ChatEvent::Done { ok: true, .. }));
        assert_eq!(std::fs::read_to_string(d.join("env.txt")).unwrap().trim(), "token=tok123");
        let args = std::fs::read_to_string(d.join("args.txt")).unwrap();
        assert!(!args.contains("tok123") && args.contains("exec --json"));
        assert!(std::fs::read_to_string(d.join("stdin.txt")).unwrap().ends_with("make the title bigger"));

        // niet ingelogd: stopt zonder resultaat, met de reden uit de foutuitvoer
        std::fs::write(&fake, "#!/bin/sh\ncat > /dev/null\necho 'Not logged in' >&2\nexit 1\n").unwrap();
        let mut got = vec![];
        CodexEngine::with_program(fake).run_turn(&r, &RunHandle::default(), &mut |e| got.push(e)).unwrap();
        assert!(matches!(got.last().unwrap(), ChatEvent::Error { code, .. } if code == "not_logged_in"), "{got:?}");
        std::fs::remove_dir_all(d).ok();
    }
}
