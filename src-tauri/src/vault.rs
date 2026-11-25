//! De kluis (fase 10): alle geheime en publieke waarden van integraties.
//!
//! * Tekstwaarden staan in de OS-keychain (Windows Credential Manager, macOS Keychain, Linux keyutils).
//! * Bestanden (`.p8`, service-account-JSON, keystore) staan versleuteld (XChaCha20-Poly1305) in de app-datamap; de
//!   sleutel staat in de keychain, want keychains hebben een maximale grootte.
//! * SQLite bevat alleen metadata: veld, integratie, scope, bestandsnaam, afgeleide niet-geheime waarden, tijdstippen.
//! * Waarden gaan nooit naar logs of Claude. Een build of upload krijgt ze via `env_for`, als omgevingsvariabelen en
//!   tijdelijke bestanden die na afloop verdwijnen.

use crate::db::{Db, SecretMeta};
use crate::error::{AppError, AppResult};
use crate::manifest::{self, Field, FieldKind, Manifest, Rejection, Scope};
use crate::scrub;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng, Payload};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

const SERVICE: &str = "app.studio.desktop";

// ───────────── opslag achter een trait (keychain, of een nep in tests) ─────────────

pub trait SecretBackend: Send + Sync {
    fn get(&self, key: &str) -> AppResult<Option<String>>;
    fn set(&self, key: &str, value: &str) -> AppResult<()>;
    fn delete(&self, key: &str) -> AppResult<()>;
}

pub struct KeychainBackend;

fn entry(key: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, key).map_err(|e| AppError::Keychain(e.to_string()))
}

impl SecretBackend for KeychainBackend {
    fn get(&self, key: &str) -> AppResult<Option<String>> {
        match entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Keychain(e.to_string())),
        }
    }
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        entry(key)?.set_password(value).map_err(|e| AppError::Keychain(e.to_string()))
    }
    fn delete(&self, key: &str) -> AppResult<()> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Keychain(e.to_string())),
        }
    }
}

/// Geheugen-kluis voor tests (en voor de demo-modus zonder keychain).
#[derive(Default)]
pub struct MemoryBackend(pub Mutex<BTreeMap<String, String>>);

impl SecretBackend for MemoryBackend {
    fn get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> AppResult<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
        Ok(())
    }
}

// ───────────── statusmodel (alleen metadata, nooit waarden) ─────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FieldStatus {
    pub id: String,
    pub present: bool,
    pub public: bool,
    pub scope: Scope,
    pub file_name: Option<String>,
    pub derived: BTreeMap<String, String>,
    pub set_at: Option<String>,
    pub last_used: Option<String>,
    pub used_by: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub id: String,
    pub enabled: bool,
    /// off | missing | incomplete | complete | warning
    pub state: String,
    pub missing: Vec<String>,
    pub fields: Vec<FieldStatus>,
    pub last_tested: Option<String>,
    pub last_test_ok: Option<bool>,
    pub last_test_code: Option<String>,
    pub expires_at: Option<String>,
    /// Dagen tot de sleutel verloopt (negatief = al verlopen).
    pub expires_in_days: Option<i64>,
    /// Integraties waar deze van afhangt en die nog uit staan.
    pub dependencies_off: Vec<String>,
}

pub const EXPIRY_WARNING_DAYS: i64 = 14;

/// Integraties die een checklistpunt altijd nodig heeft, ook als de gebruiker ze niet zelf heeft aangezet (Android eerst).
pub const BASELINE: &[(&str, &[&str])] = &[("accounts_signing", &["android_signing", "google_play"])];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MissingSecret {
    pub integration: String,
    pub field: String,
    pub enabled: bool,
    pub items: Vec<String>,
}

/// Dagen vanaf 1970 voor een burgerlijke datum (Howard Hinnants algoritme); genoeg om verloopdata te vergelijken.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn days_until(date: &str, now_unix: i64) -> Option<i64> {
    let d = date.get(..10)?;
    let mut it = d.split('-');
    let (y, m, day) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    Some(days_from_civil(y, m, day) - now_unix.div_euclid(86400))
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

// ───────────── tijdelijke bestanden voor subprocessen ─────────────

/// Een ontsleuteld bestand dat alleen voor een kortlevend subproces bestaat; verdwijnt bij `drop`.
pub struct TempSecret {
    path: PathBuf,
    dir: PathBuf,
}

impl TempSecret {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSecret {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.dir);
    }
}

pub struct EnvBundle {
    pub vars: Vec<(String, String)>,
    pub files: Vec<TempSecret>,
}

// ───────────── de kluis zelf ─────────────

pub struct Vault {
    backend: Arc<dyn SecretBackend>,
    files_dir: PathBuf,
    tmp_dir: PathBuf,
    /// Waar oudere Studio-versies een keystore als gewoon bestand bewaarden (wordt eenmalig ingelezen en verwijderd).
    legacy_dir: PathBuf,
    db: Arc<Db>,
}

fn owner(scope: &str, integration: &str, field: &str) -> String {
    format!("{scope}/{integration}/{field}")
}

