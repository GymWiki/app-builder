//! Ingebouwde terminal (portable-pty). Wordt gebruikt voor de Claude-login en begeleide
//! installaties; later ook voor de "geavanceerd"-tab. De frontend kan alleen vaste
//! commando's openen (zie commands.rs) en daarna invoer sturen naar een bestaande sessie.

use crate::error::{AppError, AppResult};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

const BACKLOG_MAX: usize = 64 * 1024;
const BACKLOG_SESSIONS: usize = 8;

/// Uitvoer die al is verstuurd, zodat een terminal die pas later in de UI verschijnt
/// (xterm laadt lazy) niets mist. `end` telt het totaal aantal verstuurde bytes.
#[derive(Default, Clone, Serialize)]
pub struct Backlog {
    pub data: String,
    pub end: usize,
}

#[derive(Default)]
pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    backlogs: Arc<Mutex<HashMap<String, Backlog>>>,
}

#[derive(Clone, Serialize)]
struct DataEvent {
    id: String,
    data: String,
    end: usize,
}

#[derive(Clone, Serialize)]
struct ExitEvent {
    id: String,
    code: Option<u32>,
}

fn pty_err(e: impl std::fmt::Display) -> AppError {
    AppError::Pty(e.to_string())
}

/// Splitst bytes in geldige UTF-8 en een onvolledig staartje dat bij de volgende read hoort.
fn split_utf8(buf: &[u8]) -> (String, usize) {
    match std::str::from_utf8(buf) {
        Ok(s) => (s.to_string(), buf.len()),
        Err(e) if e.error_len().is_none() => {
            let n = e.valid_up_to();
            (String::from_utf8_lossy(&buf[..n]).into_owned(), n)
        }
        Err(_) => (String::from_utf8_lossy(buf).into_owned(), buf.len()),
    }
}

