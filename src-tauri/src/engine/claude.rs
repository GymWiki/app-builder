//! Engine voor de officiële `claude` CLI (headless: `claude -p --output-format stream-json`).
//! Studio praat nooit zelf met de API en raakt geen tokens aan: inloggen gebeurt in de CLI.

use super::stream::parse_line;
use super::{AiEngine, ChatEvent, RunHandle, TurnOutcome, TurnRequest};
use crate::error::{AppError, AppResult};
use crate::proc;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// Naam waaronder de lokale Studio-MCP-server bij Claude bekendstaat.
pub const MCP_SERVER: &str = "studio";
pub const PERMISSION_TOOL: &str = "mcp__studio__approve";

/// Tools die zonder vragen mogen: lezen/zoeken, bewerken (binnen het project regelt
/// `--permission-mode acceptEdits`), Flutter/Dart-commando's, alleen-lezen git en de Studio-tools.
/// Alles daarbuiten (netwerk, andere commando's, paden buiten het project) wordt in de UI bevestigd.
pub const ALLOWED_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "LS",
    "Edit",
    "MultiEdit",
    "Write",
    "Bash(flutter *)",
    "Bash(dart *)",
    "Bash(git status*)",
    "Bash(git diff*)",
    "Bash(git log*)",
    "mcp__studio",
];

pub struct ClaudeEngine {
    program: Option<PathBuf>,
}

impl ClaudeEngine {
    pub fn new() -> Self {
        Self { program: None }
    }
    #[cfg(test)]
    pub fn with_program(p: PathBuf) -> Self {
        Self { program: Some(p) }
    }
    fn program(&self) -> AppResult<PathBuf> {
        self.program
            .clone()
            .or_else(|| proc::find("claude"))
            .ok_or_else(|| AppError::ToolMissing("claude".into()))
    }
}

impl Default for ClaudeEngine {
    fn default() -> Self {
        Self::new()
    }
}

static HELP: Mutex<Option<HashMap<PathBuf, String>>> = Mutex::new(None);

/// `claude --help` (per programma één keer bewaard) om flags te controleren in plaats van aan te nemen.
/// Een mislukte aanroep wordt niet onthouden.
fn help_text(program: &Path) -> String {
    let mut cache = HELP.lock().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(h) = map.get(program) {
        return h.clone();
    }
    let text = proc::run(program, &["--help"], None, Duration::from_secs(30)).map(|o| o.combined()).unwrap_or_default();
    if !text.trim().is_empty() {
        map.insert(program.to_path_buf(), text.clone());
    }
    text
}

const REQUIRED_FLAGS: &[&str] = &["--output-format", "--mcp-config", "--resume", "--allowedTools", "--permission-mode"];

pub fn system_prompt(language: &str) -> String {
    let lang = if language.starts_with("nl") { "Dutch" } else { "English" };
    format!(
        "You are working inside Studio, a tool that lets non-programmers build Flutter apps by chatting. \
         The user is not a developer: explain in plain {lang}, avoid jargon, keep changes small and focused, and always answer in {lang}. \
         Read STUDIO.md first and keep it up to date when the app's behavior or an important decision changes. \
         Use the `studio` MCP tools to inspect and steer the project (project_status, list_screens, get_theme/set_theme, \
         create_snapshot, bump_version, get_checklist, get_store_listing/set_store_listing, preview_status, preview_screenshot). \
         Prefer set_theme over hand-editing colors. After code changes run `flutter analyze` and fix problems. \
         Never put secrets (API keys, passwords, signing keys) in project files."
    )
}

/// Bouwt de argumenten; de prompt gaat via stdin (veilig voor meerregelige tekst en Windows `.cmd`).
pub fn build_args(help: &str, req: &TurnRequest) -> AppResult<Vec<String>> {
    for flag in REQUIRED_FLAGS {
        if !help.contains(flag) {
            // Een te oude CLI: de UI toont dit als "bijwerken".
            return Err(AppError::CommandFailed(format!("outdated claude cli: missing {flag}")));
        }
    }
    let mut a: Vec<String> = vec![
        "-p".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--permission-mode".into(),
        "acceptEdits".into(),
        "--mcp-config".into(),
        req.mcp_config.to_string_lossy().into_owned(),
        "--allowedTools".into(),
        ALLOWED_TOOLS.join(","),
        "--append-system-prompt".into(),
        system_prompt(&req.language),
    ];
    if help.contains("--strict-mcp-config") {
        a.push("--strict-mcp-config".into());
    }
    if help.contains("--include-partial-messages") {
        a.push("--include-partial-messages".into());
    }
    // `--permission-prompt-tool` staat niet als eigen optie in `--help`, maar wordt in de tekst
    // bij `--permission-prompts` genoemd; zonder die vermelding laten we de CLI zelf weigeren.
    if help.contains("--permission-prompt-tool") {
        a.push("--permission-prompt-tool".into());
        a.push(PERMISSION_TOOL.into());
    }
    if let Some(id) = &req.resume {
        a.push("--resume".into());
        a.push(id.clone());
    }
    Ok(a)
}

