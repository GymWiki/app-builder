//! Eén foutentype voor alle Tauri-commands.
//! De frontend krijgt `{ code, message }`; `code` wordt vertaald naar een
//! vriendelijke melding mét oplossing (i18n: `errors.<code>`), `message` is
//! alleen technische context voor het logbestand.

use serde::{ser::SerializeStruct, Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0} not found")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("required tool is missing: {0}")]
    ToolMissing(String),
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("command timed out: {0}")]
    Timeout(String),
    #[error("folder already exists: {0}")]
    PathExists(String),
    #[error("refused for safety: {0}")]
    Unsafe(String),
    #[error("terminal error: {0}")]
    Pty(String),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("not supported yet: {0}")]
    Unsupported(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
            AppError::Json(_) => "invalid",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::ToolMissing(_) => "tool_missing",
            AppError::CommandFailed(_) => "command_failed",
            AppError::Timeout(_) => "timeout",
            AppError::PathExists(_) => "path_exists",
            AppError::Unsafe(_) => "unsafe",
            AppError::Pty(_) => "pty",
            AppError::Keychain(_) => "keychain",
            AppError::Unsupported(_) => "unsupported",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &crate::logging::redact(&self.to_string()))?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_redacted_message() {
        let e = AppError::CommandFailed("boom sk-ant-api03-abcdefghijklmnopqrstuvwxyz".into());
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "command_failed");
        assert!(!v["message"].as_str().unwrap().contains("abcdefghijkl"));
    }
}
