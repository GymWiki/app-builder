//! Gemeenschappelijk deel van de engines voor Codex en Gemini: één subproces starten, de uitvoer regel voor regel vertalen naar
//! `ChatEvent`s, en als het zonder resultaat stopt een nette fout geven (limiet, niet ingelogd of gewoon mislukt).

use super::registry::{detect_limit, detect_login_problem};
use super::{ChatEvent, RunHandle, TurnOutcome};
use crate::error::{AppError, AppResult};
use crate::proc;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::Stdio;

pub struct Spawn<'a> {
    pub program: &'a Path,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub stdin: String,
    pub dir: &'a Path,
}

/// De foutcode voor een mislukte beurt, uit de tekst van de melding.
pub fn failure_code(text: &str) -> &'static str {
    if detect_limit(text).is_some() {
        "limit"
    } else if detect_login_problem(text) {
        "not_logged_in"
    } else {
        "failed"
    }
}

pub fn run(spawn: Spawn<'_>, run: &RunHandle, parse: &mut dyn FnMut(&str) -> Vec<ChatEvent>, on_event: &mut dyn FnMut(ChatEvent)) -> AppResult<TurnOutcome> {
    let mut cmd = proc::command(spawn.program);
    cmd.args(&spawn.args).current_dir(spawn.dir).stdin(Stdio::piped());
    for (k, v) in &spawn.env {
        cmd.env(k, v);
    }
    let name = spawn.program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut child = cmd.spawn().map_err(|e| AppError::CommandFailed(format!("{name}: {e}")))?;
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    *run.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);

    let prompt = spawn.stdin.clone();
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

    let mut session = None;
    let mut done = false;
    if let Some(out) = stdout {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            for ev in parse(&line) {
                match &ev {
                    ChatEvent::Init { session_id, .. } if !session_id.is_empty() => session = Some(session_id.clone()),
                    ChatEvent::Done { .. } => done = true,
                    _ => {}
                }
                on_event(ev);
            }
        }
    }
    let _ = run.child.lock().unwrap_or_else(|e| e.into_inner()).take().and_then(|mut c| c.wait().ok());
    let stderr_text = err_thread.join().unwrap_or_default();
    if run.is_cancelled() {
        return Ok(TurnOutcome { engine_session_id: session });
    }
    if !done {
        // Gestopt zonder resultaat: de reden zit meestal in de foutuitvoer.
        let text = crate::logging::redact(stderr_text.trim());
        on_event(ChatEvent::Error { code: failure_code(&text).into(), message: text });
    }
    Ok(TurnOutcome { engine_session_id: session })
}