impl AiEngine for ClaudeEngine {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn run_turn(&self, req: &TurnRequest, run: &RunHandle, on_event: &mut dyn FnMut(ChatEvent)) -> AppResult<TurnOutcome> {
        let program = self.program()?;
        let help = help_text(&program);
        if help.trim().is_empty() {
            return Err(AppError::CommandFailed("could not run `claude --help`".into()));
        }
        let args = build_args(&help, req)?;

        let mut cmd = proc::command(&program);
        cmd.args(&args).current_dir(&req.project_dir).stdin(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| AppError::CommandFailed(format!("claude: {e}")))?;
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        *run.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);

        let prompt = req.prompt.clone();
        std::thread::spawn(move || {
            if let Some(mut s) = stdin.take() {
                let _ = s.write_all(prompt.as_bytes());
            }
        });
        let err_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.take(8192).read_to_string(&mut buf);
            }
            buf
        });

        let mut engine_session = None;
        let mut result_ok = None;
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                for ev in parse_line(&line, &req.project_dir) {
                    match &ev {
                        ChatEvent::Init { session_id, .. } if !session_id.is_empty() => engine_session = Some(session_id.clone()),
                        ChatEvent::Done { ok, .. } => result_ok = Some(*ok),
                        _ => {}
                    }
                    on_event(ev);
                }
            }
        }

        let status = run.child.lock().unwrap_or_else(|e| e.into_inner()).take().and_then(|mut c| c.wait().ok());
        let stderr_text = err_thread.join().unwrap_or_default();
        let cancelled = run.is_cancelled();
        let _ = status;

        if cancelled {
            return Ok(TurnOutcome { engine_session_id: engine_session });
        }
        if result_ok.is_none() {
            // De CLI stopte zonder resultaat (crash, kapotte installatie, ...).
            let msg = crate::logging::redact(stderr_text.trim());
            on_event(ChatEvent::Error { code: "failed".into(), message: msg });
            return Ok(TurnOutcome { engine_session_id: engine_session });
        }
        Ok(TurnOutcome { engine_session_id: engine_session })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELP: &str = "--output-format --mcp-config --resume --allowedTools --permission-mode --strict-mcp-config \
        --include-partial-messages --permission-prompts <target> (the SDK host or --permission-prompt-tool)";

    fn req(resume: Option<&str>) -> TurnRequest {
        TurnRequest {
            project_dir: "/p".into(),
            prompt: "hi".into(),
            resume: resume.map(Into::into),
            mcp_config: "/tmp/mcp.json".into(),
            language: "nl".into(),
        }
    }

    #[test]
    fn builds_args_from_verified_flags() {
        let a = build_args(HELP, &req(Some("abc"))).unwrap();
        let joined = a.join(" ");
        assert!(joined.contains("-p --output-format stream-json --verbose"));
        assert!(joined.contains("--permission-prompt-tool mcp__studio__approve"));
        assert!(joined.contains("--resume abc"));
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        assert!(!a.iter().any(|x| x == "hi"), "prompt must go through stdin, not argv");
        assert!(a.iter().any(|x| x.contains("Dutch")));
    }

    #[test]
    fn drops_optional_flags_the_cli_does_not_know() {
        let help = "--output-format --mcp-config --resume --allowedTools --permission-mode";
        let a = build_args(help, &req(None)).unwrap();
        assert!(!a.iter().any(|x| x == "--include-partial-messages" || x == "--permission-prompt-tool" || x == "--resume"));
    }

    #[test]
    fn too_old_cli_is_reported() {
        assert!(matches!(build_args("--output-format", &req(None)), Err(AppError::CommandFailed(m)) if m.contains("outdated")));
    }
}