fn rand_bytes<const N: usize>() -> [u8; N] {
    use chacha20poly1305::aead::rand_core::RngCore;
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

impl Vault {
    pub fn new(backend: Arc<dyn SecretBackend>, data_dir: &Path, db: Arc<Db>) -> Self {
        Vault { backend, files_dir: data_dir.join("secrets"), tmp_dir: data_dir.join("tmp"), legacy_dir: data_dir.to_path_buf(), db }
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.db
    }

    fn lookup(&self, integration: &str, field: &str) -> AppResult<(&'static Manifest, &'static Field)> {
        let m = manifest::get(integration).ok_or_else(|| AppError::NotFound(format!("integration {integration}")))?;
        let f = m.field(field).ok_or_else(|| AppError::NotFound(format!("field {integration}.{field}")))?;
        Ok((m, f))
    }

    fn scope_of(f: &Field, project_id: &str) -> AppResult<String> {
        match f.scope {
            Scope::Global => Ok("global".into()),
            Scope::Project => {
                if project_id.is_empty() || !project_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    return Err(AppError::Invalid("invalid project id".into()));
                }
                Ok(project_id.to_string())
            }
        }
    }

    fn bkey(scope: &str, integration: &str, field: &str) -> String {
        format!("v1/{scope}/{integration}/{field}")
    }

    fn enc_path(&self, scope: &str, integration: &str, field: &str) -> PathBuf {
        self.files_dir.join(scope).join(integration).join(format!("{field}.enc"))
    }

    fn reject(r: Rejection) -> AppError {
        AppError::Rejected { code: r.code.to_string(), detail: r.detail }
    }

    // ── schrijven ──

    pub fn set_text(&self, project_id: &str, integration: &str, field: &str, value: &str) -> AppResult<()> {
        let (_, f) = self.lookup(integration, field)?;
        manifest::validate_text(f, value).map_err(Self::reject)?;
        let scope = Self::scope_of(f, project_id)?;
        let value = value.trim();
        self.backend.set(&Self::bkey(&scope, integration, field), value)?;
        self.db.secret_meta_upsert(&SecretMeta { scope: scope.clone(), integration: integration.into(), field: field.into(), kind: "text".into(), ..Default::default() })?;
        self.after_change(project_id, f, &scope, integration, Some(vec![value.to_string()]));
        Ok(())
    }

    pub fn set_file(&self, project_id: &str, integration: &str, field: &str, file_name: &str, bytes: &[u8]) -> AppResult<BTreeMap<String, String>> {
        let (_, f) = self.lookup(integration, field)?;
        let derived = manifest::validate_file(f, file_name, bytes).map_err(Self::reject)?;
        let scope = Self::scope_of(f, project_id)?;
        let bkey = Self::bkey(&scope, integration, field);
        // Elke keer een nieuwe sleutel en nonce. Eerst de sleutel in de keychain: lukt dat niet, dan schrijven we niets.
        let key = XChaCha20Poly1305::generate_key(&mut OsRng);
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let cipher = XChaCha20Poly1305::new(&key);
        let ct = cipher.encrypt(&nonce, Payload { msg: bytes, aad: bkey.as_bytes() }).map_err(|_| AppError::Keychain("encryption failed".into()))?;
        self.backend.set(&format!("{bkey}#key"), &base64::engine::general_purpose::STANDARD.encode(key))?;
        let path = self.enc_path(&scope, integration, field);
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
        let mut blob = nonce.to_vec();
        blob.extend(ct);
        let tmp = path.with_extension("enc.tmp");
        std::fs::write(&tmp, blob)?;
        std::fs::rename(&tmp, &path)?;
        let clean_name: String = file_name.rsplit(['/', '\\']).next().unwrap_or("").chars().take(120).collect();
        self.db.secret_meta_upsert(&SecretMeta { scope: scope.clone(), integration: integration.into(), field: field.into(), kind: "file".into(), file_name: clean_name, derived: derived.clone(), ..Default::default() })?;
        self.after_change(project_id, f, &scope, integration, Some(scrub::values_from_file(bytes)));
        Ok(derived)
    }

    fn after_change(&self, project_id: &str, f: &Field, scope: &str, integration: &str, values: Option<Vec<String>>) {
        // Een nieuwe waarde maakt een eerder testresultaat ongeldig.
        if !project_id.is_empty() {
            let _ = self.db.integration_clear_test(project_id, integration);
        }
        if f.geheim {
            scrub::register(&owner(scope, integration, &f.id), &values.unwrap_or_default());
        }
    }

    // ── lezen (alleen intern; nooit als MCP-tool) ──

    pub fn get_text(&self, project_id: &str, integration: &str, field: &str) -> AppResult<Option<String>> {
        let (_, f) = self.lookup(integration, field)?;
        if f.kind == FieldKind::File {
            return Ok(self.get_file(project_id, integration, field)?.and_then(|b| String::from_utf8(b).ok()));
        }
        let scope = Self::scope_of(f, project_id)?;
        self.backend.get(&Self::bkey(&scope, integration, field))
    }

    pub fn get_file(&self, project_id: &str, integration: &str, field: &str) -> AppResult<Option<Vec<u8>>> {
        let (_, f) = self.lookup(integration, field)?;
        let scope = Self::scope_of(f, project_id)?;
        let bkey = Self::bkey(&scope, integration, field);
        let path = self.enc_path(&scope, integration, field);
        let Ok(blob) = std::fs::read(&path) else { return Ok(None) };
        let key_b64 = self.backend.get(&format!("{bkey}#key"))?.ok_or_else(|| AppError::Keychain("the key for this file is missing from the keychain".into()))?;
        let key = base64::engine::general_purpose::STANDARD.decode(key_b64).map_err(|_| AppError::Keychain("damaged key".into()))?;
        if blob.len() < 24 + 16 || key.len() != 32 {
            return Err(AppError::Keychain("damaged secret file".into()));
        }
        let (nonce, ct) = blob.split_at(24);
        XChaCha20Poly1305::new(key.as_slice().into())
            .decrypt(nonce.into(), Payload { msg: ct, aad: bkey.as_bytes() })
            .map(Some)
            .map_err(|_| AppError::Keychain("could not decrypt the secret file (wrong key or damaged file)".into()))
    }

    /// Bestaat de waarde? Via de metadata, dus zonder de keychain aan te raken.
    pub fn has(&self, project_id: &str, integration: &str, field: &str) -> bool {
        let Ok((_, f)) = self.lookup(integration, field) else { return false };
        let Ok(scope) = Self::scope_of(f, project_id) else { return false };
        self.db.secret_meta_get(&scope, integration, field).ok().flatten().is_some()
    }

    pub fn meta(&self, project_id: &str, integration: &str, field: &str) -> Option<SecretMeta> {
        let (_, f) = self.lookup(integration, field).ok()?;
        let scope = Self::scope_of(f, project_id).ok()?;
        self.db.secret_meta_get(&scope, integration, field).ok().flatten()
    }

    pub fn delete(&self, project_id: &str, integration: &str, field: &str) -> AppResult<()> {
        let (_, f) = self.lookup(integration, field)?;
        let scope = Self::scope_of(f, project_id)?;
        let bkey = Self::bkey(&scope, integration, field);
        self.backend.delete(&bkey)?;
        self.backend.delete(&format!("{bkey}#key"))?;
        let _ = std::fs::remove_file(self.enc_path(&scope, integration, field));
        self.db.secret_meta_delete(&scope, integration, field)?;
        scrub::unregister(&owner(&scope, integration, field));
        if !project_id.is_empty() {
            let _ = self.db.integration_clear_test(project_id, integration);
        }
        Ok(())
    }

    /// Verwijdert de projectwaarden van een integratie (voor "geheimen verwijderen" bij uitzetten). Globale waarden, zoals
    /// een Apple-account, horen ook bij andere projecten: die verwijdert alleen een expliciete actie op de Geheimen-pagina.
    pub fn delete_integration(&self, project_id: &str, integration: &str) -> AppResult<usize> {
        let m = manifest::get(integration).ok_or_else(|| AppError::NotFound(integration.into()))?;
        let mut n = 0;
        for f in m.velden.iter().filter(|f| f.scope == Scope::Project) {
            if self.has(project_id, integration, &f.id) {
                self.delete(project_id, integration, &f.id)?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Alle projectwaarden van een project (bij het verwijderen van het project).
    pub fn delete_project(&self, project_id: &str) {
        for m in manifest::all() {
            for f in m.velden.iter().filter(|f| f.scope == Scope::Project) {
                let _ = self.delete(project_id, &m.id, &f.id);
            }
        }
    }

    // ── gebruik door Studio zelf ──

    pub fn touch(&self, project_id: &str, integration: &str, field: &str, by: &str) {
        if let Ok((_, f)) = self.lookup(integration, field) {
            if let Ok(scope) = Self::scope_of(f, project_id) {
                let _ = self.db.secret_touch_used(&scope, integration, field, by);
            }
        }
    }

    /// Schrijft een bestandsgeheim tijdelijk naar schijf (alleen eigenaar) voor een subproces.
    pub fn materialize(&self, project_id: &str, integration: &str, field: &str, by: &str) -> AppResult<Option<TempSecret>> {
        let Some(bytes) = self.get_file(project_id, integration, field)? else { return Ok(None) };
        let ext = self.meta(project_id, integration, field).and_then(|m| m.file_name.rsplit_once('.').map(|(_, e)| e.to_string())).unwrap_or_else(|| "bin".into());
        let dir = self.tmp_dir.join(uuid::Uuid::new_v4().simple().to_string());
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let path = dir.join(format!("{field}.{ext}"));
        std::fs::write(&path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        self.touch(project_id, integration, field, by);
        Ok(Some(TempSecret { path, dir }))
    }

    /// Omgevingsvariabelen voor een kortlevend subproces (build, upload). Bestanden worden tijdelijk ontsleuteld.
    pub fn env_for(&self, project_id: &str, integrations: &[&str], by: &str) -> AppResult<EnvBundle> {
        let mut bundle = EnvBundle { vars: vec![], files: vec![] };
        for id in integrations {
            let m = manifest::get(id).ok_or_else(|| AppError::NotFound((*id).into()))?;
            for f in &m.velden {
                let Some(var) = f.env_var.clone() else { continue };
                if !self.has(project_id, id, &f.id) {
                    if let Some(d) = &f.standaard {
                        bundle.vars.push((var, d.clone()));
                    }
                    continue;
                }
                if f.kind == FieldKind::File {
                    if let Some(t) = self.materialize(project_id, id, &f.id, by)? {
                        bundle.vars.push((var, t.path().to_string_lossy().into_owned()));
                        bundle.files.push(t);
                    }
                } else if let Some(v) = self.get_text(project_id, id, &f.id)? {
                    self.touch(project_id, id, &f.id, by);
                    bundle.vars.push((var, v));
                }
            }
        }
        Ok(bundle)
    }

    // ── status ──

    pub fn enabled_ids(&self, project_id: &str) -> Vec<String> {
        self.db.integration_states(project_id).unwrap_or_default().into_iter().filter(|s| s.enabled).map(|s| s.integration).collect()
    }

    pub fn status_of(&self, project_id: &str, m: &Manifest) -> AppResult<IntegrationStatus> {
        let states = self.db.integration_states(project_id)?;
        let st = states.iter().find(|s| s.integration == m.id).cloned().unwrap_or_default();
        let mut fields = vec![];
        let mut missing = vec![];
        for f in &m.velden {
            let meta = self.meta(project_id, &m.id, &f.id);
            if f.verplicht && meta.is_none() {
                missing.push(f.id.clone());
            }
            fields.push(FieldStatus {
                id: f.id.clone(),
                present: meta.is_some(),
                public: !f.geheim,
                scope: f.scope,
                file_name: meta.as_ref().filter(|x| !x.file_name.is_empty()).map(|x| x.file_name.clone()),
                derived: meta.as_ref().map(|x| x.derived.clone()).unwrap_or_default(),
                set_at: meta.as_ref().map(|x| x.set_at.clone()),
                last_used: meta.as_ref().and_then(|x| x.last_used.clone()),
                used_by: meta.as_ref().map(|x| x.used_by.clone()).unwrap_or_default(),
            });
        }
        let expires_in_days = st.expires_at.as_deref().and_then(|d| days_until(d, now_unix()));
        let required = m.velden.iter().filter(|f| f.verplicht).count();
        let state = if !st.enabled {
            "off"
        } else if required > 0 && missing.len() == required {
            "missing"
        } else if !missing.is_empty() {
            "incomplete"
        } else if st.last_test_ok == Some(false) || expires_in_days.is_some_and(|d| d < EXPIRY_WARNING_DAYS) {
            "warning"
        } else {
            "complete"
        };
        let dependencies_off = m.afhankelijkheden.iter().filter(|d| !states.iter().any(|s| &s.integration == *d && s.enabled)).cloned().collect();
        Ok(IntegrationStatus {
            id: m.id.clone(),
            enabled: st.enabled,
            state: state.into(),
            missing,
            fields,
            last_tested: st.last_tested,
            last_test_ok: st.last_test_ok,
            last_test_code: st.last_test_code,
            expires_at: st.expires_at,
            expires_in_days,
            dependencies_off,
        })
    }

    pub fn statuses(&self, project_id: &str) -> AppResult<Vec<IntegrationStatus>> {
        manifest::all().iter().map(|m| self.status_of(project_id, m)).collect()
    }

    /// Verplichte velden die nog ontbreken (en, met een leeg veld, integraties met een verlopen of ongeldige sleutel): van geactiveerde integraties en van wat het publiceren sowieso nodig heeft.
    pub fn missing_secrets(&self, project_id: &str) -> Vec<MissingSecret> {
        let enabled = self.enabled_ids(project_id);
        let mut out = vec![];
        for m in manifest::all() {
            let baseline = BASELINE.iter().any(|(_, ids)| ids.contains(&m.id.as_str()));
            let on = enabled.contains(&m.id);
            if !on && !baseline {
                continue;
            }
            for f in m.velden.iter().filter(|f| f.verplicht && !self.has(project_id, &m.id, &f.id)) {
                out.push(MissingSecret { integration: m.id.clone(), field: f.id.clone(), enabled: on, items: m.checklist_items.clone() });
            }
        }
        // Een verlopen of ongeldige sleutel (⚠): geen veld ontbreekt, maar het punt is niet in orde. Veld = "" betekent "probleem".
        for m in manifest::all().iter().filter(|m| enabled.contains(&m.id)) {
            if self.status_of(project_id, m).map(|st| st.state == "warning").unwrap_or(false) {
                out.push(MissingSecret { integration: m.id.clone(), field: String::new(), enabled: true, items: m.checklist_items.clone() });
            }
        }
        out
    }

    /// Laadt alle bekende geheime waarden in de scrubber (bij het starten van Studio, op de achtergrond).
    pub fn load_scrubber(&self, project_ids: &[String]) {
        for m in manifest::all() {
            for f in m.velden.iter().filter(|f| f.geheim) {
                let mut scopes: Vec<&str> = vec![];
                if f.scope == Scope::Global {
                    scopes.push("");
                } else {
                    scopes.extend(project_ids.iter().map(String::as_str));
                }
                for pid in scopes {
                    let Ok(scope) = Self::scope_of(f, pid) else { continue };
                    if !self.has(pid, &m.id, &f.id) {
                        continue;
                    }
                    let values = if f.kind == FieldKind::File {
                        self.get_file(pid, &m.id, &f.id).ok().flatten().map(|b| scrub::values_from_file(&b)).unwrap_or_default()
                    } else {
                        self.get_text(pid, &m.id, &f.id).ok().flatten().into_iter().collect()
                    };
                    scrub::register(&owner(&scope, &m.id, &f.id), &values);
                }
            }
        }
    }

    // ── oude Studio-opslag eenmalig overnemen ──

    /// Neemt een oude gewone keystore (`keystores/<project>/upload-keystore.jks`) en oude keychain-namen over.
    pub fn migrate_legacy(&self, project_id: &str) {
        let old = self.legacy_dir.join("keystores").join(project_id).join("upload-keystore.jks");
        if old.is_file() && !self.has(project_id, "android_signing", "keystore") {
            if let Ok(bytes) = std::fs::read(&old) {
                if self.set_file(project_id, "android_signing", "keystore", "upload-keystore.jks", &bytes).is_ok() {
                    let _ = std::fs::remove_file(&old);
                }
            }
        }
        for (old_name, integration, field) in [("android_keystore_password", "android_signing", "keystore_password"), ("supabase_token", "supabase", "access_token"), ("play_service_account_json", "google_play", "service_account")] {
            if self.has(project_id, integration, field) {
                continue;
            }
            let legacy_key = format!("{project_id}:{old_name}");
            if let Ok(Some(v)) = self.backend.get(&legacy_key) {
                let ok = if old_name == "play_service_account_json" { self.set_file(project_id, integration, field, "service-account.json", v.as_bytes()).is_ok() } else { self.set_text(project_id, integration, field, &v).is_ok() };
                if ok {
                    let _ = self.backend.delete(&legacy_key);
                }
            }
        }
    }
}

// ───────────── versleutelde back-up (wachtwoordzin) ─────────────

const MAGIC: &[u8] = b"STUDIOVAULT1\n";

fn derive_key(passphrase: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    let mut key = [0u8; 32];
    argon2::Argon2::default().hash_password_into(passphrase.as_bytes(), salt, &mut key).map_err(|e| AppError::Invalid(format!("key derivation: {e}")))?;
    Ok(key)
}

impl Vault {
    /// Versleutelde back-up van alle waarden die voor dit project gelden (globaal + dit project).
    pub fn export(&self, project_id: &str, passphrase: &str) -> AppResult<Vec<u8>> {
        if passphrase.chars().count() < 8 {
            return Err(AppError::Rejected { code: "passphrase_short".into(), detail: Some("8".into()) });
        }
        let mut entries = vec![];
        for m in manifest::all() {
            for f in &m.velden {
                let Some(meta) = self.meta(project_id, &m.id, &f.id) else { continue };
                let data = if f.kind == FieldKind::File { self.get_file(project_id, &m.id, &f.id)? } else { self.get_text(project_id, &m.id, &f.id)?.map(String::into_bytes) };
                let Some(data) = data else { continue };
                entries.push(serde_json::json!({
                    "scope": if f.scope == Scope::Global { "global" } else { "project" },
                    "integration": m.id, "field": f.id, "kind": meta.kind, "fileName": meta.file_name,
                    "data": base64::engine::general_purpose::STANDARD.encode(data),
                }));
            }
        }
        let plain = serde_json::to_vec(&serde_json::json!({ "version": 1, "entries": entries }))?;
        let salt: [u8; 16] = rand_bytes();
        let key = derive_key(passphrase, &salt)?;
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ct = XChaCha20Poly1305::new((&key).into()).encrypt(&nonce, Payload { msg: &plain, aad: MAGIC }).map_err(|_| AppError::Invalid("encryption failed".into()))?;
        let mut out = MAGIC.to_vec();
        out.extend(salt);
        out.extend(nonce);
        out.extend(ct);
        Ok(out)
    }

    /// Zet een back-up terug. Projectwaarden komen in `project_id` terecht. Elke waarde wordt opnieuw gevalideerd.
    pub fn import(&self, project_id: &str, passphrase: &str, data: &[u8]) -> AppResult<usize> {
        let wrong = || AppError::Rejected { code: "backup_unreadable".into(), detail: None };
        if data.len() < MAGIC.len() + 16 + 24 + 16 || !data.starts_with(MAGIC) {
            return Err(wrong());
        }
        let rest = &data[MAGIC.len()..];
        let (salt, rest) = rest.split_at(16);
        let (nonce, ct) = rest.split_at(24);
        let key = derive_key(passphrase, salt)?;
        let plain = XChaCha20Poly1305::new((&key).into()).decrypt(nonce.into(), Payload { msg: ct, aad: MAGIC }).map_err(|_| wrong())?;
        let v: serde_json::Value = serde_json::from_slice(&plain).map_err(|_| wrong())?;
        let mut n = 0;
        for e in v["entries"].as_array().ok_or_else(wrong)? {
            let (integration, field) = (e["integration"].as_str().unwrap_or(""), e["field"].as_str().unwrap_or(""));
            let bytes = base64::engine::general_purpose::STANDARD.decode(e["data"].as_str().unwrap_or("")).map_err(|_| wrong())?;
            let ok = if e["kind"] == "file" {
                self.set_file(project_id, integration, field, e["fileName"].as_str().unwrap_or("file"), &bytes).is_ok()
            } else {
                self.set_text(project_id, integration, field, &String::from_utf8_lossy(&bytes)).is_ok()
            };
            n += ok as usize;
        }
        Ok(n)
    }
}

// ───────────── globale instantie ─────────────

static GLOBAL: OnceLock<Vault> = OnceLock::new();

pub fn init(v: Vault) {
    let _ = GLOBAL.set(v);
}

/// De kluis van deze app. Zonder `init` (tests, losse hulpprogramma's) een lege kluis met een geheugen-backend.
pub fn global() -> &'static Vault {
    GLOBAL.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("studio-vault-{}", uuid::Uuid::new_v4()));
        let db = Arc::new(Db::open_in_memory().expect("in-memory database"));
        Vault::new(Arc::new(MemoryBackend::default()), &dir, db)
    })
}

