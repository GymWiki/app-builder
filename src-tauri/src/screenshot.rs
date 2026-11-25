//! Schermafdruk van de live preview voor `preview_screenshot`: een kortstondige headless
//! Edge/Chrome (die staat bijna altijd al op de computer) laadt de preview-URL en maakt een PNG.
//! Eigen tijdelijk profiel, dus de gewone browser van de gebruiker wordt niet verstoord.

use crate::error::{AppError, AppResult};
use crate::proc;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn candidates() -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    if let Ok(p) = std::env::var("STUDIO_BROWSER") {
        c.push(PathBuf::from(p));
    }
    if cfg!(windows) {
        for base in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Ok(b) = std::env::var(base) {
                for rel in ["Microsoft\\Edge\\Application\\msedge.exe", "Google\\Chrome\\Application\\chrome.exe"] {
                    c.push(PathBuf::from(&b).join(rel));
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        for p in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ] {
            c.push(p.into());
        }
    } else {
        for n in ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "microsoft-edge"] {
            if let Some(p) = proc::find(n) {
                c.push(p);
            }
        }
    }
    c
}

pub fn find_browser() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

pub fn chrome_args(url: &str, out: &Path, profile: &Path, w: u32, h: u32) -> Vec<String> {
    let mut a = vec![
        "--headless=new".into(),
        "--disable-gpu".into(),
        "--hide-scrollbars".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        format!("--user-data-dir={}", profile.display()),
        format!("--window-size={w},{h}"),
        // Flutter web start asynchroon op; geef de pagina tijd om te tekenen.
        "--virtual-time-budget=12000".into(),
        format!("--screenshot={}", out.display()),
    ];
    // Als root (bv. in een container) weigert Chrome zonder deze vlag te starten.
    if std::env::var("STUDIO_BROWSER_NO_SANDBOX").is_ok() {
        a.push("--no-sandbox".into());
    }
    a.push(url.into());
    a
}

/// Maakt een PNG van `url`; geeft de bytes terug.
pub fn capture(url: &str, w: u32, h: u32, work_dir: &Path) -> AppResult<Vec<u8>> {
    if !(url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")) {
        return Err(AppError::Unsafe("only the local preview can be captured".into()));
    }
    let browser = find_browser().ok_or_else(|| AppError::ToolMissing("browser".into()))?;
    std::fs::create_dir_all(work_dir)?;
    let id = uuid::Uuid::new_v4();
    let out = work_dir.join(format!("shot-{id}.png"));
    let profile = work_dir.join(format!("profile-{id}"));
    let args = chrome_args(url, &out, &profile, w.clamp(200, 1600), h.clamp(300, 2400));
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let res = proc::run(&browser, &argv, None, Duration::from_secs(60));
    let _ = std::fs::remove_dir_all(&profile);
    res?;
    let bytes = std::fs::read(&out).map_err(|_| AppError::CommandFailed("the browser did not produce a screenshot".into()));
    let _ = std::fs::remove_file(&out);
    bytes
}

/// Minimale base64 (RFC 4648) voor MCP-afbeeldingen; voorkomt een extra dependency.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_vectors() {
        for (i, o) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(i.as_bytes()), o);
        }
    }

    #[test]
    fn only_local_urls_are_captured() {
        let r = capture("https://evil.example.com", 390, 844, &std::env::temp_dir());
        assert!(matches!(r, Err(AppError::Unsafe(_))));
    }

    #[test]
    fn args_contain_isolated_profile_and_size() {
        let a = chrome_args("http://127.0.0.1:1", Path::new("/o.png"), Path::new("/p"), 390, 844);
        assert!(a.contains(&"--user-data-dir=/p".to_string()) && a.contains(&"--window-size=390,844".to_string()));
        assert_eq!(a.last().unwrap(), "http://127.0.0.1:1");
    }

    /// Echte Chromium nodig (bv. die van Playwright): `STUDIO_BROWSER=... cargo test real_browser -- --ignored`
    #[test]
    #[ignore]
    fn real_browser_screenshot() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let body = "<html><body style='background:#4f46e5;color:white;font:40px sans-serif'>Studio preview</body></html>";
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        let png = capture(&format!("http://127.0.0.1:{port}"), 390, 844, &std::env::temp_dir().join("studio-shots")).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']) && png.len() > 1000, "len {}", png.len());
    }
}
