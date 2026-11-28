//! "Test verbinding" (fase 10c): per integratie een testfunctie uit het manifest. Echte verbindingen voor Supabase, GitHub en
//! Vercel; voor Apple, Google Play en Firebase een nette controle van de ingevulde gegevens zonder netwerk (de echte
//! aanroep vraagt een ondertekend token en volgt later). De geheime waarde gaat via stdin naar curl, nooit via de
//! opdrachtregel, en de uitkomst bevat nooit een waarde.

use crate::error::{AppError, AppResult};
use crate::manifest::{self, Manifest};
use crate::vault::Vault;
use serde::Serialize;
use std::io::Write;
use std::process::Stdio;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpResp {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub trait Http: Send + Sync {
    fn get(&self, url: &str, headers: &[(String, String)]) -> AppResult<HttpResp>;
}

pub struct CurlHttp;

fn conf_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace(['\n', '\r'], "")
}

/// Splitst `curl -i`-uitvoer in status, headers en body.
pub fn parse_response(raw: &str) -> Option<HttpResp> {
    let raw = raw.replace("\r\n", "\n");
    // Bij proxy-CONNECT of 100-continue staan er meerdere kopblokken; de laatste telt.
    let mut rest = raw.as_str();
    loop {
        let (head, body) = rest.split_once("\n\n").unwrap_or((rest, ""));
        let mut lines = head.lines();
        let first = lines.next()?;
        if !first.starts_with("HTTP/") {
            return None;
        }
        let status: u16 = first.split_whitespace().nth(1)?.parse().ok()?;
        if (status == 100 || (status == 200 && first.to_ascii_lowercase().contains("connection established"))) && body.starts_with("HTTP/") {
            rest = body;
            continue;
        }
        let headers = lines.filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))).collect();
        return Some(HttpResp { status, headers, body: body.to_string() });
    }
}

