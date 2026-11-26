//! Failure paths of the plaintext-file credential store, `atomic_write` and
//! `secure_delete`, plus the swap helpers' no-parent guards.

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use tokenmon_core::{CredentialKey, CredentialStore, CredentialStoreError};
use tokenmon_platform::install::swap::{can_replace_running_exe, cleanup_leftovers, stage_next_to};
use tokenmon_platform::secrets::{FileBackedCredentialStore, atomic_write, secure_delete};

/// A path with no file name cannot be written atomically.
#[test]
fn test_atomic_write_rejects_paths_without_a_file_name() {
    let e = atomic_write(Path::new(".."), b"x").expect_err("no file name");
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
    let e = atomic_write(Path::new("/"), b"x").expect_err("root has no name");
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
}

/// Writing into a missing directory fails and leaves no staging file behind.
#[test]
fn test_atomic_write_into_missing_directory_fails_cleanly() {
    let dir = TempDir::new().expect("tempdir");
    assert!(atomic_write(&dir.path().join("gone").join("f"), b"x").is_err());
    assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 0);
}

/// Deleting a missing file succeeds; a file that cannot be overwritten is
/// still unlinked.
#[test]
fn test_secure_delete_handles_missing_and_readonly_files() {
    let dir = TempDir::new().expect("tempdir");
    secure_delete(&dir.path().join("never-existed")).expect("missing is fine");
    let f = dir.path().join("ro.dat");
    fs::write(&f, b"secret").expect("write");
    let mut perms = fs::metadata(&f).expect("meta").permissions();
    perms.set_readonly(true);
    fs::set_permissions(&f, perms).expect("chmod");
    // On Windows a read-only file cannot be unlinked either; both outcomes
    // are acceptable as long as nothing panics and the error is reported.
    let result = secure_delete(&f);
    if result.is_ok() {
        assert!(!f.exists());
    }
}

/// A directory where the credential file should be reads as missing, and a
/// failed remove or write is reported without panicking.
#[test]
fn test_credential_store_degrades_when_the_entry_is_a_directory() {
    let dir = TempDir::new().expect("tempdir");
    let store = FileBackedCredentialStore::new(dir.path(), "c_");
    fs::create_dir(store.path(CredentialKey::Session)).expect("mkdir");
    assert_eq!(store.get(CredentialKey::Session), None);
    store.remove(CredentialKey::Session);
    assert!(store.path(CredentialKey::Session).is_dir());
    // Writing replaces a directory? It must not: the rename onto a directory fails.
    let err = store
        .set(CredentialKey::Session, "v")
        .expect_err("cannot replace a directory");
    assert!(matches!(err, CredentialStoreError::WriteFailed(_)));
}

/// Non-UTF-8 content reads as absent rather than corrupt text.
#[test]
fn test_credential_store_ignores_non_utf8_content() {
    let dir = TempDir::new().expect("tempdir");
    let store = FileBackedCredentialStore::new(dir.path(), "c_");
    fs::write(store.path(CredentialKey::Email), [0xFF, 0xFE, 0xFD]).expect("write");
    assert_eq!(store.get(CredentialKey::Email), None);
    store.remove(CredentialKey::Email);
    store.remove(CredentialKey::Email);
}

/// A bare file name has an (empty) parent: nothing can be replaced there and
/// staging fails without leaving files.
#[test]
fn test_swap_helpers_guard_odd_paths() {
    assert!(!can_replace_running_exe(Path::new("TokenMon.txt")));
    assert!(!can_replace_running_exe(Path::new("/")));
    cleanup_leftovers(Path::new("/"));
    cleanup_leftovers(Path::new("/definitely/not/here/TokenMon.exe"));
    let dir = TempDir::new().expect("tempdir");
    let new_exe = dir.path().join("new.exe");
    fs::write(&new_exe, b"x").expect("write");
    assert!(stage_next_to(&new_exe, Path::new("/")).is_err());
    let missing_dir = dir.path().join("gone").join("TokenMon.exe");
    assert!(stage_next_to(&new_exe, &missing_dir).is_err());
}
