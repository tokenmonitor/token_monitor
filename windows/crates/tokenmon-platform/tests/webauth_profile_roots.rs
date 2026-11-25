//! Sign-in profile naming and the guards in front of the recursive purge.

use std::fs;
use std::io;
use std::path::PathBuf;

use tempfile::TempDir;
use tokenmon_platform::webauth::profile::{ProfileStore, remove_dir_if_exists};

fn store_in(dir: &TempDir) -> ProfileStore {
    ProfileStore::new(dir.path().join("TokenMon").join("signin"))
}

/// Names are 1 to 64 characters of `a-z`, digits, `-` and `_`.
#[test]
fn test_profile_names_are_validated_at_the_boundaries() {
    let dir = TempDir::new().expect("tempdir");
    let s = store_in(&dir);
    assert!(s.provider_dir(&"a".repeat(64)).is_ok());
    for bad in [
        "",
        &"a".repeat(65),
        "Claude",
        "a/b",
        "a.b",
        "..",
        "a b",
        "é",
    ] {
        assert!(s.provider_dir(bad).is_err(), "{bad:?}");
        assert!(s.session_dir("claude", bad).is_err(), "{bad:?}");
    }
    let ok = s.session_dir("claude", "tok_1-x").expect("valid");
    assert_eq!(ok, s.root().join("claude").join("tok_1-x"));
    assert!(s.session_dir("Bad", "tok").is_err());
    let err = s.provider_dir("Bad").expect_err("invalid");
    assert!(err.to_string().contains("invalid profile name"));
}

/// Roots that are not `<...>/TokenMon/signin` are never purged.
#[test]
fn test_check_root_refuses_the_wrong_shapes() {
    let dir = TempDir::new().expect("tempdir");
    let base = dir.path();
    let cases: [PathBuf; 4] = [
        base.join("TokenMon").join("other"),
        base.join("Elsewhere").join("signin"),
        PathBuf::from("signin"),
        base.join("TokenMon")
            .join("..")
            .join("TokenMon")
            .join("signin"),
    ];
    for root in cases {
        let e = ProfileStore::new(&root)
            .check_root()
            .expect_err(&format!("{root:?}"));
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{root:?}");
    }
    assert!(
        store_in(&dir).check_root().is_ok(),
        "a missing root is fine"
    );
    // Matching is case-insensitive like the Windows file system.
    let upper = ProfileStore::new(base.join("TOKENMON").join("SignIn"));
    assert!(upper.check_root().is_ok());
}

/// A root that exists as a regular file is refused.
#[test]
fn test_check_root_refuses_a_file_root() {
    let dir = TempDir::new().expect("tempdir");
    let s = store_in(&dir);
    fs::create_dir_all(s.root().parent().expect("parent")).expect("mkdir");
    fs::write(s.root(), b"x").expect("write");
    let e = s.check_root().expect_err("file root");
    assert!(e.to_string().contains("not a directory"));
}

/// A root that is a symlink is refused, and a path whose parent is a file
/// reports the underlying I/O error.
#[cfg(unix)]
#[test]
fn test_check_root_refuses_symlink_and_reports_io_errors() {
    let dir = TempDir::new().expect("tempdir");
    let s = store_in(&dir);
    fs::create_dir_all(s.root().parent().expect("parent")).expect("mkdir");
    let target = dir.path().join("target");
    fs::create_dir(&target).expect("mkdir target");
    std::os::unix::fs::symlink(&target, s.root()).expect("symlink");
    let e = s.check_root().expect_err("symlink root");
    assert!(e.to_string().contains("symlink"));

    let file = dir.path().join("plain");
    fs::write(&file, b"x").expect("write");
    let under_file = ProfileStore::new(file.join("TokenMon").join("signin"));
    let e = under_file.check_root().expect_err("not a directory path");
    assert_ne!(e.kind(), io::ErrorKind::NotFound);
}

/// Purging one provider leaves the others; purging everything empties the
/// root; both tolerate missing folders and reject invalid names.
#[test]
fn test_purge_provider_and_purge_all() {
    let dir = TempDir::new().expect("tempdir");
    let s = store_in(&dir);
    s.purge_all().expect("missing root is fine");
    s.purge_provider("claude")
        .expect("missing provider is fine");
    let claude = s.session_dir("claude", "t1").expect("dir");
    let grok = s.session_dir("grok", "t1").expect("dir");
    fs::create_dir_all(&claude).expect("mkdir");
    fs::create_dir_all(&grok).expect("mkdir");
    s.purge_provider("claude").expect("purge");
    assert!(!claude.exists() && grok.exists());
    let e = s.purge_provider("../etc").expect_err("invalid name");
    assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
    s.purge_all().expect("purge all");
    assert!(!s.root().exists());
    let bad = ProfileStore::new(dir.path().join("signin"));
    assert!(bad.purge_all().is_err());
    assert!(bad.purge_provider("claude").is_err());
}

/// `remove_dir_if_exists` removes trees, tolerates absence and unlinks
/// symlinks without following them.
#[test]
fn test_remove_dir_if_exists_handles_trees_missing_and_links() {
    let dir = TempDir::new().expect("tempdir");
    let tree = dir.path().join("tree");
    fs::create_dir_all(tree.join("a/b")).expect("mkdir");
    fs::write(tree.join("a/b/f"), b"x").expect("write");
    remove_dir_if_exists(&tree).expect("remove tree");
    assert!(!tree.exists());
    remove_dir_if_exists(&tree).expect("already gone");
    #[cfg(unix)]
    {
        let real = dir.path().join("real");
        fs::create_dir(&real).expect("mkdir");
        fs::write(real.join("keep"), b"x").expect("write");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        remove_dir_if_exists(&link).expect("unlink");
        assert!(!link.exists() && real.join("keep").exists());
    }
}