impl Http for CurlHttp {
    fn get(&self, url: &str, headers: &[(String, String)]) -> AppResult<HttpResp> {
        let curl = crate::proc::find("curl").ok_or_else(|| AppError::ToolMissing("curl".into()))?;
        let mut cmd = crate::proc::command(&curl);
        cmd.args(["-sS", "-i", "-m", "15", "--proto", "=https", "-K", "-"]).stdin(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| AppError::CommandFailed(format!("curl: {e}")))?;
        let mut conf = format!("url = \"{}\"\n", conf_escape(url));
        for (k, v) in headers {
            conf.push_str(&format!("header = \"{}: {}\"\n", conf_escape(k), conf_escape(v)));
        }
        if let Some(mut s) = child.stdin.take() {
            let _ = s.write_all(conf.as_bytes());
        }
        let out = child.wait_with_output().map_err(|e| AppError::CommandFailed(format!("curl: {e}")))?;
        if !out.status.success() {
            return Err(AppError::CommandFailed("unreachable".into()));
        }
        parse_response(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| AppError::CommandFailed("unreadable response".into()))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TestOutcome {
    pub ok: bool,
    /// Code die de UI vertaalt: ok, checked_locally, invalid_credentials, unreachable, incomplete, forbidden, ...
    pub code: String,
    /// Niet-geheime extra info (bv. de gebruikersnaam).
    pub detail: Option<String>,
    pub expires_at: Option<String>,
}

fn out(ok: bool, code: &str, detail: Option<String>, expires_at: Option<String>) -> TestOutcome {
    TestOutcome { ok, code: code.into(), detail, expires_at }
}

fn bearer(token: &str) -> (String, String) {
    ("Authorization".into(), format!("Bearer {token}"))
}

fn classify(status: u16) -> &'static str {
    match status {
        200..=299 => "ok",
        401 => "invalid_credentials",
        403 => "forbidden",
        404 => "not_found",
        429 => "rate_limited",
        _ => "unexpected",
    }
}

/// Voert de testfunctie uit het manifest uit. `http` is vervangbaar zodat de tests geen netwerk nodig hebben.
pub fn run(vault: &Vault, http: &dyn Http, project_id: &str, m: &Manifest) -> AppResult<TestOutcome> {
    let missing: Vec<&str> = m.velden.iter().filter(|f| f.verplicht && !vault.has(project_id, &m.id, &f.id)).map(|f| f.id.as_str()).collect();
    if !missing.is_empty() {
        return Ok(out(false, "incomplete", None, None));
    }
    let text = |f: &str| vault.get_text(project_id, &m.id, f);
    let reach = |r: AppResult<HttpResp>| -> Result<HttpResp, TestOutcome> { r.map_err(|_| out(false, "unreachable", None, None)) };
    let outcome = match m.test_verbinding.as_deref() {
        Some("supabase") => {
            let (url, key) = (text("url")?.unwrap_or_default(), text("publishable_key")?.unwrap_or_default());
            let base = url.trim_end_matches('/');
            // Gezondheidscheck van de inlogdienst: werkt met de publieke sleutel en laat de gegevens van de gebruiker ongemoeid.
            match reach(http.get(&format!("{base}/auth/v1/health"), &[("apikey".into(), key.clone()), bearer(&key)])) {
                Ok(r) => out(r.status < 300, classify(r.status), None, None),
                Err(o) => o,
            }
        }
        Some("github") => {
            let token = text("token")?.unwrap_or_default();
            let headers = [bearer(&token), ("Accept".into(), "application/vnd.github+json".into()), ("X-GitHub-Api-Version".into(), "2022-11-28".into()), ("User-Agent".into(), "Studio".into())];
            match reach(http.get("https://api.github.com/user", &headers)) {
                Ok(r) => {
                    let login = serde_json::from_str::<serde_json::Value>(&r.body).ok().and_then(|v| v["login"].as_str().map(String::from));
                    // GitHub meldt de einddatum van persoonlijke tokens in een antwoordheader.
                    out(r.status < 300, classify(r.status), login.filter(|_| r.status < 300), r.header("github-authentication-token-expiration").map(String::from).filter(|_| r.status < 300))
                }
                Err(o) => o,
            }
        }
        Some("vercel") => {
            let token = text("token")?.unwrap_or_default();
            match reach(http.get("https://api.vercel.com/v2/user", &[bearer(&token)])) {
                Ok(r) => {
                    let name = serde_json::from_str::<serde_json::Value>(&r.body).ok().and_then(|v| v["user"]["username"].as_str().map(String::from));
                    out(r.status < 300, classify(r.status), name.filter(|_| r.status < 300), None)
                }
                Err(o) => o,
            }
        }
        // Netjes gecontroleerd zonder netwerk: alle verplichte velden staan er en bestanden zijn nog te ontsleutelen en geldig.
        Some("apple_local") | Some("google_play_local") | Some("firebase_local") | None => {
            for f in m.velden.iter().filter(|f| f.kind == manifest::FieldKind::File) {
                if let Some(bytes) = vault.get_file(project_id, &m.id, &f.id)? {
                    let name = vault.meta(project_id, &m.id, &f.id).map(|x| x.file_name).unwrap_or_default();
                    if manifest::validate_file(f, &name, &bytes).is_err() {
                        return Ok(out(false, "file_invalid", Some(f.id.clone()), None));
                    }
                }
            }
            out(true, "checked_locally", None, None)
        }
        Some(other) => return Err(AppError::Unsupported(format!("test {other}"))),
    };
    Ok(outcome)
}

/// Voert de test uit en bewaart het resultaat (alleen code, tijd en einddatum) in de metadata.
pub fn run_and_record(vault: &Vault, http: &dyn Http, project_id: &str, m: &Manifest) -> AppResult<TestOutcome> {
    let o = run(vault, http, project_id, m)?;
    vault.db().integration_record_test(project_id, &m.id, o.ok, &o.code, o.expires_at.as_deref())?;
    for f in &m.velden {
        vault.touch(project_id, &m.id, &f.id, "test");
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::test_vault;
    use std::sync::Mutex;

    type Call = (String, Vec<(String, String)>);

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Call>>,
        reply: Mutex<Option<AppResult<HttpResp>>>,
    }
    impl Http for Fake {
        fn get(&self, url: &str, headers: &[(String, String)]) -> AppResult<HttpResp> {
            self.calls.lock().unwrap().push((url.into(), headers.to_vec()));
            self.reply.lock().unwrap().take().unwrap_or_else(|| Err(AppError::CommandFailed("unreachable".into())))
        }
    }
    fn resp(status: u16, headers: &[(&str, &str)], body: &str) -> AppResult<HttpResp> {
        Ok(HttpResp { status, headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), body: body.into() })
    }