#[cfg(test)]
pub fn test_vault() -> (Vault, Arc<MemoryBackend>, String, PathBuf) {
    let dir = std::env::temp_dir().join(format!("studio-vault-test-{}", uuid::Uuid::new_v4()));
    let db = Arc::new(Db::open_in_memory().unwrap());
    let p = db
        .project_insert(&crate::db::NewProject { name: "Demo".into(), slug: "demo".into(), path: "/p".into(), template: "blank".into(), description: String::new(), bundle_id: "com.example.demo".into(), initial_commit: None, profile_version: 1 })
        .unwrap();
    let mem = Arc::new(MemoryBackend::default());
    (Vault::new(mem.clone(), &dir, db), mem, p.id, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----\nMIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQgVAULTfake0123456789\n-----END PRIVATE KEY-----\n";
    const SA: &[u8] = br#"{"type":"service_account","project_id":"proj-1","client_email":"bot@proj-1.iam.gserviceaccount.com","private_key":"-----BEGIN PRIVATE KEY-----\nVAULTSAKEYLINE0123456789abcdefABCDEF\n-----END PRIVATE KEY-----\n"}"#;

    #[test]
    fn text_values_roundtrip_through_the_backend_and_only_metadata_reaches_sqlite() {
        let (v, mem, pid, _) = test_vault();
        v.set_text(&pid, "apple", "key_id", "ABCDE12345").unwrap();
        v.set_text(&pid, "github", "token", "github_pat_11AAAAAAAAAAAAAAAAAAAA_zzzzzzzzzz").unwrap();
        assert_eq!(v.get_text(&pid, "apple", "key_id").unwrap().as_deref(), Some("ABCDE12345"));
        assert!(v.has(&pid, "github", "token") && !v.has(&pid, "github", "nothing"));
        // Alles wat de metadata-tabellen bevatten, bevat geen enkele waarde.
        let dump = serde_json::to_string(&v.db().secret_meta_list(&["global", &pid]).unwrap()).unwrap();
        assert!(!dump.contains("ABCDE12345") && !dump.contains("github_pat_"), "{dump}");
        assert!(dump.contains("\"integration\":\"github\"") && dump.contains("\"field\":\"token\""));
        // De waarden staan wel in de (nep-)keychain, onder een vaste sleutelstructuur.
        assert!(mem.0.lock().unwrap().keys().all(|k| k.starts_with("v1/")));
    }

    #[test]
    fn scope_decides_whether_a_secret_is_shared_between_projects() {
        let (v, _, pid, _) = test_vault();
        let other = v.db().project_insert(&crate::db::NewProject { name: "B".into(), slug: "b".into(), path: "/b".into(), template: "blank".into(), description: String::new(), bundle_id: "x.y".into(), initial_commit: None, profile_version: 1 }).unwrap().id;
        // Apple-account: globaal, dus ook zichtbaar in een ander project.
        v.set_text(&pid, "apple", "team_id", "TEAM123456").unwrap();
        assert_eq!(v.get_text(&other, "apple", "team_id").unwrap().as_deref(), Some("TEAM123456"));
        // Supabase-URL: per project.
        v.set_text(&pid, "supabase", "url", "https://abcdefghijklmnopqrst.supabase.co").unwrap();
        assert!(v.has(&pid, "supabase", "url") && !v.has(&other, "supabase", "url"));
        assert!(v.get_text(&other, "supabase", "url").unwrap().is_none());
        // Een projectveld zonder geldig project-id wordt geweigerd.
        assert!(matches!(v.set_text("", "supabase", "url", "https://a.supabase.co"), Err(AppError::Invalid(_))));
        assert!(matches!(v.set_text("../x", "supabase", "url", "https://a.supabase.co"), Err(AppError::Invalid(_))));
    }

    #[test]
    fn files_are_encrypted_at_rest_with_the_key_in_the_keychain() {
        let (v, mem, pid, dir) = test_vault();
        let derived = v.set_file(&pid, "google_play", "service_account", "sa.json", SA).unwrap();
        assert_eq!(derived["client_email"], "bot@proj-1.iam.gserviceaccount.com");
        let enc = std::fs::read(dir.join("secrets/global/google_play/service_account.enc")).unwrap();
        let hay = String::from_utf8_lossy(&enc);
        assert!(!hay.contains("VAULTSAKEYLINE") && !hay.contains("service_account") && !hay.contains("client_email"), "ciphertext must not leak plaintext");
        assert_eq!(v.get_file(&pid, "google_play", "service_account").unwrap().unwrap(), SA);
        // Zonder de sleutel uit de keychain is het bestand onleesbaar.
        mem.0.lock().unwrap().retain(|k, _| !k.ends_with("#key"));
        assert!(matches!(v.get_file(&pid, "google_play", "service_account"), Err(AppError::Keychain(_))));
        // De sleutel verandert bij elke nieuwe waarde (geen hergebruik van nonce/sleutel).
        let (v2, mem2, pid2, _) = test_vault();
        v2.set_file(&pid2, "apple", "private_key", "a.p8", PEM).unwrap();
        let k1 = mem2.0.lock().unwrap().iter().find(|(k, _)| k.ends_with("#key")).unwrap().1.clone();
        v2.set_file(&pid2, "apple", "private_key", "a.p8", PEM).unwrap();
        let k2 = mem2.0.lock().unwrap().iter().find(|(k, _)| k.ends_with("#key")).unwrap().1.clone();
        assert_ne!(k1, k2);
        // Beschadigd bestand: nette fout, geen paniek.
        let path = _dir_of(&v2).join("secrets/global/apple/private_key.enc");
        let mut b = std::fs::read(&path).unwrap();
        let last = b.len() - 1;
        b[last] ^= 0xFF;
        std::fs::write(&path, b).unwrap();
        assert!(matches!(v2.get_file(&pid2, "apple", "private_key"), Err(AppError::Keychain(_))));
    }

    fn _dir_of(v: &Vault) -> PathBuf {
        v.legacy_dir.clone()
    }

    #[test]
    fn invalid_values_are_rejected_before_anything_is_stored() {
        let (v, mem, pid, dir) = test_vault();
        let e = v.set_text(&pid, "apple", "key_id", "nope").unwrap_err();
        assert!(matches!(&e, AppError::Rejected { code, .. } if code == "pattern"));
        let e = v.set_file(&pid, "apple", "private_key", "key.txt", PEM).unwrap_err();
        assert!(matches!(&e, AppError::Rejected { code, .. } if code == "file_type"));
        assert!(mem.0.lock().unwrap().is_empty() && !v.has(&pid, "apple", "key_id"));
        assert!(!dir.join("secrets").exists());
        assert!(matches!(v.set_text(&pid, "nope", "x", "y"), Err(AppError::NotFound(_))));
        // De fout bevat de waarde niet.
        let v2 = serde_json::to_value(v.set_text(&pid, "apple", "key_id", "secretvalue-xyz").unwrap_err()).unwrap();
        assert!(!v2.to_string().contains("secretvalue"), "{v2}");
    }

    #[test]
    fn delete_removes_value_file_key_metadata_and_scrubber_entry() {
        let (v, mem, pid, dir) = test_vault();
        // Eigen unieke inhoud en een projectveld (eigen scrubber-bron), zodat parallelle tests elkaar niet storen.
        let unique = format!("UNIQUEDELKEY{}", uuid::Uuid::new_v4().simple());
        let sa = format!(r#"{{"type":"service_account","client_email":"a@b.c","private_key":"-----BEGIN PRIVATE KEY-----\n{unique}\n-----END PRIVATE KEY-----\n"}}"#);
        v.set_file(&pid, "firebase", "service_account", "sa.json", sa.as_bytes()).unwrap();
        assert!(crate::scrub::scrub(&format!("x {unique} y")).contains("••••"), "registered after set");
        v.delete(&pid, "firebase", "service_account").unwrap();
        assert!(!v.has(&pid, "firebase", "service_account") && v.get_file(&pid, "firebase", "service_account").unwrap().is_none());
        assert!(mem.0.lock().unwrap().is_empty());
        assert!(!dir.join("secrets").join(&pid).join("firebase/service_account.enc").exists());
        assert!(crate::scrub::scrub(&format!("x {unique} y")).contains(&unique), "unregistered after delete");
        v.delete(&pid, "firebase", "service_account").unwrap(); // dubbel verwijderen mag
    }

    #[test]
    fn status_walks_from_off_to_complete_and_warns_on_failed_tests_and_expiry() {
        let (v, _, pid, _) = test_vault();
        let m = manifest::get("apple").unwrap();
        assert_eq!(v.status_of(&pid, m).unwrap().state, "off");
        v.db().integration_set_enabled(&pid, "apple", true).unwrap();
        let st = v.status_of(&pid, m).unwrap();
        assert_eq!((st.state.as_str(), st.missing.len()), ("missing", 4));
        v.set_text(&pid, "apple", "issuer_id", "69a6de7e-1234-47e3-e053-5b8c7c11a4d1").unwrap();
        assert_eq!(v.status_of(&pid, m).unwrap().state, "incomplete");
        v.set_text(&pid, "apple", "key_id", "ABCDE12345").unwrap();
        v.set_text(&pid, "apple", "team_id", "TEAM123456").unwrap();
        v.set_file(&pid, "apple", "private_key", "AuthKey.p8", PEM).unwrap();
        let st = v.status_of(&pid, m).unwrap();
        assert_eq!((st.state.as_str(), st.missing.is_empty()), ("complete", true));
        assert_eq!(st.fields.iter().find(|f| f.id == "private_key").unwrap().file_name.as_deref(), Some("AuthKey.p8"));
        v.db().integration_record_test(&pid, "apple", false, "invalid", None).unwrap();
        assert_eq!(v.status_of(&pid, m).unwrap().state, "warning");
        v.db().integration_record_test(&pid, "apple", true, "ok", Some("2999-01-01 00:00:00 UTC")).unwrap();
        assert_eq!(v.status_of(&pid, m).unwrap().state, "complete");
        v.db().integration_record_test(&pid, "apple", true, "ok", Some("2001-01-01")).unwrap();
        let st = v.status_of(&pid, m).unwrap();
        assert_eq!(st.state, "warning");
        assert!(st.expires_in_days.unwrap() < 0, "already expired");
        // Een nieuwe waarde maakt het oude testresultaat ongeldig.
        v.set_text(&pid, "apple", "key_id", "ZZZZZ12345").unwrap();
        let st = v.status_of(&pid, m).unwrap();
        assert!(st.last_tested.is_none() && st.state == "complete");
    }

    #[test]
    fn dependencies_that_are_off_are_reported() {
        let (v, _, pid, _) = test_vault();
        v.db().integration_set_enabled(&pid, "google_play", true).unwrap();
        let st = v.status_of(&pid, manifest::get("google_play").unwrap()).unwrap();
        assert_eq!(st.dependencies_off, vec!["android_signing"]);
        v.db().integration_set_enabled(&pid, "android_signing", true).unwrap();
        assert!(v.status_of(&pid, manifest::get("google_play").unwrap()).unwrap().dependencies_off.is_empty());
    }

    #[test]
    fn date_math_matches_known_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
        assert_eq!(days_until("1970-01-11 00:00:00 UTC", 0), Some(10));
        assert_eq!(days_until("1970-01-01", 86400 * 3 + 5), Some(-3));
        assert_eq!(days_until("garbage", 0), None);
    }

    #[test]
    fn env_for_injects_values_and_temp_files_that_disappear() {
        let (v, _, pid, dir) = test_vault();
        v.set_text(&pid, "android_signing", "keystore_password", "pw-1234567890").unwrap();
        v.set_file(&pid, "android_signing", "keystore", "up.jks", &[0xFE, 0xED, 0xFE, 0xED, 9, 9, 9]).unwrap();
        let (path, vars) = {
            let b = v.env_for(&pid, &["android_signing"], "build").unwrap();
            let get = |k: &str| b.vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
            assert_eq!(get("STUDIO_KEYSTORE_PASSWORD").as_deref(), Some("pw-1234567890"));
            assert_eq!(get("STUDIO_KEY_ALIAS").as_deref(), Some("upload"), "the default alias is used when none is stored");
            assert!(get("STUDIO_KEY_PASSWORD").is_none(), "an optional field that is not set is not injected");
            let path = PathBuf::from(get("STUDIO_KEYSTORE_PATH").unwrap());
            assert!(path.starts_with(dir.join("tmp")) && path.extension().unwrap() == "jks");
            assert_eq!(std::fs::read(&path).unwrap(), vec![0xFE, 0xED, 0xFE, 0xED, 9, 9, 9]);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "owner only");
            }
            (path, b.vars.len())
        };
        assert!(!path.exists() && !path.parent().unwrap().exists(), "the decrypted copy is gone after the subprocess");
        assert_eq!(vars, 3);
        // "laatst gebruikt" wordt bijgehouden.
        let m = v.meta(&pid, "android_signing", "keystore_password").unwrap();
        assert_eq!(m.used_by, "build");
        assert!(m.last_used.is_some());
    }

    #[test]
    fn backup_roundtrips_and_refuses_wrong_passphrases_and_garbage() {
        let (v, _, pid, _) = test_vault();
        v.set_text(&pid, "android_signing", "keystore_password", "pw-1234567890").unwrap();
        v.set_file(&pid, "android_signing", "keystore", "up.jks", &[0xFE, 0xED, 0xFE, 0xED, 1, 2, 3]).unwrap();
        v.set_text(&pid, "apple", "key_id", "ABCDE12345").unwrap();
        assert!(matches!(v.export(&pid, "short"), Err(AppError::Rejected { code, .. }) if code == "passphrase_short"));
        let blob = v.export(&pid, "correct horse battery").unwrap();
        assert!(!String::from_utf8_lossy(&blob).contains("pw-1234567890") && blob.starts_with(b"STUDIOVAULT1\n"));
        // In een verse kluis (andere computer, ander project-id) terugzetten.
        let (v2, _, pid2, _) = test_vault();
        assert!(matches!(v2.import(&pid2, "wrong passphrase!", &blob), Err(AppError::Rejected { code, .. }) if code == "backup_unreadable"));
        assert!(matches!(v2.import(&pid2, "x", b"garbage"), Err(AppError::Rejected { .. })));
        let mut damaged = blob.clone();
        let l = damaged.len() - 3;
        damaged[l] ^= 1;
        assert!(v2.import(&pid2, "correct horse battery", &damaged).is_err());
        assert_eq!(v2.import(&pid2, "correct horse battery", &blob).unwrap(), 3);
        assert_eq!(v2.get_text(&pid2, "android_signing", "keystore_password").unwrap().as_deref(), Some("pw-1234567890"));
        assert_eq!(v2.get_file(&pid2, "android_signing", "keystore").unwrap().unwrap(), vec![0xFE, 0xED, 0xFE, 0xED, 1, 2, 3]);
        assert_eq!(v2.get_text(&pid2, "apple", "key_id").unwrap().as_deref(), Some("ABCDE12345"));
    }

    #[test]
    fn legacy_storage_is_imported_once_and_the_plaintext_keystore_is_removed() {
        let (v, mem, pid, dir) = test_vault();
        let old = dir.join("keystores").join(&pid);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("upload-keystore.jks"), [0xFE, 0xED, 0xFE, 0xED, 5, 5, 5]).unwrap();
        mem.0.lock().unwrap().insert(format!("{pid}:android_keystore_password"), "legacy-password-123".into());
        mem.0.lock().unwrap().insert(format!("{pid}:play_service_account_json"), String::from_utf8(SA.to_vec()).unwrap());
        v.migrate_legacy(&pid);
        assert!(!old.join("upload-keystore.jks").exists(), "no plaintext keystore left behind");
        assert!(v.has(&pid, "android_signing", "keystore") && v.has(&pid, "android_signing", "keystore_password") && v.has(&pid, "google_play", "service_account"));
        assert_eq!(v.get_text(&pid, "android_signing", "keystore_password").unwrap().as_deref(), Some("legacy-password-123"));
        assert!(!mem.0.lock().unwrap().contains_key(&format!("{pid}:android_keystore_password")), "old keychain entry removed");
        v.migrate_legacy(&pid); // tweede keer: niets te doen
        assert!(v.has(&pid, "android_signing", "keystore"));
    }

    #[test]
    fn switching_off_with_delete_removes_only_the_project_values_of_that_integration() {
        let (v, _, pid, _) = test_vault();
        v.set_text(&pid, "supabase", "url", "https://abcdefghijklmnopqrst.supabase.co").unwrap();
        v.set_text(&pid, "android_signing", "alias", "upload").unwrap();
        v.set_text(&pid, "apple", "key_id", "ABCDE12345").unwrap();
        assert_eq!(v.delete_integration(&pid, "supabase").unwrap(), 1);
        assert!(!v.has(&pid, "supabase", "url") && v.has(&pid, "android_signing", "alias"));
        assert_eq!(v.delete_integration(&pid, "apple").unwrap(), 0, "global values stay");
        assert!(v.has(&pid, "apple", "key_id"));
    }

    #[test]
    fn deleting_a_project_removes_its_project_secrets_but_not_the_global_ones() {
        let (v, _, pid, _) = test_vault();
        v.set_text(&pid, "supabase", "url", "https://abcdefghijklmnopqrst.supabase.co").unwrap();
        v.set_text(&pid, "apple", "key_id", "ABCDE12345").unwrap();
        v.delete_project(&pid);
        assert!(!v.has(&pid, "supabase", "url") && v.has(&pid, "apple", "key_id"));
    }

    /// De echte keychain (Windows/macOS altijd; Linux met keyutils). Niet overal beschikbaar.
    #[test]
    #[ignore]
    fn real_keychain_roundtrip_with_an_encrypted_file() {
        let dir = std::env::temp_dir().join(format!("studio-vault-real-{}", uuid::Uuid::new_v4()));
        let db = Arc::new(Db::open_in_memory().unwrap());
        let p = db.project_insert(&crate::db::NewProject { name: "R".into(), slug: "r".into(), path: "/r".into(), template: "blank".into(), description: String::new(), bundle_id: "x.y".into(), initial_commit: None, profile_version: 1 }).unwrap();
        let v = Vault::new(Arc::new(KeychainBackend), &dir, db);
        v.set_file(&p.id, "google_play", "service_account", "sa.json", SA).unwrap();
        assert_eq!(v.get_file(&p.id, "google_play", "service_account").unwrap().unwrap(), SA);
        v.delete(&p.id, "google_play", "service_account").unwrap();
        assert!(v.get_file(&p.id, "google_play", "service_account").unwrap().is_none());
    }
}
