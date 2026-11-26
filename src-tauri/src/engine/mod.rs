//! Engine-abstractielaag: Studio praat nooit rechtstreeks met een AI-API, alleen met een
//! `AiEngine`. Nu is er één implementatie (Claude Code CLI); Codex CLI, Gemini CLI of een
//! API-sleutel kunnen later naast `claude.rs` worden gezet zonder de rest te wijzigen.

pub mod claude;
pub mod stream;

use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Genormaliseerde gebeurtenissen die elke engine moet kunnen produceren.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    /// De gebruiker stuurde een bericht (alleen voor opslag/herspelen).
    User { text: String, context: Option<String> },
    Init {
        session_id: String,
        model: Option<String>,
        /// (servernaam, status) zoals de CLI die bij de start meldt.
        #[serde(default)]
        mcp: Vec<(String, String)>,
    },
    TextDelta { message_id: String, text: String },
    Text { message_id: String, text: String },
    ToolUse {
        id: String,
        name: String,
        /// read | edit | command | network | studio | other
        kind: String,
        summary: String,
        path: Option<String>,
        detail: Option<String>,
    },
    ToolResult { tool_use_id: String, ok: bool, text: String },
    Snapshot { commit: String, label: String },
    Done {
        ok: bool,
        text: String,
        duration_ms: Option<u64>,
        /// Herkende oorzaak bij een mislukte beurt: not_logged_in | failed
        code: Option<String>,
        #[serde(default)]
        cost_usd: Option<f64>,
        #[serde(default)]
        turns: Option<u32>,
        /// Totaal aantal tokens van de beurt (invoer, uitvoer en cache) waar de CLI dat meldt.
        #[serde(default)]
        tokens: Option<u64>,
        /// Ruwe tokens per soort, voor zover de CLI ze geeft.
        #[serde(default)]
        usage: Option<TokenUsage>,
    },
    /// Een statusregel van Studio zelf (werkgebied gekozen, gerepareerd, teruggedraaid, ...). De UI vertaalt `code`.
    Status { code: String, detail: Option<String> },
    Error { code: String, message: String },
}

/// Ruwe tokens van een beurt: invoer, uitvoer, uit de cache gelezen en naar de cache geschreven.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl TokenUsage {
    pub fn add(&mut self, o: &TokenUsage) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
    }
}

pub struct TurnRequest {
    pub project_dir: PathBuf,
    pub prompt: String,
    /// Sessie-id van de engine om het gesprek te hervatten.
    pub resume: Option<String>,
    /// MCP-configuratiebestand met de lokale Studio-server.
    pub mcp_config: PathBuf,
    /// UI-taal ("en", "nl"): Claude antwoordt in die taal.
    pub language: String,
    /// Extra instellingen voor alleen deze run (bv. het werkgebied), als bestand.
    pub settings: Option<PathBuf>,
    /// Alias of naam van het model voor deze beurt (`None` = de standaard van de CLI).
    pub model: Option<String>,
    /// Alleen een plan maken, niets bewerken (plan-modus van de CLI).
    pub plan_only: bool,
}

pub struct TurnOutcome {
    pub engine_session_id: Option<String>,
}

/// Handvat om een lopende beurt te kunnen stoppen.
#[derive(Default)]
pub struct RunHandle {
    pub child: Mutex<Option<Child>>,
    pub cancelled: AtomicBool,
}

impl RunHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = c.kill();
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub trait AiEngine: Send + Sync {
    fn id(&self) -> &'static str;
    /// Voert één beurt uit en meldt gebeurtenissen via `on_event`.
    /// Eén korte vraag zonder gereedschap of project (goedkoop model). `None` als de engine dat niet kan.
    fn classify(&self, _question: &str, _model: Option<&str>) -> Option<String> {
        None
    }
    /// Kent de CLI dit model (alias)? Zo komen modelnamen uit de CLI en de configuratie, niet uit de code.
    fn model_available(&self, _alias: &str) -> bool {
        true
    }
    fn run_turn(&self, req: &TurnRequest, run: &RunHandle, on_event: &mut dyn FnMut(ChatEvent)) -> AppResult<TurnOutcome>;
}
