//! The `TokenMon` executable end to end: argument handling, logging setup and
//! exit codes, run hermetically with a throwaway data directory.

use std::ffi::OsStr;
use std::process::{Command, Output};

fn run(args: &[&str], envs: &[(&str, &OsStr)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_TokenMon"));
    cmd.args(args).env_clear();
    // Keep coverage instrumentation working in the child process.
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("the binary starts")
}

/// `--smoke` assembles the app over a temp dir, finishes one refresh cycle
/// and exits successfully, logging to stdout rather than to a file.
#[test]
fn test_smoke_flag_runs_headless_cycle_and_exits_success() {
    let home = tempfile::tempdir().expect("tempdir");
    let out = run(
        &["--smoke"],
        &[
            ("HOME", home.path().as_os_str()),
            ("RUST_LOG", OsStr::new("info")),
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("smoke finished"), "log: {text}");
}

/// Without `--smoke` on a non-Windows host the binary refuses to start the
/// tray, logs into the data directory's log folder and exits with failure.
#[cfg(not(windows))]
#[test]
fn test_plain_start_off_windows_fails_and_logs_to_data_dir() {
    let data = tempfile::tempdir().expect("tempdir");
    let out = run(&[], &[("XDG_DATA_HOME", data.path().as_os_str())]);
    assert!(!out.status.success());
    let logs = data.path().join("TokenMon").join("logs");
    let text: String = std::fs::read_dir(&logs)
        .expect("log folder created")
        .map(|e| std::fs::read_to_string(e.expect("entry").path()).unwrap_or_default())
        .collect();
    assert!(text.contains("is a Windows application"), "log: {text}");
}

/// When no data directory can be resolved (empty environment) the binary
/// falls back to stdout logging instead of aborting.
#[cfg(not(windows))]
#[test]
fn test_plain_start_without_data_dir_falls_back_to_stdout_logging() {
    let out = run(&[], &[("RUST_LOG", OsStr::new("error"))]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("is a Windows application"), "stdout: {text}");
}

/// `--smoke` fails (exit non-zero) when no scratch directory can be created,
/// instead of reporting a pass it did not earn.
#[cfg(unix)]
#[test]
fn test_smoke_fails_when_temp_directory_is_unavailable() {
    let missing = std::path::Path::new("/nonexistent-tokenmon-tmp");
    let out = run(
        &["--smoke"],
        &[
            ("TMPDIR", missing.as_os_str()),
            ("RUST_LOG", OsStr::new("error")),
        ],
    );
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("smoke: no temp directory"), "stdout: {text}");
}
