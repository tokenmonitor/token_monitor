//! Read-only access to the Swift app's Keychain vault.
//!
//! The Swift app keeps every provider secret in one generic-password item
//! (service [`SWIFT_SERVICE`], account [`SWIFT_VAULT_ACCOUNT`]) whose data is a
//! JSON object `{"entries": {"<prefix><key>": "<secret>"}, ...}`. The user
//! approved the beta *reading* that item so nobody signs in twice (issue #89,
//! decision 8 amendment). This module only reads: the trait has no write or
//! delete, nothing is copied to disk, and no value ever reaches a log or an
//! error string.
//!
//! The Keychain call itself sits behind [`SecretSource`] (real implementation in
//! `shell::keychain_mac`, a fake here for hermetic tests).

use std::collections::BTreeMap;

use tokenmon_platform::secrets::{PayloadError, Vault, decode_payload};
use zeroize::Zeroizing;

/// Keychain service of the Swift app's vault item.
pub const SWIFT_SERVICE: &str = "com.modelmonitor.app.credentials";
/// Account of the vault item inside that service.
pub const SWIFT_VAULT_ACCOUNT: &str = "vault";

/// Why a secret could not be read. Carries no secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The user (or policy) denied access to the item.
    Denied,
    /// The Keychain failed for another reason (status code only).
    Failed(i32),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied => f.write_str("access to the Keychain item was denied"),
            Self::Failed(code) => write!(f, "the Keychain returned status {code}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// A read-only secret store: one lookup, no writes.
pub trait SecretSource: Send + Sync {
    /// The item's data, `Ok(None)` when it does not exist.
    fn read(&self, service: &str, account: &str)
    -> Result<Option<Zeroizing<Vec<u8>>>, SourceError>;
}

/// A fake source holding items in memory; counts reads so tests can assert
/// that only the one vault item is touched.
#[derive(Default)]
pub struct FakeSecretSource {
    items: BTreeMap<(String, String), Vec<u8>>,
    failure: Option<SourceError>,
    reads: std::sync::Mutex<Vec<(String, String)>>,
}

impl FakeSecretSource {
    /// A source with no items.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an item.
    #[must_use]
    pub fn with_item(mut self, service: &str, account: &str, data: &[u8]) -> Self {
        self.items
            .insert((service.to_owned(), account.to_owned()), data.to_vec());
        self
    }

    /// Makes every read fail with `error`.
    #[must_use]
    pub fn failing(mut self, error: SourceError) -> Self {
        self.failure = Some(error);
        self
    }

    /// The `(service, account)` pairs read so far.
    pub fn reads(&self) -> Vec<(String, String)> {
        self.reads.lock().expect("reads lock").clone()
    }
}

impl SecretSource for FakeSecretSource {
    fn read(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, SourceError> {
        self.reads
            .lock()
            .expect("reads lock")
            .push((service.to_owned(), account.to_owned()));
        if let Some(e) = &self.failure {
            return Err(e.clone());
        }
        Ok(self
            .items
            .get(&(service.to_owned(), account.to_owned()))
            .map(|d| Zeroizing::new(d.clone())))
    }
}

/// What a seeding run did. Counts and account names only, never values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SeedReport {
    /// The Swift vault item was found.
    pub found: bool,
    /// Accounts copied into the beta's in-process vault.
    pub accounts: Vec<String>,
    /// Accounts the beta's vault refused.
    pub refused: usize,
}

/// Why seeding stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedError {
    /// The Keychain read failed.
    Source(SourceError),
    /// The item's data is not the vault JSON.
    Payload(PayloadError),
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(e) => write!(f, "{e}"),
            Self::Payload(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SeedError {}

/// Reads the Swift vault from `source` (one read, read-only) and sets every
/// entry in `vault` (the beta's own vault, sealed with a per-process key).
///
/// A missing item is not an error: the beta just starts signed out.
pub fn seed_from_swift_vault(
    source: &dyn SecretSource,
    vault: &Vault,
) -> Result<SeedReport, SeedError> {
    let Some(data) = source
        .read(SWIFT_SERVICE, SWIFT_VAULT_ACCOUNT)
        .map_err(SeedError::Source)?
    else {
        return Ok(SeedReport::default());
    };
    let payload = decode_payload(&data).map_err(SeedError::Payload)?;
    let mut report = SeedReport {
        found: true,
        ..SeedReport::default()
    };
    for (account, value) in &payload.entries {
        if vault.set(value, account) {
            report.accounts.push(account.clone());
        } else {
            report.refused += 1;
        }
    }
    Ok(report)
}
