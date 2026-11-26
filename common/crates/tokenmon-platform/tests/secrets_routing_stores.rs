#![allow(clippy::unwrap_used)]
//! Routing between the vault and the plain files: labels never need the vault,
//! and removing a secret clears every place it could be (upstream #57).

use std::fs;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;
use tokenmon_core::{CredentialKey, CredentialStore};
use tokenmon_platform::secrets::{
    FakeEncryptor, FileBackedCredentialStore, SecretRoutingCredentialStore, Vault,
};

const PREFIX: &str = "claude_auth_";

fn vault_in(dir: &Path, enc: &Arc<FakeEncryptor>) -> Arc<Vault> {
    Arc::new(Vault::new(dir, enc.clone()))
}

/// Non-secret labels keep going to the file store while the vault refuses secrets.
#[test]
fn labels_still_save_while_the_secure_store_refuses() {
    let dir = TempDir::new().unwrap();
    let enc = Arc::new(FakeEncryptor::default());
    let store = SecretRoutingCredentialStore::new(vault_in(dir.path(), &enc), dir.path(), PREFIX);
    enc.set_fail_protect(true);

    store
        .set(CredentialKey::Email, "user@example.com")
        .expect("a label never needs the vault");

    let file = FileBackedCredentialStore::new(dir.path(), PREFIX);
    assert_eq!(
        fs::read_to_string(file.path(CredentialKey::Email)).unwrap(),
        "user@example.com"
    );
    assert_eq!(
        store.get(CredentialKey::Email).as_deref(),
        Some("user@example.com")
    );
    assert!(
        store.set(CredentialKey::Session, "secret").is_err(),
        "the secret itself is still refused"
    );
}

/// Removing a secret clears the vault and a stray plaintext copy, so signing
/// out also deletes a plaintext session left behind, not only the vault entry.
#[test]
fn remove_clears_a_secret_from_both_stores() {
    let dir = TempDir::new().unwrap();
    let enc = Arc::new(FakeEncryptor::default());
    let store = SecretRoutingCredentialStore::new(vault_in(dir.path(), &enc), dir.path(), PREFIX);
    store.set(CredentialKey::Session, "sid=vault").unwrap();
    let stray = FileBackedCredentialStore::new(dir.path(), PREFIX).path(CredentialKey::Session);
    fs::write(&stray, "sid=stray").unwrap();

    store.remove(CredentialKey::Session);

    assert_eq!(store.get(CredentialKey::Session), None);
    assert!(!stray.exists(), "the stray plaintext copy is deleted");
    let reopened =
        SecretRoutingCredentialStore::new(vault_in(dir.path(), &enc), dir.path(), PREFIX);
    assert_eq!(reopened.get(CredentialKey::Session), None);
}
