#![allow(clippy::unwrap_used)]
//! The Windows owner-only policy: SDDL strings, the atomic DACL at creation and
//! the SID-based directory restriction. The SDDL builders and the Unix mode
//! branches run on every host; the DACL read-backs run on Windows only.

use std::fs;

use tempfile::TempDir;
#[cfg(windows)]
use tokenmon_platform_windows::secrets::create_new_owner_only;
use tokenmon_platform_windows::secrets::{owner_only_directory_sddl, owner_only_sddl};

#[test]
fn sddl_is_a_protected_single_ace_dacl() {
    let sddl = owner_only_sddl("S-1-5-21-1-2-3-1001");
    assert_eq!(sddl, "D:P(A;;FA;;;S-1-5-21-1-2-3-1001)");
    assert!(sddl.starts_with("D:P"), "inheritance must be blocked");
    assert_eq!(sddl.matches("(A;").count(), 1, "exactly one ACE");
}

#[cfg(windows)]
#[test]
fn windows_file_gets_only_the_current_user_ace_at_creation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("vault.bin");
    drop(create_new_owner_only(&path).unwrap());
    let out = std::process::Command::new("icacls")
        .arg(&path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    for wide in ["Everyone", "BUILTIN\\Users", "Authenticated Users"] {
        assert!(!text.contains(wide), "{wide} has access:\n{text}");
    }
    // Protected: no inherited ACE "(I)" survives.
    assert!(!text.contains("(I)"), "inherited ACE present:\n{text}");
}

/// #47: the data-directory DACL is protected and grants one SID, inherited by
/// children, with no dependency on `USERNAME`/`USERDOMAIN` or a child process.
#[test]
fn directory_sddl_is_protected_single_inheritable_ace() {
    let sddl = owner_only_directory_sddl("S-1-5-21-1-2-3-1001");
    assert_eq!(sddl, "D:P(A;OICI;FA;;;S-1-5-21-1-2-3-1001)");
    assert_eq!(sddl.matches("(A;").count(), 1);
}

#[test]
fn owner_only_implementation_restricts_and_stays_usable() {
    use tokenmon_core::storage::OwnerOnlyPermissions;
    use tokenmon_platform_windows::secrets::WindowsOwnerOnly;
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("f");
    fs::write(&file, b"x").unwrap();
    WindowsOwnerOnly.restrict_directory(dir.path()).unwrap();
    WindowsOwnerOnly.restrict_file(&file).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(dir.path()), 0o700);
        assert_eq!(mode(&file), 0o600);
    }
    // On every OS the owner keeps full access afterwards.
    fs::write(dir.path().join("new"), b"y").unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"x");
}

/// Lists the ACEs `icacls` reports for `path`, as `(principal, rights)` pairs.
///
/// `icacls` prints the path followed by `DOMAIN\user:(OI)(CI)(F)` lines and a
/// trailing status line; the path prefix is stripped from the first ACE.
#[cfg(windows)]
fn read_back_aces(path: &std::path::Path) -> Vec<(String, String)> {
    let out = std::process::Command::new("icacls")
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success(), "icacls failed: {out:?}");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let path_text = path.display().to_string();
    text.lines()
        .map(|l| l.trim().trim_start_matches(&path_text).trim())
        .filter(|l| l.contains(":("))
        .filter_map(|l| {
            let (who, rights) = l.split_once(":(")?;
            Some((who.trim().to_owned(), format!("({rights}")))
        })
        .collect()
}

/// FEAT-core-storage-owner-only-acl: reads the DACL back from disk. The
/// directory and the file each carry exactly one ACE, for the current user,
/// with full control, and neither inherits from its parent (no `(I)` flag);
/// the directory's ACE is inheritable by children.
#[cfg(windows)]
#[test]
fn windows_owner_only_dacl_read_back_has_a_single_current_user_ace() {
    use tokenmon_core::storage::OwnerOnlyPermissions;
    use tokenmon_platform_windows::secrets::WindowsOwnerOnly;
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("f");
    fs::write(&file, b"x").unwrap();
    WindowsOwnerOnly.restrict_directory(dir.path()).unwrap();
    WindowsOwnerOnly.restrict_file(&file).unwrap();

    let whoami = std::process::Command::new("whoami").output().unwrap();
    let user = String::from_utf8_lossy(&whoami.stdout)
        .trim()
        .to_lowercase();

    let dir_aces = read_back_aces(dir.path());
    assert_eq!(dir_aces.len(), 1, "directory ACEs: {dir_aces:?}");
    assert_eq!(dir_aces[0].0.to_lowercase(), user);
    assert!(dir_aces[0].1.contains("(F)"), "{dir_aces:?}");
    assert!(dir_aces[0].1.contains("(OI)") && dir_aces[0].1.contains("(CI)"));
    assert!(
        !dir_aces[0].1.contains("(I)"),
        "inherited ACE: {dir_aces:?}"
    );

    let file_aces = read_back_aces(&file);
    assert_eq!(file_aces.len(), 1, "file ACEs: {file_aces:?}");
    assert_eq!(file_aces[0].0.to_lowercase(), user);
    assert!(file_aces[0].1.contains("(F)"), "{file_aces:?}");
    assert!(
        !file_aces[0].1.contains("(I)"),
        "inherited ACE: {file_aces:?}"
    );
}
