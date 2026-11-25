//! Relaunch and MSI command construction, path classification, the real
//! launcher and the `--wait-for-pid` command-line contract.

use std::ffi::OsString;
use std::path::Path;

use tokenmon_platform::install::handle_startup_args;
use tokenmon_platform::install::relaunch::{
    CommandSpec, ProcessLauncher, StdLauncher, is_absolute_windows_path, msiexec_spec_in,
    parse_wait_for_pid, relaunch_spec, system_directory, system_directory_from, wait_for_pid_exit,
};

/// Drive letters with either slash and UNC paths are absolute; everything
/// else (relative, drive-relative, too short, empty) is not.
#[test]
fn test_is_absolute_windows_path_classification() {
    for yes in [r"C:\Windows", "d:/x", r"Z:\", r"\\server\share", r"\\"] {
        assert!(is_absolute_windows_path(yes), "{yes}");
    }
    for no in [
        "",
        "C:",
        "C:x",
        r"C:Windows",
        "1:\\x",
        "/usr/bin",
        r"\x",
        "ab",
        "relative\\path",
    ] {
        assert!(!is_absolute_windows_path(no), "{no}");
    }
}

/// A usable `%SystemRoot%` is trimmed and suffixed; a missing or non-absolute
/// one falls back to `C:\Windows`.
#[test]
fn test_system_directory_from_root_value() {
    assert_eq!(system_directory_from(Some(r"D:\Win")), r"D:\Win\System32");
    assert_eq!(system_directory_from(Some("D:/Win//")), r"D:/Win\System32");
    assert_eq!(system_directory_from(None), r"C:\Windows\System32");
    assert_eq!(system_directory_from(Some("evil")), r"C:\Windows\System32");
    assert_eq!(system_directory_from(Some("")), r"C:\Windows\System32");
}

/// The resolved system directory is always an absolute `...\System32` path.
#[test]
fn test_system_directory_is_absolute_system32() {
    let dir = system_directory();
    assert!(is_absolute_windows_path(&dir), "{dir}");
    assert!(dir.to_lowercase().ends_with("system32"), "{dir}");
}

/// The relaunch command passes the old pid through `--wait-for-pid`.
#[test]
fn test_relaunch_spec_carries_exe_and_pid() {
    let spec = relaunch_spec(Path::new(r"C:\Apps\TokenMon.exe"), 42);
    assert_eq!(spec.program, OsString::from(r"C:\Apps\TokenMon.exe"));
    assert_eq!(spec.args, ["--wait-for-pid", "42"]);
}

/// `msiexec` is addressed absolutely, with a trailing separator collapsed.
#[test]
fn test_msiexec_spec_in_uses_absolute_program_and_per_user_flags() {
    let spec = msiexec_spec_in(r"C:\Windows\System32\", Path::new(r"C:\t\x.msi"));
    assert_eq!(
        spec.program,
        OsString::from(r"C:\Windows\System32\msiexec.exe")
    );
    assert_eq!(
        spec.args,
        [
            "/i",
            r"C:\t\x.msi",
            "/passive",
            "/norestart",
            "ALLUSERS=2",
            "MSIINSTALLPERUSER=1"
        ]
    );
}

/// Every accepted spelling of the flag yields the pid; the first occurrence wins.
#[test]
fn test_parse_wait_for_pid_accepts_both_spellings() {
    assert_eq!(parse_wait_for_pid(["x", "--wait-for-pid", "42"]), Some(42));
    assert_eq!(parse_wait_for_pid(["x", "--wait-for-pid=7"]), Some(7));
    assert_eq!(parse_wait_for_pid(["x", "--wait-for-pid", " 9 "]), Some(9));
    assert_eq!(
        parse_wait_for_pid(vec![
            "a".to_owned(),
            "b".to_owned(),
            "--wait-for-pid=5".to_owned()
        ]),
        Some(5)
    );
    assert_eq!(
        parse_wait_for_pid(["--wait-for-pid", "3", "--wait-for-pid", "4"]),
        Some(3)
    );
}

/// Missing, zero, negative, overflowing, empty or dangling values give `None`.
#[test]
fn test_parse_wait_for_pid_rejects_bad_input() {
    for args in [
        vec![],
        vec!["x"],
        vec!["--wait-for-pid"],
        vec!["--wait-for-pid", "0"],
        vec!["--wait-for-pid=0"],
        vec!["--wait-for-pid", "-3"],
        vec!["--wait-for-pid="],
        vec!["--wait-for-pid", "4294967296"],
        vec!["--wait-for-pid-extra", "5"],
    ] {
        assert_eq!(parse_wait_for_pid(args.clone()), None, "{args:?}");
    }
}

/// With a pid on the command line startup waits (immediately true on a gone
/// pid) and still sweeps leftovers; the pid is reported back.
#[test]
fn test_handle_startup_args_with_pid_and_owned_strings() {
    let dir = tempfile::tempdir().expect("tempdir");
    let exe = dir.path().join("TokenMon.exe");
    std::fs::write(&exe, b"x").expect("write exe");
    let args = vec![
        "TokenMon.exe".to_owned(),
        "--wait-for-pid=2147480000".to_owned(),
    ];
    assert_eq!(handle_startup_args(args, &exe), Some(2_147_480_000));
    assert_eq!(handle_startup_args(Vec::<String>::new(), &exe), None);
    assert!(exe.exists());
}

/// Waiting on a pid that is not running reports it gone.
#[test]
fn test_wait_for_pid_exit_returns_true_for_missing_process() {
    assert!(wait_for_pid_exit(
        u32::MAX - 1,
        std::time::Duration::from_millis(50)
    ));
}

/// The real launcher starts a program and reports spawn failures as errors.
#[test]
fn test_std_launcher_spawns_and_reports_missing_program() {
    let me = std::env::current_exe().expect("current exe");
    let ok = CommandSpec {
        program: me.into_os_string(),
        args: vec!["--list".into()],
    };
    StdLauncher
        .spawn_detached(&ok)
        .expect("test harness starts");
    let missing = CommandSpec {
        program: "/definitely/not/a/real/program-tokenmon".into(),
        args: vec![],
    };
    assert!(StdLauncher.spawn_detached(&missing).is_err());
}
