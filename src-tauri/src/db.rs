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
];

/// De 8 verplichte checklistpunten (zie fase 5). Volgorde = weergavevolgorde.
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
    fn settings_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.setting_get("k").unwrap(), None);
        db.setting_set("k", "1").unwrap();
        db.setting_set("k", "2").unwrap();
        assert_eq!(db.setting_get("k").unwrap().as_deref(), Some("2"));
    }
}
