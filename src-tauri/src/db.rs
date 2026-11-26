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
    // v4 — versie van het projectprofiel (0 = nog geen profiel)
    r#"
    ALTER TABLE projects ADD COLUMN profile_version INTEGER NOT NULL DEFAULT 0;
    "#,
    // v5 — een gesprek per scherm: '' = het hoofdgesprek (begin van een nieuw gesprek), anders de naam van het scherm
    r#"
    ALTER TABLE sessions ADD COLUMN scope_key TEXT NOT NULL DEFAULT '';
    "#,
    // v6 — metingen per run (fase 9): alleen lokaal, nooit verstuurd
    r#"
    CREATE TABLE runs (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        level           TEXT NOT NULL,
        final_level     TEXT NOT NULL,
        screen          TEXT NOT NULL DEFAULT '',
        why             TEXT NOT NULL DEFAULT '',
        expansions      INTEGER NOT NULL DEFAULT 0,
        denied_edits    INTEGER NOT NULL DEFAULT 0,
        files_changed   INTEGER NOT NULL DEFAULT 0,
        issues_before   INTEGER NOT NULL DEFAULT 0,
        repairs         INTEGER NOT NULL DEFAULT 0,
        rolled_back     INTEGER NOT NULL DEFAULT 0,
        ok              INTEGER NOT NULL DEFAULT 1,
        duration_ms     INTEGER NOT NULL DEFAULT 0,
        cost_usd        REAL NOT NULL DEFAULT 0,
        turns           INTEGER NOT NULL DEFAULT 0,
        ai_questions    INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_runs_project ON runs(project_id, id DESC);
    "#,
    // v7 — integraties en geheimen (fase 10): ALLEEN metadata; de waarden staan in de keychain / versleutelde bestanden
    r#"
    CREATE TABLE secret_meta (
        scope       TEXT NOT NULL,
        integration TEXT NOT NULL,
        field       TEXT NOT NULL,
        kind        TEXT NOT NULL DEFAULT 'text' CHECK (kind IN ('text','file')),
        file_name   TEXT NOT NULL DEFAULT '',
        derived     TEXT NOT NULL DEFAULT '{}',
        set_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        last_used   TEXT,
        used_by     TEXT NOT NULL DEFAULT '',
        PRIMARY KEY (scope, integration, field)
    );
    CREATE TABLE integration_state (
        project_id     TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        integration    TEXT NOT NULL,
        enabled        INTEGER NOT NULL DEFAULT 0,
        last_tested    TEXT,
        last_test_ok   INTEGER,
        last_test_code TEXT,
        expires_at     TEXT,
        PRIMARY KEY (project_id, integration)
    );
    INSERT INTO checklist_items(id, project_id, key, blocking)
    SELECT lower(hex(randomblob(16))), id, 'no_leaks', 1 FROM projects;
    "#,
    // v8 — modelkeuze per run (fase 11): klasse, niveau, model, engine, doel, opschaling en tokens
    r#"
    ALTER TABLE runs ADD COLUMN class     TEXT    NOT NULL DEFAULT '';
    ALTER TABLE runs ADD COLUMN tier      TEXT    NOT NULL DEFAULT '';
    ALTER TABLE runs ADD COLUMN model     TEXT    NOT NULL DEFAULT '';
    ALTER TABLE runs ADD COLUMN engine    TEXT    NOT NULL DEFAULT 'claude-code';
    ALTER TABLE runs ADD COLUMN purpose   TEXT    NOT NULL DEFAULT 'work';
    ALTER TABLE runs ADD COLUMN escalated INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE runs ADD COLUMN tokens    INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE runs ADD COLUMN planned   INTEGER NOT NULL DEFAULT 0;
    "#,
    // v9 — ruwe tokens per soort (invoer, uitvoer, cache gelezen, cache geschreven)
    r#"
    ALTER TABLE runs ADD COLUMN input_tokens  INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE runs ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE runs ADD COLUMN cache_read_tokens  INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE runs ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0;
    "#,
    // v10 — kopie van studio/purchases.json voor de UI (geen geheimen)
    r#"
    CREATE TABLE purchases_copy (
        project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
        json       TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
    "#,
];

/// De verplichte (blokkerende) checklistpunten. Volgorde = weergavevolgorde.
pub const CHECKLIST_KEYS: [&str; 9] = [
    "app_basics",
    "data_auth",
    "legal",
    "ai_test_run",
    "store_listing",
    "accounts_signing",
    "version_changelog",
    "final_confirmation",
    "no_leaks",
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
    pub profile_version: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub engine_session_id: Option<String>,
}

/// Metadata van één geheim veld. De waarde zelf staat hier nooit.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SecretMeta {
    pub scope: String,
    pub integration: String,
    pub field: String,
    pub kind: String,
    pub file_name: String,
    /// Niet-geheime waarden uit een bestand (bv. het e-mailadres van een service account).
    pub derived: std::collections::BTreeMap<String, String>,
    pub set_at: String,
    pub last_used: Option<String>,
    pub used_by: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationState {
    pub integration: String,
    pub enabled: bool,
    pub last_tested: Option<String>,
    pub last_test_ok: Option<bool>,
    pub last_test_code: Option<String>,
    pub expires_at: Option<String>,
}

/// Eén run, zoals in de tabel `runs`.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub level: String,
    pub final_level: String,
    pub screen: String,
    pub why: String,
    pub expansions: i64,
    pub denied_edits: i64,
    pub files_changed: i64,
    pub issues_before: i64,
    pub repairs: i64,
    pub rolled_back: bool,
    pub ok: bool,
    pub duration_ms: i64,
    pub cost_usd: f64,
    pub turns: i64,
    pub ai_questions: i64,
    /// Moeilijkheidsklasse (trivial | normal | complex) en gekozen niveau/model (fase 11).
    pub class: String,
    pub tier: String,
    pub model: String,
    pub engine: String,
    /// work | helper | release | override (een override telt niet mee voor de statistiek)
    pub purpose: String,
    pub escalated: bool,
    pub tokens: i64,
    pub planned: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

/// Cijfers per klasse of per model.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroupStats {
    pub runs: usize,
    pub ok: usize,
    pub clean: usize,
    pub escalated: usize,
    pub cost_usd: f64,
    pub tokens: i64,
}

