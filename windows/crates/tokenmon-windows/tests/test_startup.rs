//! The startup decisions of `TokenMon.exe`, run in-process with fake
//! collaborators: which exit code a smoke run earns, that a normal start
//! hands over to the app, and the logging fallbacks. These tests install the
//! process-wide log subscriber, so they live in their own test binary.

use std::cell::Cell;
use std::process::ExitCode;

use tokenmon_windows::exe_resources::resources_match_with;
use tokenmon_windows::startup::{
    Collaborators, data_dir_from_env, init_file_logging, is_smoke, process_env, start,
};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// `ExitCode` has no `PartialEq`; its debug text is stable enough to compare.
fn same(a: ExitCode, b: ExitCode) -> bool {
    format!("{a:?}") == format!("{b:?}")
}

/// Runs `start` with fakes and reports (exit code, resources checked, cycle run,
/// app started, data dir resolved).
fn drive(list: &[&str], resources: bool, cycle: bool) -> (ExitCode, [bool; 4]) {
    let (res_called, cycle_called, app_called, dir_called) = (
        Cell::new(false),
        Cell::new(false),
        Cell::new(false),
        Cell::new(false),
    );
    let code = start(
        &args(list),
        "9.9.9",
        &Collaborators {
            env: &|_| {
                dir_called.set(true);
                None
            },
            resources_ok: &|v| {
                assert_eq!(v, "9.9.9");
                res_called.set(true);
                resources
            },
            smoke_ok: &|_| {
                cycle_called.set(true);
                cycle
            },
            app: &|a, v| {
                assert_eq!((a.len(), v), (list.len(), "9.9.9"));
                app_called.set(true);
                ExitCode::from(7)
            },
        },
    );
    (
        code,
        [
            res_called.get(),
            cycle_called.get(),
            app_called.get(),
            dir_called.get(),
        ],
    )
}

/// Only the exact `--smoke` argument selects the headless run.
#[test]
fn test_is_smoke_matches_only_the_exact_flag() {
    assert!(is_smoke(&args(&["TokenMon.exe", "--smoke"])));
    assert!(is_smoke(&args(&["--smoke", "--other"])));
    assert!(!is_smoke(&args(&["TokenMon.exe"])));
    assert!(!is_smoke(&args(&["TokenMon.exe", "--smoke-test", "smoke"])));
    assert!(!is_smoke(&[]));
}

/// A smoke run whose resources and cycle pass exits with success and never
/// starts the app or touches the data directory.
#[test]
fn test_smoke_run_that_passes_exits_success_without_starting_the_app() {
    let (code, [res, cycle, app, dir]) = drive(&["x", "--smoke"], true, true);
    assert!(same(code, ExitCode::SUCCESS));
    assert_eq!([res, cycle, app, dir], [true, true, false, false]);
}

/// A failing resource check fails the smoke run and skips the (slower) cycle.
#[test]
fn test_smoke_run_with_bad_resources_fails_without_running_the_cycle() {
    let (code, [res, cycle, app, _]) = drive(&["x", "--smoke"], false, true);
    assert!(same(code, ExitCode::FAILURE));
    assert_eq!([res, cycle, app], [true, false, false]);
}

/// A failing cycle fails the smoke run even when the resources match.
#[test]
fn test_smoke_run_with_failing_cycle_exits_failure() {
    let (code, [res, cycle, app, _]) = drive(&["x", "--smoke"], true, false);
    assert!(same(code, ExitCode::FAILURE));
    assert_eq!([res, cycle, app], [true, true, false]);
}

/// Without `--smoke` the app starts with the arguments and its exit code is
/// the process's; the smoke checks never run.
#[test]
fn test_plain_start_hands_over_to_the_app_and_returns_its_exit_code() {
    let (code, [res, cycle, app, dir]) = drive(&["x", "--wait-for-pid", "5"], true, true);
    assert!(same(code, ExitCode::from(7)));
    assert_eq!([res, cycle, app, dir], [false, false, true, true]);
}

/// File logging starts in a usable data directory and creates the log folder;
/// a missing directory or an unusable one falls back to stdout instead of
/// aborting.
#[test]
fn test_file_logging_falls_back_to_stdout_when_the_folder_is_unavailable() {
    let data = tempfile::tempdir().expect("tempdir");
    assert!(init_file_logging(Some(data.path().to_path_buf())));
    assert!(data.path().join("logs").is_dir(), "log folder created");

    assert!(!init_file_logging(None), "no data dir: stdout fallback");

    let file = data.path().join("not-a-dir");
    std::fs::write(&file, b"x").expect("write");
    assert!(
        !init_file_logging(Some(file)),
        "a file where the folder should be: stdout fallback"
    );
}

/// The data directory comes from the environment lookup and is created; an
/// environment without any base directory yields `None` (stdout logging).
#[test]
fn test_data_dir_from_env_creates_the_directory_or_is_none() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().to_path_buf();
    let dir = data_dir_from_env(|name| (name == "XDG_DATA_HOME").then(|| home.clone().into()))
        .expect("resolved");
    assert!(dir.is_dir());
    assert!(dir.starts_with(base.path()));
    assert!(data_dir_from_env(|_| None).is_none());
}

/// The resource check reports a version-resource mismatch first and stops
/// there, so the icon check is not consulted.
#[test]
fn test_resource_check_stops_at_a_version_mismatch() {
    let icon_asked = Cell::new(false);
    let ok = resources_match_with("1.0.0", &|v| v != "1.0.0", &|| {
        icon_asked.set(true);
        true
    });
    assert!(!ok);
    assert!(!icon_asked.get());
}

/// A missing icon fails the check even when the version matches.
#[test]
fn test_resource_check_fails_without_an_icon() {
    assert!(!resources_match_with("1.0.0", &|_| true, &|| false));
}

/// Both resources present passes, and the version is handed to the check.
#[test]
fn test_resource_check_passes_when_both_resources_are_present() {
    assert!(resources_match_with("2.3.4", &|v| v == "2.3.4", &|| true));
}

/// The production environment lookup reads the real process environment: a
/// variable that is set resolves, an unset one does not.
#[test]
fn test_process_env_reads_the_real_environment() {
    let (name, value) = std::env::vars_os()
        .find_map(|(k, v)| k.into_string().ok().map(|k| (k, v)))
        .expect("the test process has an environment variable");
    assert_eq!(process_env(&name), Some(value));
    assert!(process_env("TOKENMON_SURELY_UNSET_VAR").is_none());
}
