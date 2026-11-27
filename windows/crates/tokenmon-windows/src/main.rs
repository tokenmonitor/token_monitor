//! TokenMon for Windows: the system-tray binary. Thin by design: everything
//! testable lives in the `tokenmon_app` library and in this package's lib
//! (`tokenmon_windows::startup`); `shell` is the Win32 glue.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

use tokenmon_windows::exe_resources;
use tokenmon_windows::startup::{Collaborators, process_env, start};

#[cfg(windows)]
mod shell;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    start(
        &args,
        env!("CARGO_PKG_VERSION"),
        &Collaborators {
            env: &process_env,
            resources_ok: &exe_resources::own_resources_match,
            smoke_ok: &tokenmon_app::smoke::run_headless,
            app: &run_app,
        },
    )
}

#[cfg(windows)]
fn run_app(args: &[String], version: &str) -> ExitCode {
    shell::run(args, version)
}

#[cfg(not(windows))]
fn run_app(_args: &[String], version: &str) -> ExitCode {
    tracing::error!("TokenMon {version} is a Windows application; use --smoke to self-test");
    ExitCode::FAILURE
}
