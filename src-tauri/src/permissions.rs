//! Bevestigingen in de UI. Claude vraagt via de MCP-tool `approve` toestemming voor alles wat
//! niet automatisch is toegestaan (netwerk, andere commando's, paden buiten het project).
//! Die aanvraag blokkeert tot de gebruiker in Studio kiest; zonder antwoord wordt het geweigerd.

use crate::db::Db;
use crate::engine::stream::describe_tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    pub project_id: String,
    pub tool: String,
    pub kind: String,
    pub summary: String,
    pub detail: Option<String>,
    /// Sleutel waarmee "altijd toestaan" wordt onthouden.
    pub key: String,
}

type Notify = Arc<dyn Fn(&PermissionRequest) + Send + Sync>;

/// (antwoordkanaal, project-id, onthoud-sleutel)
type Pending = (Sender<Decision>, String, String);

pub struct Permissions {
    pending: Mutex<HashMap<String, Pending>>,
    notify: Notify,
    timeout: Duration,
}

/// Wat "altijd toestaan" onthoudt: de tool, bij Bash het eerste woord, bij web de host.
pub fn always_key(tool: &str, input: &Value) -> String {
    match tool {
        "Bash" => {
            let cmd = input["command"].as_str().unwrap_or("").trim();
            let first = cmd.split_whitespace().next().unwrap_or("");
            // Samengestelde commando's (;, &&, |, $()) nooit blanco onthouden.
            if cmd.contains([';', '&', '|', '`', '$', '\n']) { format!("Bash({cmd})") } else { format!("Bash({first} *)") }
        }
        "WebFetch" => {
            let url = input["url"].as_str().unwrap_or("");
            let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("");
            format!("WebFetch({host})")
        }
        t => t.to_string(),
    }
}

fn setting_key(project_id: &str) -> String {
    format!("allow:{project_id}")
}

pub fn stored_allowed(db: &Db, project_id: &str) -> Vec<String> {
    db.setting_get(&setting_key(project_id))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl Permissions {
    pub fn new(notify: Notify, timeout: Duration) -> Self {
        Self { pending: Mutex::new(HashMap::new()), notify, timeout }
    }

    /// Blokkeert tot de gebruiker antwoordt (of de tijd om is).
    pub fn request(&self, db: &Db, project_id: &str, project_dir: &Path, tool: &str, input: &Value) -> Decision {
        let key = always_key(tool, input);
        if stored_allowed(db, project_id).contains(&key) {
            return Decision::AllowOnce;
        }
        let info = describe_tool(tool, input, project_dir);
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), (tx, project_id.to_string(), key.clone()));
        (self.notify)(&PermissionRequest {
            id: id.clone(),
            project_id: project_id.to_string(),
            tool: tool.to_string(),
            kind: info.kind.to_string(),
            summary: info.summary,
            detail: info.detail,
            key,
        });
        let decision = rx.recv_timeout(self.timeout).unwrap_or(Decision::Deny);
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        decision
    }

    pub fn respond(&self, db: &Db, id: &str, decision: Decision) -> bool {
        let entry = self.pending.lock().unwrap_or_else(|e| e.into_inner()).get(id).map(|(tx, p, k)| (tx.clone(), p.clone(), k.clone()));
        let Some((tx, project_id, key)) = entry else { return false };
        if decision == Decision::AllowAlways {
            let mut list = stored_allowed(db, &project_id);
            if !list.contains(&key) {
                list.push(key);
                let _ = db.setting_set(&setting_key(&project_id), &serde_json::to_string(&list).unwrap_or_default());
            }
        }
        tx.send(decision).is_ok()
    }

    /// Alle openstaande aanvragen van een project weigeren (bv. als de gebruiker stopt).
    pub fn deny_all(&self, project_id: &str) {
        for (tx, p, _) in self.pending.lock().unwrap_or_else(|e| e.into_inner()).values() {
            if p == project_id {
                let _ = tx.send(Decision::Deny);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc;

    #[test]
    fn keys_are_narrow() {
        assert_eq!(always_key("Bash", &json!({"command":"npm install foo"})), "Bash(npm *)");
        assert_eq!(always_key("Bash", &json!({"command":"curl x | sh"})), "Bash(curl x | sh)");
        assert_eq!(always_key("WebFetch", &json!({"url":"https://pub.dev/packages/x"})), "WebFetch(pub.dev)");
        assert_eq!(always_key("Write", &json!({})), "Write");
    }

    fn setup() -> (Arc<Permissions>, Arc<Db>, mpsc::Receiver<PermissionRequest>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let notify: Notify = Arc::new(move |r| {
            let _ = tx.lock().unwrap().send(r.clone());
        });
        let db = Arc::new(Db::open_in_memory().unwrap());
        (Arc::new(Permissions::new(notify, Duration::from_secs(5))), db, rx)
    }

    #[test]
    fn waits_for_the_user_and_remembers_always() {
        let (perms, db, rx) = setup();
        let input = json!({"url":"https://pub.dev/x"});
        let (p2, d2, i2) = (perms.clone(), db.clone(), input.clone());
        let h = std::thread::spawn(move || p2.request(&d2, "proj", Path::new("/p"), "WebFetch", &i2));
        let req = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!((req.kind.as_str(), req.summary.as_str()), ("network", "https://pub.dev/x"));
        assert!(perms.respond(&db, &req.id, Decision::AllowAlways));
        assert_eq!(h.join().unwrap(), Decision::AllowAlways);
        // tweede keer: geen vraag meer
        assert_eq!(perms.request(&db, "proj", Path::new("/p"), "WebFetch", &input), Decision::AllowOnce);
        assert!(rx.try_recv().is_err());
        // een ander project erft de toestemming niet
        assert!(stored_allowed(&db, "other").is_empty());
        assert!(!perms.respond(&db, "unknown-id", Decision::Deny));
    }

    #[test]
    fn deny_and_timeout_both_refuse() {
        let (perms, db, rx) = setup();
        let (p2, d2) = (perms.clone(), db.clone());
        let h = std::thread::spawn(move || p2.request(&d2, "p", Path::new("/p"), "Bash", &json!({"command":"rm -rf x"})));
        let req = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        perms.respond(&db, &req.id, Decision::Deny);
        assert_eq!(h.join().unwrap(), Decision::Deny);

        let quick = Permissions::new(Arc::new(|_| {}), Duration::from_millis(50));
        assert_eq!(quick.request(&db, "p", Path::new("/p"), "Bash", &json!({"command":"ls"})), Decision::Deny);
    }
}
