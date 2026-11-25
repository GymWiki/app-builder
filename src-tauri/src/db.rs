//! SQLite-opslag (rusqlite, gebundeld). Schema-versies via `PRAGMA user_version`.

use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

const MIGRATIONS: &[&str] = &[
    // v1 — basisschema
    r#"
    CREATE TABLE projects (
        id            TEXT PRIMARY KEY,
        name          TEXT NOT NULL,
        slug          TEXT NOT NULL,
        path          TEXT NOT NULL UNIQUE,
        template      TEXT NOT NULL DEFAULT 'blank',
        description   TEXT NOT NULL DEFAULT '',
        status        TEXT NOT NULL DEFAULT 'draft'
                      CHECK (status IN ('draft','testing','ready','published')),
        version       TEXT NOT NULL DEFAULT '1.0.0',
        build_number  INTEGER NOT NULL DEFAULT 1,
        bundle_id     TEXT NOT NULL DEFAULT '',
        created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        last_opened_at TEXT
    );
    CREATE INDEX idx_projects_updated ON projects(updated_at DESC);

    CREATE TABLE sessions (
        id                TEXT PRIMARY KEY,
        project_id        TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        engine            TEXT NOT NULL DEFAULT 'claude-code',
        engine_session_id TEXT,
        title             TEXT NOT NULL DEFAULT '',
        created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
    CREATE INDEX idx_sessions_project ON sessions(project_id, updated_at DESC);

    CREATE TABLE snapshots (
        id          TEXT PRIMARY KEY,
        project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        git_commit  TEXT NOT NULL,
        label       TEXT NOT NULL DEFAULT '',
        kind        TEXT NOT NULL DEFAULT 'auto' CHECK (kind IN ('auto','manual','publish')),
        version     TEXT NOT NULL DEFAULT '',
        created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
    CREATE INDEX idx_snapshots_project ON snapshots(project_id, created_at DESC);

    CREATE TABLE checklist_items (
        id          TEXT PRIMARY KEY,
        project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        key         TEXT NOT NULL,
        blocking    INTEGER NOT NULL DEFAULT 1,
        status      TEXT NOT NULL DEFAULT 'todo' CHECK (status IN ('todo','done','failed','skipped')),
        detail      TEXT NOT NULL DEFAULT '',
        code_commit TEXT,
        updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        UNIQUE (project_id, key)
    );

    CREATE TABLE settings (
        key        TEXT PRIMARY KEY,
        value      TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
    "#,
    // v2 — chatberichten per sessie
    r#"
    CREATE TABLE messages (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        kind       TEXT NOT NULL,
        data       TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
    CREATE INDEX idx_messages_session ON messages(session_id, id);
    "#,
    // v3 — adviespunten voor bestaande projecten
    r#"
    INSERT OR IGNORE INTO checklist_items(id, project_id, key, blocking)
    SELECT lower(hex(randomblob(16))), p.id, k.key, 0
    FROM projects p, (SELECT 'device_test' AS key UNION SELECT 'native_manual') k;
    "#,
];

/// De verplichte (blokkerende) checklistpunten. Volgorde = weergavevolgorde.
pub const CHECKLIST_KEYS: [&str; 8] = [
    "app_basics",
    "data_auth",
    "legal",
    "ai_test_run",
    "store_listing",
    "accounts_signing",
    "version_changelog",
    "final_confirmation",
];
/// Adviespunten: blokkeren het publiceren niet.
pub const ADVISORY_KEYS: [&str; 2] = ["device_test", "native_manual"];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub path: String,
    pub template: String,
    pub description: String,
    pub status: String,
    pub version: String,
    pub build_number: i64,
    pub bundle_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_opened_at: Option<String>,
    /// Aantal afgeronde verplichte checklistpunten (voor "X/8").
    pub checklist_done: i64,
    pub checklist_total: i64,
}

#[derive(Debug, Clone)]
pub struct NewProject {
    pub name: String,
    pub slug: String,
    pub path: String,
    pub template: String,
    pub description: String,
    pub bundle_id: String,
    pub initial_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub engine_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub commit: String,
    pub label: String,
    pub kind: String,
    pub version: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub key: String,
    pub blocking: bool,
    pub status: String,
    pub detail: String,
}

pub struct Db {
    conn: Mutex<Connection>,
}

const PROJECT_SELECT: &str = "SELECT p.id, p.name, p.slug, p.path, p.template, p.description, p.status,
        p.version, p.build_number, p.bundle_id, p.created_at, p.updated_at, p.last_opened_at,
        (SELECT COUNT(*) FROM checklist_items c WHERE c.project_id = p.id AND c.blocking = 1 AND c.status = 'done'),
        (SELECT COUNT(*) FROM checklist_items c WHERE c.project_id = p.id AND c.blocking = 1)
    FROM projects p";

fn row_to_project(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        slug: r.get(2)?,
        path: r.get(3)?,
        template: r.get(4)?,
        description: r.get(5)?,
        status: r.get(6)?,
        version: r.get(7)?,
        build_number: r.get(8)?,
        bundle_id: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        last_opened_at: r.get(12)?,
        checklist_done: r.get(13)?,
        checklist_total: r.get(14)?,
    })
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // Een vergiftigde mutex betekent alleen dat een eerdere thread paniekte;
        // de verbinding zelf is nog bruikbaar.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ── instellingen ──
    pub fn setting_get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self
            .lock()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn setting_set(&self, key: &str, value: &str) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn settings_all(&self) -> AppResult<Vec<(String, String)>> {
        let conn = self.lock();
        let mut st = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ── projecten ──
    pub fn project_insert(&self, p: &NewProject) -> AppResult<Project> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO projects(id, name, slug, path, template, description, bundle_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, p.name, p.slug, p.path, p.template, p.description, p.bundle_id],
        )?;
        for key in CHECKLIST_KEYS {
            tx.execute(
                "INSERT INTO checklist_items(id, project_id, key, blocking) VALUES (?1, ?2, ?3, 1)",
                params![uuid::Uuid::new_v4().to_string(), id, key],
            )?;
        }
        if let Some(commit) = &p.initial_commit {
            tx.execute(
                "INSERT INTO snapshots(id, project_id, git_commit, label, kind, version)
                 VALUES (?1, ?2, ?3, 'Start', 'auto', '1.0.0')",
                params![uuid::Uuid::new_v4().to_string(), id, commit],
            )?;
        }
        for key in ADVISORY_KEYS {
            tx.execute(
                "INSERT INTO checklist_items(id, project_id, key, blocking) VALUES (?1, ?2, ?3, 0)",
                params![uuid::Uuid::new_v4().to_string(), id, key],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.project_get(&id)
    }

    pub fn project_get(&self, id: &str) -> AppResult<Project> {
        self.lock()
            .query_row(&format!("{PROJECT_SELECT} WHERE p.id = ?1"), [id], row_to_project)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("project {id}")))
    }

    pub fn projects_list(&self, query: Option<&str>) -> AppResult<Vec<Project>> {
        let conn = self.lock();
        let like = format!("%{}%", query.unwrap_or("").trim().replace('%', "").replace('_', "\\_"));
        let mut st = conn.prepare(&format!(
            "{PROJECT_SELECT} WHERE p.name LIKE ?1 ESCAPE '\\' OR p.description LIKE ?1 ESCAPE '\\'
             ORDER BY p.updated_at DESC"
        ))?;
        let rows = st.query_map([like], row_to_project)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn project_touch_opened(&self, id: &str) -> AppResult<()> {
        let n = self.lock().execute(
            "UPDATE projects SET last_opened_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            [id],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("project {id}")));
        }
        Ok(())
    }

    pub fn project_rename(&self, id: &str, name: &str) -> AppResult<Project> {
        let n = self.lock().execute(
            "UPDATE projects SET name = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![id, name],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("project {id}")));
        }
        self.project_get(id)
    }

    pub fn project_delete_record(&self, id: &str) -> AppResult<()> {
        self.lock().execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn project_set_version(&self, id: &str, version: &str, build: i64) -> AppResult<()> {
        self.lock().execute(
            "UPDATE projects SET version = ?2, build_number = ?3,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![id, version, build],
        )?;
        Ok(())
    }

    pub fn project_touch_updated(&self, id: &str) -> AppResult<()> {
        self.lock().execute(
            "UPDATE projects SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    // ── sessies en berichten ──
    /// De actieve sessie is de nieuwste van het project.
    pub fn session_active(&self, project_id: &str) -> AppResult<Option<Session>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id, engine_session_id FROM sessions WHERE project_id = ?1
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [project_id],
                |r| Ok(Session { id: r.get(0)?, engine_session_id: r.get(1)? }),
            )
            .optional()?)
    }

    pub fn session_create(&self, project_id: &str, engine: &str) -> AppResult<Session> {
        let id = uuid::Uuid::new_v4().to_string();
        self.lock().execute(
            "INSERT INTO sessions(id, project_id, engine) VALUES (?1, ?2, ?3)",
            params![id, project_id, engine],
        )?;
        Ok(Session { id, engine_session_id: None })
    }

    pub fn session_set_engine_id(&self, session_id: &str, engine_session_id: &str) -> AppResult<()> {
        self.lock().execute(
            "UPDATE sessions SET engine_session_id = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![session_id, engine_session_id],
        )?;
        Ok(())
    }

    pub fn session_set_title_if_empty(&self, session_id: &str, title: &str) -> AppResult<()> {
        self.lock()
            .execute("UPDATE sessions SET title = ?2 WHERE id = ?1 AND title = ''", params![session_id, title])?;
        Ok(())
    }

    pub fn message_insert(&self, session_id: &str, kind: &str, data: &str) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO messages(session_id, kind, data) VALUES (?1, ?2, ?3)",
            params![session_id, kind, data],
        )?;
        Ok(())
    }

    /// Opgeslagen gebeurtenissen (JSON) van de actieve sessie, oudste eerst.
    pub fn messages_active(&self, project_id: &str) -> AppResult<Vec<String>> {
        let Some(s) = self.session_active(project_id)? else { return Ok(vec![]) };
        let conn = self.lock();
        let mut st = conn.prepare("SELECT data FROM messages WHERE session_id = ?1 ORDER BY id")?;
        let rows = st.query_map([s.id], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ── snapshots en checklist ──
    pub fn snapshot_insert(&self, project_id: &str, commit: &str, label: &str, kind: &str, version: &str) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO snapshots(id, project_id, git_commit, label, kind, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![uuid::Uuid::new_v4().to_string(), project_id, commit, label, kind, version],
        )?;
        Ok(())
    }

    pub fn snapshots_list(&self, project_id: &str) -> AppResult<Vec<Snapshot>> {
        let conn = self.lock();
        let mut st = conn.prepare(
            "SELECT id, git_commit, label, kind, version, created_at FROM snapshots WHERE project_id = ?1 ORDER BY created_at DESC, rowid DESC",
        )?;
        let rows = st.query_map([project_id], |r| {
            Ok(Snapshot { id: r.get(0)?, commit: r.get(1)?, label: r.get(2)?, kind: r.get(3)?, version: r.get(4)?, created_at: r.get(5)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn project_set_bundle(&self, id: &str, bundle_id: &str) -> AppResult<()> {
        self.lock().execute(
            "UPDATE projects SET bundle_id = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![id, bundle_id],
        )?;
        Ok(())
    }

    pub fn project_set_status(&self, id: &str, status: &str) -> AppResult<()> {
        self.lock().execute("UPDATE projects SET status = ?2 WHERE id = ?1", params![id, status])?;
        Ok(())
    }

    pub fn checklist_list(&self, project_id: &str) -> AppResult<Vec<ChecklistItem>> {
        let conn = self.lock();
        let mut st = conn.prepare(
            "SELECT key, blocking, status, detail FROM checklist_items WHERE project_id = ?1 ORDER BY rowid",
        )?;
        let rows = st.query_map([project_id], |r| {
            Ok(ChecklistItem { key: r.get(0)?, blocking: r.get::<_, i64>(1)? != 0, status: r.get(2)?, detail: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn checklist_update(&self, project_id: &str, key: &str, status: &str, detail: &str, code_commit: Option<&str>) -> AppResult<()> {
        self.lock().execute(
            "UPDATE checklist_items SET status = ?3, detail = ?4, code_commit = COALESCE(?5, code_commit),
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE project_id = ?1 AND key = ?2",
            params![project_id, key, status, detail, code_commit],
        )?;
        Ok(())
    }

    pub fn project_exists_at(&self, path: &str) -> AppResult<bool> {
        Ok(self
            .lock()
            .query_row("SELECT 1 FROM projects WHERE path = ?1", [path], |_| Ok(()))
            .optional()?
            .is_some())
    }
}

fn migrate(conn: &mut Connection) -> AppResult<()> {
    let current: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > MIGRATIONS.len() {
        return Err(AppError::Invalid(format!(
            "database schema v{current} is newer than this version of Studio supports (v{})",
            MIGRATIONS.len()
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str, path: &str) -> NewProject {
        NewProject {
            name: name.into(),
            slug: "slug".into(),
            path: path.into(),
            template: "blank".into(),
            description: "d".into(),
            bundle_id: "com.example.slug".into(),
            initial_commit: Some("abc123".into()),
        }
    }

    #[test]
    fn migrations_create_all_tables_and_are_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let names: Vec<String> = {
            let c = db.lock();
            let mut st = c.prepare("SELECT name FROM sqlite_master WHERE type='table'").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
        };
        for t in ["projects", "sessions", "snapshots", "checklist_items", "settings"] {
            assert!(names.contains(&t.to_string()), "missing table {t}");
        }
        migrate(&mut db.lock()).unwrap(); // tweede keer: geen fout
    }

    #[test]
    fn insert_seeds_checklist_and_snapshot() {
        let db = Db::open_in_memory().unwrap();
        let p = db.project_insert(&sample("Mijn app", "/tmp/a")).unwrap();
        assert_eq!(p.checklist_total, 8);
        assert_eq!(p.checklist_done, 0);
        assert_eq!(p.status, "draft");
        let n: i64 = db
            .lock()
            .query_row("SELECT COUNT(*) FROM snapshots WHERE project_id = ?1", [&p.id], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn list_search_and_unique_path() {
        let db = Db::open_in_memory().unwrap();
        db.project_insert(&sample("Scorebord", "/tmp/a")).unwrap();
        db.project_insert(&sample("Tracker", "/tmp/b")).unwrap();
        assert_eq!(db.projects_list(None).unwrap().len(), 2);
        assert_eq!(db.projects_list(Some("score")).unwrap().len(), 1);
        assert_eq!(db.projects_list(Some("%")).unwrap().len(), 2); // % wordt genegeerd
        assert!(db.project_insert(&sample("Dubbel", "/tmp/a")).is_err());
    }

    #[test]
    fn delete_cascades() {
        let db = Db::open_in_memory().unwrap();
        let p = db.project_insert(&sample("X", "/tmp/a")).unwrap();
        db.project_delete_record(&p.id).unwrap();
        let n: i64 = db.lock().query_row("SELECT COUNT(*) FROM checklist_items", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert!(matches!(db.project_get(&p.id), Err(AppError::NotFound(_))));
    }

    #[test]
    fn sessions_and_messages() {
        let db = Db::open_in_memory().unwrap();
        let p = db.project_insert(&sample("X", "/tmp/a")).unwrap();
        assert!(db.session_active(&p.id).unwrap().is_none());
        let s1 = db.session_create(&p.id, "claude-code").unwrap();
        db.session_set_engine_id(&s1.id, "eng-1").unwrap();
        db.message_insert(&s1.id, "user", r#"{"type":"user","text":"a"}"#).unwrap();
        db.message_insert(&s1.id, "text", r#"{"type":"text"}"#).unwrap();
        assert_eq!(db.messages_active(&p.id).unwrap().len(), 2);
        assert_eq!(db.session_active(&p.id).unwrap().unwrap().engine_session_id.as_deref(), Some("eng-1"));
        // een nieuw gesprek wordt de actieve sessie en begint leeg
        let s2 = db.session_create(&p.id, "claude-code").unwrap();
        assert_eq!(db.session_active(&p.id).unwrap().unwrap().id, s2.id);
        assert!(db.messages_active(&p.id).unwrap().is_empty());
        db.session_set_title_if_empty(&s2.id, "eerste").unwrap();
        db.session_set_title_if_empty(&s2.id, "tweede").unwrap();
    }

    #[test]
    fn snapshots_bundle_status_and_checklist_updates() {
        let db = Db::open_in_memory().unwrap();
        let p = db.project_insert(&sample("X", "/tmp/a")).unwrap();
        db.snapshot_insert(&p.id, "c2", "Tweede", "manual", "1.0.0").unwrap();
        let list = db.snapshots_list(&p.id).unwrap();
        assert_eq!(list.len(), 2, "creation snapshot + manual");
        assert_eq!(list[0].label, "Tweede");
        db.project_set_bundle(&p.id, "nl.x.y").unwrap();
        db.project_set_status(&p.id, "ready").unwrap();
        let p = db.project_get(&p.id).unwrap();
        assert_eq!((p.bundle_id.as_str(), p.status.as_str()), ("nl.x.y", "ready"));
        assert!(db.project_set_status(&p.id, "weird").is_err(), "status is constrained by the schema");
        db.checklist_update(&p.id, "legal", "done", "[]", None).unwrap();
        db.checklist_update(&p.id, "ai_test_run", "done", "[]", Some("fp1")).unwrap();
        assert_eq!(db.project_get(&p.id).unwrap().checklist_done, 2);
    }

    #[test]
    fn version_and_checklist() {
        let db = Db::open_in_memory().unwrap();
        let p = db.project_insert(&sample("X", "/tmp/a")).unwrap();
        db.project_set_version(&p.id, "1.2.0", 7).unwrap();
        let p = db.project_get(&p.id).unwrap();
        assert_eq!((p.version.as_str(), p.build_number), ("1.2.0", 7));
        let items = db.checklist_list(&p.id).unwrap();
        assert_eq!(items.len(), 10);
        assert_eq!(items.iter().filter(|i| i.blocking).count(), 8);
        assert_eq!(items[0].key, "app_basics");
    }

    #[test]
    fn settings_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.setting_get("k").unwrap(), None);
        db.setting_set("k", "1").unwrap();
        db.setting_set("k", "2").unwrap();
        assert_eq!(db.setting_get("k").unwrap().as_deref(), Some("2"));
    }
}