impl GroupStats {
    fn add(&mut self, r: &RunRecord) {
        self.runs += 1;
        self.ok += (r.ok && !r.rolled_back) as usize;
        self.clean += r.is_clean() as usize;
        self.escalated += r.escalated as usize;
        self.cost_usd += r.cost_usd;
        self.tokens += r.tokens;
    }
}

impl RunRecord {
    /// Meteen goed: geslaagd zonder reparatie, opschaling of terugdraaien.
    pub fn is_clean(&self) -> bool {
        self.ok && !self.rolled_back && self.issues_before == 0 && self.repairs == 0 && !self.escalated
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunsSummary {
    /// De nieuwste run zelf (voor tests en het ontwikkelaarspaneel).
    pub last: Option<RunRecord>,
    pub by_class: std::collections::BTreeMap<String, GroupStats>,
    pub by_model: std::collections::BTreeMap<String, GroupStats>,
    pub runs: usize,
    /// Aantal runs per niveau (small, screen, shared, project) aan het begin.
    pub by_level: std::collections::BTreeMap<String, usize>,
    pub expanded: usize,
    pub clean_first_time: usize,
    pub repaired: usize,
    pub rolled_back: usize,
    pub ai_questions: i64,
    pub avg_duration_ms: i64,
    pub total_cost_usd: f64,
    pub avg_files_changed: f64,
    pub denied_edits: i64,
}

impl RunsSummary {
    pub fn from_runs(rows: &[RunRecord]) -> Self {
        let mut s = RunsSummary { runs: rows.len(), last: rows.first().cloned(), ..Default::default() };
        for r in rows {
            *s.by_level.entry(r.level.clone()).or_default() += 1;
            s.expanded += (r.expansions > 0) as usize;
            s.clean_first_time += (r.issues_before == 0 && !r.rolled_back) as usize;
            s.repaired += (r.repairs > 0 && !r.rolled_back) as usize;
            s.rolled_back += r.rolled_back as usize;
            s.ai_questions += r.ai_questions;
            s.total_cost_usd += r.cost_usd;
            s.denied_edits += r.denied_edits;
            s.avg_duration_ms += r.duration_ms;
            s.avg_files_changed += r.files_changed as f64;
            if !r.class.is_empty() {
                s.by_class.entry(r.class.clone()).or_default().add(r);
            }
            if !r.model.is_empty() {
                s.by_model.entry(r.model.clone()).or_default().add(r);
            }
        }
        if !rows.is_empty() {
            s.avg_duration_ms /= rows.len() as i64;
            s.avg_files_changed /= rows.len() as f64;
        }
        s
    }
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
    pub fn purchases_store(&self, project_id: &str, json: &str) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO purchases_copy(project_id, json) VALUES (?1, ?2) ON CONFLICT(project_id) DO UPDATE SET json = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![project_id, json],
        )?;
        Ok(())
    }

