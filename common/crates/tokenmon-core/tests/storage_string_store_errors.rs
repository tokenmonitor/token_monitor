//! `FileBackedStringStore` and `InMemoryStringStore` failure and accessor paths.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;
use tokenmon_core::storage::{
    FileBackedStringStore, InMemoryStringStore, NoopPermissions, OwnerOnlyPermissions, TextStore,
};

#[derive(Debug)]
struct Refusing;

impl OwnerOnlyPermissions for Refusing {
    fn restrict_directory(&self, _path: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "no"))
    }
    fn restrict_file(&self, _path: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "no"))
    }
}

fn store(dir: &TempDir) -> FileBackedStringStore {
    FileBackedStringStore::new(dir.path(), "p_").with_permissions(NoopPermissions)
}

/// The accessors report the configured directory and the keyed file name.
#[test]
fn test_directory_and_path_for() {
    let dir = TempDir::new().expect("tempdir");
    let s = store(&dir);
    assert_eq!(s.directory(), dir.path());
    assert_eq!(s.path_for("k"), Some(dir.path().join("p_k.dat")));
}

/// Keys that could escape the directory are rejected for every operation.
#[test]
fn test_unsafe_keys_are_rejected() {
    let dir = TempDir::new().expect("tempdir");
    let s = store(&dir);
    for key in ["", "..", "a/b", "a\\b", "a\0b", "c:x", "x..y"] {
        assert_eq!(s.path_for(key), None, "{key:?}");
        assert!(!s.set("v", key), "{key:?}");
        assert_eq!(s.value(key), None, "{key:?}");
        s.remove(key);
    }
    assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 0);
}

/// A value roundtrips, is replaced, removed, and a second remove is harmless.
#[test]
fn test_set_replace_remove_cycle() {
    let dir = TempDir::new().expect("tempdir");
    let s = store(&dir);
    assert_eq!(s.value("k"), None);
    assert!(s.set("one", "k"));
    assert!(s.set("two", "k"));
    assert_eq!(s.value("k").as_deref(), Some("two"));
    s.remove("k");
    s.remove("k");
    assert_eq!(s.value("k"), None);
}

/// An unreadable entry (a directory in the file's place) reads as missing and
/// resists removal and overwrite without panicking.
#[test]
fn test_unreadable_entry_degrades_gracefully() {
    let dir = TempDir::new().expect("tempdir");
    let s = store(&dir);
    let blocker = s.path_for("k").expect("safe key");
    fs::create_dir(&blocker).expect("mkdir");
    assert_eq!(s.value("k"), None);
    s.remove("k");
    assert!(
        blocker.is_dir(),
        "the directory must survive a failed remove"
    );
    assert!(!s.set("v", "k"));
}

/// A refusing permission hook fails the write, and the shared-hook setter
/// installs it.
#[test]
fn test_shared_permissions_hook_is_used_and_can_fail_writes() {
    let dir = TempDir::new().expect("tempdir");
    let s =
        FileBackedStringStore::new(dir.path(), "p_").with_shared_permissions(Arc::new(Refusing));
    assert!(!s.set("v", "k"));
    assert_eq!(s.value("k"), None);
}

/// The in-memory fake stores, counts writes, removes and ignores unknown keys.
#[test]
fn test_in_memory_store_roundtrip_and_remove() {
    let s = InMemoryStringStore::default();
    assert_eq!(s.value("k"), None);
    assert!(s.set("v", "k"));
    assert!(s.set("w", "k"));
    assert_eq!(s.write_count(), 2);
    assert_eq!(s.value("k").as_deref(), Some("w"));
    s.remove("k");
    s.remove("never-set");
    assert_eq!(s.value("k"), None);
    assert_eq!(s.write_count(), 2);
}
