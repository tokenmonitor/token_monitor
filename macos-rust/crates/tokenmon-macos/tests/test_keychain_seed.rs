//! Seeding the beta's vault from the Swift app's Keychain item, read-only and
//! without ever exposing a value.

use std::sync::Arc;

use tokenmon_macos::keychain::{
    FakeSecretSource, SWIFT_SERVICE, SWIFT_VAULT_ACCOUNT, SeedError, SourceError,
    seed_from_swift_vault,
};
use tokenmon_platform::secrets::{FakeEncryptor, PayloadError, Vault};

const SECRET: &str = "sess-SECRET-VALUE-123";

fn vault(dir: &tempfile::TempDir) -> Vault {
    Vault::new(dir.path(), Arc::new(FakeEncryptor::default()))
}

fn swift_item() -> FakeSecretSource {
    let json = format!(
        r#"{{"entries":{{"claude_auth_session":"{SECRET}","chatgpt_auth_session":"other"}},"checkedSeparateItems":["x"]}}"#
    );
    FakeSecretSource::new().with_item(SWIFT_SERVICE, SWIFT_VAULT_ACCOUNT, json.as_bytes())
}

/// Every entry of the Swift vault is copied into the beta vault, and the only
/// Keychain item read is the vault item.
#[test]
fn test_entries_are_copied_and_only_the_vault_item_is_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = vault(&dir);
    let source = swift_item();
    let report = seed_from_swift_vault(&source, &vault).expect("seeded");
    assert!(report.found);
    assert_eq!(report.refused, 0);
    assert_eq!(
        report.accounts,
        ["chatgpt_auth_session", "claude_auth_session"]
    );
    assert_eq!(vault.value("claude_auth_session").as_deref(), Some(SECRET));
    assert_eq!(
        source.reads(),
        [(SWIFT_SERVICE.to_owned(), SWIFT_VAULT_ACCOUNT.to_owned())]
    );
}

/// The report carries names and counts only; no value reaches Debug output.
#[test]
fn test_report_never_contains_secret_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    let report = seed_from_swift_vault(&swift_item(), &vault(&dir)).expect("seeded");
    assert!(!format!("{report:?}").contains(SECRET));
}

/// No Swift item means the beta starts signed out, not an error.
#[test]
fn test_missing_item_is_a_signed_out_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let report = seed_from_swift_vault(&FakeSecretSource::new(), &vault(&dir)).expect("ok");
    assert!(!report.found);
    assert_eq!(report.accounts.len(), 0);
}

/// A denied or failed Keychain read is a typed error with no secret in it.
#[test]
fn test_keychain_failures_are_typed() {
    let dir = tempfile::tempdir().expect("tempdir");
    for failure in [SourceError::Denied, SourceError::Failed(-25299)] {
        let source = FakeSecretSource::new().failing(failure.clone());
        let err = seed_from_swift_vault(&source, &vault(&dir)).expect_err("fails");
        assert_eq!(err, SeedError::Source(failure));
        assert_ne!(err.to_string(), "");
    }
}

/// Item data that is not the vault JSON is refused without echoing its content.
#[test]
fn test_malformed_payload_is_refused_without_echoing_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source =
        FakeSecretSource::new().with_item(SWIFT_SERVICE, SWIFT_VAULT_ACCOUNT, SECRET.as_bytes());
    let err = seed_from_swift_vault(&source, &vault(&dir)).expect_err("malformed");
    assert_eq!(err, SeedError::Payload(PayloadError::Malformed));
    assert!(!format!("{err} {err:?}").contains(SECRET));
}

/// A vault that refuses writes is counted, not silently dropped.
#[test]
fn test_refused_writes_are_counted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let encryptor = Arc::new(FakeEncryptor::default());
    encryptor.set_fail_protect(true);
    let vault = Vault::new(dir.path(), encryptor);
    let report = seed_from_swift_vault(&swift_item(), &vault).expect("ok");
    assert_eq!(report.refused, 2);
    assert_eq!(report.accounts.len(), 0);
}

/// The source trait offers no way to write: the fake only exposes reads.
#[test]
fn test_source_reads_are_recorded_per_call() {
    let source = swift_item();
    let _ = tokenmon_macos::keychain::SecretSource::read(&source, "a", "b");
    assert_eq!(source.reads().len(), 1);
}
