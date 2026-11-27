//! Startup decisions of the executable: logging set-up and the exit code of a
//! `--smoke` run versus a normal start.

use std::path::PathBuf;
use std::process::ExitCode;

/// The command-line switch that runs the headless self-test (CI runs the real
/// exe with it).
pub const SMOKE_FLAG: &str = "--smoke";

/// Whether `args` ask for the headless smoke run.
///
/// # Examples
///
/// ```
/// use tokenmon_windows::startup::is_smoke;
/// assert!(is_smoke(&["TokenMon.exe".into(), "--smoke".into()]));
/// assert!(!is_smoke(&["TokenMon.exe".into()]));
/// ```
#[must_use]
pub fn is_smoke(args: &[String]) -> bool {
    args.iter().any(|a| a == SMOKE_FLAG)
}

/// Logs to stdout for the headless smoke run (CI reads it).
///
/// A second initialisation (tests, embedding) is ignored.
pub fn init_stdout_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

/// Logs to the log folder under `data_dir` (the app has no console) and
/// records panics before the release profile aborts. Falls back to stdout when
/// there is no data directory or the folder is unavailable.
///
/// Returns whether file logging was set up.
pub fn init_file_logging(data_dir: Option<PathBuf>) -> bool {
    let started = data_dir.is_some_and(|d| tokenmon_app::logging::init_file_logging(&d).is_ok());
    if !started {
        init_stdout_logging();
    }
    started
}

/// The production data directory (`%LOCALAPPDATA%\TokenMon`), if it can be
/// resolved and created.
#[must_use]
pub fn production_data_dir() -> Option<PathBuf> {
    data_dir_from_env(|name| std::env::var_os(name))
}

/// The data directory resolved from an environment lookup, created on first
/// use; `None` when no base directory is set or it cannot be created.
#[must_use]
pub fn data_dir_from_env(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    tokenmon_core::storage::AppPaths::from_env(get)
        .ok()
        .and_then(|p| p.directory().ok())
}

/// What a run needs from the outside world; `main` passes the real ones.
pub struct Collaborators<'a> {
    /// Resolves the data directory the log folder lives in.
    pub data_dir: &'a dyn Fn() -> Option<PathBuf>,
    /// Whether the exe carries the resources this build declares.
    pub resources_ok: &'a dyn Fn(&str) -> bool,
    /// Runs the headless refresh cycle; `true` when it passed.
    pub smoke_ok: &'a dyn Fn(&str) -> bool,
    /// Starts the real application (the Win32 shell) and returns its exit code.
    pub app: &'a dyn Fn(&[String], &str) -> ExitCode,
}

/// Runs the executable: `--smoke` logs to stdout and exits with the outcome of
/// the resource check and the headless cycle (the cycle only runs when the
/// resources match); anything else logs to the data directory and starts the
/// application.
pub fn start(args: &[String], version: &str, with: &Collaborators<'_>) -> ExitCode {
    if !is_smoke(args) {
        init_file_logging((with.data_dir)());
        return (with.app)(args, version);
    }
    init_stdout_logging();
    if (with.resources_ok)(version) && (with.smoke_ok)(version) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
