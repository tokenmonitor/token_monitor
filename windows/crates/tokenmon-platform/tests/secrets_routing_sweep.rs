//! Migration sweep details of the routing credential store: staging
//! leftovers, unrelated files, odd names, a missing directory, and the
//! migration log lines (captured with a recording subscriber).

use std::fmt::Write as _;
use std::fs;
use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use tokenmon_core::{CredentialKey, CredentialStore};
use tokenmon_platform::secrets::{
    FakeEncryptor, FileBackedCredentialStore, SecretRoutingCredentialStore, Vault,
};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

/// Records every event's formatted fields so tests can assert on log output.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<String>>>);

struct FieldText<'a>(&'a mut String);

impl tracing::field::Visit for FieldText<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, "{}={value:?} ", field.name());
    }
}

impl Subscriber for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut text = String::new();
        event.record(&mut FieldText(&mut text));
        self.0.lock().expect("lock").push(text);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

fn routing(dir: &std::path::Path) -> (Arc<Vault>, SecretRoutingCredentialStore) {
    let vault = Arc::new(Vault::new(dir, Arc::new(FakeEncryptor::default())));
    let store = SecretRoutingCredentialStore::new(vault.clone(), dir, "c_");
    (vault, store)
}

/// Staging files of a crashed write are removed; look-alikes are not.
#[test]
fn test_sweep_removes_only_tmp_staging_files_of_secret_keys() {
    let dir = TempDir::new().expect("tempdir");
    let stale = dir.path().join(".c_session.dat.7-1.tmp");
    let stale_upper = dir.path().join(".c_session.dat.7-2.TMP");
    let not_tmp = dir.path().join(".c_session.dat.7-3.bak");
    let other_key = dir.path().join(".c_email.dat.7-4.tmp");
    let unrelated = dir.path().join("notes.txt");
    for p in [&stale, &stale_upper, &not_tmp, &other_key, &unrelated] {
        fs::write(p, b"x").expect("write");
    }
    let _ = routing(dir.path());
    assert!(!stale.exists() && !stale_upper.exists());
    assert!(not_tmp.exists() && other_key.exists() && unrelated.exists());
}

/// A store over a directory that does not exist yet is constructed without
/// panicking and serves plain keys once the directory appears.
#[test]
fn test_missing_directory_is_tolerated_by_the_sweep() {
    let dir = TempDir::new().expect("tempdir");
    let missing = dir.path().join("later");
    let (_, store) = routing(&missing);
    assert_eq!(store.get(CredentialKey::Session), None);
}

/// Entries whose names are not valid UTF-8 are skipped, not mistaken for
/// leftovers (Linux only: APFS refuses such names).
#[cfg(target_os = "linux")]
#[test]
fn test_non_utf8_entries_are_skipped() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let dir = TempDir::new().expect("tempdir");
    let odd = dir
        .path()
        .join(OsString::from_vec(b".c_session.dat.\xff.tmp".to_vec()));
    fs::write(&odd, b"x").expect("write");
    let _ = routing(dir.path());
    assert!(odd.exists());
}

/// Migrating a readable plaintext secret logs success without the value.
#[test]
fn test_successful_migration_is_logged_without_the_secret() {
    let dir = TempDir::new().expect("tempdir");
    FileBackedCredentialStore::new(dir.path(), "c_")
        .set(CredentialKey::Session, "S3CR3T-VALUE")
        .expect("legacy write");
    let rec = Recorder::default();
    let (vault, _store) = tracing::subscriber::with_default(rec.clone(), || routing(dir.path()));
    assert_eq!(vault.value("c_session").as_deref(), Some("S3CR3T-VALUE"));
    let log = rec.0.lock().expect("lock").join("\n");
    assert!(log.contains("moved a plaintext credential"), "{log}");
    assert!(!log.contains("S3CR3T-VALUE"));
}

/// A secret the vault refuses is deleted and the failure is logged.
#[test]
fn test_refused_migration_deletes_the_file_and_logs() {
    let dir = TempDir::new().expect("tempdir");
    let legacy = FileBackedCredentialStore::new(dir.path(), "c_");
    legacy
        .set(CredentialKey::Session, "S3CR3T-VALUE")
        .expect("legacy write");
    let enc = Arc::new(FakeEncryptor::default());
    enc.set_fail_protect(true);
    let vault = Arc::new(Vault::new(dir.path(), enc));
    let rec = Recorder::default();
    tracing::subscriber::with_default(rec.clone(), || {
        let _ = SecretRoutingCredentialStore::new(vault.clone(), dir.path(), "c_");
    });
    assert!(!legacy.path(CredentialKey::Session).exists());
    assert_eq!(vault.value("c_session"), None);
    let log = rec.0.lock().expect("lock").join("\n");
    assert!(
        log.contains("could not move a plaintext credential"),
        "{log}"
    );
    assert!(!log.contains("S3CR3T-VALUE"));
}
