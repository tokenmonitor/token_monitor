//! Vault write failures and the pending-removals record: replay on load,
//! garbage tolerance, and failure handling.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;
/// The on-disk name of the pending-removals record.
const PENDING_FILE_NAME: &str = "vault_pending_removals.json";
use tokenmon_platform::secrets::{FakeEncryptor, Vault};

fn vault(dir: &Path, enc: &Arc<FakeEncryptor>) -> Vault {
    Vault::new(dir, enc.clone())
}

fn pending_file(dir: &Path) -> std::path::PathBuf {
    dir.join(PENDING_FILE_NAME)
}

/// Debug output names the file but never a secret value.
#[test]
fn test_vault_debug_hides_secrets() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    let v = vault(dir.path(), &enc);
    assert!(v.set("TOP-SECRET-VALUE", "acct"));
    let shown = format!("{v:?}");
    assert!(
        shown.contains("Vault") && shown.contains("vault.bin"),
        "{shown}"
    );
    assert!(!shown.contains("TOP-SECRET-VALUE"));
}

/// A write into a directory that does not exist fails and leaves the cache
/// untouched, so the caller can report the failed save.
#[test]
fn test_set_fails_when_the_directory_is_missing() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    let v = vault(&dir.path().join("missing"), &enc);
    assert!(!v.set("secret", "acct"));
    assert_eq!(v.value("acct"), None);
}

/// A removal queued from a previous run that names an account which is not in
/// the vault is dropped and the record deleted.
#[test]
fn test_pending_record_for_unknown_account_is_cleared() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    assert!(vault(dir.path(), &enc).set("keep", "acct"));
    fs::write(pending_file(dir.path()), br#"["ghost"]"#).expect("write pending");
    let v = vault(dir.path(), &enc);
    assert_eq!(v.value("acct").as_deref(), Some("keep"));
    assert!(!pending_file(dir.path()).exists());
}

/// A queued removal of a real account is applied when the vault next loads.
#[test]
fn test_pending_removal_is_replayed_on_load() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    let first = vault(dir.path(), &enc);
    assert!(first.set("gone", "acct"));
    assert!(first.set("stays", "other"));
    fs::write(pending_file(dir.path()), br#"["acct"]"#).expect("write pending");
    let second = vault(dir.path(), &enc);
    assert_eq!(second.value("acct"), None);
    assert_eq!(second.value("other").as_deref(), Some("stays"));
    assert!(!pending_file(dir.path()).exists());
    // Durable: a third instance agrees.
    assert_eq!(vault(dir.path(), &enc).value("acct"), None);
}

/// A garbage record is ignored and does not hide stored secrets.
#[test]
fn test_garbage_pending_record_is_ignored() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    assert!(vault(dir.path(), &enc).set("keep", "acct"));
    fs::write(pending_file(dir.path()), b"{not json").expect("write pending");
    assert_eq!(
        vault(dir.path(), &enc).value("acct").as_deref(),
        Some("keep")
    );
}

/// When replaying a removal cannot be written, the secret stays readable and
/// the record is kept so the removal is retried on the next run.
#[test]
fn test_failed_replay_keeps_the_record() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    assert!(vault(dir.path(), &enc).set("v", "acct"));
    fs::write(pending_file(dir.path()), br#"["acct"]"#).expect("write pending");
    enc.set_fail_protect(true);
    let v = vault(dir.path(), &enc);
    assert_eq!(v.value("acct").as_deref(), Some("v"));
    assert!(pending_file(dir.path()).exists());
    enc.set_fail_protect(false);
    assert_eq!(vault(dir.path(), &enc).value("acct"), None);
    assert!(!pending_file(dir.path()).exists());
}

/// A failed removal rewrite is queued (best effort) and the cached secret
/// stays until the removal really succeeds.
#[test]
fn test_remove_queues_when_the_rewrite_fails() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    let v = vault(dir.path(), &enc);
    assert!(v.set("v", "acct"));
    enc.set_fail_protect(true);
    v.remove("acct");
    assert_eq!(v.value("acct").as_deref(), Some("v"));
    // The queue itself is plain JSON, so it was persisted without encryption.
    assert!(pending_file(dir.path()).exists());
    enc.set_fail_protect(false);
    assert_eq!(vault(dir.path(), &enc).value("acct"), None);
}

/// A removal that cannot even be queued on disk does not panic or lose the
/// in-memory secret.
#[cfg(unix)]
#[test]
fn test_remove_survives_an_unwritable_directory() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    let v = vault(dir.path(), &enc);
    assert!(v.set("v", "acct"));
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).expect("chmod");
    let writable = fs::write(dir.path().join("probe"), b"x").is_ok();
    v.remove("acct");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).expect("restore");
    if !writable {
        assert_eq!(v.value("acct").as_deref(), Some("v"));
    }
}

/// Removing an account that was never stored is a no-op that writes nothing.
#[test]
fn test_remove_of_unknown_account_is_a_noop() {
    let dir = TempDir::new().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    let v = vault(dir.path(), &enc);
    v.remove("nobody");
    assert!(!dir.path().join("vault.bin").exists());
    assert!(!pending_file(dir.path()).exists());
}
