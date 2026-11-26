#![allow(clippy::unwrap_used)]
//! Owner-only creation of secrets files and the no-plaintext refusal path.

use std::fs;
use std::sync::Arc;

use tempfile::TempDir;
use tokenmon_core::{CredentialKey, CredentialStore};
use tokenmon_platform::secrets::{
    FakeEncryptor, SecretRoutingCredentialStore, SecretStoreError, VAULT_UNAVAILABLE_REASON, Vault,
    create_new_owner_only,
};

#[test]
fn create_new_refuses_an_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("x.dat");
    fs::write(&path, b"pre-created by someone else").unwrap();
    assert!(create_new_owner_only(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"pre-created by someone else");
}

#[cfg(unix)]
#[test]
fn every_secrets_file_is_created_0600() {
    use std::os::unix::fs::PermissionsExt;
    use tokenmon_platform::secrets::{VAULT_FILE_NAME, atomic_write};
    let dir = TempDir::new().unwrap();
    let vault = Arc::new(Vault::new(dir.path(), Arc::new(FakeEncryptor::default())));
    assert!(vault.set("secret", "a_session"));
    atomic_write(&dir.path().join("a_email.dat"), b"me@x").unwrap();
    for name in [VAULT_FILE_NAME, "a_email.dat"] {
        let mode = fs::metadata(dir.path().join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}");
    }
}

fn broken_vault_store(dir: &TempDir) -> (SecretRoutingCredentialStore, Arc<FakeEncryptor>) {
    let enc = Arc::new(FakeEncryptor::default());
    let vault = Arc::new(Vault::new(dir.path(), enc.clone()));
    enc.set_fail_protect(true);
    (
        SecretRoutingCredentialStore::new(vault, dir.path(), "a_"),
        enc,
    )
}

#[test]
fn failed_vault_write_surfaces_a_typed_vault_unavailable_error() {
    let dir = TempDir::new().unwrap();
    let (store, _) = broken_vault_store(&dir);
    let err = store.set(CredentialKey::Session, "cookie").unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("credential store write failed: {VAULT_UNAVAILABLE_REASON}")
    );
    let ui = SecretStoreError::classify(&err);
    assert_eq!(ui, SecretStoreError::VaultUnavailable);
    assert_eq!(ui.to_string(), tokenmon_core::VAULT_UNAVAILABLE_MESSAGE);
    assert!(!dir.path().join("a_session.dat").exists(), "no plaintext");
    assert_eq!(store.get(CredentialKey::Session), None);
}

#[test]
fn other_write_failures_are_classified_as_other() {
    let err = tokenmon_core::CredentialStoreError::WriteFailed("disk full".into());
    assert_eq!(
        SecretStoreError::classify(&err),
        SecretStoreError::Other("disk full".into())
    );
}