impl PtyManager {
    pub fn spawn<R: Runtime>(&self, app: &AppHandle<R>, program: &Path, args: &[String], cwd: Option<&Path>) -> AppResult<String> {
        let pair = native_pty_system()
            .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
            .map_err(pty_err)?;

        // `.cmd`/`.bat` kunnen onder Windows niet direct in een ConPTY starten.
        let ext = program.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut cmd = if cfg!(windows) && (ext == "cmd" || ext == "bat") {
            let mut c = CommandBuilder::new("cmd.exe");
            c.arg("/C");
            c.arg(program);
            c
        } else {
            CommandBuilder::new(program)
        };
        cmd.args(args);
        cmd.env("TERM", "xterm-256color");
        if let Some(d) = cwd {
            cmd.cwd(d);
        }

        let child = pair.slave.spawn_command(cmd).map_err(pty_err)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(pty_err)?;
        let writer = pair.master.take_writer().map_err(pty_err)?;

        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut b = self.backlogs.lock().unwrap_or_else(|e| e.into_inner());
            if b.len() >= BACKLOG_SESSIONS {
                b.clear(); // oude sessies zijn al lang gesloten
            }
            b.insert(id.clone(), Backlog::default());
        }
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), Session { master: pair.master, writer, child });

        let sessions = Arc::clone(&self.sessions);
        let backlogs = Arc::clone(&self.backlogs);
        let app = app.clone();
        let sid = id.clone();
        std::thread::spawn(move || {
            let mut pending: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&chunk[..n]);
                        let (text, used) = split_utf8(&pending);
                        pending.drain(..used);
                        if !text.is_empty() {
                            let end = {
                                let mut map = backlogs.lock().unwrap_or_else(|e| e.into_inner());
                                let b = map.entry(sid.clone()).or_default();
                                b.data.push_str(&text);
                                b.end += text.len();
                                if b.data.len() > BACKLOG_MAX {
                                    let mut cut = b.data.len() - BACKLOG_MAX;
                                    while !b.data.is_char_boundary(cut) {
                                        cut += 1;
                                    }
                                    b.data.drain(..cut);
                                }
                                b.end
                            };
                            let _ = app.emit("pty-data", DataEvent { id: sid.clone(), data: text, end });
                        }
                    }
                }
            }
            let code = sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&sid)
                .and_then(|mut s| s.child.wait().ok())
                .map(|s| s.exit_code());
            let _ = app.emit("pty-exit", ExitEvent { id: sid, code });
        });
        Ok(id)
    }

    fn with<T>(&self, id: &str, f: impl FnOnce(&mut Session) -> AppResult<T>) -> AppResult<T> {
        let mut map = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let s = map.get_mut(id).ok_or_else(|| AppError::NotFound(format!("terminal {id}")))?;
        f(s)
    }

    pub fn write(&self, id: &str, data: &str) -> AppResult<()> {
        self.with(id, |s| {
            s.writer.write_all(data.as_bytes()).map_err(pty_err)?;
            s.writer.flush().map_err(pty_err)
        })
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()> {
        self.with(id, |s| {
            s.master
                .resize(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 })
                .map_err(pty_err)
        })
    }

    pub fn backlog(&self, id: &str) -> Backlog {
        self.backlogs.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned().unwrap_or_default()
    }

    pub fn kill(&self, id: &str) -> AppResult<()> {
        self.backlogs.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        match self.with(id, |s| s.child.kill().map_err(pty_err)) {
            Err(AppError::NotFound(_)) => Ok(()), // al afgesloten
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_utf8;

    #[test]
    fn keeps_incomplete_multibyte_tail() {
        let bytes = "✓ ok".as_bytes(); // ✓ = 3 bytes
        let (text, used) = split_utf8(&bytes[..2]);
        assert_eq!((text.as_str(), used), ("", 0));
        let (text, used) = split_utf8(bytes);
        assert_eq!((text.as_str(), used), ("✓ ok", bytes.len()));
    }

    #[test]
    fn invalid_bytes_do_not_stall() {
        let (text, used) = split_utf8(&[0x66, 0xff, 0x6f]);
        assert_eq!(used, 3);
        assert!(text.contains('f') && text.contains('o'));
    }
}

#[cfg(all(test, unix))]
mod pty_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri::Listener;

    #[test]
    fn runs_a_real_process_end_to_end() {
        let app = tauri::test::mock_app();
        let (tx, rx) = mpsc::channel::<String>();
        let tx2 = tx.clone();
        app.listen("pty-data", move |e| {
            let _ = tx.send(format!("data:{}", e.payload()));
        });
        app.listen("pty-exit", move |e| {
            let _ = tx2.send(format!("exit:{}", e.payload()));
        });

        let mgr = PtyManager::default();
        let id = mgr
            .spawn(
                app.handle(),
                Path::new("/bin/sh"),
                &["-c".into(), "echo hello-✓; read x; echo got:$x; exit 3".into()],
                None,
            )
            .unwrap();

        let mut seen = String::new();
        let mut wrote = false;
        let mut exit = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while exit.is_none() && std::time::Instant::now() < deadline {
            if let Ok(m) = rx.recv_timeout(Duration::from_millis(200)) {
                if let Some(rest) = m.strip_prefix("exit:") {
                    exit = Some(rest.to_string());
                } else {
                    seen.push_str(&m);
                }
            }
            if !wrote && seen.contains("hello-") {
                mgr.write(&id, "typed\n").unwrap();
                mgr.resize(&id, 100, 30).unwrap();
                wrote = true;
            }
        }
        assert!(seen.contains("hello-✓"), "output was: {seen}");
        assert!(seen.contains("got:typed"), "input should reach the process: {seen}");
        assert!(exit.expect("process should exit").contains("\"code\":3"));
        // Na afloop is de backlog nog op te halen voor een terminal die te laat komt.
        let b = mgr.backlog(&id);
        assert!(b.data.contains("hello-✓") && b.end >= b.data.len());
        assert!(matches!(mgr.write(&id, "x"), Err(AppError::NotFound(_))));
    }
}