    #[test]
    fn parses_curl_output_with_proxy_and_100_blocks() {
        let raw = "HTTP/1.1 200 Connection Established\r\n\r\nHTTP/2 200 \r\ncontent-type: application/json\r\ngithub-authentication-token-expiration: 2027-01-01 00:00:00 UTC\r\n\r\n{\"login\":\"octo\"}";
        let r = parse_response(raw).unwrap();
        assert_eq!((r.status, r.body.as_str()), (200, "{\"login\":\"octo\"}"));
        assert_eq!(r.header("GitHub-Authentication-Token-Expiration"), Some("2027-01-01 00:00:00 UTC"));
        assert!(parse_response("garbage").is_none());
        assert_eq!(parse_response("HTTP/2 401 \n\nnope").unwrap().status, 401);
    }

    #[test]
    fn github_test_reports_user_expiry_and_sends_the_token_only_as_a_header() {
        let (v, _, pid, _) = test_vault();
        let token = "github_pat_11AAAAAAAAAAAAAAAAAAAA_zzzzzzzzzz";
        v.set_text(&pid, "github", "token", token).unwrap();
        let http = Fake::default();
        *http.reply.lock().unwrap() = Some(resp(200, &[("github-authentication-token-expiration", "2027-01-01 00:00:00 UTC")], "{\"login\":\"octo\"}"));
        let o = run_and_record(&v, &http, &pid, manifest::get("github").unwrap()).unwrap();
        assert_eq!((o.ok, o.code.as_str(), o.detail.as_deref(), o.expires_at.as_deref()), (true, "ok", Some("octo"), Some("2027-01-01 00:00:00 UTC")));
        let calls = http.calls.lock().unwrap();
        assert_eq!(calls[0].0, "https://api.github.com/user");
        assert!(calls[0].1.iter().any(|(k, val)| k == "Authorization" && val.ends_with(token)));
        assert!(!calls[0].0.contains(token), "never in the url");
        assert!(!serde_json::to_string(&o).unwrap().contains("github_pat_"), "never in the outcome");
        let st = v.status_of(&pid, manifest::get("github").unwrap()).unwrap();
        assert_eq!((st.last_test_ok, st.expires_at.as_deref()), (Some(true), Some("2027-01-01 00:00:00 UTC")));
    }

    #[test]
    fn invalid_tokens_and_network_problems_become_friendly_codes() {
        let (v, _, pid, _) = test_vault();
        v.set_text(&pid, "vercel", "token", "vercel-token-abcdefghijklmnop").unwrap();
        let m = manifest::get("vercel").unwrap();
        let http = Fake::default();
        *http.reply.lock().unwrap() = Some(resp(403, &[], "{\"error\":{\"code\":\"forbidden\"}}"));
        let o = run(&v, &http, &pid, m).unwrap();
        assert_eq!((o.ok, o.code.as_str()), (false, "forbidden"));
        *http.reply.lock().unwrap() = Some(resp(401, &[], ""));
        assert_eq!(run(&v, &http, &pid, m).unwrap().code, "invalid_credentials");
        assert_eq!(run(&v, &http, &pid, m).unwrap().code, "unreachable", "no reply = unreachable");
        *http.reply.lock().unwrap() = Some(resp(200, &[], "{\"user\":{\"username\":\"jan\"}}"));
        let o = run_and_record(&v, &http, &pid, m).unwrap();
        assert_eq!(o.detail.as_deref(), Some("jan"));
        assert_eq!(classify(429), "rate_limited");
    }

