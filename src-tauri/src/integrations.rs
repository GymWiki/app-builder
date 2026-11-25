//! Koppeling met de eigen backend van de gebruiker. Claude krijgt via een extra MCP-server toegang
//! om tabellen en regels op te zetten. Supabase gebruikt een persoonlijk toegangstoken uit de
//! keychain; Firebase gebruikt de eigen Firebase-CLI-login van de gebruiker (Studio ziet geen sleutels).
//! Elke aanroep van deze tools wordt in de chat bevestigd (ze staan niet in de vaste toestemmingslijst).

use crate::meta::Meta;
use serde_json::{json, Value};
use std::path::Path;

pub const SUPABASE_PREFIX: &str = "https://mcp.supabase.com/mcp";

/// Zuivere opbouw (testbaar): welke extra servers horen bij deze configuratie?
pub fn servers_for(meta: &Meta, supabase_token: Option<&str>, project_dir: &Path) -> Value {
    let mut out = json!({});
    match meta.data.provider.as_str() {
        "supabase" if !meta.data.supabase_project_ref.is_empty() => {
            if let Some(t) = supabase_token {
                out["supabase"] = json!({
                    "type": "http",
                    "url": format!("{SUPABASE_PREFIX}?project_ref={}", meta.data.supabase_project_ref),
                    "headers": { "Authorization": format!("Bearer {t}") }
                });
            }
        }
        "firebase" => {
            out["firebase"] = json!({
                "command": "npx",
                "args": ["-y", "firebase-tools@latest", "mcp", "--dir", project_dir.to_string_lossy()]
            });
        }
        _ => {}
    }
    out
}

pub fn extra_servers(project_id: &str, project_dir: &Path) -> Value {
    let meta = crate::meta::load(project_dir).unwrap_or_default();
    let token = crate::keychain::get(project_id, "supabase_token").ok().flatten();
    servers_for(&meta, token.as_deref(), project_dir)
}

/// Prompt voor "laat Claude dit doen" bij Data & Auth (Engels voor Claude, met de gemaakte keuzes).
pub fn setup_prompt(meta: &Meta) -> String {
    let provider = match meta.data.provider.as_str() {
        "supabase" => "Supabase (use the `supabase` MCP server to create tables and row level security policies; ask me before destructive changes)",
        "firebase" => "Firebase (use the `firebase` MCP server and firebase_core / the FlutterFire packages; ask me before changing security rules)",
        "local" => "a local SQLite database on the phone (no account needed; use the `sqflite` or `drift` package)",
        _ => "no backend",
    };
    let auth = if meta.data.auth_methods.is_empty() { "no login".to_string() } else { format!("login with: {}", meta.data.auth_methods.join(", ")) };
    format!(
        "Set up the data layer for this app with {provider} and {auth}. Look at the screens first, then design the tables and rules the app needs, \
         wire the packages into the app, add a working \"delete my account\" function if there are accounts, and record the choices in STUDIO.md. \
         Never put keys or passwords in project files; use the Studio tools or ask me."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(provider: &str, reference: &str) -> Meta {
        let mut m = Meta::default();
        m.data.provider = provider.into();
        m.data.supabase_project_ref = reference.into();
        m
    }

    #[test]
    fn supabase_needs_a_reference_and_a_token() {
        let d = Path::new("/p");
        assert!(servers_for(&meta("supabase", "abcdefghij"), None, d).as_object().unwrap().is_empty());
        assert!(servers_for(&meta("supabase", ""), Some("sbp_x"), d).as_object().unwrap().is_empty());
        let s = servers_for(&meta("supabase", "abcdefghij"), Some("sbp_secret"), d);
        assert_eq!(s["supabase"]["url"], "https://mcp.supabase.com/mcp?project_ref=abcdefghij");
        assert_eq!(s["supabase"]["headers"]["Authorization"], "Bearer sbp_secret");
    }

    #[test]
    fn firebase_uses_the_users_own_cli_login() {
        let s = servers_for(&meta("firebase", ""), None, Path::new("/my/app"));
        assert_eq!(s["firebase"]["command"], "npx");
        assert!(s["firebase"]["args"].as_array().unwrap().iter().any(|a| a == "/my/app"));
        assert!(!s.to_string().contains("Bearer"));
    }

    #[test]
    fn local_or_no_backend_adds_nothing() {
        for p in ["local", "none", ""] {
            assert!(servers_for(&meta(p, ""), Some("t"), Path::new("/p")).as_object().unwrap().is_empty());
        }
    }

    #[test]
    fn extra_servers_can_never_replace_studio() {
        let info = crate::mcp::McpInfo { port: 1234, token: "tok".into() };
        let cfg = crate::mcp::config_json_with(&info, "pid", &json!({"studio": {"url": "http://evil"}, "supabase": {"type": "http"}}));
        assert!(cfg["mcpServers"]["studio"]["url"].as_str().unwrap().starts_with("http://127.0.0.1:1234/"));
        assert!(cfg["mcpServers"]["supabase"].is_object());
    }

    #[test]
    fn prompt_contains_choices_but_no_secrets() {
        let mut m = meta("supabase", "abcdefghij");
        m.data.auth_methods = vec!["email".into(), "google".into()];
        let p = setup_prompt(&m);
        assert!(p.contains("Supabase") && p.contains("email, google") && p.contains("delete my account"));
        assert!(!p.contains("abcdefghij") || !p.contains("sbp_"));
    }
}