    pub fn purchases_load(&self, project_id: &str) -> AppResult<Option<String>> {
        Ok(self.lock().query_row("SELECT json FROM purchases_copy WHERE project_id = ?1", params![project_id], |r| r.get(0)).optional()?)
    }

    /// De laatste run van een project (om na een Aankopen-flow te zien of hij geslaagd is).
    pub fn last_run(&self, project_id: &str) -> AppResult<Option<RunRecord>> {
        let conn = self.lock();
        let mut st = conn.prepare(&format!("SELECT {} FROM runs WHERE project_id = ?1 ORDER BY id DESC LIMIT 1", Self::RUN_COLS))?;
        let rows: Vec<RunRecord> = st.query_map(params![project_id], Self::run_row)?.collect::<Result<_, _>>()?;
        Ok(rows.into_iter().next())
    }

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
            "INSERT INTO projects(id, name, slug, path, template, description, bundle_id, profile_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, p.name, p.slug, p.path, p.template, p.description, p.bundle_id, p.profile_version],
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
    #[cfg(test)]
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

    /// Opgeslagen gebeurtenissen (JSON) van het huidige gesprek (alle schermgesprekken sinds het laatste nieuwe
    /// gesprek), oudste eerst.
    pub fn messages_active(&self, project_id: &str) -> AppResult<Vec<String>> {
        let root = self.root_rowid(project_id)?;
        let conn = self.lock();
        let mut st = conn.prepare(
            "SELECT m.data FROM messages m JOIN sessions s ON s.id = m.session_id
             WHERE s.project_id = ?1 AND s.rowid >= ?2 ORDER BY m.id",
        )?;
        let rows = st.query_map(params![project_id, root], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Het gesprek van één scherm (of "project"/"shared"), binnen het huidige hoofdgesprek. Bestaat het nog niet, dan
    /// wordt het aangemaakt. Zo blijft de context per scherm klein en lopen schermen elkaar niet in de weg.
    pub fn session_for_scope(&self, project_id: &str, key: &str, engine: &str) -> AppResult<Session> {
        let root = self.root_rowid(project_id)?;
        let found = self
            .lock()
            .query_row(
                "SELECT id, engine_session_id FROM sessions WHERE project_id = ?1 AND scope_key = ?2 AND rowid >= ?3
                 ORDER BY rowid DESC LIMIT 1",
                params![project_id, key, root],
                |r| Ok(Session { id: r.get(0)?, engine_session_id: r.get(1)? }),
            )
            .optional()?;
        match found {
            Some(s) => Ok(s),
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                self.lock().execute("INSERT INTO sessions(id, project_id, engine, scope_key) VALUES (?1, ?2, ?3, ?4)", params![id, project_id, engine, key])?;
                Ok(Session { id, engine_session_id: None })
            }
        }
    }

    /// Het begin van het huidige hoofdgesprek (nieuwste sessie zonder scherm-sleutel), 0 als er nog geen is.
    fn root_rowid(&self, project_id: &str) -> AppResult<i64> {
        Ok(self
            .lock()
            .query_row("SELECT rowid FROM sessions WHERE project_id = ?1 AND scope_key = '' ORDER BY rowid DESC LIMIT 1", [project_id], |r| r.get(0))
            .optional()?
            .unwrap_or(0))
    }

    pub fn run_insert(&self, project_id: &str, r: &RunRecord) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO runs(project_id, level, final_level, screen, why, expansions, denied_edits, files_changed, issues_before, repairs,
                              rolled_back, ok, duration_ms, cost_usd, turns, ai_questions, class, tier, model, engine, purpose, escalated, tokens, planned,
                              input_tokens, output_tokens, cache_read_tokens, cache_write_tokens)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28)",
            params![
                project_id, r.level, r.final_level, r.screen, r.why, r.expansions, r.denied_edits, r.files_changed, r.issues_before, r.repairs,
                r.rolled_back, r.ok, r.duration_ms, r.cost_usd, r.turns, r.ai_questions, r.class, r.tier, r.model, r.engine, r.purpose, r.escalated, r.tokens, r.planned,
                r.input_tokens, r.output_tokens, r.cache_read_tokens, r.cache_write_tokens
            ],
        )?;
        Ok(())
    }

    fn run_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
        Ok(RunRecord {
            level: r.get(0)?,
            final_level: r.get(1)?,
            screen: r.get(2)?,
            why: r.get(3)?,
            expansions: r.get(4)?,
            denied_edits: r.get(5)?,
            files_changed: r.get(6)?,
            issues_before: r.get(7)?,
            repairs: r.get(8)?,
            rolled_back: r.get(9)?,
            ok: r.get(10)?,
            duration_ms: r.get(11)?,
            cost_usd: r.get(12)?,
            turns: r.get(13)?,
            ai_questions: r.get(14)?,
            class: r.get(15)?,
            tier: r.get(16)?,
            model: r.get(17)?,
            engine: r.get(18)?,
            purpose: r.get(19)?,
            escalated: r.get(20)?,
            tokens: r.get(21)?,
            planned: r.get(22)?,
            input_tokens: r.get(23)?,
            output_tokens: r.get(24)?,
            cache_read_tokens: r.get(25)?,
            cache_write_tokens: r.get(26)?,
        })
    }

    const RUN_COLS: &'static str = "level, final_level, screen, why, expansions, denied_edits, files_changed, issues_before, repairs, rolled_back, ok,
        duration_ms, cost_usd, turns, ai_questions, class, tier, model, engine, purpose, escalated, tokens, planned,
        input_tokens, output_tokens, cache_read_tokens, cache_write_tokens";

    /// Samenvatting van de laatste `limit` runs van een project (alles als getallen, voor het ontwikkelaarspaneel).
    pub fn runs_summary(&self, project_id: &str, limit: i64) -> AppResult<RunsSummary> {
        let conn = self.lock();
        let mut st = conn.prepare(&format!("SELECT {} FROM runs WHERE project_id = ?1 ORDER BY id DESC LIMIT ?2", Self::RUN_COLS))?;
        let rows: Vec<RunRecord> = st.query_map(params![project_id, limit], Self::run_row)?.collect::<Result<_, _>>()?;
        Ok(RunsSummary::from_runs(&rows))
    }

    /// De laatste runs van één klasse over alle projecten (nieuwste eerst), voor het bijstellen van de modelkeuze. Alleen gewone
    /// opdrachten: hulptaken, release-werk en handmatige overrides zeggen niets over de juiste keuze.
    pub fn class_history(&self, class: &str, limit: i64) -> AppResult<Vec<(String, bool)>> {
        let conn = self.lock();
        let mut st = conn.prepare(&format!("SELECT {} FROM runs WHERE class = ?1 AND purpose = 'work' AND tier != '' ORDER BY id DESC LIMIT ?2", Self::RUN_COLS))?;
        let rows: Vec<RunRecord> = st.query_map(params![class, limit], Self::run_row)?.collect::<Result<_, _>>()?;
        Ok(rows.into_iter().map(|r| (r.tier.clone(), r.is_clean())).collect())
    }

    /// Is de vorige run op dit scherm mislukt (teruggedraaid of niet geslaagd)? Dan telt "vorige poging faalde" mee.
    pub fn last_run_failed(&self, project_id: &str, screen: &str) -> AppResult<bool> {
        Ok(self
            .lock()
            .query_row(
                "SELECT rolled_back, ok FROM runs WHERE project_id = ?1 AND screen = ?2 AND purpose != 'override' ORDER BY id DESC LIMIT 1",
                params![project_id, screen],
                |r| Ok(r.get::<_, bool>(0)? || !r.get::<_, bool>(1)?),
            )
            .optional()?
            .unwrap_or(false))
    }

    // ── integraties en geheimen (alleen metadata) ──
    pub fn secret_meta_upsert(&self, m: &SecretMeta) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO secret_meta(scope, integration, field, kind, file_name, derived) VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(scope, integration, field) DO UPDATE SET kind=?4, file_name=?5, derived=?6,
               set_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![m.scope, m.integration, m.field, m.kind, m.file_name, serde_json::to_string(&m.derived).unwrap_or_else(|_| "{}".into())],
        )?;
        Ok(())
    }

    fn secret_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SecretMeta> {
        let derived: String = r.get(5)?;
        Ok(SecretMeta {
            scope: r.get(0)?,
            integration: r.get(1)?,
            field: r.get(2)?,
            kind: r.get(3)?,
            file_name: r.get(4)?,
            derived: serde_json::from_str(&derived).unwrap_or_default(),
            set_at: r.get(6)?,
            last_used: r.get(7)?,
            used_by: r.get(8)?,
        })
    }

    pub fn secret_meta_get(&self, scope: &str, integration: &str, field: &str) -> AppResult<Option<SecretMeta>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT scope, integration, field, kind, file_name, derived, set_at, last_used, used_by FROM secret_meta
                 WHERE scope=?1 AND integration=?2 AND field=?3",
                params![scope, integration, field],
                Self::secret_row,
            )
            .optional()?)
    }

    #[cfg(test)]
    pub fn secret_meta_list(&self, scopes: &[&str]) -> AppResult<Vec<SecretMeta>> {
        let conn = self.lock();
        let mut st = conn.prepare("SELECT scope, integration, field, kind, file_name, derived, set_at, last_used, used_by FROM secret_meta ORDER BY integration, field")?;
        let rows = st.query_map([], Self::secret_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?.into_iter().filter(|m| scopes.contains(&m.scope.as_str())).collect())
    }

    pub fn secret_meta_delete(&self, scope: &str, integration: &str, field: &str) -> AppResult<()> {
        self.lock().execute("DELETE FROM secret_meta WHERE scope=?1 AND integration=?2 AND field=?3", params![scope, integration, field])?;
        Ok(())
    }

    pub fn secret_touch_used(&self, scope: &str, integration: &str, field: &str, by: &str) -> AppResult<()> {
        self.lock().execute(
            "UPDATE secret_meta SET last_used=strftime('%Y-%m-%dT%H:%M:%fZ','now'), used_by=?4 WHERE scope=?1 AND integration=?2 AND field=?3",
            params![scope, integration, field, by],
        )?;
        Ok(())
    }

    pub fn integration_set_enabled(&self, project_id: &str, integration: &str, enabled: bool) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO integration_state(project_id, integration, enabled) VALUES (?1,?2,?3)
             ON CONFLICT(project_id, integration) DO UPDATE SET enabled=?3",
            params![project_id, integration, enabled as i64],
        )?;
        Ok(())
    }

    /// Verwijdert een integratie die niet meer bestaat uit alle projecten.
    pub fn integration_forget(&self, integration: &str) -> AppResult<()> {
        self.lock().execute("DELETE FROM integration_state WHERE integration=?1", params![integration])?;
        Ok(())
    }

    pub fn integration_states(&self, project_id: &str) -> AppResult<Vec<IntegrationState>> {
        let conn = self.lock();
        let mut st = conn.prepare("SELECT integration, enabled, last_tested, last_test_ok, last_test_code, expires_at FROM integration_state WHERE project_id=?1")?;
        let rows = st.query_map([project_id], |r| {
            Ok(IntegrationState {
                integration: r.get(0)?,
                enabled: r.get::<_, i64>(1)? != 0,
                last_tested: r.get(2)?,
                last_test_ok: r.get::<_, Option<i64>>(3)?.map(|v| v != 0),
                last_test_code: r.get(4)?,
                expires_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn integration_record_test(&self, project_id: &str, integration: &str, ok: bool, code: &str, expires_at: Option<&str>) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO integration_state(project_id, integration, enabled, last_tested, last_test_ok, last_test_code, expires_at)
             VALUES (?1,?2,0,strftime('%Y-%m-%dT%H:%M:%fZ','now'),?3,?4,?5)
             ON CONFLICT(project_id, integration) DO UPDATE SET last_tested=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
               last_test_ok=?3, last_test_code=?4, expires_at=?5",
            params![project_id, integration, ok as i64, code, expires_at],
        )?;
        Ok(())
    }

    /// Een nieuwe waarde maakt het oude testresultaat ongeldig.
    pub fn integration_clear_test(&self, project_id: &str, integration: &str) -> AppResult<()> {
        self.lock().execute(
            "UPDATE integration_state SET last_tested=NULL, last_test_ok=NULL, last_test_code=NULL, expires_at=NULL WHERE project_id=?1 AND integration=?2",
            params![project_id, integration],
        )?;
        Ok(())
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

    pub fn profile_version(&self, id: &str) -> AppResult<u32> {
        Ok(self.lock().query_row("SELECT profile_version FROM projects WHERE id = ?1", [id], |r| r.get(0)).optional()?.unwrap_or(0))
    }

    pub fn set_profile_version(&self, id: &str, v: u32) -> AppResult<()> {
        self.lock().execute("UPDATE projects SET profile_version = ?2 WHERE id = ?1", params![id, v])?;
        Ok(())
    }

    /// Projecten waarvan het profiel ouder is dan `current` (id, pad).
    pub fn projects_below_profile(&self, current: u32) -> AppResult<Vec<(String, String)>> {
        let conn = self.lock();
        let mut st = conn.prepare("SELECT id, path FROM projects WHERE profile_version < ?1 ORDER BY created_at")?;
        let rows = st.query_map([current], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
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
            profile_version: 0,
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
        assert_eq!(p.checklist_total, 9);
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
    fn run_summary_counts_levels_repairs_and_rollbacks() {
        let db = Db::open_in_memory().unwrap();
        let p = db
            .project_insert(&NewProject { name: "A".into(), slug: "a".into(), path: "/a".into(), template: "blank".into(), description: String::new(), bundle_id: "x.y".into(), initial_commit: None, profile_version: 1 })
            .unwrap();
        let run = |level: &str, expansions, issues, repairs, rolled| RunRecord {
            level: level.into(),
            final_level: level.into(),
            expansions,
            issues_before: issues,
            repairs,
            rolled_back: rolled,
            ok: !rolled,
            duration_ms: 1000,
            cost_usd: 0.5,
            files_changed: 2,
            ..Default::default()
        };
        for r in [run("small", 0, 0, 0, false), run("small", 0, 2, 1, false), run("screen", 1, 3, 2, true), run("shared", 0, 0, 0, false)] {
            db.run_insert(&p.id, &r).unwrap();
        }
        let s = db.runs_summary(&p.id, 100).unwrap();
        assert_eq!((s.runs, s.expanded, s.clean_first_time, s.repaired, s.rolled_back), (4, 1, 2, 1, 1));
        assert_eq!((s.by_level["small"], s.by_level["screen"], s.by_level["shared"]), (2, 1, 1));
        assert!((s.total_cost_usd - 2.0).abs() < 1e-9 && s.avg_duration_ms == 1000 && (s.avg_files_changed - 2.0).abs() < 1e-9);
        assert_eq!(db.runs_summary(&p.id, 2).unwrap().runs, 2, "limit counts the newest runs");
        assert_eq!(RunsSummary::from_runs(&[]), RunsSummary::default());
    }

    #[test]
    fn conversations_are_per_scope_and_a_new_root_resets_the_feed() {
        let db = Db::open_in_memory().unwrap();
        let p = db
            .project_insert(&NewProject { name: "A".into(), slug: "a".into(), path: "/a".into(), template: "blank".into(), description: String::new(), bundle_id: "x.y".into(), initial_commit: None, profile_version: 1 })
            .unwrap();
        let root = db.session_for_scope(&p.id, "", "claude-code").unwrap();
        let home = db.session_for_scope(&p.id, "HomeScreen", "claude-code").unwrap();
        let again = db.session_for_scope(&p.id, "HomeScreen", "claude-code").unwrap();
        assert_eq!(home.id, again.id, "same screen, same conversation");
        assert_ne!(root.id, home.id);
        db.session_set_engine_id(&home.id, "engine-1").unwrap();
        assert_eq!(db.session_for_scope(&p.id, "HomeScreen", "claude-code").unwrap().engine_session_id.as_deref(), Some("engine-1"));
        assert!(db.session_for_scope(&p.id, "SettingsScreen", "claude-code").unwrap().engine_session_id.is_none());
        db.message_insert(&home.id, "text", "{\"a\":1}").unwrap();
        let other = db.session_for_scope(&p.id, "SettingsScreen", "claude-code").unwrap();
        db.message_insert(&other.id, "text", "{\"a\":2}").unwrap();
        assert_eq!(db.messages_active(&p.id).unwrap().len(), 2);
        // "Nieuw gesprek": alles begint opnieuw, ook de schermgesprekken.
        db.session_create(&p.id, "claude-code").unwrap();
        assert!(db.messages_active(&p.id).unwrap().is_empty());
        assert!(db.session_for_scope(&p.id, "HomeScreen", "claude-code").unwrap().engine_session_id.is_none());
    }

    #[test]
    fn profile_versions_drive_migration() {
        let db = Db::open_in_memory().unwrap();
        let a = db.project_insert(&sample("A", "/tmp/a")).unwrap();
        let mut nb = sample("B", "/tmp/b");
        nb.profile_version = 1;
        let b = db.project_insert(&nb).unwrap();
        assert_eq!(db.profile_version(&a.id).unwrap(), 0);
        assert_eq!(db.profile_version(&b.id).unwrap(), 1);
        assert_eq!(db.projects_below_profile(1).unwrap().len(), 1);
        db.set_profile_version(&a.id, 1).unwrap();
        assert!(db.projects_below_profile(1).unwrap().is_empty());
        assert_eq!(db.projects_below_profile(2).unwrap().len(), 2, "a newer Studio profile selects every project");
    }

    #[test]
    fn version_and_checklist() {
        let db = Db::open_in_memory().unwrap();
        let p = db.project_insert(&sample("X", "/tmp/a")).unwrap();
        db.project_set_version(&p.id, "1.2.0", 7).unwrap();
        let p = db.project_get(&p.id).unwrap();
        assert_eq!((p.version.as_str(), p.build_number), ("1.2.0", 7));
        let items = db.checklist_list(&p.id).unwrap();
        assert_eq!(items.len(), 11);
        assert_eq!(items.iter().filter(|i| i.blocking).count(), 9);
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