    #[test]
    fn supabase_test_uses_the_public_key_against_the_auth_health_endpoint() {
        let (v, _, pid, _) = test_vault();
        let m = manifest::get("supabase").unwrap();
        let http = Fake::default();
        assert_eq!(run(&v, &http, &pid, m).unwrap().code, "incomplete", "nothing filled in yet");
        assert!(http.calls.lock().unwrap().is_empty());
        v.set_text(&pid, "supabase", "url", "https://abcdefghijklmnopqrst.supabase.co/").unwrap();
        v.set_text(&pid, "supabase", "publishable_key", "sb_publishable_abcdefghijkl").unwrap();
        *http.reply.lock().unwrap() = Some(resp(200, &[], "{\"name\":\"GoTrue\"}"));
        let o = run(&v, &http, &pid, m).unwrap();
        assert!(o.ok);
        let calls = http.calls.lock().unwrap();
        assert_eq!(calls[0].0, "https://abcdefghijklmnopqrst.supabase.co/auth/v1/health");
        assert!(calls[0].1.iter().any(|(k, val)| k == "apikey" && val == "sb_publishable_abcdefghijkl"));
        assert!(!calls[0].1.iter().any(|(_, val)| val.contains("sb_secret")), "the secret key is never used for a test");
    }

    #[test]
    fn apple_and_google_play_are_checked_locally_without_network() {
        let (v, _, pid, _) = test_vault();
        let http = Fake::default();
        let m = manifest::get("apple").unwrap();
        assert_eq!(run(&v, &http, &pid, m).unwrap().code, "incomplete");
        v.set_text(&pid, "apple", "issuer_id", "69a6de7e-1234-47e3-e053-5b8c7c11a4d1").unwrap();
        v.set_text(&pid, "apple", "key_id", "ABCDE12345").unwrap();
        v.set_text(&pid, "apple", "team_id", "TEAM123456").unwrap();
        v.set_file(&pid, "apple", "private_key", "AuthKey.p8", b"-----BEGIN PRIVATE KEY-----\nMIGTAgEAfake\n-----END PRIVATE KEY-----\n").unwrap();
        let o = run_and_record(&v, &http, &pid, m).unwrap();
        assert_eq!((o.ok, o.code.as_str()), (true, "checked_locally"));
        assert!(http.calls.lock().unwrap().is_empty(), "no network for the placeholder tests");
        assert!(matches!(run(&v, &http, &pid, &Manifest { test_verbinding: Some("nope".into()), ..m.clone() }), Err(AppError::Unsupported(_))));
    }

    #[test]
    fn config_values_are_escaped_so_a_value_cannot_inject_curl_options() {
        assert_eq!(conf_escape("a\"b\\c\nd"), "a\\\"b\\\\cd");
    }

    /// Echt netwerk: een GitHub-aanroep wordt echt uitgevoerd en gelezen en een onbestaand Supabase-project is onbereikbaar.
    #[test]
    #[ignore]
    fn real_network_failures_are_reported_as_friendly_codes() {
        let (v, _, pid, _) = test_vault();
        v.set_text(&pid, "github", "token", "ghp_abcdefghijklmnopqrstuvwxyz0123456789").unwrap();
        let o = run(&v, &CurlHttp, &pid, manifest::get("github").unwrap()).unwrap();
        println!("github with a fake token: {o:?}");
        // In sommige sandboxes zet een proxy zelf geldige inloggegevens in het verzoek; dan is 200 het echte antwoord.
        assert!(matches!(o.code.as_str(), "invalid_credentials" | "ok"), "{o:?}");
        v.set_text(&pid, "supabase", "url", "https://zzzzzzzzzzzzzzzzzzzz.supabase.co").unwrap();
        v.set_text(&pid, "supabase", "publishable_key", "sb_publishable_abcdefghijkl").unwrap();
        let o = run(&v, &CurlHttp, &pid, manifest::get("supabase").unwrap()).unwrap();
        println!("supabase non-existent project: {o:?}");
        assert!(!o.ok);
    }
}
