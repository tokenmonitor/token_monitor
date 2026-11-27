//! Owner-only file creation and process-wide registration, on every host
//! (modes 0600/0700 off Windows, the protected DACL on Windows). This test
//! binary registers the process-wide hooks, so it stays on its own.

use std::io::Write;

use tokenmon_platform_windows::secrets::{create_new_owner_only, register_owner_only_permissions};

/// A new secret file is created empty and writable, refuses to adopt an
/// existing path (so a pre-created permissive file is never reused), and is
/// readable only by its owner where the host has modes.
#[test]
fn test_create_new_owner_only_creates_once_and_refuses_existing_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.bin");
    let mut file = create_new_owner_only(&path).expect("created");
    file.write_all(b"secret").expect("owner can write");
    drop(file);
    assert_eq!(std::fs::read(&path).expect("owner can read"), b"secret");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    let err = create_new_owner_only(&path).expect_err("existing path is refused");
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&path).expect("untouched"), b"secret");

    let missing_parent = dir.path().join("no-such-dir").join("f");
    assert!(create_new_owner_only(&missing_parent).is_err());
}

/// Registration installs the hooks once: the first call reports it, later
/// calls are refused, and afterwards the portable creator in
/// `tokenmon_platform::secrets` goes through the owner-only one.
#[test]
fn test_register_owner_only_permissions_installs_the_hooks_once() {
    assert!(register_owner_only_permissions(), "first registration");
    assert!(!register_owner_only_permissions(), "second is refused");

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("via-platform.dat");
    let file = tokenmon_platform::secrets::create_new_owner_only(&path).expect("created");
    drop(file);
    assert!(path.is_file());
    assert!(tokenmon_platform::secrets::create_new_owner_only(&path).is_err());
}
