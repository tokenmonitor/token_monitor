//! Credentials the M0 spike takes by hand (no sign-in UI yet).
//!
//! The beta reads its own, namespaced variables so a developer's real
//! `OPENROUTER_API_KEY` (or any other provider variable) is never picked up by
//! accident. Nothing here logs or formats a secret: errors name the variable
//! or the file, never the value.

use std::path::{Path, PathBuf};

/// Variable holding the OpenRouter API key itself.
pub const OPENROUTER_KEY_ENV: &str = "TOKENMON_RUSTBETA_OPENROUTER_API_KEY";

/// Variable holding the path of a file whose content is the OpenRouter key.
pub const OPENROUTER_KEY_FILE_ENV: &str = "TOKENMON_RUSTBETA_OPENROUTER_KEY_FILE";

/// Where a key came from (for the status line; never the key itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// The environment variable [`OPENROUTER_KEY_ENV`].
    Env,
    /// The file named by [`OPENROUTER_KEY_FILE_ENV`].
    File(PathBuf),
    /// A built-in placeholder for the loopback mock server (not a secret).
    Mock,
}

/// A key and its origin. `Debug` redacts the key.
#[derive(Clone, PartialEq, Eq)]
pub struct LoadedKey {
    key: String,
    /// Where it was read from.
    pub source: KeySource,
}

impl LoadedKey {
    /// A placeholder token for the loopback mock server.
    pub fn mock(token: &str) -> Self {
        Self {
            key: token.to_owned(),
            source: KeySource::Mock,
        }
    }

    /// The secret. Callers hand it straight to the credential session.
    pub fn expose(&self) -> &str {
        &self.key
    }
}

impl std::fmt::Debug for LoadedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedKey")
            .field("key", &"<redacted>")
            .field("source", &self.source)
            .finish()
    }
}

/// Why a key could not be loaded. Never contains the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// The key file could not be read.
    UnreadableFile(String),
    /// The variable or file is present but holds only whitespace.
    Empty(String),
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnreadableFile(path) => write!(f, "cannot read the key file {path}"),
            Self::Empty(origin) => write!(f, "{origin} is set but empty"),
        }
    }
}

impl std::error::Error for CredentialError {}

/// Resolves the OpenRouter key from `env` (a variable lookup) and `read` (a
/// file reader): the variable wins over the file; `Ok(None)` when neither is
/// set. Surrounding whitespace (a trailing newline) is trimmed.
///
/// ```
/// use tokenmon_macos::credentials::*;
/// let env = |n: &str| (n == OPENROUTER_KEY_ENV).then(|| " sk-or-test \n".to_owned());
/// let got = load_openrouter_key(&env, &|_| Ok(String::new())).unwrap().unwrap();
/// assert_eq!(got.expose(), "sk-or-test");
/// assert_eq!(got.source, KeySource::Env);
/// ```
pub fn load_openrouter_key(
    env: &dyn Fn(&str) -> Option<String>,
    read: &dyn Fn(&Path) -> Result<String, std::io::Error>,
) -> Result<Option<LoadedKey>, CredentialError> {
    if let Some(raw) = env(OPENROUTER_KEY_ENV) {
        return trimmed(&raw, KeySource::Env, OPENROUTER_KEY_ENV).map(Some);
    }
    let Some(path) = env(OPENROUTER_KEY_FILE_ENV).filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    let shown = path.display().to_string();
    let raw = read(&path).map_err(|_| CredentialError::UnreadableFile(shown.clone()))?;
    trimmed(&raw, KeySource::File(path), &shown).map(Some)
}

fn trimmed(raw: &str, source: KeySource, origin: &str) -> Result<LoadedKey, CredentialError> {
    let key = raw.trim().to_owned();
    if key.is_empty() {
        return Err(CredentialError::Empty(origin.to_owned()));
    }
    Ok(LoadedKey { key, source })
}
